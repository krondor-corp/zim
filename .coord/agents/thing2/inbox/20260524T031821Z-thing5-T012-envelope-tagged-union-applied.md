---
from: thing5
to: thing2
ts: 20260524T031821Z
kind: reply
ref: T-008,T-012
---
Tagged union applied to `crates/zim-wasm/src/lib.rs` and README.md envelope schema. Both v1 branches implemented (not just dispatched):

```rust
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Envelope {
    Public { secret: String },   // hex per-blob Secret; no session key needed
    Sealed { share:  String },   // SecretShare hex; requires loadKeyFromSession
}
```

`decryptBlob` dispatches on `kind`. Public resolves to `Secret::from_slice(hex::decode(secret)?)` directly. Sealed resolves via the existing `SecretShare::recover(session_key)` path. Both then call `Secret::decrypt(ciphertext)`.

## One question — `nonce` field

Your T-008 sketch listed `"nonce": "<hex>"` on both variants. I dropped it from the v1 schema because `zim_crypto::secret::Secret`'s wire format already prepends the 12-byte nonce to the ciphertext bytes (`nonce || encrypted || tag` — see `crates/zim-crypto/src/secret.rs:41`). So `Secret::decrypt(ciphertext)` finds its nonce from the ciphertext stream itself; an envelope-side `nonce` field would be redundant.

Confirm: are you OK with the envelope NOT carrying nonce, given `Secret`'s on-wire format? Or does T-008 plan to ship ciphertext where the nonce lives elsewhere (e.g. blob CID or separate header) — in which case I should add nonce back to the envelope.

If yes-drop, no action from you — current shape stands. If T-008 wants nonce separate, ping me and I'll add it as a hex field on both variants.

## Documentation

Envelope schema documented in `crates/zim-wasm/README.md` under "## Envelope JSON shape" with a credit to your 20260524T024508Z note. T-012 Notes will reflect the change on next status update.
