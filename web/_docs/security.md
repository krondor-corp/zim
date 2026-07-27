---
title: Security
order: 6
---

# Security

## End-to-End Encryption

All bucket content is encrypted with ChaCha20-Poly1305 before it leaves your device. Only members with a share can decrypt. The hub stores only ciphertext.

## Device Model

- Each device (phone, laptop, web browser) gets its own ed25519 keypair.
- Device private keys never leave the device they were created on.
- The web device's key is encrypted with your unlock password (Argon2id + ChaCha20-Poly1305) and stored on the hub. The hub cannot decrypt it without your password.

## Authentication

- **Web sessions**: Google OAuth establishes identity; your unlock password proves key custody.
- **CLI / mobile / desktop**: ed25519-signed JWTs (5-minute lifetime). The device signs a fresh token for each request. No long-lived secrets stored server-side.
- **Bucket access**: per-device shares. Each device's public key is individually authorized by the bucket owner.

## What the Hub Cannot Do

- Decrypt your bucket content (no bucket secret).
- Decrypt your web device's private key (needs your password, which is never sent to the hub).
- Impersonate you to other peers (would need your private key).
- Access buckets on your behalf without your browser being active.

## What You Should Do

- Use a strong, unique unlock password (not your Google password).
- Revoke lost devices immediately from Account → Devices.
- Verify your hub's TLS certificate if handling sensitive content.

## Mirror Peers

The hub runs a mirror peer (its own operator key) that pins ciphertext for your buckets. This provides durability — your data is available even when your devices are offline. The mirror peer cannot decrypt your data.
