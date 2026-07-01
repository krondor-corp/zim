# Identity and key management

Zim's identity model follows a **vault-not-custodian** pattern: the hub escrows the user's web key as ciphertext but never holds the plaintext. Unlock happens client-side in the browser via the WASM SDK.

## DID forms (identities)

Every peer in zim is named by a **DID**. The in-memory type is `zim_did::Identity`; it is exactly a DID in typed form, round-tripping losslessly via `Identity::to_did()` / `Identity::parse()`. There are two methods, and the method tells you **how to reach the peer**:

### `did:key` — direct (self-hosted)

```
did:key:z6Mk…            // base58btc multicodec-prefixed ed25519 pubkey
```

The pubkey *is* the identifier — no resolution, no network. `Identity::Key(pk)`. Always dialable directly as an iroh peer (the iroh `NodeId` is the ed25519 key). This is what daemons use.

### `did:web` — hosted

```
did:web:<host>[:<path-segment>…][#<vm-fragment>]
```

A **hosted** identity: the `<host>` is the always-on peer that hosts it. `Identity::Web { did }`. Resolution is an HTTPS GET of the host's DID document, per W3C did:web §3.1.2 (`crates/zim-did/src/resolver.rs::did_web_url`):

| DID | Resolves to | |
|---|---|---|
| `did:web:hub.example.com` | `https://hub.example.com/.well-known/did.json` | the hub's own peer identity |
| `did:web:hub.example.com:u:alice` | `https://hub.example.com/u/alice/did.json` | a user (lists every enrolled device) |
| `did:web:hub.example.com:u:alice:dev:browser` | `https://hub.example.com/u/alice/dev/browser/did.json` | **(proposed)** a single hosted device |

The first two are served today (`/.well-known/did.json`, `/u/<user_id>/did.json`); the per-device `…:dev:<id>` document is the target shape, not yet built.

- Path segments are colon-separated (`/` is forbidden — use `:`).
- A port is percent-encoded in the host: `did:web:127.0.0.1%3A8080` → `http://127.0.0.1:8080/.well-known/did.json` (dev uses `http` via `allow_http`; production is always `https`).
- A `#<vm-fragment>` selects one verification method *within* the resolved document; it's stripped when building the document URL.

The hub serves two of these today: its **own** peer identity at `/.well-known/did.json` (`http/well_known.rs`), and **per-user** identities at `/u/<user_id>/did.json` (`http/user_did.rs`), whose document lists one verification method per enrolled device.

### The hosted-DID protocol: "make the share for the client"

A hosted DID is the unifying concept behind relays and mirrors — there is no separate `Relay` type. A relay is simply **a share whose identity is a `did:web`**, and the host *is* the old `via`. Sharing always seals the `SecretShare` to the **client** key, never the host:

- **Seal target (the client key):** the verification method the DID resolves to — for `did:web:host:u:alice:dev:browser`, the browser's key in that document. The host never receives the vault secret, so a hosted DID preserves zero-knowledge: the host relays ciphertext only.
- **Dial target (how to reach them):** derived from the method.
  - `did:key:…` → dial the key directly.
  - `did:web:<host>:…` → dial the **host** — the ephemeral client (e.g. a browser) is never dialed.

**Resolution returns the full relay form.** A hosted DID carries both endpoints, so resolving it yields them together in one pass — the caller never stitches two lookups by hand:

```
resolve(did) -> Reach {
    client: PublicKey,            // seal target — the resolved verification method
    via: Option<(Identity, PublicKey)>,  // dial target — None for did:key;
                                         // Some(did:web:<host>, host_key) for hosted
}
```

For `did:web:host:u:alice:dev:browser` the one resolution returns `client = browser key` (from the device document) and `via = (did:web:host, host peer key)` (from the host's `/.well-known/did.json`). For `did:key:…`, `client` is the key itself and `via` is `None`.

So a single operation — *share to a DID* — generalizes across a daemon (`did:key`), an ephemeral browser (`did:web:hub:u:alice:dev:browser`), and a whole account (`did:web:hub:u:alice`, which resolves to N device clients, each sealed individually).

> This supersedes the old `Share::dialable` boolean described in the device-model section below: dialability is now derived from the DID method, not stored on the share.

## Architecture

### Web key — the escrowed master identity

A user's **web key** is the master identity for their account: one Ed25519 key pair, created in the browser by the WASM SDK and **escrowed** on the hub as ciphertext — the hub never sees the plaintext.

1. **Key derivation**: Argon2id (m_cost=19456 KiB, t_cost=2, p_cost=1) derives a 32-byte KEK from the user's passphrase + a random 16-byte salt.
2. **Encryption**: ChaCha20-Poly1305 (the same AEAD as content encryption) wraps the Ed25519 secret with the KEK; 12-byte nonce prepended per `zim_crypto::Secret` wire format.
3. **Escrow**: the wrapped blob + salt + KDF params + public key go to the hub's `escrowed_keys` table via `PUT /api/v0/escrow`, keyed by the authenticated user. Unlock is `GET /api/v0/escrow`; decryption happens client-side in WASM and the secret lives only in a `SESSION_KEY` thread-local.

The hub can be fully compromised without leaking the web key — the attacker gets ciphertext + Google identities but needs each user's passphrase to decrypt.

### Devices and the user's `did:web`

Every enrolled key — the web key plus any daemons — is one **verification method** in the user's `did:web:<host>:u:<id>` document (`http/user_did.rs`). Resolving that DID yields the full, current device set; sharing a vault to it seals a `SecretShare` to each device individually (see [the hosted-DID protocol](#the-hosted-did-protocol-make-the-share-for-the-client)).

- **Web key** — created via escrow (above).
- **Daemon** — an on-device key whose secret never leaves the machine; enrolled by possession proof: `zim login` runs a device-code grant, or `POST /api/v0/devices/self` verifies an Ed25519 signature over `challenge || pubkey`. The hub keeps only the public key + metadata (`user_peers`).

Dialability is **derived from the DID method**, never stored on the share: a `did:key` device is dialed directly; a `did:web` device is reached via its host.

### Hub peer identity

The hub embeds its own iroh peer with a persistent Ed25519 key (`identity.key` in its data dir, generated on first start). This operator-side network identity is unrelated to user keys and is published at `/.well-known/did.json` (`http/well_known.rs`), so `did:web:<hub-host>` resolves to it — and that is the `via` (dial target) every hosted device on this hub resolves to.

There is no `manifest.mirrors` list: the hub mirrors a vault exactly when one of its shareholders is a `did:web` whose host is this hub. The ciphertext it stores is never decryptable by the hub itself.

## Trust model

| Adversary | Mitigation |
|---|---|
| Compromised hub server | Cannot decrypt web keys without per-user passphrases. Cannot impersonate users in protocol (key never leaves the browser). **Critical residual**: attacker can swap the served JS/WASM bundle to phish passphrases. Mitigated by SRI hashes on script tags + CSP `script-src 'self'`. |
| Compromised Google account | Attacker can log into the hub as the victim but still needs the unlock passphrase (second factor). |
| Hub operator | Same as compromised server — cannot decrypt without passphrases; can phish via swapped JS. SRI + CSP mitigate. |
| Brute force on a stolen blob | Argon2id with 19 MB memory cost. Recommend ≥16-char passphrases. |

**The single load-bearing trust assumption**: the user trusts the JS+WASM bundle served by the hub. Mitigation: SRI hashes baked into HTML templates, CSP `script-src 'self'`, vendored/version-pinned JS bundles.

## Key flows

### Web-key enrolment (first time)

1. Google OAuth2 (PKCE) → hub extracts the Google identity and establishes a signed-cookie session.
2. The WASM SDK generates the Ed25519 web key in WASM memory.
3. The user picks a passphrase → the SDK wraps the secret (Argon2id + ChaCha20-Poly1305).
4. Browser `PUT /api/v0/escrow` with `{wrapped, salt, kdf params, public_key}`.
5. The web key becomes the first verification method in the user's `did:web:<host>:u:<id>` document.

### Login (returning)

1. Google OAuth (or an existing session).
2. Browser `GET /api/v0/escrow` → wrapped blob + salt + params.
3. The SDK Argon2id-derives the KEK → ChaCha20-decrypts → Ed25519 secret in the `SESSION_KEY` thread-local. Sealed envelopes targeted at this key now decrypt.

### Enrolling a daemon

`zim login --hub <hub>` runs a device-code grant: the daemon proves possession of its on-device key (signature over the challenge), the user approves in the browser, and the hub adds the key as a new verification method (`user_peers`). The daemon also records the hub in its peer book as a `did:web` entry, so it can later resolve and dial it.

### Sharing a vault

A single operation — *share to a DID* — covers every case (see [hosted-DID protocol](#the-hosted-did-protocol-make-the-share-for-the-client)):

- **Daemon**: `zim vault <id> shares add <did>`.
- **Browser**: creating a vault auto-shares to the account's `did:web` — every device — sealing a `SecretShare` to each resolved client key.

Conceptually each share resolves the DID to its `Reach { client, via }`: the secret is sealed to `client`, and `via` (the host, or `None` for `did:key`) is the dial/ping target for sync.

### Deauthorization

Remove the device (Settings → Devices, or `DELETE /api/v0/devices/:pubkey`) or drop a share from the manifest. **Known gap**: per-node secrets are not yet re-keyed, so cached ciphertext stays decryptable by a removed key until secret rotation ships.

## Relevant code

| Concern | Location |
|---|---|
| Key types | `crates/zim-crypto/src/keys/{private,public}.rs` |
| Secret encryption | `crates/zim-crypto/src/secret.rs` (ChaCha20-Poly1305) |
| Secret sharing (ECDH) | `crates/zim-crypto/src/secret_share.rs` |
| `Identity` / DID forms | `crates/zim-did/src/{identity.rs,did_key.rs}` |
| DID resolution (dial/seal targets) | `crates/zim-did/src/resolver.rs` |
| Share + manifest (`Share` carries `identity`) | `crates/zim-core/src/fs/manifest.rs` |
| Hub DID documents | `crates/zim-hub/src/http/{well_known.rs,user_did.rs}` |
| Escrow store | `crates/zim-hub/src/http/api/v0/escrow.rs`, `migrations/*create_escrowed_keys*` |
| Device enrolment | `crates/zim-hub/src/http/api/v0/devices.rs` |
| Browser SDK (keys, sharing, fs) | `crates/zim-hub/wasm/src/{lib.rs,api.rs,fs.rs}` |
| Vault share CLI | `crates/zim/src/cli/ops/vault/shares/` |
