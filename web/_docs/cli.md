---
title: CLI reference
order: 4
---

The `zim` command-line tool runs the daemon, manages vaults, and drives
peer-to-peer sync. Commands talk to a local daemon over its HTTP API on
loopback; start it with `zim daemon run` (or install it as a service).

## Setup

```bash
zim init                 # create your data dir + device identity
zim id                   # print this device's public key
zim daemon run           # run the daemon in the foreground
zim update               # update the binary (and restart the service)
```

## Vaults

A vault is an encrypted, versioned folder. Commands are verb-first —
`zim vault <op> <vault> …`, where `<vault>` is a name or id.

```bash
zim vault create notes                    # create a vault
zim vault list                            # list vaults this device holds
zim vault head notes                      # current version + height

zim vault ls notes /                      # list a directory
zim vault cat notes /readme.md            # read a file to stdout
echo "hi" | zim vault add notes /hi.md    # write a file (content from stdin)
zim vault mkdir notes /docs               # create a directory
zim vault mv notes /a.md /docs/a.md       # move a path
zim vault rm notes /draft.md              # remove a path
```

## Sharing

Grant another device or account access to a vault by adding its
identity as a shareholder. A peer can be a `did:key`, a `did:web`
account (to share into someone's hub), or a nickname from your address
book.

```bash
zim vault shares add notes <did-or-nickname>    # grant access
zim vault shares list notes                     # who can decrypt this vault
zim vault shares rm notes <did-or-nickname>     # revoke access
```

Once shared, the vault syncs to the recipient automatically over the
peer-to-peer transport (relayed through a hub when one of you is
offline). To pull from a specific peer on demand:

```bash
zim vault sync notes <peer>
```

## Peers

Your local address book maps nicknames to public keys, so you can use
names instead of raw keys in `shares` and `sync`.

```bash
zim peers add alice <pubkey>    # remember a peer under a nickname
zim peers list                  # everyone this device knows
zim peers ping alice            # round-trip: identity, version, RTT
zim peers rm alice              # forget a nickname
```

## Hub

Pair a device with a hub to sync through your account and reach the
browser. See [Your devices]({{ '/docs/devices/' | relative_url }}) for
the full flow.

```bash
zim hub login --hub https://hub.zim.krondor.org   # pair this device
zim hub peers sync                                 # sync the account roster
zim hub peers ls                                   # roster + who's in your book
```

> Vaults created in the browser won't appear on a device (and vice
> versa) until that device has run `zim hub peers sync`.

## Mounting

Mount a vault as a local folder (requires FUSE — see
[Mounting]({{ '/docs/mounting/' | relative_url }})):

```bash
zim mount add notes ~/zim/notes    # mount at a path
zim mount list                     # mounts + status
zim mount stop notes               # unmount (keeps the registration)
zim mount remove notes             # unmount and forget
```

## Status

```bash
zim health        # daemon health
zim version       # build info
```
