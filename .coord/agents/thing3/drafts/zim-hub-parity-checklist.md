# Desktop → zim-hub functional parity checklist

**Author:** thing3 (T-003)
**Policy:** clean-break, single-user, read-only mirror; pack-aesthetic (`broadcast/20260524T015636Z-pack-design-language.md`, clarified at `20260524T015900Z-pack-is-aesthetic-only.md`).
**Primary reference:** <https://github.com/krondor-corp/pack> (aesthetic/layout). Hypermedia divergence: Zim uses **Datastar**, not pack's HTMX. Future editor target: Milkdown-style, **non-collaborative** (no Yjs).
**Scope:** functional flows only — UI fidelity is explicitly NOT a goal. The desktop app's job was a single-user file manager + viewer + history + share/publish surface; zim-hub re-implements the **read** half as a hypermedia web view.

This checklist enumerates every desktop user flow (derived from `crates/desktop/src/pages/` and `crates/desktop/src/lib/api.ts`) and decides one of: **port**, **drop**, or **moved to zim-peer CLI**.

## Legend

- **Port** → zim-hub must implement this flow (read-only).
- **Drop** → the flow is gone in the new shape; nothing in zim-hub replaces it.
- **Peer** → mutation flow; remains on `zim-peer` (CLI / `zim-peer` HTTP API), out of zim-hub scope.

## A. Top-level navigation

| Desktop page | Disposition | zim-hub route | Notes |
|---|---|---|---|
| Home (`pages/Home.tsx`) | Port | `GET /` | Landing page: signed-in identity card + list of accessible buckets. |
| Buckets list (`pages/Buckets.tsx`) | Port | `GET /` (merged) or `GET /buckets` | Single-user — collapses to "your buckets" on the home page. Drop the separate route. |
| Settings (`pages/Settings.tsx`) | Port (subset) | `GET /settings` | Read-only: identity (Google account), node id, peer endpoint URL, sign-out. No daemon config knobs (those live on `zim-peer`). |
| Logs (`pages/Logs.tsx`) | Drop | — | Logs are an operator concern on `zim-peer`, not a hub feature. `journalctl` / `zim-peer logs` covers it. |
| Mounts (`pages/Mounts.tsx`) | Drop | — | FUSE mounts are peer-local. Has no meaning on a remote read-only web view. |

## B. Bucket exploration (Explorer / Viewer)

| Desktop page | Disposition | zim-hub route | Notes |
|---|---|---|---|
| Explorer (`pages/Explorer.tsx`) | Port | `GET /b/{bucket_id}/tree/{path:*}` | Directory listing. Server-rendered. SSE updates via `data-on-load`. Shows only **published-set** entries per T-008 — no full bucket tree without a capability. |
| Viewer (`pages/Viewer.tsx`) | Port | `GET /b/{bucket_id}/blob/{path:*}` | Rendered preview (HTML for known mime types). Raw bytes at `GET /b/{bucket_id}/raw/{path:*}` for download / non-renderable types. |
| Editor (`pages/Editor.tsx`) | Drop (v1) — future as Milkdown-non-collab | — | zim-hub v1 is read-only; editing stays on `zim-peer` CLI (`zim bucket update`). Future capability: a **Milkdown-style, non-collaborative** editor surface (no Yjs, no CRDT collab, single-user). Reserved seam in zim-hub layout but not built; if/when adopted, T-002 (or a future editor task) picks whether it lives in zim-hub (with calls back to `zim-peer`'s mutation API) or moves into `zim-peer`'s own HTTP surface. |
| Breadcrumb (`components/Breadcrumb.tsx`) | Port | partial: `partials/breadcrumb.html` | Rendered into every tree/viewer template. |

## C. History

| Desktop page | Disposition | zim-hub route | Notes |
|---|---|---|---|
| History (`pages/History.tsx`) | Port | `GET /b/{bucket_id}/history` | Lists past `link_hash` versions, height, `published` boolean. Each row links to a snapshot view at `GET /b/{bucket_id}/at/{link_hash}/tree/{path:*}`. |
| Snapshot tree view | New | `GET /b/{bucket_id}/at/{link_hash}/tree/{path:*}` | Same template as Explorer, sourced from a historical snapshot. |
| Snapshot blob view | New | `GET /b/{bucket_id}/at/{link_hash}/blob/{path:*}` | Same template as Viewer, sourced from a historical snapshot. |

## D. Sharing / publication

| Desktop component | Disposition | zim-hub route | Notes |
|---|---|---|---|
| SharePanel (`components/SharePanel.tsx`) | Drop | — | "Mirror" role gone (T-006). Sharing/membership changes stay on `zim-peer`. |
| Publish / Unpublish (current `Manifest::public`) | Drop | — | Whole-bucket publish removed (T-008). zim-hub instead surfaces the **published-set** at `GET /b/{bucket_id}/published` (read-only). Adding/removing entries from the set is a peer operation. |
| Published-set view (new) | Port | `GET /b/{bucket_id}/published` | Lists files/folders explicitly marked public. The same data drives the tree filter on `GET /b/{bucket_id}/tree/*`. |

## E. Confirmations / dialogs

| Desktop component | Disposition | Notes |
|---|---|---|
| ConfirmDialog (`components/ConfirmDialog.tsx`) | Drop | Read-only hub has no destructive actions to confirm. If sign-out is reintroduced as a confirm step, do it inline with a Datastar `data-on-click` signal — no modal component needed. |

## F. Daemon / system tray

| Desktop concern | Disposition | Notes |
|---|---|---|
| System tray (`src-tauri/src/tray.rs`) | Drop | Web app, no tray. |
| Embedded daemon (Tauri sidecar) | Drop | zim-hub is a separate binary that connects to a separately-running `zim-peer` over HTTP. No in-process embedding (zim-peer remains the in-process embed option for other hosts). |
| Autostart plugin (`tauri-plugin-autostart`) | Drop | Out of scope; use systemd / launchd for zim-peer. |
| Dialog plugin (`tauri-plugin-dialog`) | Drop | No native dialogs in a web app. |

## G. Mutation surface (everything that creates / writes / changes)

All of these were desktop IPC commands; they are explicitly **NOT** ported to zim-hub. They remain available via `zim-peer`'s HTTP API + CLI:

- `createBucket`, `deleteBucket`
- `addFile`, `updateFile`, `renamePath`, `mv`
- `addShare`, `removeShare`, `approveShare`
- `publish_file(path)`, `unpublish_file(path)`, `publish_folder(path)`, `unpublish_folder(path)` (T-008 verbs)
- Mount create/start/stop/delete

zim-hub never invokes these endpoints. The peer client (`crates/zim-hub/src/peer_client.rs`) exposes only read methods.

## H. Identity flow (delegated to T-001, summarised here)

| Step | Where | Notes |
|---|---|---|
| Google sign-in | zim-hub | OAuth2 PKCE. Cookie session, single-user. |
| Encrypted private key unlock | zim-hub | Password-protected blob; unlocked after sign-in. Held in memory only. |
| Remote peer authorization | zim-hub → zim-peer | zim-hub uses the unlocked key to authenticate to a remote `zim-peer` and fetch the published-set + blobs. |

## I. Parity matrix at a glance

| Capability | Desktop | zim-peer CLI | zim-hub (target) |
|---|---|---|---|
| List buckets | ✓ | ✓ | ✓ (yours only) |
| Browse tree | ✓ | ✓ | ✓ (published-set) |
| View file content | ✓ | ✓ (cat) | ✓ |
| View history | ✓ | ✓ | ✓ |
| Browse historical snapshot | partial | ✓ | ✓ |
| Create / delete bucket | ✓ | ✓ | ✗ (intentional) |
| Add / update / rename file | ✓ | ✓ | ✗ |
| Manage shares | ✓ | ✓ | ✗ |
| Publish/unpublish path | (whole bucket) | ✓ (per file/folder, T-008) | ✗ (read view only) |
| Mount FUSE | ✓ | ✓ | ✗ |
| View logs | ✓ | ✓ | ✗ |
| Settings UI | ✓ | ✓ (config) | partial (identity card + sign-out) |
| System tray | ✓ | ✗ | ✗ |
| Autostart | ✓ | (systemd/launchd) | ✗ |

## J. Acceptance: parity coverage statement

zim-hub at T-002 v1 must implement everything marked ✓ in the "zim-hub (target)" column. Anything marked ✗ is *intentionally absent*, not a gap. The desktop app's mutation surface migrates to `zim-peer`'s existing HTTP API + the `zim` CLI; that move requires no new endpoints (the API already covers all desktop IPC commands).
