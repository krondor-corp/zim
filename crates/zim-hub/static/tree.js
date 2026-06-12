// zim-hub vault tree browser.
//
// All plaintext stays in this tab. Flow:
//
//   1. Load zim-wasm; get the web key into WASM memory:
//      a. sessionStorage cache (survives navigation, dies with tab)
//      b. IndexedDB JWK (same browser that ran device setup)
//      c. escrow recovery: GET /api/v0/escrow/list → pick fragment →
//         GET /api/v0/escrow?did=… → PBKDF2+AES-GCM unwrap with the
//         user's passphrase (same KDF device-setup.js used to wrap)
//   2. WasmVault.open(manifest) — finds our share, recovers the
//      vault secret via x25519 SecretShare.recover in WASM.
//   3. Walk: fetch /blob/{hash} ciphertext, readRootDir/readDir for
//      dirs, readFile for files. Per-entry secrets ride along in
//      the parent dir's decrypted body.

import init, {
  loadKeyFromSession,
  WasmVault,
} from "/static/vendor/zim-wasm/zim_wasm.js";

const PBKDF2_ITERS = 100_000; // must match device-setup.js
const SS_KEY = "zim:webkey"; // sessionStorage slot, hex seed

const app = document.getElementById("tree-app");
const vaultId = app.dataset.vaultId;

const els = {
  unlock: document.getElementById("unlock"),
  unlockForm: document.getElementById("unlock-form"),
  passphrase: document.getElementById("unlock-passphrase"),
  status: document.getElementById("status"),
  error: document.getElementById("error"),
  breadcrumbs: document.getElementById("breadcrumbs"),
  listing: document.getElementById("listing"),
  preview: document.getElementById("preview"),
  previewName: document.getElementById("preview-name"),
  previewDownload: document.getElementById("preview-download"),
  previewClose: document.getElementById("preview-close"),
  previewBody: document.getElementById("preview-body"),
};

const setStatus = (msg) => {
  els.status.textContent = msg || "";
  els.status.style.display = msg ? "" : "none";
};
const setError = (msg) => {
  els.error.textContent = msg;
  els.error.style.display = "";
  setStatus("");
};

// ── byte helpers ─────────────────────────────────────────────────

const hexToBytes = (hex) => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return out;
};
const bytesToHex = (bytes) =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
const b64urlToBytes = (s) => {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
};
const b64ToBytes = (s) => b64urlToBytes(s); // atob handles standard b64 too

const fetchJson = async (url) => {
  const r = await fetch(url, { credentials: "same-origin" });
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.json();
};
const fetchBytes = async (url) => {
  const r = await fetch(url, { credentials: "same-origin" });
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
};

// ── key acquisition ──────────────────────────────────────────────

const idbGetFirstKey = () =>
  new Promise((resolve) => {
    const req = indexedDB.open("zim-devices", 1);
    req.onupgradeneeded = () =>
      req.result.createObjectStore("keys", { keyPath: "did" });
    req.onerror = () => resolve(null);
    req.onsuccess = () => {
      const db = req.result;
      try {
        const tx = db.transaction("keys", "readonly");
        const getAll = tx.objectStore("keys").getAll();
        getAll.onsuccess = () => {
          db.close();
          const rows = getAll.result || [];
          resolve(rows.length ? rows[0] : null);
        };
        getAll.onerror = () => {
          db.close();
          resolve(null);
        };
      } catch {
        db.close();
        resolve(null);
      }
    };
  });

const idbPutKey = (did, jwk) =>
  new Promise((resolve) => {
    const req = indexedDB.open("zim-devices", 1);
    req.onupgradeneeded = () =>
      req.result.createObjectStore("keys", { keyPath: "did" });
    req.onerror = () => resolve();
    req.onsuccess = () => {
      const db = req.result;
      const tx = db.transaction("keys", "readwrite");
      tx.objectStore("keys").put({ did, jwk });
      tx.oncomplete = () => {
        db.close();
        resolve();
      };
      tx.onerror = () => {
        db.close();
        resolve();
      };
    };
  });

const promptPassphrase = () =>
  new Promise((resolve) => {
    els.unlock.style.display = "";
    els.passphrase.focus();
    els.unlockForm.addEventListener(
      "submit",
      (ev) => {
        ev.preventDefault();
        const pp = els.passphrase.value;
        els.unlock.style.display = "none";
        resolve(pp);
      },
      { once: true },
    );
  });

// PBKDF2-SHA256 → AES-GCM unwrap; payload is iv(12) || ct. Mirrors
// device-setup.js's wrapPrivateKey. Plaintext is the JWK JSON.
const unwrapEscrow = async (blob, passphrase) => {
  const salt = b64ToBytes(blob.salt);
  const wrapped = b64ToBytes(blob.wrapped_secret);
  const enc = new TextEncoder();
  const base = await crypto.subtle.importKey(
    "raw",
    enc.encode(passphrase),
    { name: "PBKDF2" },
    false,
    ["deriveKey"],
  );
  const wrapKey = await crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations: PBKDF2_ITERS, hash: "SHA-256" },
    base,
    { name: "AES-GCM", length: 256 },
    false,
    ["decrypt"],
  );
  const iv = wrapped.slice(0, 12);
  const ct = wrapped.slice(12);
  const plaintext = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv },
    wrapKey,
    ct,
  );
  return JSON.parse(new TextDecoder().decode(plaintext));
};

// Load the web key into WASM memory by whatever path works.
const ensureKey = async () => {
  // a. sessionStorage cache.
  const cached = sessionStorage.getItem(SS_KEY);
  if (cached) {
    loadKeyFromSession(hexToBytes(cached));
    return;
  }

  // b. IndexedDB JWK from device setup on this browser.
  const row = await idbGetFirstKey();
  if (row && row.jwk && row.jwk.d) {
    const seed = b64urlToBytes(row.jwk.d);
    loadKeyFromSession(seed);
    sessionStorage.setItem(SS_KEY, bytesToHex(seed));
    return;
  }

  // c. Escrow recovery with passphrase.
  setStatus("");
  const list = await fetchJson("/api/v0/escrow/list");
  if (!list.length) {
    throw new Error(
      "No browser key found. Set one up under Devices → This browser.",
    );
  }
  const blob = await fetchJson(
    `/api/v0/escrow?did=${encodeURIComponent(list[0].did)}`,
  );
  const passphrase = await promptPassphrase();
  setStatus("unwrapping key…");
  let jwk;
  try {
    jwk = await unwrapEscrow(blob, passphrase);
  } catch {
    throw new Error("Unlock failed — wrong passphrase?");
  }
  const seed = b64urlToBytes(jwk.d);
  loadKeyFromSession(seed);
  sessionStorage.setItem(SS_KEY, bytesToHex(seed));
  await idbPutKey(list[0].did, jwk); // skip the prompt next visit
};

// ── tree state + rendering ───────────────────────────────────────

let vault = null;
// Path from root: [{name, hash, secret}]. Empty = at root.
let stack = [];

const loadDir = async (hash, secret) => {
  const ct = await fetchBytes(`/api/v0/v/${vaultId}/blob/${hash}`);
  const json = secret === null ? vault.readRootDir(ct) : vault.readDir(secret, ct);
  return JSON.parse(json);
};

const renderBreadcrumbs = () => {
  els.breadcrumbs.style.display = "";
  els.breadcrumbs.innerHTML = "";
  const mk = (label, depth) => {
    const a = document.createElement("a");
    a.href = "#";
    a.textContent = label;
    a.addEventListener("click", (ev) => {
      ev.preventDefault();
      stack = stack.slice(0, depth);
      void renderCurrent();
    });
    return a;
  };
  els.breadcrumbs.appendChild(mk("/", 0));
  stack.forEach((seg, i) => {
    els.breadcrumbs.appendChild(document.createTextNode(" / "));
    els.breadcrumbs.appendChild(mk(seg.name, i + 1));
  });
};

const renderEntries = (entries) => {
  els.listing.style.display = "";
  els.listing.innerHTML = "";
  if (!entries.length) {
    const empty = document.createElement("div");
    empty.className = "muted";
    empty.style.padding = "0.75rem 1rem";
    empty.textContent = "(empty directory)";
    els.listing.appendChild(empty);
    return;
  }
  // Dirs first, then files; alphabetical within each.
  const sorted = [...entries].sort(
    (a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "dir" ? -1 : 1),
  );
  for (const e of sorted) {
    const row = document.createElement("a");
    row.href = "#";
    row.style.cssText =
      "display:flex;gap:0.6rem;align-items:center;padding:0.55rem 1rem;text-decoration:none;color:inherit;border-bottom:1px solid hsl(var(--border));font-size:0.9rem;";
    const icon = document.createElement("span");
    icon.textContent = e.kind === "dir" ? "📁" : "📄";
    const name = document.createElement("span");
    name.textContent = e.kind === "dir" ? `${e.name}/` : e.name;
    name.style.fontFamily = "var(--font-mono)";
    row.appendChild(icon);
    row.appendChild(name);
    if (e.mime) {
      const mime = document.createElement("span");
      mime.className = "muted";
      mime.style.cssText = "margin-left:auto;font-size:0.75rem;";
      mime.textContent = e.mime;
      row.appendChild(mime);
    }
    row.addEventListener("click", (ev) => {
      ev.preventDefault();
      if (e.kind === "dir") {
        stack.push({ name: e.name, hash: e.hash, secret: e.secret });
        void renderCurrent();
      } else {
        void openFile(e);
      }
    });
    els.listing.appendChild(row);
  }
};

const renderCurrent = async () => {
  try {
    setStatus("loading…");
    els.preview.style.display = "none";
    const top = stack.length ? stack[stack.length - 1] : null;
    const entries = top
      ? await loadDir(top.hash, top.secret)
      : await loadDir(vault.rootHash, null);
    renderBreadcrumbs();
    renderEntries(entries);
    setStatus("");
  } catch (e) {
    console.error(e);
    setError(e.message || String(e));
  }
};

let previewUrl = null;
const openFile = async (entry) => {
  try {
    setStatus(`decrypting ${entry.name}…`);
    const ct = await fetchBytes(`/api/v0/v/${vaultId}/blob/${entry.hash}`);
    const plaintext = vault.readFile(entry.secret, ct);
    setStatus("");

    if (previewUrl) URL.revokeObjectURL(previewUrl);
    const mime = entry.mime || "application/octet-stream";
    previewUrl = URL.createObjectURL(new Blob([plaintext], { type: mime }));

    els.preview.style.display = "";
    els.previewName.textContent = entry.name;
    els.previewDownload.href = previewUrl;
    els.previewDownload.download = entry.name;
    els.previewBody.innerHTML = "";

    const isText =
      mime.startsWith("text/") ||
      mime === "application/json" ||
      mime === "application/toml" ||
      mime === "application/javascript";
    if (isText && plaintext.length < 1_000_000) {
      const pre = document.createElement("pre");
      pre.style.cssText = "margin:0;white-space:pre-wrap;word-break:break-word;font-size:0.8rem;";
      pre.textContent = new TextDecoder().decode(plaintext);
      els.previewBody.appendChild(pre);
    } else if (mime.startsWith("image/")) {
      const img = document.createElement("img");
      img.src = previewUrl;
      img.style.maxWidth = "100%";
      els.previewBody.appendChild(img);
    } else {
      const note = document.createElement("p");
      note.className = "muted";
      note.textContent = `${plaintext.length} bytes (${mime}) — use Download.`;
      els.previewBody.appendChild(note);
    }
  } catch (e) {
    console.error(e);
    setError(`${entry.name}: ${e.message || e}`);
  }
};

els.previewClose.addEventListener("click", () => {
  els.preview.style.display = "none";
});

// ── boot ─────────────────────────────────────────────────────────

(async () => {
  try {
    setStatus("loading zim-wasm…");
    await init();

    setStatus("unlocking key…");
    await ensureKey();

    setStatus("opening vault…");
    const manifest = await fetchJson(`/api/v0/v/${vaultId}/manifest`);
    vault = WasmVault.open(JSON.stringify(manifest));

    await renderCurrent();
  } catch (e) {
    console.error(e);
    setError(e.message || String(e));
  }
})();
