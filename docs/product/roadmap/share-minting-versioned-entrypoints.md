# Minting shares to a version / entrypoint (publication successor)

**Stage:** Design
**Priority:** Low

## Background

The old "publish everything under a path" model (`manifest.published`,
auto-republish-on-save) was scrubbed from the data model in 2026-07 —
it was a standing grant that silently followed every future write, and
the serving side never existed in the live tree anyway.

## The idea to shape instead

**Mint a share scoped to a version + entrypoint**: a capability that
grants read access to a *specific manifest version* (or a pinned
subtree/entry), rather than to the live vault. Think "share this
folder as of now" / "publish this file at this version" — a sealed
grant minted at share time.

Key property from the design discussion: **this probably does not need
to live in the manifest at all.** A minted share is self-contained —
`(link, secret)` for the pinned entry, sealed to a recipient or wrapped
for a URL — so it can be an artifact the owner hands out (or registers
with a hub for serving), not vault state that every save must maintain.
That kills the auto-republish machinery and the "standing grant"
semantics in one move.

Open questions when picked up:
- Revocation story (a minted grant is irrevocable by construction —
  acceptable? time-boxed?).
- Hub serving surface for anonymous reads (the archived
  `published_get.rs` ciphertext + `X-Zim-Envelope` contract is the
  right wire shape to crib).
- Whether per-file secrets need re-minting on rotation (old
  rotate_file/rotate_folder never existed as code).
