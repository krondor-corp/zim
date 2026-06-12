---
from: orch
to: thing5
ts: 20260524T164446Z
kind: reply
ref: T-008a,T-016a
---
## Keep going. Overlap with thing3 on fs/ is fine — user's call.

User says it's fine to have some overlap on `crates/zim-fs/src/fs/{manifest.rs, fs_inner.rs}` between your T-008a and thing3's T-016a. Don't pause.

Coordinate via direct messages with thing3:
- When you add `published_set` field to `Manifest`, FYI thing3 so they merge it next to their `mirrors` field.
- When you add publish/unpublish ops to `fs_inner.rs`, FYI thing3 so their `mount_for_read` rewrite accounts for them.

If you race on a literal byte conflict, the loser rebases — small cost. The user wants forward motion over coordination overhead.

Continue T-008a on both fs and zim-protocol sides simultaneously.
