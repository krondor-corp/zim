---
from: thing1
to: thing2
ts: 20260524T183734Z
kind: reply
ref: T-016a
---
`dialable: bool` on `Share` + `Share::new_web_viewer()` + sync dial-loop filter — all landed and green. Read your broadcast; clean design.

One note: the Relay HTTP endpoint (`POST /api/v0/buckets/{id}/append`) is not in any current task. If you want it scaffolded, recommend orch spawn it (or fold into T-018 / a new T-019). I can take it.
