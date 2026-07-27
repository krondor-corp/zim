//! `Vault<B, L>` — the persistent vault.
//!
//! Wraps the [`Manifest`] (signed top-level record: id, name, shares,
//! relays, height, root link, …), the local peer's
//! [`PrivateKey`] (used to sign manifests and recover the peer's
//! [`Share`](crate::fs::Share) of the vault secret), the in-memory
//! [`Fs`] tree, and a handle to the vault [`Log`](VaultLog).
//!
//! Generic over the blob store and the log, with no iroh dependency
//! and no sync orchestration — those layers live one tier up. The
//! browser/wasm port consumes this same type via an HTTP-backed
//! blob store and HTTP-backed log.
//!
//! See `docs/research/hub-revival.md` and the plan at
//! `/Users/al/.claude/plans/purrfect-puzzling-crystal.md` for the
//! architectural story.

pub mod error;
pub mod history;
pub mod id;
pub mod log;

#[cfg(test)]
mod tests;

pub use error::VaultError;
pub use history::HistoryEntry;
pub use id::VaultId;
pub use log::{Head, VaultLog, VaultLogError};

use zim_crypto::{PrivateKey, PublicKey, Secret, SecretShare};
use zim_did::Did;

use crate::blobs::BlobStore;
use crate::fs::{Fs, FsError, Manifest, Share};
use crate::linked_data::Link;

/// Versioned, encrypted file tree bound to a single UUID.
///
/// Cheap to clone — every field is internally Arc-backed or trivially
/// copyable.
#[derive(Clone)]
pub struct Vault<B: BlobStore, L: VaultLog> {
    /// Derived identity: blake3 of the genesis manifest blob. Set at
    /// init (where genesis is written) or supplied at open (where the
    /// caller looked it up in a log whose entries were verified at
    /// append time).
    id: VaultId,
    manifest: Manifest,
    /// Link of the currently persisted manifest blob. Updated on
    /// every successful [`Self::save`].
    manifest_link: Link,
    private_key: PrivateKey,
    fs: Fs<B>,
    log: L,
}

impl<B: BlobStore, L: VaultLog> Vault<B, L> {
    // -- Constructors --

    /// Bootstrap a brand-new vault at genesis, shared with `owner`
    /// only. Sugar for [`Self::init_with_shares`] with no extra keys.
    pub async fn init(
        name: String,
        owner: &PrivateKey,
        blobs: B,
        log: L,
    ) -> Result<Self, VaultError<L::Error>> {
        Self::init_with_shares(name, owner, None, &[], blobs, log).await
    }

    /// Bootstrap a brand-new vault at genesis, sealing the vault secret
    /// to `owner` **and** every key in `shares` — so the owner's other
    /// devices are shareholders from the very first manifest.
    ///
    /// This keeps a freshly-shared vault a single genesis head (height
    /// 0) rather than a genesis followed by an immediate height-1
    /// `save`. That matters against a hub: the hub accepts a genesis
    /// unconditionally but verifies chain continuity on every *advance*,
    /// so folding the shares into genesis avoids a spurious conflict.
    ///
    /// `owner_via` sets the genesis owner share's reachability: `None`
    /// means the owner is dialed directly (a daemon), `Some(host)` means
    /// the owner is reached *via* `host` (a browser key, reachable only
    /// through its hub). Without this a peer advancing the vault would
    /// try to dial a browser owner directly and fail — the announce must
    /// target the hub, which mirrors the new head for the browser to read.
    ///
    /// The vault's [`VaultId`] is *derived* — the blake3 hash of the
    /// genesis blob just written. Read it back via [`Self::id`].
    pub async fn init_with_shares(
        name: String,
        owner: &PrivateKey,
        owner_via: Option<Did>,
        shares: &[PublicKey],
        blobs: B,
        log: L,
    ) -> Result<Self, VaultError<L::Error>> {
        let secret = Secret::generate();
        let owner_pubkey = owner.public();

        let (fs, root_link) = Fs::init_tree(owner_pubkey, &secret, blobs).await?;

        // Owner share + genesis manifest, then seal the same secret to
        // each additional device so they're shareholders from genesis.
        let owner_share = SecretShare::new(&secret, &owner_pubkey).map_err(FsError::from)?;
        let mut manifest = Manifest::new(name.clone(), owner, owner_share, root_link.clone(), 0)?;
        // Re-stamp the owner share's `via` when the owner is a hosted
        // (browser) key. `Manifest::new` seals it with `via = None`
        // (direct dial); a browser owner must carry `via = Some(hub)`.
        if owner_via.is_some() {
            let owner_share = SecretShare::new(&secret, &owner_pubkey).map_err(FsError::from)?;
            manifest.add_share(
                owner_pubkey,
                Share::new(owner_share, Did::from_key(&owner_pubkey), owner_via),
            );
        }
        for pk in shares {
            if *pk == owner_pubkey {
                continue;
            }
            let secret_share = SecretShare::new(&secret, pk).map_err(FsError::from)?;
            manifest.add_share(*pk, Share::new(secret_share, Did::from_key(pk), None));
        }

        // Snapshot the metadata pack so the genesis manifest is
        // self-contained — every dir body the root references is
        // embedded. Without this the persisted manifest blob holds an
        // empty `metadata` field and any subsequent `Vault::open`
        // can't find the root dir body. Re-sign after the mutation
        // since `metadata` + `shares` are part of the signable bytes.
        manifest.set_metadata(fs.blobs().snapshot_metadata());
        manifest.sign(owner)?;

        // Put the manifest blob through the inner store of the Fs's
        // ContentStore — manifests are signed, not encrypted, so they
        // skip the metadata-encryption tier.
        let manifest_link = fs.blobs().inner().put_cbor(&manifest).await?;

        // Identity is the genesis blob's hash — derived, not chosen.
        let id = VaultId::from_genesis_link(&manifest_link);

        log.append(id, name, manifest_link.clone(), None, 0).await?;

        Ok(Self {
            id,
            manifest,
            manifest_link,
            private_key: owner.clone(),
            fs,
            log,
        })
    }

    /// Open an existing vault at its current head.
    ///
    /// Walks the log to find the head link, fetches the manifest,
    /// locates the local peer's share, recovers the vault secret,
    /// and materialises the tree.
    ///
    /// Errors with [`VaultError::Fs(FsError::ShareNotFound)`] when
    /// the local peer isn't a shareholder. Relays don't construct a
    /// `Vault` — they operate on [`BlobStore`] + [`VaultLog`]
    /// directly.
    pub async fn open(
        id: VaultId,
        blobs: B,
        log: L,
        secret_key: &PrivateKey,
    ) -> Result<Self, VaultError<L::Error>> {
        let head = log.head(id, None).await?;
        Self::open_with_head(id, head.link, blobs, log, secret_key).await
    }

    /// Open a vault from a known head link, bypassing the log
    /// lookup.
    ///
    /// Used when the head was learned out-of-band — e.g. the browser
    /// fetched it from a hub's `/api/v0/vaults/{id}/head` and there's no
    /// local log to query, or a remote peer pushed it in a
    /// `ShareOffered` wire message before the local log has any
    /// entries.
    pub async fn open_with_head(
        id: VaultId,
        head_link: Link,
        blobs: B,
        log: L,
        secret_key: &PrivateKey,
    ) -> Result<Self, VaultError<L::Error>> {
        let manifest: Manifest = blobs.get_cbor(&head_link).await?;

        let owner_pubkey = secret_key.public();
        let share = manifest
            .get_share(&owner_pubkey)
            .ok_or(crate::fs::FsError::ShareNotFound)?;
        let secret = share
            .secret_share()
            .recover(secret_key)
            .map_err(FsError::from)?;

        let fs = Fs::load_tree(
            manifest.root(),
            secret,
            manifest.metadata().clone(),
            manifest.pins().clone(),
            manifest.ops_clock(),
            owner_pubkey,
            blobs,
        )
        .await?;

        Ok(Self {
            id,
            manifest,
            manifest_link: head_link,
            private_key: secret_key.clone(),
            fs,
            log,
        })
    }

    // -- Accessors --

    pub fn id(&self) -> VaultId {
        self.id
    }

    /// Local peer's private key. Used internally for signing manifests
    /// and recovering this peer's [`Share`] of the vault secret. Made
    /// public so external sync orchestration in `zim-peer` can drive
    /// cross-vault merges (decrypt incoming ops-log blobs) without
    /// re-reading the on-disk identity file.
    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    pub fn name(&self) -> &str {
        self.manifest.name()
    }

    pub fn height(&self) -> u64 {
        self.manifest.height()
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The link of the currently persisted manifest blob.
    pub fn manifest_link(&self) -> &Link {
        &self.manifest_link
    }

    /// Non-Optional FS accessor — by construction every `Vault` has
    /// a loaded, decrypted working copy.
    pub fn fs(&self) -> &Fs<B> {
        &self.fs
    }

    pub fn log(&self) -> &L {
        &self.log
    }

    pub fn blobs(&self) -> &B {
        self.fs.blobs().inner()
    }

    /// Current canonical head + height from the log.
    pub async fn head(&self) -> Result<Head, VaultLogError<L::Error>> {
        self.log.head(self.id(), None).await
    }

    /// Whether the log knows this vault at all.
    pub async fn exists(&self) -> Result<bool, VaultLogError<L::Error>> {
        self.log.exists(self.id()).await
    }

    // -- Save --

    /// Persist a new version of the vault.
    ///
    /// Generates a fresh vault secret, asks the tree to encrypt
    /// itself under it ([`Fs::save_tree`]), re-mints every share's
    /// [`SecretShare`] against the new secret, refreshes the
    /// auto-publish set, signs the manifest with `private_key`,
    /// writes the manifest blob, and appends the new entry to the
    /// log.
    pub async fn save(&mut self) -> Result<Link, VaultError<L::Error>> {
        let new_secret = Secret::generate();
        let previous_link = self.manifest_link.clone();
        let new_height = self.manifest.height() + 1;
        let prior_root_hash = self.manifest.root().hash();

        let tree = self
            .fs
            .save_tree(prior_root_hash, new_secret.clone())
            .await?;

        // Re-mint each share's encrypted secret against the new vault
        // secret. The pubkey is the BTreeMap key.
        for (pubkey, share) in self.manifest.shares_mut().iter_mut() {
            let secret_share = SecretShare::new(&new_secret, pubkey).map_err(FsError::from)?;
            share.set_secret_share(secret_share);
        }

        self.manifest.set_previous(previous_link.clone());
        self.manifest.set_root(tree.root_link.clone());
        self.manifest.set_height(new_height);
        self.manifest.set_metadata(tree.metadata);
        self.manifest.set_ops(tree.ops_log_link);
        self.manifest.set_ops_clock(tree.ops_clock);

        // Pins: tree-level pins + the previous manifest link.
        let mut pins = tree.pins;
        pins.insert(previous_link.hash());
        self.manifest.set_pins(pins);

        self.manifest.sign(&self.private_key)?;
        let new_link = self.fs.blobs().inner().put_cbor(&self.manifest).await?;

        self.log
            .append(
                self.id(),
                self.manifest.name().to_string(),
                new_link.clone(),
                Some(previous_link),
                new_height,
            )
            .await?;

        self.manifest_link = new_link.clone();
        Ok(new_link)
    }

    /// Fast-forward this handle to the log's current head.
    ///
    /// A long-lived open vault (a browser tab, a mount) goes stale when
    /// another device advances the head — its next [`Self::save`] would
    /// chain off the old head and be rejected (the hub answers 409).
    /// `refresh` re-checks the log and, when the head moved,
    /// re-materialises the tree at it. Returns `true` when the vault
    /// changed, `false` when it was already current.
    ///
    /// Staged, unsaved mutations are discarded on a refresh that
    /// changes the vault — save first, or replay them after.
    pub async fn refresh(&mut self) -> Result<bool, VaultError<L::Error>> {
        let head = self.log.head(self.id, None).await?;
        if head.link == self.manifest_link {
            return Ok(false);
        }

        let blobs = self.fs.blobs().inner().clone();
        let manifest: Manifest = blobs.get_cbor(&head.link).await?;

        let owner_pubkey = self.private_key.public();
        let share = manifest
            .get_share(&owner_pubkey)
            .ok_or(crate::fs::FsError::ShareNotFound)?;
        let secret = share
            .secret_share()
            .recover(&self.private_key)
            .map_err(FsError::from)?;

        self.fs = Fs::load_tree(
            manifest.root(),
            secret,
            manifest.metadata().clone(),
            manifest.pins().clone(),
            manifest.ops_clock(),
            owner_pubkey,
            blobs,
        )
        .await?;
        self.manifest = manifest;
        self.manifest_link = head.link;
        Ok(true)
    }

    // -- Manifest mutations --

    /// Grant `pubkey` direct access to this vault (dialed as an iroh
    /// peer). Persisted on the next [`Self::save`].
    #[allow(clippy::result_large_err)]
    pub fn add_share(&mut self, pubkey: PublicKey) -> Result<(), VaultError<L::Error>> {
        self.add_share_via(pubkey, None)
    }

    /// Grant `client` access, reached through `via` (the always-on host
    /// for a hosted client such as a browser) or directly when `via` is
    /// `None`. The secret is sealed to `client`; the host never holds it.
    /// Persisted on the next [`Self::save`].
    #[allow(clippy::result_large_err)]
    pub fn add_share_via(
        &mut self,
        client: PublicKey,
        via: Option<Did>,
    ) -> Result<(), VaultError<L::Error>> {
        // Placeholder secret share — re-minted against the live vault
        // secret on the next `save`.
        let secret_share = SecretShare::new(&Secret::default(), &client).map_err(FsError::from)?;
        self.manifest.add_share(
            client,
            Share::new(secret_share, Did::from_key(&client), via),
        );
        Ok(())
    }

    /// Grant access from a resolved [`zim_did::Reach`] — the unit
    /// [`zim_did::resolve_reaches`] returns. Seals to `reach.client`,
    /// reached via `reach.via` (the host). One call per device when a
    /// `did:web` expands to several.
    #[allow(clippy::result_large_err)]
    pub fn add_reach(&mut self, reach: zim_did::Reach) -> Result<(), VaultError<L::Error>> {
        let via = reach.via.map(|(_, host_key)| Did::from_key(&host_key));
        self.add_share_via(reach.client, via)
    }

    /// Revoke `pubkey`'s share. Errors when the local peer isn't
    /// itself a shareholder (only members can edit the share list).
    #[allow(clippy::result_large_err)]
    pub fn remove_share(&mut self, pubkey: PublicKey) -> Result<(), VaultError<L::Error>> {
        let our_key = self.private_key.public();
        if self.manifest.get_share(&our_key).is_none() {
            return Err(crate::fs::FsError::ShareNotFound.into());
        }
        if self.manifest.shares_mut().remove(&pubkey).is_none() {
            return Err(crate::fs::FsError::ShareNotFound.into());
        }
        Ok(())
    }

    /// Revoke a hosted client's share (the relay recipient). Errors when
    /// the local peer isn't itself a shareholder.
    #[allow(clippy::result_large_err)]
    pub fn remove_relay(&mut self, recipient: PublicKey) -> Result<bool, VaultError<L::Error>> {
        let our_key = self.private_key.public();
        if self.manifest.get_share(&our_key).is_none() {
            return Err(crate::fs::FsError::ShareNotFound.into());
        }
        Ok(self.manifest.shares_mut().remove(&recipient).is_some())
    }

    // -- History --

    /// Walk the log backward from `from` (or current head) and
    /// return up to `limit` entries.
    ///
    /// Plaintext metadata only — no decryption, so relays and the
    /// browser walk history the same way.
    pub async fn history(
        &self,
        from: Option<u64>,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, VaultError<L::Error>> {
        let top = match from {
            Some(h) => h,
            None => self.log.height(self.id()).await?,
        };
        let stop = top.saturating_sub(limit as u64);
        let mut out = Vec::with_capacity(limit);
        let mut h = top;
        loop {
            let heads = self.log.heads(self.id(), h).await?;
            if let Some(link) = heads.into_iter().max() {
                out.push(HistoryEntry { height: h, link });
            }
            if h == 0 || out.len() >= limit || h <= stop {
                break;
            }
            h -= 1;
        }
        Ok(out)
    }
}
