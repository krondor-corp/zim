// Browser-as-device setup.
//
// Generates an ed25519 keypair locally, derives an AES-GCM-256 wrap
// key from a passphrase via PBKDF2-SHA256, encrypts the private-key
// JWK with that wrap key, and PUTs the wrapped blob to the hub's
// escrow store. The pubkey is then registered via the same form
// `/app/devices/add` takes.
//
// The unwrapped private key stays in this browser (held in
// IndexedDB so subsequent visits can sign without re-prompting for
// the passphrase). Recovery from another browser requires only the
// same passphrase + a fetch from `/api/v0/escrow`.
//
// Required browser support: WebCrypto Ed25519 (Chrome 113+,
// Safari 17+, Firefox 130+). If unavailable, the form refuses and
// surfaces a clear error.

(() => {
  'use strict';

  const PBKDF2_ITERS = 100_000;
  const SALT_LEN = 16;
  const IV_LEN = 12;
  const KDF_LABEL = `pbkdf2-sha256/${PBKDF2_ITERS}+aes-gcm-256`;

  const form = document.getElementById('browser-device-form');
  if (!form) return;

  const els = {
    label: document.getElementById('field-label'),
    passphrase: document.getElementById('field-passphrase'),
    confirm: document.getElementById('field-confirm'),
    submit: document.getElementById('submit-btn'),
    status: document.getElementById('status'),
    error: document.getElementById('error'),
    hubHost: form.dataset.hubHost,
    userId: form.dataset.userId,
  };

  function setStatus(msg) {
    els.status.textContent = msg;
    els.status.style.display = msg ? '' : 'none';
    els.error.style.display = 'none';
  }
  function setError(msg) {
    els.error.textContent = msg;
    els.error.style.display = '';
    els.status.style.display = 'none';
    els.submit.disabled = false;
  }

  function bytesToB64(bytes) {
    let s = '';
    for (const b of bytes) s += String.fromCharCode(b);
    return btoa(s);
  }
  function hex(bytes) {
    return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  }
  function randomFragmentSuffix() {
    const buf = crypto.getRandomValues(new Uint8Array(6));
    return hex(buf);
  }

  // Check capability up front so the user doesn't waste passphrase
  // entry on an unsupported browser.
  async function checkEd25519Support() {
    try {
      await crypto.subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify']);
      return true;
    } catch (e) {
      return false;
    }
  }

  async function deriveWrapKey(passphrase, salt) {
    const enc = new TextEncoder();
    const base = await crypto.subtle.importKey(
      'raw', enc.encode(passphrase),
      { name: 'PBKDF2' }, false, ['deriveKey']
    );
    return crypto.subtle.deriveKey(
      { name: 'PBKDF2', salt, iterations: PBKDF2_ITERS, hash: 'SHA-256' },
      base,
      { name: 'AES-GCM', length: 256 },
      false,
      ['encrypt', 'decrypt']
    );
  }

  async function exportPubkeyHex(publicKey) {
    // raw = 32 bytes of the ed25519 public scalar.
    const raw = new Uint8Array(await crypto.subtle.exportKey('raw', publicKey));
    return hex(raw);
  }

  async function wrapPrivateKey(privateKey, wrapKey) {
    // Export as JWK so we can re-import on recovery without needing
    // PKCS8 ASN.1 parsing. The JWK is fully self-describing.
    const jwk = await crypto.subtle.exportKey('jwk', privateKey);
    const plaintext = new TextEncoder().encode(JSON.stringify(jwk));
    const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
    const ct = new Uint8Array(
      await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, wrapKey, plaintext)
    );
    // Pack iv || ct so the recovery side can split them.
    const out = new Uint8Array(iv.length + ct.length);
    out.set(iv, 0);
    out.set(ct, iv.length);
    return out;
  }

  async function storeIdb(didFragment, jwk) {
    const db = await new Promise((resolve, reject) => {
      const req = indexedDB.open('zim-devices', 1);
      req.onupgradeneeded = () => req.result.createObjectStore('keys', { keyPath: 'did' });
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    await new Promise((resolve, reject) => {
      const tx = db.transaction('keys', 'readwrite');
      tx.objectStore('keys').put({ did: didFragment, jwk });
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error);
    });
    db.close();
  }

  async function putEscrow(didFragment, salt, wrappedSecret) {
    const body = {
      did: didFragment,
      salt: bytesToB64(salt),
      kdf: KDF_LABEL,
      wrapped_secret: bytesToB64(wrappedSecret),
      created_at: '',
    };
    const res = await fetch('/api/v0/escrow', {
      method: 'PUT',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`escrow PUT failed: ${res.status} ${text}`);
    }
  }

  async function getEnrollChallenge() {
    const res = await fetch('/api/v0/devices/enroll-challenge', {
      method: 'GET',
      credentials: 'same-origin',
      headers: { Accept: 'application/json' },
    });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`challenge request failed: ${res.status} ${text}`);
    }
    return res.json();
  }

  /// Sign `challenge_bytes || pubkey_bytes` with the freshly minted
  /// ed25519 private key. Both halves are raw bytes (not hex) — the
  /// hex is just transport encoding. Returns hex(signature).
  async function signEnrollPayload(privateKey, challengeHex, pubkeyHex) {
    function hexToBytes(s) {
      const out = new Uint8Array(s.length / 2);
      for (let i = 0; i < out.length; i++) {
        out[i] = parseInt(s.substr(i * 2, 2), 16);
      }
      return out;
    }
    const challenge = hexToBytes(challengeHex);
    const pubkey = hexToBytes(pubkeyHex);
    const payload = new Uint8Array(challenge.length + pubkey.length);
    payload.set(challenge, 0);
    payload.set(pubkey, challenge.length);
    const sig = new Uint8Array(
      await crypto.subtle.sign({ name: 'Ed25519' }, privateKey, payload)
    );
    return hex(sig);
  }

  async function registerPubkey(pubkeyHex, label, privateKey) {
    setStatus('Requesting enrollment challenge…');
    const { challenge } = await getEnrollChallenge();

    setStatus('Signing possession proof…');
    const signature = await signEnrollPayload(privateKey, challenge, pubkeyHex);

    setStatus('Posting enrollment…');
    const res = await fetch('/api/v0/devices/self', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        pubkey: pubkeyHex,
        label,
        kind: 'web',
        challenge,
        signature,
      }),
    });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`enroll failed: ${res.status} ${text}`);
    }
  }

  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    els.error.style.display = 'none';

    const label = (els.label.value || 'browser').trim();
    const passphrase = els.passphrase.value;
    const confirmPp = els.confirm.value;
    if (passphrase.length < 8) {
      setError('Passphrase must be at least 8 characters.');
      return;
    }
    if (passphrase !== confirmPp) {
      setError('Passphrases do not match.');
      return;
    }

    els.submit.disabled = true;
    try {
      setStatus('Checking browser support…');
      if (!(await checkEd25519Support())) {
        throw new Error('This browser lacks WebCrypto Ed25519 support. Use Chrome 113+, Safari 17+, or Firefox 130+.');
      }

      setStatus('Generating ed25519 keypair…');
      const kp = await crypto.subtle.generateKey(
        { name: 'Ed25519' }, true, ['sign', 'verify']
      );
      const pubkeyHex = await exportPubkeyHex(kp.publicKey);
      const jwk = await crypto.subtle.exportKey('jwk', kp.privateKey);

      setStatus('Deriving wrap key from passphrase…');
      const salt = crypto.getRandomValues(new Uint8Array(SALT_LEN));
      const wrapKey = await deriveWrapKey(passphrase, salt);

      setStatus('Wrapping private key…');
      const wrapped = await wrapPrivateKey(kp.privateKey, wrapKey);

      const didFragment = `did:web:${els.hubHost}:u:${els.userId}#browser-${randomFragmentSuffix()}`;

      setStatus('Pushing wrapped key to escrow…');
      await putEscrow(didFragment, salt, wrapped);

      setStatus('Storing local copy in IndexedDB…');
      await storeIdb(didFragment, jwk);

      setStatus('Registering pubkey on the hub…');
      const fullLabel = `browser: ${label}`;
      await registerPubkey(pubkeyHex, fullLabel, kp.privateKey);

      setStatus('Done. Redirecting…');
      window.location.href = '/app';
    } catch (e) {
      console.error(e);
      setError(e.message || String(e));
    }
  });
})();
