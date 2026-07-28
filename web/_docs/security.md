---
title: Security
order: 7
---

## End-to-end encryption

All vault content is encrypted with ChaCha20-Poly1305 before it leaves
your device. Only devices holding a share can decrypt. The hub stores
and relays only ciphertext.

## Device model

- Each device (browser, laptop, phone) gets its own Ed25519 keypair.
- Device private keys never leave the device they were created on.
- The browser device's key is encrypted with your passphrase and stored
  on the hub. The hub cannot decrypt it without your passphrase, which
  is never sent to the hub.

## Authentication

- **Browser sessions** — Google sign-in establishes your account
  identity; your passphrase unlocks (proves custody of) your web key.
- **CLI devices** — Ed25519-signed tokens, minted fresh per request and
  short-lived. No long-lived secret is stored server-side.
- **Vault access** — per-device shares. Each device is granted access
  individually; add or revoke a device by adding or removing its share,
  no key rotation required.

## What the hub cannot do

- Decrypt your vault content (it never holds a vault secret).
- Decrypt your web key (that needs your passphrase, which it never sees).
- Impersonate you to other peers (that needs your device's private key).

## The hub as a relay

The hub runs as a peer that pins **ciphertext** for your vaults, so your
data stays available and syncs between your devices even when some are
offline. It is a durability and rendezvous point, not a member of your
vaults — it cannot read anything it stores.

You are trusting the hub for availability and for your account roster
(which devices belong to you). It is a trusted coordinator by design;
the encryption is what keeps your content private regardless.

## What you should do

- Use a strong, unique passphrase — it is not recoverable, and it
  protects the key that decrypts everything.
- Revoke lost devices promptly from the devices page.
- Prefer a hub you run or trust for anything sensitive.

> Zim has not been independently audited. Don't use it to protect data
> you can't afford to lose.
