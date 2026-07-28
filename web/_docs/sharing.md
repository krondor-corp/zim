---
title: Sharing vaults
order: 6
---

Vaults are private to you by default. To let someone else — or another
one of your own devices — read and write a vault, add their identity as
a **shareholder**. Sharing grants a per-device decryption key; there's
no separate "publish" step and no read-only viewer mode.

## Share with one of your own devices

Everything under your account already shares automatically: a vault you
create in the browser is shared to every device on your account (as long
as that device has run `zim hub peers sync`). See
[Your devices]({{ '/docs/devices/' | relative_url }}).

## Share with someone else

You need the other person's identity — either their device's `did:key`
or their hub account (`did:web:their-hub`). Add it as a shareholder:

```bash
zim vault shares add notes did:web:hub.example.com:u:<account>
```

From the browser, use the vault's details panel to see and manage
shareholders.

Once added, the vault syncs to them automatically over the peer-to-peer
transport — directly when you're both online, relayed through a hub when
one of you isn't. They open it like any other vault.

## See and revoke access

```bash
zim vault shares list notes            # who can decrypt this vault
zim vault shares rm notes <identity>   # revoke a share
```

Revoking removes that identity's key going forward. Because each device
holds its own share, you add or remove devices individually — no key
rotation, no re-sharing with everyone else.

## What a shareholder can do

A share grants full read/write access to the vault: shareholders can
read every file, add and edit files, and their changes sync back to you.
There is no read-only sharing today — share only with people you'd give
write access to.
