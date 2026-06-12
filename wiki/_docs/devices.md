---
title: Managing Devices
order: 5
---

# Managing Devices

Your Zim account can have multiple devices: your laptop, your phone, and the web browser you're reading this on. Each device has its own private key and authenticates independently.

## Your Web Device

When you first sign in to a Zim hub with Google, a keypair is created in your browser. You'll choose an unlock password — this encrypts the key before it's stored on the hub. The hub never sees your actual key.

Every time you sign in, you'll enter your unlock password to access your buckets.

## Adding a New Device (Phone, CLI, Desktop)

1. Open the hub URL on your new device and sign in with Google.
2. You'll see an "Enrol Device" page. Give it a name (e.g., "My Phone") and submit.
3. On an **existing device** (like your web browser), a notification appears asking you to approve.
4. Click **Approve** on your existing device.
5. Done — the new device is registered and can access your buckets.

**First device is automatic.** If you've never registered before, your first sign-in is auto-approved.

## Adding a CLI Device

```bash
zim hub register --hub https://hub.example.com
```

This opens a browser window. Sign in with Google, name the device, and approve it from an existing device. The CLI stores its key locally at `~/.config/zim/<hub>/device.key`.

## What Happens When You Lose a Device

1. Sign in on any remaining device.
2. Go to **Account → Devices**.
3. Click **Revoke** on the lost device.

The lost device immediately loses access — its keys stop working. If the lost device held bucket shares, those shares are invalidated for that device (other devices retain their own copies).

## Lost All Devices

If you've lost every device (including the web browser where your key was stored):

1. Sign in via Google on a fresh browser.
2. The hub will walk you through creating a new web key with a new password.
3. Your old encrypted key blob is unrecoverable (the old password is gone).
4. Revoke all old devices from **Account → Devices**.
5. Ask the bucket owner to re-authorize you (they need to issue new shares for your new device keys).

## Security Notes

- Each device has its own ed25519 keypair. Compromising one device doesn't compromise the others.
- The hub never holds your private key in plaintext — only the web device's encrypted blob.
- Revoking a device = deleting its public key from the hub. Any JWTs it signed immediately fail verification.
- Your unlock password is not stored anywhere. Use a strong, unique password.
