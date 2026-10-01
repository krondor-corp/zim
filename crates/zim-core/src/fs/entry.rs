//! Directory tree primitives: [`Entry`] (a typed pointer a parent
//! [`Dir`] holds for each child) and [`Dir`] (a children map).
//!
//! The on-disk dir body is the DAG-CBOR encoding of [`Dir`], encrypted
//! per-dir with its [`Secret`]. See [`ContentStore::put_metadata`] and
//! [`ContentStore::get_metadata`] for the round-trip.
//!
//! [`ContentStore::put_metadata`]: super::content_store::ContentStore::put_metadata
//! [`ContentStore::get_metadata`]: super::content_store::ContentStore::get_metadata

#![allow(clippy::doc_lazy_continuation)]

use std::collections::BTreeMap;
use std::path::Path;

use crate::linked_data::Hash;
use mime::Mime;
use serde::{Deserialize, Serialize};

use crate::linked_data::{Link, LinkedData};
use zim_crypto::{EntryId, EntryRatchet, Secret};

use super::maybe_mime::MaybeMime;

/// Free-form metadata attached to a file entry (key → linked-data value).
type Metadata = BTreeMap<String, LinkedData>;
type MaybeMetadata = Option<Metadata>;

/// A directory entry — what a [`Dir`] holds in its children map.
///
/// Each variant carries everything needed to fetch and decrypt the data
/// the link points at: a [`Link`] (content-addressed pointer) and a
/// per-entry [`Secret`].
///
/// - [`Entry::File`] — `link` addresses the encrypted file content in the
///   inner blob store. `mime` and `metadata` are optional client-side
///   annotations (the store doesn't inspect them).
/// - [`Entry::Dir`] — `link` addresses an encrypted [`Dir`] body in the
///   metadata pack. Dereference via
///   [`ContentStore::get_metadata`](super::content_store::ContentStore::get_metadata).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// A file entry. The link points at encrypted bytes; `mime` and
    /// `metadata` are caller-supplied annotations.
    File {
        /// Content-addressed pointer to the encrypted file bytes.
        link: Link,
        /// Per-file encryption secret.
        secret: Secret,
        /// Optional MIME type, typically inferred from a filename via
        /// [`Entry::file_from_path`].
        mime: MaybeMime,
        /// Optional caller-defined metadata.
        metadata: MaybeMetadata,
        /// `blake3` of the *plaintext* file body. Holders of the
        /// vault key can recompute this off any candidate plaintext
        /// to decide "did this file change?" without fetching or
        /// decrypting the ciphertext blob — the load-bearing
        /// optimisation for sync diffing. `None` on entries written
        /// before this field existed, or on synthetic constructions
        /// (tests, defaults).
        #[serde(default)]
        plaintext_hash: Option<Hash>,
        /// Stable identity across rewrites and renames. `None` only for
        /// the root (whose identity is the vault id).
        #[serde(default)]
        id: Option<EntryId>,
        /// This revision's key schedule; `secret` is derived from it
        /// (`secret == ratchet.key()`). `None` only for the share-
        /// governed root.
        #[serde(default)]
        ratchet: Option<EntryRatchet>,
        /// Prior version of THIS entity (its previous `link`), so one
        /// file's history is an O(1) walk. `None` at creation.
        #[serde(default)]
        previous: Option<Link>,
    },
    /// A subdirectory entry. The link points at an encrypted [`Dir`] body
    /// staged in the metadata pack.
    Dir {
        /// Content-addressed pointer to the encrypted [`Dir`] body.
        link: Link,
        /// Per-dir encryption secret (rotated when the dir is rewritten).
        secret: Secret,
        /// See [`Entry::File::id`]. `None` only for the root.
        #[serde(default)]
        id: Option<EntryId>,
        /// See [`Entry::File::ratchet`]. `None` only for the root.
        #[serde(default)]
        ratchet: Option<EntryRatchet>,
        /// See [`Entry::File::previous`].
        #[serde(default)]
        previous: Option<Link>,
    },
}

impl Entry {
    /// A bare file entry at revision 0 of a fresh entity — mints an id,
    /// derives `secret` from `ratchet`. `mime`/`metadata`/`plaintext_hash`
    /// default to `None`. Tests and synthetic fixtures; production uses
    /// [`Self::file_at_path`].
    pub fn file(link: Link, ratchet: EntryRatchet) -> Self {
        Self::file_at_path(
            link,
            EntryId::generate(),
            ratchet,
            None,
            Path::new(""),
            None,
        )
    }

    /// Production file constructor. `secret` is derived from `ratchet`
    /// so the two can never disagree; `mime` is inferred from `path`.
    /// On a rewrite the caller passes the entity's existing `id`, its
    /// *advanced* ratchet, and `previous = Some(old link)`; on creation
    /// a fresh id, a seeded ratchet, and `None`.
    pub fn file_at_path(
        link: Link,
        id: EntryId,
        ratchet: EntryRatchet,
        previous: Option<Link>,
        path: &Path,
        plaintext_hash: Option<Hash>,
    ) -> Self {
        Entry::File {
            link,
            secret: ratchet.key(),
            mime: MaybeMime::from_path(path),
            metadata: None,
            plaintext_hash,
            id: Some(id),
            ratchet: Some(ratchet),
            previous,
        }
    }

    /// A directory entry at revision 0 of a fresh entity. See
    /// [`Self::dir_versioned`] for rewrites.
    pub fn dir(link: Link, ratchet: EntryRatchet) -> Self {
        Self::dir_versioned(link, EntryId::generate(), ratchet, None)
    }

    /// Production directory constructor; `secret` derived from `ratchet`.
    pub fn dir_versioned(
        link: Link,
        id: EntryId,
        ratchet: EntryRatchet,
        previous: Option<Link>,
    ) -> Self {
        Entry::Dir {
            link,
            secret: ratchet.key(),
            id: Some(id),
            ratchet: Some(ratchet),
            previous,
        }
    }

    /// The ROOT directory entry. The root is not ratcheted in this
    /// version: its key is the share-governed vault secret, minted fresh
    /// per save and sealed to shareholders, and its identity is the
    /// vault id. So `id`/`ratchet`/`previous` are `None` here, and only
    /// here.
    pub fn root_dir(link: Link, secret: Secret) -> Self {
        Entry::Dir {
            link,
            secret,
            id: None,
            ratchet: None,
            previous: None,
        }
    }

    /// The content-addressed link to this entry's bytes.
    pub fn link(&self) -> &Link {
        match self {
            Entry::File { link, .. } => link,
            Entry::Dir { link, .. } => link,
        }
    }

    /// The per-entry secret used to encrypt the addressed bytes.
    pub fn secret(&self) -> &Secret {
        match self {
            Entry::File { secret, .. } => secret,
            Entry::Dir { secret, .. } => secret,
        }
    }

    /// Stable entity identity; `None` only for the root.
    pub fn id(&self) -> Option<EntryId> {
        match self {
            Entry::File { id, .. } | Entry::Dir { id, .. } => *id,
        }
    }

    /// This revision's key schedule; `None` only for the root.
    pub fn ratchet(&self) -> Option<&EntryRatchet> {
        match self {
            Entry::File { ratchet, .. } | Entry::Dir { ratchet, .. } => ratchet.as_ref(),
        }
    }

    /// The prior version of this entity, if any.
    pub fn previous(&self) -> Option<&Link> {
        match self {
            Entry::File { previous, .. } | Entry::Dir { previous, .. } => previous.as_ref(),
        }
    }

    /// The MIME type, if known and if this is a file. Always `None` for
    /// directories.
    pub fn mime(&self) -> Option<&Mime> {
        match self {
            Entry::File { mime, .. } => mime.0.as_ref(),
            Entry::Dir { .. } => None,
        }
    }

    /// The caller-supplied metadata map, if any and if this is a file.
    /// Always `None` for directories.
    pub fn metadata(&self) -> Option<&Metadata> {
        match self {
            Entry::File { metadata, .. } => metadata.as_ref(),
            Entry::Dir { .. } => None,
        }
    }

    /// `blake3(plaintext)` of this file's body, when known. `None` for
    /// directories and for legacy file entries written before the
    /// field existed.
    pub fn plaintext_hash(&self) -> Option<Hash> {
        match self {
            Entry::File { plaintext_hash, .. } => *plaintext_hash,
            Entry::Dir { .. } => None,
        }
    }

    /// Insert a `(key, value)` into the file's metadata map, allocating
    /// the map if absent. No-op on a directory entry.
    pub fn set_metadata(&mut self, key: String, value: LinkedData) {
        if let Entry::File { metadata, .. } = self {
            let m = metadata.get_or_insert_with(BTreeMap::new);
            m.insert(key, value);
        }
    }

    /// True if this is a [`Entry::Dir`].
    pub fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir { .. })
    }

    /// True if this is a [`Entry::File`].
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File { .. })
    }
}

/// A directory's children — a `BTreeMap` from name to [`Entry`]. The
/// on-disk dir body is this struct, DAG-CBOR encoded and encrypted with
/// the dir's [`Secret`].
///
/// Path traversal joins names with `/`. The empty `Dir` represents an
/// empty directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Dir(BTreeMap<String, Entry>);

impl Dir {
    /// Create an empty directory.
    pub fn new() -> Self {
        Dir(BTreeMap::new())
    }

    /// Look up an entry by name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.0.get(name)
    }

    /// Insert (or overwrite) the entry stored under `name`. Returns the
    /// previous value, if any.
    pub fn insert(&mut self, name: String, entry: Entry) -> Option<Entry> {
        self.0.insert(name, entry)
    }

    /// Remove and return the entry stored under `name`, if present.
    pub fn remove(&mut self, name: &str) -> Option<Entry> {
        self.0.remove(name)
    }

    /// Borrow the underlying name → entry map (e.g. for iteration).
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.0
    }

    /// Number of immediate children (non-recursive).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if this directory has no children.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::linked_data::BlockEncoded;

    #[test]
    fn test_dir_encode_decode() {
        let mut dir = Dir::new();
        dir.insert(
            "example".to_string(),
            Entry::file(Link::default(), EntryRatchet::seed()),
        );

        let encoded = dir.encode().unwrap();
        let decoded = Dir::decode(&encoded).unwrap();

        assert_eq!(dir, decoded);
    }

    #[test]
    fn test_entry_from_path() {
        use std::path::PathBuf;

        let link = Link::default();

        let entry = Entry::file_at_path(
            link.clone(),
            EntryId::generate(),
            EntryRatchet::seed(),
            None,
            &PathBuf::from("/test/file.json"),
            None,
        );
        assert_eq!(entry.mime().map(|m| m.as_ref()), Some("application/json"));
        assert!(entry.is_file());

        let entry = Entry::dir(link, EntryRatchet::seed());
        assert!(entry.is_dir());
        assert_eq!(entry.mime(), None);
    }

    #[test]
    fn test_entry_metadata() {
        let mut entry = Entry::file(Link::default(), EntryRatchet::seed());
        assert_eq!(entry.metadata(), None);

        entry.set_metadata("key".to_string(), LinkedData::Null);
        assert!(entry.metadata().is_some());
        assert_eq!(entry.metadata().unwrap().len(), 1);
    }
}
