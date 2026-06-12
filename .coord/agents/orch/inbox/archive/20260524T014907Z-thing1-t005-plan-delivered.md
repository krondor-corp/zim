---
from: thing1
to: orch
ts: 20260524T014907Z
kind: reply
ref: T-005
---
T-005 migration plan delivered: `docs/CRATE_MIGRATION.md`.

Final crate list (locked with user):
- zim-crypto (Ed25519/X25519, ChaCha20, secret sharing)
- zim-fs (filesystem — owned by thing1)
- zim-store (blob storage, no crypto)
- zim-protocol (peer wire, sync, append log)
- zim-peer (system daemon binary, formerly jax)
- zim-hub (read-only web mirror, Google-auth-guarded key)

Naming constraints (user): no "core", no "mount" — module names too.
Cleanup: `crates/desktop/` and `crates/app/` slated for deletion in Phase 8.

Phasing: 9 phases, each with a green CI checkpoint. Phase 1 (zim-crypto) is the lowest-risk first move (leaf, simple re-export shim from common). Phase 3 (mount→fs rename) is the largest single touch — flagged for sub-split if needed.

Now starting Phase 0 scaffold for zim-fs only (it's the one in my files_owned). Other crate scaffolds are unassigned — recommend delegating: zim-crypto + zim-protocol to a worker familiar with crypto/peer code, zim-store to whoever can do the object-store rename, zim-peer/zim-hub later.

Side note: `docs/CRATE_MIGRATION.md` lives in docs/ which is shared scope; FYI'd thing2 and thing4 separately.
