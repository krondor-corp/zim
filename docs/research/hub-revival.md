# Hub revival plan (DID-first)

Status: proposed, awaiting approval before execution.

## What the hub is, in one paragraph

A rendezvous server that (1) **hosts the user's did:web identity document** so all of their devices and browsers resolve to a stable, name-bearing identity; (2) **escrows passphrase-wrapped private key material** for browser-resident verification methods so any browser can restore them with a passphrase; and (3) **mirrors vault ciphertext + log** so any verification method authorized on a vault can browse it from any browser via zim-wasm. The hub never holds a vault secret, never sees plaintext, never sees a passphrase. The passphrase is the access control on the escrow; the DID document signature is the access control on identity changes.

## Headline UX wins from going DID-first

- Granting a user access to a vault is **one command, once**: `zim vault demo shares add did:web:hub.example.com:u:alice`. After that, any new device or browser that alice adds to her DID doc inherits access — no per-vault re-share.
- Web keys carry **logical names** (`did:web:hub.example.com:u:alice#browser-laptop`) instead of opaque hex. `dialable: bool` goes away — the verification method's declared purpose tells us.
- Key rotation works: rotate the browser key, the DID doc updates, the share survives.

## Identity model

### Three DID methods, three roles

| Role | DID method | Resolution | Mutable? |
|---|---|---|---|
| Daemon | `did:key:<multibase ed25519>` | zero network — the pubkey is encoded in the DID string | No (key rotation = new DID) |
| Hub | `did:web:hub.example.com` | one HTTPS GET to `/.well-known/did.json` | Yes (signed by hub operator) |
| User | `did:web:hub.example.com:u:alice` | one HTTPS GET to `/u/alice/.well-known/did.json` | Yes (signed by a controller) |

A user DID document lists multiple verification methods — one per device or browser:

```json
{
  "id": "did:web:hub.example.com:u:alice",
  "controller": ["did:web:hub.example.com:u:alice"],
  "verificationMethod": [
    {
      "id": "did:web:hub.example.com:u:alice#daemon-home",
      "type": "Ed25519VerificationKey2020",
      "purpose": "peer",
      "publicKeyMultibase": "z6Mk..."
    },
    {
      "id": "did:web:hub.example.com:u:alice#browser-laptop",
      "type": "Ed25519VerificationKey2020",
      "purpose": "web",
      "publicKeyMultibase": "z6Mk..."
    }
  ]
}
```

`purpose` is custom (not a standard DID-core field): `peer` means dialable as an iroh peer; `web` means browser-resident, never dialed. The sync dial loop skips `web` verification methods. This is what replaces `dialable: bool`.

### Vault model

`Share` and `Relay` carry an `Identity` enum, not a raw `PublicKey`:

```rust
pub enum Identity {
    /// Self-describing — the DID encodes the pubkey directly.
    Key(PublicKey),
    /// Resolved via HTTPS at use time. Cached fingerprint detects
    /// silent rotation that bypasses the share's invariant.
    Web { did: String, cached_fingerprint: Hash },
}

pub struct Share {
    identity: Identity,
    secret_share: SecretShare,
}
```

Share `dialable: bool` goes away. The dial loop resolves `Identity` and asks "is there a verification method I can dial?" — `did:key` always yes, `did:web` only if the doc declares a `peer`-purpose method.

### Vault save loop with DID expansion

When a vault is saved with a `Share(Web{did, ...})` entry, the saving peer:

1. Resolves the DID (HTTP fetch, signature verify, cache by fingerprint).
2. For each verification method in the doc, mints a `SecretShare` encrypted to that method's pubkey.
3. The manifest's share list expands: one DID share entry becomes N per-key entries internally, but presents as one logical share to the user.

Adding a browser to alice's DID doc → next vault save automatically includes a fresh `SecretShare` for the new browser. No per-vault re-share command.

Open question to settle: where do the per-key SecretShares live? Two options:

- **(a) Flatten at save**: manifest's share list contains per-key entries. Compact, fast to verify (no resolution needed to open a vault). Cost: every vault save resolves every DID.
- **(b) Layer of indirection**: share entry references the DID; per-key SecretShares live in a sidecar that the DID resolution returns. The vault open path resolves the DID, finds the matching verification-method id, fetches its SecretShare from the sidecar. Cost: opening a vault needs DID resolution.

Recommend **(a)** for first cut — simpler open path, save cost is bounded by share count.

### did:web authentication

Updating a user DID doc requires authentication. Options:

- **Self-signed updates**: the doc lists `controller` DIDs; updates must be signed by a controller's key. The first version is self-signed and self-controller. Subsequent updates are signed by the current controller list. Hub validates signature, accepts.
- **OAuth (Phase 5)**: hub gates writes through Google. Simpler infra but pushes auth into the critical path.

Recommend self-signed for the architecture; OAuth as an orthogonal Phase 5 wrap around the signed-update path (OAuth lets you discover who alice is; signature lets the hub verify the bytes came from her key).

## What the hub stores

`$ZIM_HUB_HOME/`:

```
identity.key                    -- hub's own ed25519 secret (powers did:web:hub.example.com)
config.toml
state/
  hub.db                        -- escrow table + DID doc table + audit log
  vault.db                      -- mirrored vault log
blobs/                          -- mirrored ciphertext
```

Two new SQLite tables (`hub.db`):

```sql
CREATE TABLE did_documents (
  did            TEXT PRIMARY KEY,    -- e.g. "did:web:hub.example.com:u:alice"
  document_json  TEXT NOT NULL,
  signature      BLOB NOT NULL,        -- detached signature by a current controller
  fingerprint    BLOB NOT NULL,        -- hash for cache invalidation
  updated_at     INTEGER NOT NULL
);

CREATE TABLE escrowed_keys (
  did_fragment    TEXT PRIMARY KEY,    -- e.g. "did:web:...:u:alice#browser-laptop"
  salt            BLOB NOT NULL,
  wrapped_secret  BLOB NOT NULL,       -- ChaCha20-Poly1305(kdf(passphrase, salt), ed25519_sk)
  kdf             TEXT NOT NULL,
  created_at      INTEGER NOT NULL
);
```

Escrow rows are keyed by the **fragmented DID** — the verification method id (`did:...#browser-laptop`). One escrow row per browser key.

## Phase plan

### Phase 0 — DID infrastructure (a precondition for everything else)

This phase lives partly outside zim-hub — it touches `zim-core`, `zim-vault`, the CLI, and adds a new `zim-did` crate. Hub revival blocks on it.

- New crate `zim-did/`:
  - `Did` type, `Identity` enum, `DidResolver` trait.
  - `did:key` impl — multibase decode/encode, zero-network.
  - `did:web` impl — `reqwest::get` + JSON validation + signature verify against a known controller set.
  - DID doc canonicalization for signing (JCS or similar).
- `zim-core/src/fs/share.rs`:
  - `Share`: replace `(identity: PublicKey, dialable: bool)` with `identity: Identity`.
  - `Relay`: replace `identity: PublicKey` with `identity: Identity`.
- `zim-vault/src/vault/vault.rs::save`:
  - Walk shares. For each `Identity::Web(did)`, resolve and expand to per-key `SecretShare` entries (option (a) above).
  - Caching: keep a resolver cache keyed by DID fingerprint; only re-resolve when the cached fingerprint diverges from the latest fetch.
- `zim-vault/src/vault/vault.rs::open`:
  - Find a share whose verification method matches my key. For `Identity::Key(my_pubkey)` that's a direct match; no resolution needed at open time.
- `zim` CLI:
  - `zim identity init` — generates the daemon key (as before) AND publishes a `did:web:<hub>:u:<name>` doc with the daemon as the sole controller and first verification method.
  - `zim identity add-method <web-key-pubkey> --label foo --purpose web` — adds a verification method to the user's DID doc, signs, publishes.
  - `zim peers add <nick> <did>` — DID, not hex.
  - `zim vault <id> shares add <did>` — DID, not hex.
- Migration: pre-launch, so the migration is just "we change the model"; no users to support.

Acceptance: full workspace builds, single-peer CLI tests pass with daemon now identified as a did:key, all `Share` entries are `Identity::Key`-shaped, no `dialable` field remains.

### Phase 1 — Hub crate revival + relay-mode ServiceState

- Crate move: `_zim-hub/` → `zim-hub/`, swap deps.
- `ServiceState::boot_relay(home)` — never generates a Share, opens vaults read-only-ciphertext-only.
- Relay extension: bit on `Relay` (`mirror_full: bool`) so the hub mirrors all ciphertext, not just published-set.
- DID infrastructure from Phase 0 is wired through — the hub identifies itself as `did:web:hub.example.com`.
- Hub hosts `/.well-known/did.json` for itself.
- Acceptance: `cargo build -p zim-hub` clean. Hub boots, prints its DID + pubkey, no vault data yet.

### Phase 2 — Ciphertext sync (relay mirroring)

- `zim peers add hub did:web:hub.example.com && zim vault demo relays add hub`.
- Verify the existing chain-walk sync path works for a relay (no decryption involved — chain validity is over hashes). Extend `zim-peer` if needed.
- Acceptance: edit a file on local peer, ciphertext blobs land in hub's `blobs/` within ~5s.

### Phase 3 — DID doc hosting + escrow service

This is the headline value-add.

Hub endpoints (signature-gated for writes):

- `GET /u/{user}/.well-known/did.json` — serves DID doc + signature header.
- `PUT /u/{user}/.well-known/did.json` — body is `(document_json, signature)`. Hub verifies signature against the previous doc's controllers (or, for the first publish, self-signature). Stores.
- `GET /api/escrow/{did_fragment}` — returns `(salt, wrapped_secret, kdf)`. Public.
- `PUT /api/escrow/{did_fragment}` — uploads escrow. First-write-wins per fragment.

Browser side (zim-wasm + JS bootstrapper):

- Onboarding new device into existing user:
  1. Browser generates ed25519 keypair in WebCrypto.
  2. Passphrase prompt → PBKDF2/argon2id → wrap key → ciphertext over private key.
  3. PUT escrow to hub.
  4. Display copy-paste line: `zim identity add-method <pubkey> --label laptop --purpose web`.
  5. User runs it on their daemon. Daemon updates DID doc, signs, publishes. Daemon also re-saves any vaults whose share lists include this user's DID (so the new browser's verification method gets a SecretShare).
  6. Browser polls hub's DID doc for its own pubkey. Once it lands, browser fetches a vault and tries to decrypt — should succeed.

- Restoring on a fresh browser:
  1. User enters their DID (`did:web:hub.example.com:u:alice`) + the verification method fragment they're restoring (`#browser-laptop`) + passphrase.
  2. Browser GETs `/api/escrow/{did}#{fragment}`.
  3. KDF + unwrap → private key. Cache in IndexedDB.
  4. Browser is now alice's `#browser-laptop`; proceeds to vault listing.

Acceptance: end-to-end on a fresh browser. New browser key → escrow upload → daemon adds it to alice's DID → daemon re-saves vaults → browser decrypts. Then fresh browser session → restore from passphrase → works.

### Phase 4 — Browse + history

- Hub endpoints: `GET /api/v/{vault_id}/{head,manifest/{link},blob/{hash},log,history}`.
- HTML shells under `/v/{vault_id}/{tree,raw,history}/*` load zim-wasm + bootstrapper. All decryption client-side.
- `Vault<L>::history(from, limit)` added to `zim-vault` — no decryption, log walk only.
- Acceptance: tree view, file view, history with per-row diff counts.

### Phase 5 — Deferred

- OAuth gate on `PUT` endpoints (currently signature-only; OAuth adds account linking and rate-limit surface).
- SSE for live updates.
- Public published-set surface (no DID needed).
- `did:plc`/`did:ion` if we ever want server-portable user DIDs.
- Social recovery of escrow passphrases.

## File-by-file fate map (zim-hub)

```
_zim-hub/
├── Cargo.toml             EDIT
├── README.md              REWRITE
├── Makefile               KEEP
├── build.rs               KEEP
├── confit.toml            DELETE
├── migrations/            REPLACE — new schema (did_documents, escrowed_keys)
├── src/
│   ├── main.rs            REWRITE — boot_relay
│   ├── lib.rs             EDIT
│   ├── config.rs          EDIT
│   ├── state.rs           REWRITE — AppState wraps relay ServiceState + hub stores
│   ├── errors.rs          KEEP
│   ├── identity.rs        DELETE — superseded by did:web hosting
│   ├── peer_client.rs     DELETE
│   ├── sri.rs             KEEP
│   ├── escrow.rs          NEW
│   ├── did_host.rs        NEW — did:web doc storage + signature verify
│   └── http/
│       ├── mod.rs                       EDIT
│       ├── sse/                         KEEP (stub)
│       ├── health/                      KEEP
│       ├── auth/                        DELETE — Phase 5 re-introduces
│       ├── api/                         REWRITE — escrow + ciphertext + DID endpoints
│       └── html/
│           ├── mod.rs                   EDIT
│           ├── index.rs                 EDIT — vault list
│           ├── static_files.rs          KEEP
│           ├── login.rs                 DELETE
│           ├── onboard.rs               NEW
│           ├── restore.rs               NEW
│           └── bucket/ → vault/         RENAME + shells
├── templates/             KEEP shells, sweep bucket→vault
└── static/                KEEP, especially vendor/zim-wasm/
```

## Open questions

1. **Per-key SecretShare layout**: flatten at save (option a, recommended) vs sidecar (b).
2. **Vault re-save on DID change**: eager (daemon subscribes/polls), lazy (`zim identity sync-vaults`), or manual? Recommend lazy with an explicit command for the first cut; revisit when usage shows the pain.
3. **DID doc canonicalization for signing**: JCS (RFC 8785) vs CBOR — recommend JCS, easier to reason about and what most did:web ecosystems use.
4. **KDF**: argon2id (recommended) vs PBKDF2. Compatible with zim-wasm size budget?
5. **Browser pubkey-as-username UX**: the user only needs to remember their DID URL (`did:web:hub.example.com:u:alice`), not a fragment, on restore. Picking which fragment to restore can be derived from a label or just "the one for this device" if the hub remembers. Recommend: restore form takes DID + passphrase + optional device label.
6. **First DID publish bootstrap**: how does the very-first daemon publish its DID to a hub it doesn't yet have an account on? Probably an unauthenticated "claim a name" endpoint that first-write-wins on `/u/{name}`. Open to squatting; Phase 5 OAuth fixes it.

## Risks

- **Phase 0 is wide**. DIDs touch every Share, Relay, Peer, and the save loop. Tests need to be re-orgd. This is the actual cost of going DID-first.
- **did:web depends on the hub being online** at save time. Offline saves of vaults whose share list contains did:web identities will either fail or rely on the resolver cache. Cache invalidation needs care.
- **DID doc updates and vault re-saves can race**. If alice adds browser-laptop and then immediately writes to a vault, the write might not yet include laptop's SecretShare. UX: the `add-method` command synchronously re-saves all vaults before returning success.
- **Signature surface is new code**. Canonicalization bugs cause silent verification failures. Property tests over canonicalize→sign→verify roundtrips before relying on it.

## Out of scope

- Multi-tenant auth beyond signed updates
- Write APIs on hub
- did:plc / portable user DIDs
- Mobile UI polish

## Acceptance for the milestone

End-to-end demo:

1. `zim identity init --hub hub.example.com --name alice` — daemon publishes alice's first DID doc, with the daemon's key as the sole controller + first verification method (`#daemon-home`, purpose=peer).
2. `zim daemon service install && start`.
3. `zim vaults create demo && zim vault demo add hello.md`.
4. `zim-hub` — hub up. Hub serves alice's DID doc back at `/u/alice/.well-known/did.json`.
5. `zim vault demo relays add did:web:hub.example.com` — hub mirroring starts. Ciphertext lands in hub.
6. Visit `localhost:17190`. Page detects no IndexedDB identity, offers "Set up browser" / "Restore".
7. Pick "Set up", choose passphrase, generate key, upload escrow. Page shows the `zim identity add-method <pubkey> --label laptop --purpose web` line.
8. Run it on local daemon. Daemon updates alice's DID doc on the hub and re-saves the demo vault to include a SecretShare for `#browser-laptop`.
9. Browser polls, detects its own pubkey in alice's DID doc, decrypts the demo vault. `hello.md` renders.
10. Edit another file on local peer. Hub mirrors it within seconds. Browser refresh shows both.
11. `/history` shows two rows with the right diff counts.
12. Fresh browser profile. Visit `localhost:17190/restore`. Enter DID + passphrase. Restore succeeds, all vaults still accessible.
