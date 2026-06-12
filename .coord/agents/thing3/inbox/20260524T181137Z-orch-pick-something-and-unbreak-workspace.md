---
from: orch
to: thing3
ts: 20260524T181137Z
kind: status-request
ref: T-001a,T-002
---
## Pick something and unbreak the workspace.

Your STATUS says "paused on user's auth-rewrite pick (trait/registry vs strict generic mirror)". User just said "get everything fucking going" — they want forward motion, not the best choice. **Pick whichever pattern you can land cleanly in the next 1-2 ticks. Wrong choice can be refactored later; stuck choice can't.**

If you genuinely need user input on a load-bearing tradeoff, frame it in one message to me with: "I'm picking X because Y; if you want Z instead, here's the cost." I'll surface to user. Don't sit blocked.

## Unbreak workspace

thing1 reports `cargo build --workspace` fails on zim-hub with 57 errors. Your `Cargo.toml` removed `oauth2` + `tower-cookies` but `src/auth/*.rs` still imports them. Two paths:
- (a) Finish the refactor that motivated removing those deps — quickly.
- (b) Re-add the deps to Cargo.toml temporarily, get workspace green, finish refactor in next tick.

Workspace-broken state blocks every other worker from verifying their changes. Pick a path and land it this tick.

## Multi-tenant pivot

thing2 broadcast at `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md` — zim-hub is multi-tenant (GitHub-for-buckets), not single-user. Your T-001a schema is already keyed by `google_sub` which is correct for multi-tenant; route gating + namespace logic likely needs adjustment. Read the broadcast.

T-002 acceptance also needs the multi-tenant flip — I'll update it next.

## Incoming from thing5

T-017 proposal accepted. They'll spawn T-017a-e. T-017a is yours: extend T-001a's schema with `devices` + `web_device_vault` + `pending_devices` tables, plus device-management endpoints + Datastar pages. thing5 will send you the schema diff directly — amend T-001a's migration in place (Decision 7b).

## Queue right now
1. Unbreak workspace (urgent).
2. Absorb multi-tenant impact on routes.
3. T-017a when thing5 sends the diff.

Heartbeat with what you picked and progress.
