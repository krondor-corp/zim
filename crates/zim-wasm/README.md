# zim-wasm

Browser-side WASM client for Zim. Loaded by `zim-hub`'s server-rendered
Datastar pages to decrypt published encrypted content client-side — `zim-hub`
serves ciphertext only and never holds bucket secrets in plaintext.

## Role (v1)

Sole responsibility: **client-side decryption of published encrypted blobs.**

Reserved seams (not implemented):
- Future Milkdown-style non-collaborative editor surface — separate task once
  zim-hub grows an editor route.
- IndexedDB offline cache for the published-set — separate task once cache
  requirements are concrete.

## JS interface

Exported via `#[wasm_bindgen]` from `src/lib.rs`:

```ts
init(): void
loadKeyFromSession(keyBytes: Uint8Array): void
decryptBlob(envelopeJson: string, ciphertext: Uint8Array): Uint8Array
clearKey(): void
```

- `init` sets the panic hook. Call once on module load.
- `loadKeyFromSession` accepts 32 bytes (raw Ed25519 secret key). The key is
  stored in WASM linear memory and is never returned to JS.
- `decryptBlob` parses an envelope JSON (currently `{ "share": "<hex>" }`),
  recovers the per-blob [`Secret`](../zim-crypto/src/secret.rs) using the
  session key, and ChaCha20-Poly1305-decrypts the ciphertext. Throws on
  missing key, malformed envelope, share-recover failure, or AEAD tag mismatch.
- `clearKey` zeroes the in-memory key. Call on logout or tab close.

## Build

```
wasm-pack build crates/zim-wasm \
  --target web \
  --out-dir ../zim-hub/static/vendor/zim-wasm \
  --out-name zim_wasm \
  --release
```

Outputs:
```
crates/zim-hub/static/vendor/zim-wasm/
├── zim_wasm.js          # ES module glue from wasm-bindgen
└── zim_wasm_bg.wasm     # binary
```

`wasm-pack` also writes a `package.json` into the out-dir; gitignore it (we
have no npm toolchain). No `wasm-opt` / code-splitting in v1.

## Script-tag wiring

zim-hub does **not** import zim-wasm server-side and does **not** load the
bundle globally. The module is loaded only on routes that need decryption
(e.g. `/b/{id}/blob/*`, `/b/{id}/raw/*`), via a per-template `{% block
extra_scripts %}` slot in `crates/zim-hub/templates/layouts/base.html`:

```html
<script type="module">
  import init, { loadKeyFromSession, decryptBlob, clearKey }
    from '/static/vendor/zim-wasm/zim_wasm.js';
  await init();
  // wire loadKeyFromSession on session start; decryptBlob on raw-blob fetch
</script>
```

## Dependency on zim-crypto

zim-wasm pulls `zim-crypto` with `default-features = false, features =
["wasm"]`. The `wasm` feature replaces the iroh-wrapped key types with raw
`ed25519_dalek`/`x25519_dalek` types so the crate compiles to
`wasm32-unknown-unknown`. `Secret` and `SecretShare` are feature-agnostic.

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
  a `SecretShare` for the viewer's `PublicKey`. Requires `loadKeyFromSession`
  to have been called with the viewer's 32-byte Ed25519 secret.

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
