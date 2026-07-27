---
title: Viewer enrolment
description: How to get access to a bucket on a Zim hub
---

This page explains how to get read access to a bucket hosted on a Zim hub.

## What you need

- A Google account (the hub uses Google Sign-In for identity).
- The hub URL from the person sharing the bucket (e.g. `https://hub.example.com`).

## Step 1 — Sign in

1. Open the hub URL in your browser.
2. Click **Sign in with Google** and complete the Google login.

## Step 2 — Set an unlock password

On first sign-in the hub shows an **Enrol** page:

1. Choose a strong unlock password. **This is not your Google password** — it protects your viewer key specifically. Use something unique.
2. Confirm the password and click **Enrol**.

Behind the scenes, your browser generates a cryptographic key pair. The private key is encrypted with your password and stored on the hub. **The hub never sees your password or your unencrypted key.**

## Step 3 — Send your public key to the bucket owner

After enrolment, your **public key** is displayed on the screen. Copy it and send it to the bucket owner through whatever channel you use (email, chat, etc.).

The owner runs a command on their end to authorize you:

```
zim bucket viewer authorize <bucket-name> <your-public-key>
```

Once they confirm, you can view the bucket's published content.

## Step 4 — Unlock on return visits

1. Open the hub URL and sign in with Google (or your existing session).
2. Enter your unlock password on the **Unlock** page.
3. Browse the bucket's published files.

Your key stays in your browser's memory while the tab is open. Closing the tab or clicking **Log out** clears it.

## Password change

Go to your account page and choose **Change password**. Your key is re-encrypted with the new password. No action needed from the bucket owner.

## Key rotation (compromise recovery)

If you suspect your key was compromised:

1. Go to your account page and choose **Rotate key**.
2. A new key pair is generated. Send the new public key to the bucket owner.
3. The owner deauthorizes the old key and authorizes the new one.

## FAQ

**Can the hub operator read my files?**
No. The hub stores your key encrypted — only your password can unlock it. The hub operator would need to replace the browser code (JavaScript/WASM) to intercept your password, which is mitigated by integrity checks built into the page.

**What if I forget my unlock password?**
You lose access to your current key. Go through enrolment again (new key pair, new password) and ask the bucket owner to authorize the new key.

**Can I use multiple devices?**
Each device gets its own key. Register each device separately and send each public key to the bucket owner.
