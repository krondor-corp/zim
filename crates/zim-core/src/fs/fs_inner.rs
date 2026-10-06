use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::lock::Mutex;

use crate::blobs::{BlobError, BlobStore};
use crate::linked_data::{BlockEncoded, CodecError, Hash, Link};
use zim_crypto::{EntryId, EntryRatchet, PublicKey, SecretError};

use super::abs_path::AbsPath;
use super::content_store::{ContentError, Metadata};
use super::crdt::{OpKind, OpsLog};
use super::entry::{Dir, Entry};
use super::manifest::ManifestError;
use super::pins::Pins;
use super::share::ShareError;

/// The mutable state inside an [`Fs`]: the decrypted root [`Dir`], the
/// pending [`OpsLog`], the pin set, and the local peer's public key.
///
/// As of the Vault-split refactor this is tree-only state. The manifest
/// (id, name, height, shares, relays, signature, …), the manifest link,
/// and the local peer's *private* key all live one level up on
/// [`Vault`](crate::vault::Vault). The Vault projects its manifest's
/// root link into an Fs via [`Fs::load_tree`] / [`Fs::init_tree`] and
/// reads back the tree state via [`Fs::save_tree`].
///
/// Cloned out of the [`Fs`] mutex by [`Fs::inner`] when callers need a
/// snapshot.
#[derive(Clone)]
pub struct FsInner {
    /// The decrypted root directory. `set_entry_at_path` cascades up
    /// into this; `save_tree` puts it into the metadata pack to mint
    /// the new root link.
    pub root: Dir,
    /// Pinned content hashes — the file blobs the tree currently
    /// references. Seeded from the manifest's file pins at load time
    /// ([`Manifest::file_pins`](super::Manifest::file_pins)); at save
    /// time the manifest's pin set is this plus the version's ops-log
    /// blob and previous manifest.
    pub pins: Pins,
    /// In-memory log of ops issued since the last load. Seeded with the
    /// persisted Lamport clock so newly recorded ops stay monotonic
    /// across save / load cycles.
    pub ops_log: OpsLog,
    /// Local peer's public key (the author of any newly recorded ops).
    pub public_key: PublicKey,
    /// Hashes of file blobs and dir bodies minted since the last save.
    /// None of them is in any manifest yet, so a rewrite of one must
    /// not chain `previous` to it — nobody could fetch or decrypt it.
    /// Cleared by [`Fs::save_tree`].
    pub unsaved: HashSet<Hash>,
}

/// The key schedule and lineage pointer for a rewrite of `old`.
///
/// If `old` was saved, this is the next revision: advance its ratchet
/// and chain to its link. If `old` is unsaved, it never became a
/// revision — nobody can fetch or decrypt it — so the rewrite *is* the
/// revision `old` was going to be: keep its ratchet (one key per saved
/// revision; every encryption draws a fresh nonce, so reuse within a
/// session is safe) and inherit the pointer it carried. Ratchet height
/// and lineage therefore both count saved revisions. See
/// [`FsInner::unsaved`].
fn next_revision(unsaved: &HashSet<Hash>, old: &Entry) -> (EntryRatchet, Option<Link>) {
    if unsaved.contains(&old.link().hash()) {
        (old.ratchet().clone(), old.previous().cloned())
    } else {
        (old.ratchet().advanced(), Some(old.link().clone()))
    }
}

impl FsInner {
    /// The decrypted root directory.
    pub fn root(&self) -> &Dir {
        &self.root
    }
    /// The current pin set.
    pub fn pins(&self) -> &Pins {
        &self.pins
    }
    /// In-memory ops log (ops issued since the last load).
    pub fn ops_log(&self) -> &OpsLog {
        &self.ops_log
    }
    /// The local peer's public key.
    pub fn peer_id(&self) -> &PublicKey {
        &self.public_key
    }
}

use super::content_store::ContentStore;

/// The in-memory filesystem implementation. Not a vault — a vault is
/// the versioned, signed, shareable entity that owns this filesystem
/// (one [`Fs`] per opened vault version).
///
/// Combines:
///
/// - the mutable [`FsInner`] (manifest + root + ops + keypair), behind
///   an `Arc<Mutex<…>>` so the type is cheap to clone and pass across
///   tasks;
/// - a [`ContentStore`] for storage operations (encrypted dir bodies
///   in the metadata pack; encrypted file content in the inner blob
///   store).
///
/// Construct via [`Fs::init_tree`] (fresh empty root) or
/// [`Fs::load_tree`] (decrypt the root at a known link). Mutate with
/// [`Fs::add`], [`Fs::mkdir`], [`Fs::rm`], [`Fs::mv`]. Persist the
/// tree under a new secret with [`Fs::save_tree`]; the caller
/// (`zim_core::vault::Vault`) takes the returned [`TreeSaveOutput`]
/// and assembles the new manifest from it.
#[derive(Clone)]
pub struct Fs<B: BlobStore>(Arc<Mutex<FsInner>>, ContentStore<B>);

/// Raw tree state returned by [`Fs::save_tree`]. Everything
/// [`Vault::save`](crate::vault::Vault) needs to mint + sign a new
/// manifest.
#[derive(Debug, Clone)]
pub struct TreeSaveOutput {
    /// Link to the freshly-encrypted root dir body in the metadata
    /// tier.
    pub root_link: Link,
    /// Link to the encrypted ops-log blob, or [`Link::default`] if
    /// no ops were issued this session.
    pub ops_log_link: Link,
    /// Lamport clock at save time. Caller writes this to
    /// `manifest.set_ops_clock`.
    pub ops_clock: u64,
    /// Snapshot of every dir body in the metadata pack — exactly the
    /// live set after the post-mutation evictions this session
    /// performed plus the new root's body.
    pub metadata: Metadata,
    /// File blob hashes the tree currently references, plus
    /// `ops_log_link.hash()` when an ops-log blob was written.
    /// Caller layers in the previous manifest hash before writing
    /// the manifest blob.
    pub pins: Pins,
}

/// Errors raised by [`Fs`] methods.
///
/// Two categories:
///
/// - **Fs-domain conditions** the caller can react to:
///   [`Self::PathNotFound`], [`Self::CannotMutate`].
/// - **Backing-layer failures** — every error from beneath the fs layer
///   (blob I/O, crypto, codec, signature verification, log backend,
///   content-store) folds into [`Self::Backing`]. Callers can't
///   usefully distinguish them at this surface; every match site in
///   the workspace currently treats `Fs(_)` the same. The original
///   concrete error is retrievable via
///   [`std::error::Error::source`].
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    /// A path segment doesn't resolve in the live tree.
    #[error("path not found: {0}")]
    PathNotFound(AbsPath),
    /// The requested mutation can't be performed at the given path —
    /// "no no don't do that." Covers:
    /// - destination already exists with an incompatible kind
    ///   (`add /a` over a dir, `mkdir /a` over a file, `mv X /a` over a dir);
    /// - traversal through a file (`/foo/bar` where `/foo` is a file);
    /// - root mutations (`rm /`, `mkdir /`, `mv / …`);
    /// - self-containing moves (`mv /a /a/b`).
    ///
    /// The path is the one the rejection is about; the string carries
    /// the human-readable reason.
    #[error("cannot mutate {0}: {1}")]
    CannotMutate(AbsPath, String),
    /// A failure from a layer beneath fs: [`BlobError`], [`SecretError`],
    /// [`CodecError`], [`ManifestError`], [`ShareError`], or a
    /// bucket-log backend error. The original error is preserved
    /// inside the [`anyhow::Error`] and walkable via
    /// [`std::error::Error::source`].
    #[error(transparent)]
    Backing(#[from] anyhow::Error),
}

// Route each backing-layer error type into `FsError::Backing` so
// `?` keeps working from any of these.
macro_rules! impl_backing_from {
    ($($err:ty),* $(,)?) => {
        $(
            impl From<$err> for FsError {
                fn from(e: $err) -> Self {
                    FsError::Backing(anyhow::anyhow!(e))
                }
            }
        )*
    };
}
impl_backing_from!(
    BlobError,
    SecretError,
    CodecError,
    ManifestError,
    ContentError,
    ShareError,
);

impl<B: BlobStore> Fs<B> {
    /// Snapshot of the current [`FsInner`]. Acquires the inner mutex
    /// briefly to clone; callers get an owned copy and don't hold the
    /// lock past this method.
    pub async fn inner(&self) -> FsInner {
        self.0.lock().await.clone()
    }

    /// The [`ContentStore`] this `Fs` was built over.
    pub fn blobs(&self) -> &ContentStore<B> {
        &self.1
    }

    /// Persist the current tree under `new_secret`. Returns the
    /// raw artefacts [`Vault::save`](crate::vault::Vault) needs to
    /// assemble + sign the new manifest:
    ///
    /// - `root_link` — link to the freshly-encrypted root dir body
    /// - `ops_log_link` — link to the encrypted ops-log blob, or
    ///   [`Link::default`] when no ops were issued this session
    /// - `ops_clock` — Lamport clock at save time
    /// - `metadata` — snapshot of every dir body in the metadata pack
    /// - `pins` — the live file blob hashes + THIS version's ops-log
    ///   blob hash (when present). Caller (Vault) layers in the previous
    ///   manifest hash before writing the manifest blob.
    ///
    /// The previous root's dir body is evicted from the metadata
    /// pack so the snapshot is exactly the live set. Mid-session
    /// mutations (`add` / `rm` / `mv` / `set_entry_at_path`) already
    /// keep the live pin set in sync; this method does not touch it.
    /// The ops-log hash goes only into the returned set: each version's
    /// ops log is pinned by that version's manifest, and sync fetches
    /// every manifest in the chain with its own pins, so carrying old
    /// ops logs forward would only make every pin set grow forever.
    pub async fn save_tree(
        &self,
        prior_root_hash: Hash,
        root_ratchet: &EntryRatchet,
    ) -> Result<TreeSaveOutput, FsError> {
        let blobs = &self.1;
        let (root_dir, mut pins, ops_log) = {
            let inner = self.0.lock().await;
            (
                inner.root.clone(),
                inner.pins.clone(),
                inner.ops_log.clone(),
            )
        };

        // Put the root with the new secret. Evict the prior root's
        // dir body so the metadata snapshot below contains only the
        // live set.
        let root_link = blobs.put_dir(&root_ratchet.key(), &root_dir)?;
        if root_link.hash() != prior_root_hash {
            blobs.evict(&prior_root_hash);
        }

        let ops_log_link = if !ops_log.is_empty() {
            // Ops-log ciphertext isn't fs-shaped — reach past the
            // content store directly to the underlying blob store.
            let encrypted = root_ratchet.key().encrypt(&ops_log.encode()?)?;
            let link = blobs.inner().put_raw(encrypted).await?;
            pins.insert(link.hash());
            link
        } else {
            Link::default()
        };

        let metadata = blobs.snapshot_metadata();
        let ops_clock = ops_log.clock();

        // Update internal state: ops_log is now persisted; future
        // operations start a fresh log but preserve the clock so new
        // ops stay monotonic. Everything minted this session is now in
        // a manifest, so rewrites may chain to it.
        {
            let mut inner = self.0.lock().await;
            inner.ops_log.clear_preserving_clock();
            inner.unsaved.clear();
        }

        Ok(TreeSaveOutput {
            root_link,
            ops_log_link,
            ops_clock,
            metadata,
            pins,
        })
    }

    /// Build a fresh, empty tree under `secret`.
    ///
    /// Stages an empty root dir in the metadata tier and returns the
    /// freshly-constructed `Fs` alongside the link to that root dir
    /// body. The caller (typically
    /// [`Vault::init`](crate::vault::Vault)) takes the root link and
    /// uses it to mint the genesis manifest; the same secret is the
    /// one whose [`Share`](crate::fs::Share) the manifest will encrypt
    /// to the owner.
    ///
    /// Tree-only — no manifest, no log, no signing. The vault layer
    /// handles all of that.
    pub async fn init_tree(
        owner_pubkey: PublicKey,
        root_ratchet: &EntryRatchet,
        blobs: B,
    ) -> Result<(Self, Link), FsError> {
        let blobs = ContentStore::new(blobs, Metadata::new());
        let root = Dir::default();
        let root_link = blobs.put_dir(&root_ratchet.key(), &root)?;
        let fs = Fs(
            Arc::new(Mutex::new(FsInner {
                root,
                pins: Pins::default(),
                ops_log: OpsLog::new(),
                public_key: owner_pubkey,
                unsaved: HashSet::new(),
            })),
            blobs,
        );
        Ok((fs, root_link))
    }

    /// Decrypt a tree at `root_link` using `secret`.
    ///
    /// The caller (`Vault::open` / `Vault::open_with_head`) has
    /// already fetched the manifest, recovered the vault secret from
    /// its own `Share`, and pulled out the values this method needs:
    /// the metadata pack, the live pin set, the persisted Lamport
    /// clock, and the owner's pubkey.
    ///
    /// `Fs::load_tree` is the inverse of [`Fs::save_tree`] — give it
    /// the artefacts that came out of a prior save (or off the head
    /// manifest) and you get back the in-memory tree.
    pub async fn load_tree(
        root_link: &Link,
        root_ratchet: EntryRatchet,
        metadata: Metadata,
        pins: Pins,
        ops_clock: u64,
        owner_pubkey: PublicKey,
        blobs: B,
    ) -> Result<Self, FsError> {
        let blobs = ContentStore::new(blobs, metadata);
        // The root has no `Entry`: its link is the manifest's, its key the
        // vault's, and its history the manifest chain.
        let root = blobs.get_dir(root_link, &root_ratchet.key()).await?;
        let ops_log = OpsLog::with_clock(ops_clock);
        Ok(Fs(
            Arc::new(Mutex::new(FsInner {
                root,
                pins,
                ops_log,
                public_key: owner_pubkey,
                unsaved: HashSet::new(),
            })),
            blobs,
        ))
    }

    /// Write a file at `path`, streaming `data` through per-file
    /// encryption into the inner blob store.
    ///
    /// POSIX overwrite semantics:
    ///
    /// - if `path` doesn't exist: creates it;
    /// - if `path` is an existing file: overwrites it (the old blob's
    ///   pin is dropped once the new write commits);
    /// - if `path` is a directory: errors with
    ///   [`FsError::CannotMutate`] — `open()` can't write a directory.
    ///
    /// Parent directories must already exist; this method doesn't auto-
    /// create them. Records an [`OpKind::AddFile`] in the ops log.
    pub async fn add<R>(&self, path: &AbsPath, data: R) -> Result<(), FsError>
    where
        R: Read + Send + Sync + 'static + Unpin,
    {
        // POSIX semantics:
        // - dest doesn't exist: proceed
        // - dest is a file: overwrite. Note the prior blob so we can drop
        //   its pin once the new write commits.
        // - dest is a directory: refuse — open() can't write a directory.
        let prior = match self.get_entry_at_path(path).await? {
            None => None,
            Some(old @ Entry::File { .. }) => Some(old),
            Some(Entry::Dir { .. }) => {
                return Err(FsError::CannotMutate(
                    path.clone(),
                    "path already exists".into(),
                ))
            }
        };

        // Key schedule (identity rides inside it). A rewrite is the SAME
        // entity at its next saved revision — see `next_revision`. A
        // create seeds a fresh entity.
        let (ratchet, previous) = match &prior {
            Some(old) => next_revision(&self.0.lock().await.unsaved, old),
            None => (EntryRatchet::seed(), None),
        };
        let secret = ratchet.key();

        // Stream encryption + storage. `put_file` tees the plaintext
        // through `blake3` on the way in, so we get the ciphertext
        // link *and* `blake3(plaintext)` in one pass without ever
        // fully materializing the body.
        let (link, plaintext_hash) = self.1.put_file(&secret, Box::new(data)).await?;

        self.add_tree(
            path,
            link.clone(),
            ratchet.clone(),
            previous.clone(),
            plaintext_hash,
        )
        .await?;

        {
            let mut inner = self.0.lock().await;
            inner.unsaved.insert(link.hash());
            let peer_id = inner.public_key;
            inner.ops_log.record(
                peer_id,
                OpKind::AddFile {
                    path: path.clone(),
                    content: link,
                    plaintext_hash,
                    ratchet,
                    previous: previous.unwrap_or_default(),
                },
            );
        }

        Ok(())
    }

    /// `add` minus the op-log record. Builds the [`Entry`], rebuilds the
    /// spine, pins `link`, unpins the file blob it overwrote (if any),
    /// and updates the root. Used by local writes and by op replay
    /// alike, so a replayed overwrite drops the old pin exactly as a
    /// local one does; a replayed op's `content` is the writer's and is
    /// not marked unsaved here — the writer's manifest pins it.
    async fn add_tree(
        &self,
        path: &AbsPath,
        link: Link,
        ratchet: EntryRatchet,
        previous: Option<Link>,
        plaintext_hash: Hash,
    ) -> Result<(), FsError> {
        let overwritten = match self.get_entry_at_path(path).await? {
            Some(Entry::File { link: old, .. }) => Some(old.hash()),
            _ => None,
        };
        let entry = Entry::file(link.clone(), ratchet, previous, path, plaintext_hash);
        let new_root = self.set_entry_at_path(entry, path).await?;
        let mut inner = self.0.lock().await;
        // Same hash means same content: the pin stays.
        if let Some(old) = overwritten.filter(|old| *old != link.hash()) {
            inner.pins.remove(&old);
        }
        inner.pins.insert(link.hash());
        inner.root = new_root;
        Ok(())
    }

    /// Remove the entry at `path`. Errors:
    ///
    /// - [`FsError::CannotMutate`] if `path` is the root, or if the path
    ///   traverses through a file.
    /// - [`FsError::PathNotFound`] if `path` doesn't exist.
    ///
    /// Every file blob under `path` is unpinned and, if `path` is a
    /// directory, every dir body in the removed subtree is evicted from
    /// the metadata pack: a version's pins describe what THAT version
    /// needs. The blobs themselves stay in the inner store — earlier
    /// manifests still pin them. Records an [`OpKind::Remove`] in the
    /// ops log.
    pub async fn rm(&self, path: &AbsPath) -> Result<(), FsError> {
        // Capture the entity before it's gone so replay can follow a
        // concurrent rename to it.
        let id = self
            .get_entry_at_path(path)
            .await?
            .map(|e| e.id())
            .ok_or_else(|| FsError::PathNotFound(path.clone()))?;
        let is_dir = self.rm_tree(path).await?;
        let mut inner = self.0.lock().await;
        let peer_id = inner.public_key;
        inner.ops_log.record(
            peer_id,
            OpKind::Remove {
                path: path.clone(),
                is_dir,
                id,
            },
        );
        Ok(())
    }

    /// `rm` minus the op-log record. Internal: used by both [`Self::rm`]
    /// (which adds the op) and [`Self::apply_ops`] (which replays an
    /// already-recorded op). Returns whether the removed entry was a
    /// directory.
    async fn rm_tree(&self, path: &AbsPath) -> Result<bool, FsError> {
        let (abs_parent, file_name) = path.split().ok_or(FsError::CannotMutate(
            AbsPath::root(),
            "cannot remove the root directory".into(),
        ))?;
        let mut parent_dir = self.get_dir_at_path(&abs_parent).await?;

        let removed_entry = parent_dir
            .remove(&file_name)
            .ok_or_else(|| FsError::PathNotFound(path.clone()))?;
        // Everything the removed subtree referenced is no longer this
        // version's to pin: dir bodies are evicted from the metadata
        // pack, file blobs are unpinned.
        let mut dir_bodies: HashSet<Hash> = HashSet::new();
        let mut file_blobs: HashSet<Hash> = HashSet::new();
        let is_dir = match &removed_entry {
            Entry::Dir { link, ratchet, .. } => {
                dir_bodies.insert(link.hash());
                let removed_dir = self.1.get_dir(link, &ratchet.key()).await?;
                Self::_collect_subtree_hashes(
                    &removed_dir,
                    &self.1,
                    &mut dir_bodies,
                    &mut file_blobs,
                )
                .await?;
                true
            }
            Entry::File { link, .. } => {
                file_blobs.insert(link.hash());
                false
            }
        };
        self.1.evict_many(&dir_bodies);

        self.set_dir_at_path(&abs_parent, parent_dir, None).await?;
        let mut inner = self.0.lock().await;
        for hash in &file_blobs {
            inner.pins.remove(hash);
        }
        Ok(is_dir)
    }

    /// Create a directory at `path`. POSIX semantics:
    ///
    /// - `parents == false` (`mkdir`): the parent must already exist.
    ///   Missing parent returns `PathNotFound`.
    /// - `parents == true` (`mkdir -p`): missing ancestors are created
    ///   on the fly, and the call is idempotent against existing dirs.
    ///
    /// In both modes: a pre-existing dir at `path` is a no-op; a
    /// pre-existing file at `path` is an error.
    pub async fn mkdir(&self, path: &AbsPath, parents: bool) -> Result<(), FsError> {
        // Mint the new directory's identity + key schedule up front so
        // the op ships them and every replaying peer builds the same
        // entity. Discarded if the dir already existed.
        let ratchet = EntryRatchet::seed();
        if self
            .mkdir_tree(path, parents, Some(ratchet.clone()))
            .await?
        {
            let mut inner = self.0.lock().await;
            let peer_id = inner.public_key;
            inner.ops_log.record(
                peer_id,
                OpKind::Mkdir {
                    path: path.clone(),
                    ratchet,
                },
            );
        }
        Ok(())
    }

    /// `mkdir` minus the op-log record. Returns `true` when a new dir was
    /// staged, `false` when `path` already resolved to a directory (the
    /// idempotent no-op).
    async fn mkdir_tree(
        &self,
        path: &AbsPath,
        parents: bool,
        ratchet: Option<EntryRatchet>,
    ) -> Result<bool, FsError> {
        let (abs_parent, dir_name) = path.split().ok_or(FsError::CannotMutate(
            AbsPath::root(),
            "cannot create the root directory".into(),
        ))?;

        // Look up the parent. Under `-p`, synthesize an empty Dir if the
        // chain is missing — `set_entry_at_path` will weave it in.
        let parent_dir = match self.get_dir_at_path(&abs_parent).await {
            Ok(dir) => dir,
            Err(FsError::PathNotFound(_)) if parents => Dir::default(),
            Err(err) => return Err(err),
        };

        // POSIX `mkdir -p` semantics: no-op if the path already resolves
        // to a directory, error if it resolves to a file.
        match parent_dir.get(&dir_name) {
            Some(Entry::Dir { .. }) => return Ok(false),
            Some(Entry::File { .. }) => {
                return Err(FsError::CannotMutate(
                    path.clone(),
                    "path already exists".into(),
                ))
            }
            None => {}
        }

        self.set_dir_at_path(path, Dir::default(), ratchet).await?;
        Ok(true)
    }

    /// Move or rename a path from `from` to `to`.
    ///
    /// The moved [`Entry`] is reused at the destination — file content
    /// and any subtrees underneath aren't re-encrypted, so moves are
    /// O(depth) in the tree regardless of subtree size.
    ///
    /// POSIX-ish overwrite semantics at the destination:
    ///
    /// - `to` doesn't exist: proceeds.
    /// - `to` is a file: overwrites it (the old file's pin is dropped
    ///   once the move commits).
    /// - `to` is a directory: errors with [`FsError::CannotMutate`].
    ///
    /// Errors:
    ///
    /// - [`FsError::CannotMutate`] — moving the root, moving a path
    ///   into its own subtree (`mv /a /a/b`), or overwriting a dir.
    /// - [`FsError::PathNotFound`] — `from` doesn't exist.
    ///
    /// Records an [`OpKind::Mv`] in the ops log.
    pub async fn mv(&self, from: &AbsPath, to: &AbsPath) -> Result<(), FsError> {
        self.mv_tree(from, to).await?;
        // The entity now at `to` is the one we moved; ship its id so
        // replaying peers can redirect concurrent edits to follow it.
        let id = self
            .get_entry_at_path(to)
            .await?
            .map(|e| e.id())
            .ok_or_else(|| {
                FsError::CannotMutate(to.clone(), "moved entity has no identity".into())
            })?;
        let mut inner = self.0.lock().await;
        let peer_id = inner.public_key;
        inner.ops_log.record(
            peer_id,
            OpKind::Mv {
                from: from.clone(),
                to: to.clone(),
                id,
            },
        );
        Ok(())
    }

    /// `mv` minus the op-log record. The full validation + tree
    /// manipulation pipeline: structural-invariant checks, source
    /// lookup, destination-overwrite handling, source removal,
    /// destination insertion, prior-pin drop.
    async fn mv_tree(&self, from: &AbsPath, to: &AbsPath) -> Result<(), FsError> {
        // Root is not movable.
        if from.relative().parent().is_none() {
            return Err(FsError::CannotMutate(
                AbsPath::root(),
                "cannot move the root directory".into(),
            ));
        }

        // Refuse moves that put a path inside its own subtree.
        if to.starts_with(from) {
            return Err(FsError::CannotMutate(
                to.clone(),
                format!("destination is inside source '{from}'"),
            ));
        }

        // STEP 1: Source entry. Reused at the destination so file content
        // and subtrees stay encrypted-as-is.
        let entry = self
            .get_entry_at_path(from)
            .await?
            .ok_or_else(|| FsError::PathNotFound(from.clone()))?;

        // STEP 2: Destination — POSIX overwrite semantics. Dropping the
        // prior file's pin is deferred until after the move commits.
        let prior_dest_file_hash = match self.get_entry_at_path(to).await? {
            None => None,
            Some(Entry::File { link, .. }) => Some(link.hash()),
            Some(Entry::Dir { .. }) => {
                return Err(FsError::CannotMutate(
                    to.clone(),
                    "path already exists".into(),
                ));
            }
        };

        // STEP 3: Detach source from its parent.
        let (abs_parent, file_name) = from.split().ok_or(FsError::CannotMutate(
            AbsPath::root(),
            "cannot move the root directory".into(),
        ))?;
        let mut parent_dir = self.get_dir_at_path(&abs_parent).await?;
        if parent_dir.remove(&file_name).is_none() {
            return Err(FsError::PathNotFound(from.clone()));
        }
        self.set_dir_at_path(&abs_parent, parent_dir, None).await?;

        // STEP 4: Attach the entry at the destination.
        let new_root = self.set_entry_at_path(entry, to).await?;

        // STEP 5: Commit and drop any prior destination pin.
        let mut inner = self.0.lock().await;
        inner.root = new_root;
        if let Some(prior) = prior_dest_file_hash {
            inner.pins.remove(&prior);
        }
        Ok(())
    }

    /// List the immediate children of the directory at `path`.
    ///
    /// Keys are paths relative to the vault root (e.g.
    /// `ls("/a")` returns entries keyed `"a/foo"`, `"a/bar"`).
    /// Errors with [`FsError::PathNotFound`] or [`FsError::CannotMutate`]
    /// via [`Self::get_dir_at_path`].
    pub async fn ls(&self, path: &AbsPath) -> Result<BTreeMap<PathBuf, Entry>, FsError> {
        let mut items = BTreeMap::new();
        let rel = path.relative().to_path_buf();
        let dir = self.get_dir_at_path(path).await?;

        for (name, entry) in dir.entries() {
            let mut full_path = rel.clone();
            full_path.push(name);
            items.insert(full_path, entry.clone());
        }

        Ok(items)
    }

    /// Recursive [`Self::ls`]. Returns every file and directory under
    /// `path`, keyed by its path relative to `path` itself (so an
    /// `ls_deep("/a")` over a tree `/a/b/c.txt` yields `"b"` and
    /// `"b/c.txt"`). Subtrees are walked depth-first via the metadata
    /// pack — no inner-store I/O.
    pub async fn ls_deep(&self, path: &AbsPath) -> Result<BTreeMap<PathBuf, Entry>, FsError> {
        let base_path = path.relative().to_path_buf();
        self._ls_deep(path, &base_path).await
    }

    async fn _ls_deep(
        &self,
        path: &Path,
        base_path: &Path,
    ) -> Result<BTreeMap<PathBuf, Entry>, FsError> {
        let mut all_items = BTreeMap::new();

        // get the initial items at the given path
        let items = self.ls(&AbsPath::from_abs(path.to_path_buf())).await?;

        for (item_path, link) in items {
            // Make path relative to the base_path
            let relative_path = if base_path == Path::new("") {
                item_path.clone()
            } else {
                item_path
                    .strip_prefix(base_path)
                    .unwrap_or(&item_path)
                    .to_path_buf()
            };
            all_items.insert(relative_path.clone(), link.clone());

            if link.is_dir() {
                // Recurse using the absolute path
                let abs_item_path = Path::new("/").join(&item_path);
                let sub_items = Box::pin(self._ls_deep(&abs_item_path, base_path)).await?;

                // Sub items already have correct relative paths from base_path
                for (sub_path, sub_link) in sub_items {
                    all_items.insert(sub_path, sub_link);
                }
            }
        }

        Ok(all_items)
    }

    /// Read the file at `path` into memory, decrypting on the fly.
    ///
    /// Errors:
    ///
    /// - [`FsError::PathNotFound`] — `path` doesn't exist.
    /// - [`FsError::CannotMutate`] — `path` is a directory.
    /// - [`FsError::Backing`] — any underlying I/O or crypto failure.
    ///
    /// For large files prefer fetching the [`Entry`] yourself and
    /// streaming its link + key through [`ContentStore::get_file`] —
    /// `cat` buffers the whole plaintext.
    pub async fn cat(&self, path: &AbsPath) -> Result<Vec<u8>, FsError> {
        let (abs_parent, file_name) = path
            .split()
            .ok_or_else(|| FsError::PathNotFound(path.clone()))?;
        let parent_dir = self.get_dir_at_path(&abs_parent).await?;

        let entry = parent_dir
            .get(&file_name)
            .ok_or_else(|| FsError::PathNotFound(path.clone()))?;

        match entry {
            Entry::File { link, ratchet, .. } => {
                let mut reader = self.1.get_file(link, &ratchet.key()).await?;
                let mut data = Vec::new();
                reader
                    .read_to_end(&mut data)
                    .map_err(|e| FsError::Backing(anyhow::anyhow!(e)))?;
                Ok(data)
            }
            Entry::Dir { .. } => Err(FsError::CannotMutate(
                path.clone(),
                "path is not a directory".into(),
            )),
        }
    }

    /// Get the [`Entry`] at `path`, or `None` if nothing exists there.
    /// Every ancestor dir-body lookup hits the in-memory metadata tier,
    /// so there's no I/O cost worth caching.
    ///
    /// `Ok(None)` covers both "the named entry isn't in its parent dir"
    /// and "a parent dir on the way doesn't exist." `Err` is reserved for
    /// real failures: traversal through a file (`PathNotNode`), blob-fetch
    /// errors, codec/decryption errors.
    pub async fn get_entry_at_path(&self, path: &AbsPath) -> Result<Option<Entry>, FsError> {
        let Some((abs_parent, file_name)) = path.split() else {
            return Ok(None);
        };
        let parent_dir = match self.get_dir_at_path(&abs_parent).await {
            Ok(dir) => dir,
            Err(FsError::PathNotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(parent_dir.get(&file_name).cloned())
    }

    /// Walk `dir`'s subtree, collecting every reachable dir-body hash
    /// into `dir_bodies` and every file blob hash into `file_blobs`.
    /// Recursion is heap-boxed because async fns can't recurse by value.
    /// Used by `rm` to find what a removed subtree referenced.
    fn _collect_subtree_hashes<'a>(
        dir: &'a Dir,
        blobs: &'a ContentStore<B>,
        dir_bodies: &'a mut HashSet<Hash>,
        file_blobs: &'a mut HashSet<Hash>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + 'a>> {
        Box::pin(async move {
            for child in dir.entries().values() {
                match child {
                    Entry::Dir { link, ratchet, .. } => {
                        if dir_bodies.insert(link.hash()) {
                            let child_dir = blobs.get_dir(link, &ratchet.key()).await?;
                            Self::_collect_subtree_hashes(
                                &child_dir, blobs, dir_bodies, file_blobs,
                            )
                            .await?;
                        }
                    }
                    Entry::File { link, .. } => {
                        file_blobs.insert(link.hash());
                    }
                }
            }
            Ok(())
        })
    }

    /// Install `dir` at `path` as the live tree's new state at that
    /// path, and update `inner.root` to reflect the cascade. For root
    /// (`/`), the dir is installed directly — no `put_dir`, since
    /// the root only enters the metadata tier at save time. For deeper
    /// paths, the dir is sealed via
    /// [`ContentStore::put_dir`](super::content_store::ContentStore::put_dir)
    /// and the resulting [`Entry`] is cascaded through
    /// [`Self::set_entry_at_path`].
    pub async fn set_dir_at_path(
        &self,
        path: &AbsPath,
        dir: Dir,
        ratchet: Option<EntryRatchet>,
    ) -> Result<(), FsError> {
        if path.as_ref() == Path::new("/") {
            let mut inner = self.0.lock().await;
            inner.root = dir;
        } else {
            // `ratchet` is Some for a brand-new dir (mkdir, local or
            // replayed — the op carries it). None means we're rewriting
            // an existing dir: same entity, next saved revision (see
            // `next_revision`). Seed only if nothing is there.
            let (ratchet, previous) = match ratchet {
                Some(r) => (r, None),
                None => match self.get_entry_at_path(path).await? {
                    Some(old @ Entry::Dir { .. }) => {
                        next_revision(&self.0.lock().await.unsaved, &old)
                    }
                    _ => (EntryRatchet::seed(), None),
                },
            };
            let link = self.1.put_dir(&ratchet.key(), &dir)?;
            let minted = link.hash();
            let entry = Entry::dir(link, ratchet, previous);
            let new_root = self.set_entry_at_path(entry, path).await?;
            let mut inner = self.0.lock().await;
            inner.root = new_root;
            inner.unsaved.insert(minted);
        }
        Ok(())
    }

    /// Walk the live tree from root to the [`Dir`] at `path`. The root
    /// (`/`) returns the in-memory root unchanged; deeper paths traverse
    /// through the metadata tier. Errors:
    ///
    /// - [`FsError::PathNotFound`] if a segment doesn't exist.
    /// - [`FsError::CannotMutate`] if a segment resolves to a file.
    pub async fn get_dir_at_path(&self, path: &AbsPath) -> Result<Dir, FsError> {
        let mut current_dir = {
            let inner = self.0.lock().await;
            inner.root.clone()
        };
        let mut consumed_path = PathBuf::from("/");

        for part in path.relative().iter() {
            consumed_path.push(part);
            let next = part.to_string_lossy().to_string();
            let next_entry =
                current_dir
                    .get(&next)
                    .ok_or(FsError::PathNotFound(AbsPath::from_abs(
                        consumed_path.clone(),
                    )))?;
            current_dir = match next_entry {
                Entry::Dir { link, ratchet, .. } => self.1.get_dir(link, &ratchet.key()).await?,
                Entry::File { .. } => {
                    return Err(FsError::CannotMutate(
                        AbsPath::from_abs(consumed_path.clone()),
                        "path is not a directory".into(),
                    ))
                }
            };
        }
        Ok(current_dir)
    }

    /// Cascade a `Entry` insertion at `path` up to a new root. Walks the
    /// current tree to gather parent dirs, rebuilds them bottom-up with
    /// fresh secrets (writing each into the metadata tier), evicts each
    /// rebuilt ancestor's old dir-body hash from the metadata pack, and
    /// returns the new root `Dir`.
    ///
    /// Eager eviction is what keeps the metadata tier bounded — each
    /// ancestor we visit is about to be orphaned and we drop it on the
    /// spot, so there's no save-time GC walk to amortize across.
    pub async fn set_entry_at_path(&self, entry: Entry, path: &AbsPath) -> Result<Dir, FsError> {
        let blobs = &self.1;
        let (mut dir, unsaved) = {
            let inner = self.0.lock().await;
            (inner.root.clone(), inner.unsaved.clone())
        };
        let rel = path.relative().to_path_buf();
        // (path, dir, the OLD Entry::Dir for it). The old entry gives the
        // rebuild its identity + ratchet to advance, its link to evict
        // (the body is about to be orphaned), and its lineage to chain
        // as `previous`. `None` for the root (it has no entry; `save`
        // evicts the prior root) and for synthetic ancestors.
        let mut visited_dirs: Vec<(PathBuf, Dir, Option<Entry>)> = Vec::new();
        let mut name = rel.file_name().unwrap().to_string_lossy().to_string();
        let parent_path = rel.parent().unwrap_or(Path::new(""));

        let mut consumed_path = PathBuf::from("/");
        visited_dirs.push((consumed_path.clone(), dir.clone(), None));

        for part in parent_path.iter() {
            let next = part.to_string_lossy().to_string();
            let next_entry = dir.get(&next).cloned();
            if let Some(next_entry) = next_entry {
                consumed_path.push(part);
                match &next_entry {
                    Entry::Dir { link, ratchet, .. } => {
                        dir = blobs.get_dir(link, &ratchet.key()).await?;
                        visited_dirs.push((
                            consumed_path.clone(),
                            dir.clone(),
                            Some(next_entry.clone()),
                        ));
                    }
                    Entry::File { .. } => {
                        return Err(FsError::CannotMutate(
                            AbsPath::from_abs(consumed_path.clone()),
                            "path is not a directory".into(),
                        ));
                    }
                }
            } else {
                dir = Dir::default();
                consumed_path.push(part);
                // Synthetic ancestor — nothing to evict.
                visited_dirs.push((consumed_path.clone(), dir.clone(), None));
            }
        }

        // Bottom-up rebuild. `visited_dirs` always has the root first, so
        // `rev()` processes it last — the final `dir` is the new root.
        let mut entry = entry;
        let mut new_root = None;
        let mut minted: Vec<Hash> = Vec::new();
        for (current_path, mut dir, old_entry) in visited_dirs.into_iter().rev() {
            // If the prior entry under `name` was a Entry::Dir, its
            // dir body is about to be orphaned by the overwrite. (For dir
            // overwrites, the *subtree* underneath is the caller's
            // responsibility — see `Fs::rm`.)
            let prior_target_hash = match dir.get(&name) {
                Some(Entry::Dir { link, .. }) => Some(link.hash()),
                _ => None,
            };
            dir.insert(name.clone(), entry.clone());
            // The root only enters metadata at save-time with the save's
            // secret. Putting it here too would write a hash no one
            // references and immediately orphan it.
            if current_path != Path::new("/") {
                // Same entity, next saved revision (see `next_revision`).
                // Synthetic ancestors seed.
                let (ratchet, previous) = match &old_entry {
                    Some(old) => next_revision(&unsaved, old),
                    None => (EntryRatchet::seed(), None),
                };
                let link = blobs.put_dir(&ratchet.key(), &dir)?;
                minted.push(link.hash());
                entry = Entry::dir(link, ratchet, previous);
                name = current_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
            }
            // Eagerly drop orphan dir bodies. Their parents' children-map
            // entries have just been replaced above; nothing live references
            // them.
            if let Some(h) = prior_target_hash {
                blobs.evict(&h);
            }
            if let Some(e) = &old_entry {
                blobs.evict(&e.link().hash());
            }
            new_root = Some(dir);
        }
        self.0.lock().await.unsaved.extend(minted);
        Ok(new_root.expect("visited_dirs always contains the root"))
    }

    /// Replay a resolved [`OpsLog`] against this filesystem as the base.
    ///
    /// For each op (latest-per-path, in [`OpId`] order):
    ///
    /// - [`OpKind::Mkdir`] — creates the directory (parents synthesized).
    ///   Idempotent against an existing dir at the same path.
    /// - [`OpKind::Remove`] — removes the entry, recursively evicting
    ///   the metadata pack for any removed subtree. Idempotent against
    ///   an already-absent path.
    /// - [`OpKind::Mv`] — moves the entry. Idempotent against an
    ///   already-absent source.
    /// - [`OpKind::AddFile`] — materializes the file. The op is
    ///   self-contained (carries `content` link + per-file `secret`),
    ///   so this works against any local tree state.
    ///
    /// `CannotMutate` errors from the underlying `_tree` methods are
    /// swallowed (they indicate the op's effect is already in place);
    /// any other error propagates.
    ///
    /// After replay, `ops` is merged into the pending [`OpsLog`] so the
    /// next [`Self::save`] persists them on the new version.
    /// `ops` must be the FULL merge window — the local side's ops as well
    /// as the incoming side's, as `chain::merge` always supplies — because
    /// rename-following is tracked across the ops replayed here: a local
    /// `Mv` absent from `ops` would be invisible to the redirect.
    pub async fn apply_ops(&self, ops: &OpsLog) -> Result<(), FsError> {
        // Replay in causal (OpId) order — NOT latest-op-per-path. Ops
        // name entities, and an entity's path can change mid-window via
        // `Mv`. `at` tracks where each entity currently lives according
        // to the ops replayed so far, so an `AddFile` recorded against a
        // path a concurrent rename vacated is redirected to the entity's
        // current path: the edit follows the rename. Per-path collapsing
        // made the outcome depend on filename sort order.
        let mut at: HashMap<EntryId, AbsPath> = HashMap::new();
        for op in ops.operations().values() {
            let path = op.path().clone();
            match &op.kind {
                OpKind::Mkdir { ratchet, .. } => {
                    match self.mkdir_tree(&path, true, Some(ratchet.clone())).await {
                        Ok(_) | Err(FsError::CannotMutate(_, _)) => {}
                        Err(e) => return Err(e),
                    }
                    at.insert(ratchet.id(), path);
                }
                OpKind::Remove { id, .. } => {
                    let target = at.get(id).cloned().unwrap_or(path);
                    match self.rm_tree(&target).await {
                        Ok(_) | Err(FsError::PathNotFound(_)) => {}
                        Err(e) => return Err(e),
                    }
                    at.remove(id);
                }
                OpKind::Mv { from, to, id } => {
                    // If an earlier op in this window already moved the
                    // entity, move it from where it is now.
                    let src = at.get(id).cloned().unwrap_or(from.clone());
                    match self.mv_tree(&src, to).await {
                        Ok(()) | Err(FsError::PathNotFound(_)) => {}
                        Err(e) => return Err(e),
                    }
                    at.insert(*id, to.clone());
                }
                OpKind::AddFile {
                    content,
                    plaintext_hash,
                    ratchet,
                    previous,
                    ..
                } => {
                    // The edit targets an ENTITY. If a rename in this
                    // window already moved it, apply the edit where the
                    // entity is now, not where the writer last saw it.
                    let id = ratchet.id();
                    let target = at.get(&id).cloned().unwrap_or(path);
                    self.add_tree(
                        &target,
                        content.clone(),
                        ratchet.clone(),
                        (*previous != Link::default()).then(|| previous.clone()),
                        *plaintext_hash,
                    )
                    .await?;
                    at.insert(id, target);
                }
            }
        }

        let mut inner = self.0.lock().await;
        inner.ops_log.merge(ops);
        Ok(())
    }
}
