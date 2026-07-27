---
title: Deploy Your Own Hub
order: 7
---

Run a zim hub on your own server. A hub is the always-on member of your
device fleet: it mirrors your vaults as ciphertext, hosts your
`did:web` account identity, and serves the browser workspace. Everything
it stores is end-to-end encrypted — the hub never holds a decryption
key for your content.

This guide takes you from a blank Linux server to a hub your devices
can enroll against.

## What you need

- A **Linux server** with Docker (or Podman) installed, ports 80/443
  reachable. 1 GB RAM is plenty — the image comes prebuilt.
- A **domain** pointed at the server (an `A` record, e.g.
  `hub.example.com`).
- A **Google OAuth client** (free) — the hub's web sign-in.

> **Pick your domain carefully — it is permanent.** The hub's domain
> *is* its identity: your account becomes `did:web:hub.example.com`,
> and every device enrollment and share references it. Changing the
> domain later means a new identity and re-enrolling everything.

## 1. DNS

Create an `A` record for your chosen hostname pointing at the server's
IP. Verify before continuing:

```bash
dig +short hub.example.com
# → your server's IP
```

## 2. Google OAuth client

1. Open [Google Cloud Console → Credentials](https://console.cloud.google.com/apis/credentials).
2. **Create Credentials → OAuth client ID → Web application.**
3. Add an **authorized redirect URI**:
   `https://hub.example.com/auth/google/callback`
4. Note the **client ID** and **client secret**.

## 3. Get the hub image

Prebuilt images are published to GitHub Container Registry:

```bash
docker pull ghcr.io/krondor-corp/zim-hub:latest
```

`latest` tracks the main branch; release versions are tagged
(`ghcr.io/krondor-corp/zim-hub:<version>`).

Prefer building it yourself? The recipe is in the repo:

```bash
git clone https://github.com/krondor-corp/zim.git
cd zim && docker build -f Dockerfile.hub -t zim-hub .
```

(The build compiles the browser workspace to WebAssembly and the hub
server into one image — allow a few minutes.)

## 4. Run it

On the server, create a data directory and start the hub:

```bash
sudo mkdir -p /var/lib/zim-hub

docker run -d --name zim-hub --restart unless-stopped \
  -p 127.0.0.1:8080:8080 \
  -v /var/lib/zim-hub:/data \
  -e ZIM_HUB_LISTEN=0.0.0.0:8080 \
  -e ZIM_HUB_HOST=hub.example.com \
  -e HOST_NAME=https://hub.example.com \
  -e ZIM_HUB_ADMIN_EMAILS=you@example.com \
  -e GOOGLE_O_AUTH_CLIENT_ID=your-client-id \
  -e GOOGLE_O_AUTH_CLIENT_SECRET=your-client-secret \
  ghcr.io/krondor-corp/zim-hub:latest
```

What the knobs mean:

| Variable | Meaning |
|----------|---------|
| `ZIM_HUB_HOST` | The hub's public hostname — becomes `did:web:<host>`. |
| `HOST_NAME` | Public base URL, used for the OAuth callback. |
| `ZIM_HUB_ADMIN_EMAILS` | Google accounts auto-promoted to admin on first sign-in. |
| `ZIM_HUB_HOME` | Data directory (defaults to `/data` in the image). |

Everything the hub is lives in `/var/lib/zim-hub` — SQLite databases,
its identity key, and (by default) your encrypted blobs. **Back up
this directory.** A session-signing secret is generated on first boot
and persisted there too.

## 5. TLS

`did:web` resolution requires HTTPS, so put any TLS-terminating proxy
in front. [Caddy](https://caddyserver.com) makes this two lines —
`/etc/caddy/Caddyfile`:

```
hub.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Caddy provisions the certificate automatically. (nginx + certbot works
the same way; the hub just needs `443 → 8080` proxying.)

## 6. Verify

```bash
curl https://hub.example.com/_status/livez        # → ok
curl https://hub.example.com/.well-known/did.json # → your hub's DID document
```

Then open `https://hub.example.com` in a browser, sign in with the
Google account you listed in `ZIM_HUB_ADMIN_EMAILS`, and complete
onboarding — this mints your **web key**, the browser's member of your
device fleet.

## 7. Connect your devices

On each machine (see [Install](/docs/install/) — build with the `hub`
feature):

```bash
zim init
zim daemon start
zim hub login --hub https://hub.example.com
```

`hub login` walks a device-code flow: it prints a link, you approve
the device in the hub UI, and the daemon is enrolled — the hub lands
in the daemon's peer book and your account roster syncs to it. From
then on:

- Vaults you share with your account are mirrored to the hub and
  reachable from the browser workspace.
- Edits made in the browser sync back to your daemons through the hub.
- Two of your devices that are rarely online together sync *through*
  the hub — it's the always-on relay, storing only ciphertext.

## Optional: S3-compatible blob storage

By default blobs live on the hub's disk. To use S3-compatible object
storage (MinIO, Garage, real S3) instead, add:

```bash
  -e ZIM_HUB_S3_ENDPOINT=http://minio-host:9000 \
  -e ZIM_HUB_S3_ACCESS_KEY=... \
  -e ZIM_HUB_S3_SECRET_KEY=... \
  -e ZIM_HUB_S3_BUCKET=zim-blobs \
```

The bucket must exist before first boot. SQLite state stays in
`/data` either way; only blob bytes move to object storage.

## Upgrading

State survives restarts and upgrades — it's all in the volume:

```bash
docker pull ghcr.io/krondor-corp/zim-hub:latest
docker stop zim-hub && docker rm zim-hub
# ... re-run the `docker run` command from step 4
```
