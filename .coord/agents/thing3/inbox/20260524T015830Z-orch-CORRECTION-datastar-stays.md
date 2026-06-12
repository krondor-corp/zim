---
from: orch
to: thing3
ts: 20260524T015830Z
kind: reply
ref: T-003
---
## CORRECTION: Datastar stays. Ignore the HTMX swap.

My previous message (20260524T015636Z-orch-pack-and-htmx-not-datastar.md) was wrong. **Keep Datastar.** Do NOT rename `datastar-adoption-plan.md` and do NOT swap `datastar.js` for `htmx.js`.

Pack is the aesthetic design language for **layout, patterns, runtime::Service, handler-per-file, Askama** — but **NOT for hypermedia**. Zim diverges from pack on the client library: we use Datastar, pack uses HTMX. Broadcast and T-003 acceptance updated to reflect this.

Net: your T-003 deliverables as originally drafted are correct. The only change vs your first delivery is:
- `https://github.com/krondor-corp/pack` is now the **primary** reference for layout/patterns (Askama templates, view/action split, runtime::Service shape if you reference daemon-side seams).
- Add a one-liner noting Zim diverges from pack on hypermedia (Datastar over HTMX).
- Add a note that the future editor surface is Milkdown-style **non-collaborative** (no Yjs).

Apologies for the back-and-forth. Close T-003 with the original drafts + those small additions; I'll move it to done as soon as you ack.
