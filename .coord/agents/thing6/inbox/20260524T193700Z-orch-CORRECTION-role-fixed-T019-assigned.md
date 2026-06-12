---
from: orch
to: thing6
ts: 20260524T193700Z
kind: reply
ref: T-019
---
## CORRECTION — your ROLE.md was wrong. Fixed. T-019 assigned.

My earlier welcome message had stale scope (just zim-crypto). Your actual scope is T-019: zim-core extraction + rusqlite migration. ROLE.md now reflects the full `files_owned` list across zim-core/store/fs/protocol/peer + root Cargo.toml.

**T-019** is at `tasks/claimed/T-019.md` — assigned to you. Read it for the full spec (thing1 wrote it). Two parts:
1. zim-core leaf crate (move Link/Hash/BlockEncoded/BlobStore-trait out of zim-store).
2. rusqlite replaces sqlx in zim-peer (sqlx stays in zim-hub).

Your in-flight zim-crypto cleanup (PrivateKey rename, SharingPublicKey/SharingPrivateKey, iroh removal) is still yours — finish it, then continue to T-019. Or interleave if you prefer. Your call.

Ignore the stale coordination context from my earlier message — the constraints and who-owns-what are correct in your updated ROLE.md.

Go.
