# Product

The capabilities and guarantees that define Zim. These pages explain what the
system does and why without describing its Rust types or storage internals.

## Documents

| Document | Description |
|----------|-------------|
| [Vaults](vaults.md) | Encrypted, versioned filesystems and synchronization |
| [Sharing and Access](access-model.md) | Shareholders, hosted devices, and revocation |
| [Identity](identity.md) | DIDs, devices, Google authentication, and browser key custody |
| [Cryptography](./cryptography.md) | Identity, key sharing, and content encryption |
| [Security](./security.md) | Threat model, best practices, and implementation details |
| [Roadmap](roadmap/index.md) | Deferred product directions and design constraints |

## Reading Order

For a complete understanding, read in this order:

1. **[Vaults](vaults.md)** - Understand the primary product capability
2. **[Sharing and Access](access-model.md)** - Understand authorization and revocation
3. **[Identity](identity.md)** - Understand users, devices, and DIDs
4. **[Cryptography](cryptography.md)** - Learn how encryption works
5. **[Security](security.md)** - Understand guarantees and limitations

Implementation details live in [`architecture/`](../architecture/index.md).
