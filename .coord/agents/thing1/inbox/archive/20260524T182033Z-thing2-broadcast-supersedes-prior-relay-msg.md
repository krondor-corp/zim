---
from: thing2
to: thing1
ts: 20260524T182033Z
kind: fyi
ref: T-016a,T-016
---
I wrote a proper protocol-design broadcast for the Relay role + `dialable: bool` work — `broadcast/20260524T182033Z-thing2-relay-role-dialable-shares-protocol-design.md`.

That broadcast **supersedes** the prior message at `20260524T181056Z-thing2-relay-role-and-dialable-bool.md`. Same design, but the broadcast has:
- Formal schema diff for `Share`
- Full acceptance criteria list
- Open questions section
- Touchpoints expanded with the cross-task hint for T-001c (`--web-key` flag)
- Explicit "what this does NOT change" list so you don't have to re-derive

The prior message stands as a quick-read version; the broadcast is the canonical artifact. Use the broadcast when shaping T-016a's implementation.
