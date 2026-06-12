# zim-wasm

Browser-side WASM client for Zim. Loaded by `zim-hub`'s server-rendered
Datastar pages to decrypt published encrypted content client-side — `zim-hub`
serves ciphertext only and never holds bucket secrets in plaintext.

Also acts as the **identity vault crypto** for the hub: viewer Ed25519 keys
are generated, encrypted, and unlocked entirely in the browser via Argon2id
+ ChaCha20-Poly1305. The hub stores only ciphertext + Argon2 salt; passwords
and plaintext keys never leave the viewer's tab. See T-001 for the design.

## Roles

1. **Client-side decryption** of published encrypted blobs (T-012 — sealed
   member-viewer path + public anonymous path).
2. **Identity vault crypto** — keypair generation, password-derived key
   wrapping, and unlock (T-001b).

Reserved seams (not implemented):
- Future Milkdown-style non-collaborative editor surface — separate task once
  zim-hub grows an editor route.
- IndexedDB offline cache for the published-set — separate task once cache
  requirements are concrete.

## JS interface

Exported via `#[wasm_bindgen]` from `src/lib.rs`.

### Session & decryption (T-012)

```ts
init(): void
loadKeyFromSession(keyBytes: Uint8Array): void
decryptBlob(envelopeJson: string, ciphertext: Uint8Array): Uint8Array
clearKey(): void
```

- `init` sets the panic hook. Call once on module load.
- `loadKeyFromSession` accepts 32 bytes (raw Ed25519 secret key). Stored in
  WASM linear memory; never returned to JS.
- `decryptBlob` parses an envelope JSON (tagged union — see schema below),
  recovers the per-blob [`Secret`](../zim-crypto/src/secret.rs), and
  ChaCha20-Poly1305-decrypts the ciphertext.
- `clearKey` zeroes the in-memory key. Call on logout or tab close.

### Identity vault (T-001b)

```ts
generateKey(): Uint8Array
encryptKeyBlob(password: string): KeyBlob
unlockKeyBlob(blob: Uint8Array, salt: Uint8Array, password: string): void

interface KeyBlob {
  readonly encryptedBlob: Uint8Array;
  readonly salt: Uint8Array;
  readonly publicKey: Uint8Array;
}
```

- `generateKey` — fresh Ed25519 keypair. Secret stored in the session
  thread-local; returns the 32-byte public key for hub-side enrolment.
- `encryptKeyBlob(password)` — wraps the currently-loaded session key with a
  password-derived KEK. Argon2id (`m=19456 KiB, t=2, p=1`; OWASP 2024) →
  32-byte KEK → ChaCha20-Poly1305 wrap (reuses `zim_crypto::Secret`'s wire
  format). Returns `(encryptedBlob, salt, publicKey)` for the hub to persist.
- `unlockKeyBlob(blob, salt, password)` — Argon2id-derive KEK, decrypt the
  blob into the session thread-local. On wrong password / corrupt blob,
  throws a `JsError` (`unlock failed: wrong password or corrupt blob`).

End-to-end flow:
- **Enrol**: `init()` → `generateKey()` → POST pubkey to hub → prompt for new
  password → `encryptKeyBlob(pw)` → POST `(encryptedBlob, salt, publicKey)`
  to hub. Hub stores the row keyed by Google `sub`.
- **Login**: `init()` → fetch `(encryptedBlob, salt)` from hub → prompt for
  password → `unlockKeyBlob(blob, salt, pw)`. `decryptBlob` now works for
  sealed envelopes.
- **Logout**: `clearKey()` + POST hub `/api/v0/identity/logout`.
- **Password change** (must be unlocked): `encryptKeyBlob(newPw)` → POST hub.
- **Key rotation** (must be unlocked): `generateKey()` (overwrites session
  thread-local) → owner deauthorises old pubkey, authorises new →
  `encryptKeyBlob(pw)` → POST hub.

## Build

```
wasm-pack build crates/zim-wasm \
  --target web \
  --out-dir ../zim-hub/static/vendor/zim-wasm \
  --out-name zim_wasm \
  --release \
&& printf 'package.json\n' > crates/zim-hub/static/vendor/zim-wasm/.gitignore
```

The trailing `printf` step is **load-bearing**: `wasm-pack` overwrites
`.gitignore` with a single `*` on every build, which would un-commit the
shipped bundle. The corrected `package.json`-only contents are the policy
agreed with thing3 (`crates/zim-hub/static/vendor/README.md`). When `bin/wasm`
lands (per thing3's T-002 M5+ note), wrap the same two steps there.

Outputs:
```
crates/zim-hub/static/vendor/zim-wasm/
├── zim_wasm.js          # ES module glue from wasm-bindgen (~17 KB)
├── zim_wasm_bg.wasm     # binary (~289 KB after Argon2id; was ~256 KB pre-T-001b)
├── zim_wasm.d.ts        # TS bindings (declarative; no tsc step)
└── zim_wasm_bg.wasm.d.ts
```

Bundle size delta after T-001b (added Argon2id): `.wasm` +32 KB
(256 KB → 289 KB, +13%), `.js` +8 KB (9 KB → 17 KB, +91% from extra
wasm-bindgen glue for `KeyBlob` getter struct + 3 new exports). Total
~42 KB additional; acceptable per T-001 acceptance (no `wasm-opt` /
code-splitting in v1).

## Script-tag wiring

zim-hub does **not** import zim-wasm server-side and does **not** load the
bundle globally. The module is loaded only on routes that need decryption
or identity flows, via a per-template `{% block extra_scripts %}` slot in
`crates/zim-hub/templates/layouts/base.html`:

```html
<script type="module">
  import init, {
    generateKey, encryptKeyBlob, unlockKeyBlob,
    loadKeyFromSession, decryptBlob, clearKey,
  } from '/static/vendor/zim-wasm/zim_wasm.js';
  await init();
  // wire Datastar actions on /enrol, /unlock, blob viewer pages, etc.
</script>
```

`init()` is idempotent — safe to call from every page that includes the
bundle.

## Dependency on zim-crypto

zim-wasm pulls `zim-crypto` with `default-features = false, features =
["wasm"]`. The `wasm` feature replaces the iroh-wrapped key types with raw
`ed25519_dalek`/`x25519_dalek` types so the crate compiles to
`wasm32-unknown-unknown`. `Secret` and `SecretShare` are feature-agnostic.

`zim_crypto::Secret`'s ChaCha20-Poly1305 wire format
(`nonce || ciphertext || tag`) is reused for the identity-vault blob — no
second AEAD impl, no second nonce-management surface.

## Envelope JSON shape

Tagged union on `kind`. Two variants from v1 (per thing2's T-008 coordination
note, 20260524T024508Z):

```json
{ "kind": "public", "secret": "<Secret-hex>" }
{ "kind": "sealed", "share":  "<SecretShare-hex>" }
```

- **`public`** — T-008 anonymous-viewer path. Per-blob `Secret` is in the
  envelope as hex. No session key required; `loadKeyFromSession` may be
  skipped.
- **`sealed`** — T-001 member-viewer path. Per-blob `Secret` is sealed inside
  a `SecretShare` for the viewer's `PublicKey`. Requires either
  `loadKeyFromSession` (raw key bytes) or `unlockKeyBlob` (password unlock)
  to have populated the session thread-local.

Both variants pass the resulting `Secret` to ChaCha20-Poly1305 decryption; the
12-byte nonce is embedded at the head of the ciphertext bytes (see
`zim_crypto::secret::Secret::encrypt` for the wire format), so the envelope
does **not** carry a separate nonce field.

T-008 / T-001 may extend additively (e.g. content-type, suggested filename
metadata) without breaking the v1 dispatch.

## Out of scope for v1

- IndexedDB caching, Service Worker, PWA features.
- WASM size optimisation (`wasm-opt`, code-splitting).
- Editor surface implementation.
- Streaming decryption (current `Secret::decrypt` is whole-blob; chunked
  decoding can be added when blob sizes warrant it).
- Password recovery (intentional — see T-001 Decision 7).
- WebAuthn / FIDO2 unlock (future direction).
- Multi-IdP support (Google only for v1).
