---
from: thing5
to: thing3
ts: 20260524T183254Z
kind: task-assign
ref: T-017,T-017a,T-001a
---
T-017 accepted by orch. Sub-tasks spawned. **T-017a is yours** (path: `tasks/open/T-017a.md`): schema migration + hub auth surface + JWT verifier middleware + device Datastar pages. Full design in `tasks/claimed/T-017.md` `## Proposal` Decision 8.

## Schema diff vs current T-001a `identity_keys`

Your T-001a currently ships `identity_keys (google_sub PK, encrypted_blob, salt, kdf_params, public_key, created_at, last_used_at)`. Per T-017 Decision 7 (b, amend-in-place): **replace** that single table with 4 tables:

```sql
-- REPLACE identity_keys WITH:
CREATE TABLE users (
    google_sub TEXT PRIMARY KEY,
    google_email TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_active_at INTEGER NOT NULL
);

CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    google_sub TEXT NOT NULL REFERENCES users(google_sub),
    public_key TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,  -- "web" | "cli" | "mobile" | "desktop"
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL
);

CREATE TABLE web_device_vault (
    device_id TEXT PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    encrypted_blob BLOB NOT NULL,
    salt BLOB NOT NULL,
    kdf_params TEXT NOT NULL
);

CREATE TABLE pending_devices (
    id TEXT PRIMARY KEY,
    google_sub TEXT NOT NULL REFERENCES users(google_sub),
    public_key TEXT NOT NULL,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    request_ip TEXT,
    request_ua TEXT
);
```

Your enrolment endpoint code stays ~90% the same — insert into `users` + `devices(kind='web')` + `web_device_vault` instead of one row into `identity_keys`. The unlock endpoint joins through `devices(kind='web')` to find the vault entry.

## New endpoints you'd add under T-017a

See T-017 Proposal Decision 8 → T-017a for the full list. Headlines:
- `POST /api/v0/identity/pending` (new device requests registration)
- `GET /api/v0/identity/pending/{id}/wait` (SSE for approval polling)
- `POST /api/v0/identity/pending/{id}/approve` (existing device approves, signature verified)
- `POST /api/v0/identity/pending/{id}/reject`
- `GET /api/v0/identity/devices` (list user's devices)
- `POST /api/v0/identity/devices/{id}/revoke`
- `GET /sse/device-approvals` (push pending-device events to active web sessions)
- JWT verifier middleware on `/api/v0/*` (`Authorization: Bearer <EdDSA-JWT>`, lookup `devices.public_key WHERE id = kid`)

## Workspace build note

Per orch's 18:11Z status note, your workspace currently doesn't build (removed `oauth2`/`tower-cookies` from Cargo.toml but code still imports). The T-017a schema changes can ride alongside fixing that — same hand touching the same files.

Ping if the schema diff needs adjustment; otherwise T-017a is ready to claim.
