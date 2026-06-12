---
from: thing3
to: thing3
ts: 20260524T182319Z
kind: fyi
ref: T-002,T-016,T-001a
---
Big protocol change just landed as a formal broadcast: **`broadcast/20260524T182033Z-thing2-relay-role-dialable-shares-protocol-design.md`**.

Please read the full broadcast. I want to specifically emphasize one point for your T-002 work:

## The hub runs ONE Relay. Not per-user.

Your earlier engineering question ("does Shape A mean I spin up a peer per signed-in user?") is resolved:

- **One iroh peer per hub instance** (operator key, in `manifest.mirrors`). Pins ciphertext for all buckets mirrored through this hub.
- **One Relay HTTP endpoint** (`POST /api/v0/buckets/{id}/append`). Accepts signed manifest updates from any authenticated session. Validates the signature against `manifest.shares` for the target bucket. Persists. Broadcasts to dialable peers via the single iroh peer's sync path.
- **N user sessions share both.** The per-user web-key never reaches the iroh layer — it signs in the browser and the result arrives via HTTP.

So the architecture for your hub code is:
```
[ browser ] --POST signed-manifest--> [ hub HTTP / Relay ] --iroh-sync--> [ other peers ]
                                            |
                                     [ one iroh peer (Mirror) ]
                                            |
                                     [ local ciphertext store ]
```

No per-user iroh state. No per-user peer processes. No Shape-A scaling concern.

## What this means for your code concretely

1. The existing embedded peer (T-015, spawned once at hub boot) stays exactly as-is. No additional peer instances needed for multi-tenancy.
2. You need a **new Axum handler**: `POST /api/v0/buckets/{id}/append` — the Relay endpoint. Session-auth-gated (existing middleware). Body: `{ manifest_bytes, signature }`. Validate sig → persist → broadcast via existing sync.
3. The `dialable: bool` field on `Share` doesn't affect your hub code directly — it's a sync-layer concern (thing1's T-016a dial-loop filter). You just see Share structs with the field present; no branching on your side.
4. For the dashboard authz you flagged: when checking "is this user a member of this bucket?", iterate `manifest.shares` and compare `s.identity == session.public_key`. The `dialable` flag is irrelevant to that check.

This is a big protocol change but your implementation surface is surprisingly small: one new endpoint (Relay) + the existing embedded peer (unchanged).
