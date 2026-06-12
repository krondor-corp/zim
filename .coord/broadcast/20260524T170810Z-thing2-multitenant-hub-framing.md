---
from: thing2
ts: 20260524T170810Z
kind: framing-update
audience: all
ref: T-001,T-002,T-008,T-012,T-015,T-016
---

# Framing update: zim-hub is multi-tenant ("GitHub for buckets")

User direction relayed through thing2 (20260524T17xx): the multi-user nature of zim-hub was a missed framing in earlier task descriptions. Stating it now as binding so in-flight work realigns.

## The framing (binding)

**zim-hub hosts many users**, GitHub-style. Each user signs in via Google, has their own buckets, and can grant access to others. The hub is not a single-user gateway and not a CDN — it's a multi-tenant web application that puts a browser face on the per-user peer mesh.

This was implicit in some designs (T-001's `identity_keys` table keyed by `google_sub` already supports many users) but explicit-single-user in others (T-002 acceptance lists "Workspace model is explicitly single-user" and "Multitenancy / org / team / cross-user sharing" as out-of-scope). The single-user framing is **removed**. Multi-user is the v1 target.

## What stays the same

- **Per-file/folder publication (T-008's `PublicEntry`/`published_set`)** — still the right shape. Public anonymous URL reads remain a first-class hub feature. Per-file is strictly more flexible than per-bucket; this part of the T-006 fold-into-T-008 was correct.
- **Mirror as a peer-type (T-016's `manifest.mirrors: Vec<PublicKey>`)** — still the right shape. Hub-peer is added there per-bucket; it ciphertext-pins but doesn't decrypt. T-006's removal of `PrincipalRole::Mirror` from the principal/role-enum layer also remains correct.
- **Browser-side unlock (T-001 thing5's design)** — still the right shape. Hub stores encrypted private-key blobs keyed by `google_sub`; the user's browser does Argon2id-derived KEK unlock; plaintext keys never exist server-side. The "key vault" framing already supports multi-tenant via the SQLite `identity_keys` table.
- **Viewer pubkey as full member** of `manifest.shares` — still the right shape. Owners add viewers' web-keys to their buckets' share lists; decryption happens in the viewer's browser via the WASM client.

## What changes in active/queued work

### T-002 (claimed by thing3)
- Acceptance line "Workspace model is explicitly single-user" → **delete**. New line: "Workspace model is multi-tenant (one hub serves many Google-authenticated users)."
- Out-of-scope line "Multitenancy / org / team / cross-user sharing" → **delete** (multi-tenant is in-scope). Org/team-level groupings can stay out-of-scope as v2; the v1 unit is the individual Google-authenticated user.
- Concretely: hub schema namespaces buckets by user (`(google_sub, bucket_id)`), hub URL routes include the user (`/u/<handle>/<bucket>/...` or `/<bucket_id>/...` with internal lookup), and the sign-in flow gates access at the user boundary.

### T-008 (currently open)
- Proposal is unchanged in its protocol/data-model content (per-file/folder publication, `PublicEntry`, auto-republish on commit).
- The "Coordination — T-012 zim-wasm envelope JSON shape" subsection's framing ("hub is non-member") is **adjusted**: hub is non-member *for anonymous public reads via `PublicEntry`*, but it custodies many per-user web-keys that ARE full members (one membership entry per user-per-bucket, per T-001). The envelope tagged-union (`{kind: "public", ...}` vs `{kind: "sealed", ...}`) stays — anonymous URL reads use the public branch, signed-in user reads use the sealed branch via their custodied web-key unlocked browser-side.
- T-008 is otherwise ready to be reclaimed and spawned into T-008a/b/c.

### T-015 / T-016 (mirror peer-type)
- No change. Hub's iroh peer key is added per-bucket to `manifest.mirrors`. This is still right and the multi-tenant framing doesn't disturb it. The hub runs one iroh peer (its own operator-side key, separately from the custodied user web-keys); that single peer is registered as a Mirror against every bucket whose owner has chosen to mirror through this hub.

## Open engineering question (not blocking — flagged for whoever picks up sync work next)

**Peer-per-key vs multiplex.** The hub custodies N user web-keys, each a member of one or more buckets. Two implementation shapes:

- **Shape A — peer-per-key**: each custodied web-key runs as a full iroh peer in the hub. Protocol is unchanged. Hub is "many peers in one process." Scales as O(users × buckets-per-user) in peer state. Probably fine to ~1k active users; painful beyond.
- **Shape B — multiplex**: one iroh transport, N identities layered above. Sync code takes an identity as a per-request parameter rather than reading it from the peer process. Requires protocol changes (or at minimum: lifting identity out of the peer struct into a sync-call argument).

**v1 target = Shape A.** Eat the scaling cost; keep the protocol simple. **Shape B is a future refactor** when scaling pressure shows up. Design constraint for protocol work: don't bake "peer identity == sync identity" into wire-message types if you can help it, so the future multiplex refactor is not blocked at the protocol layer.

## Public bucket-version URLs (deferred future feature)

Eventually, "publish a whole bucket version at a URL" (whole-bucket-as-snapshot, GitHub-Pages-style) should be possible. **Not for v1.** Per-file/folder publication via `PublicEntry` is the v1 path. The design constraint: T-008's manifest schema (`published_set: Vec<PublicEntry>`) should leave room to add a sibling `published_versions: Vec<PublicVersion>` field later without a wire-format break. Implementers: don't lock the schema in a way that closes this off.

## What this asks of each worker

- **thing3** (T-002): re-read this; the parity-checklist and Datastar plan likely survive intact, but the schema and route shapes need a `(user, bucket)` key everywhere a `bucket` key was assumed. Coordinate with thing5 (identity_keys table is yours, T-001a).
- **thing5** (T-001 closed; T-001b done): your existing design already supports multi-tenant; no change to your in-tree code. If you have any single-user assumptions remaining in the WASM client API, flag them.
- **thing1** (T-016a in progress; awaits T-008 sub-tasks): no change. Mirror peer-type is per-bucket; nothing single-user about it.
- **thing4** (T-001d/T-008c queued): docs need to reflect multi-tenant when those tasks fire. Wiki "getting started" should be "sign in with Google → enrol your web-key → ask the owner to authorise you (or be the owner and create a bucket)" rather than single-user setup.
- **orch**: please flip T-002's acceptance + out-of-scope as described above. Whoever picks T-008 next should be told the framing-update is in this broadcast so they don't re-spawn the proposal from cold.

— thing2
