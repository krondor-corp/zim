//! Directory tree primitives: [`Entry`] (a typed pointer a parent
//! [`Dir`] holds for each child) and [`Dir`] (a children map).
//!
//! The on-disk dir body is the DAG-CBOR encoding of [`Dir`], encrypted
//! per-dir with its [`Secret`]. See [`ContentStore::put_metadata`] and
//! [`ContentStore::get_dir`] for the round-trip.
//!
//! [`ContentStore::put_metadata`]: super::content_store::ContentStore::put_metadata
//! [`ContentStore::get_dir`]: super::content_store::ContentStore::get_dir

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
///   metadata pack. Dereference with its link and `ratchet.key()` via
///   [`ContentStore::get_dir`](super::content_store::ContentStore::get_dir).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// A file entry. The link points at encrypted bytes; `mime` and
    /// `metadata` are caller-supplied annotations.
    File {
        /// Content-addressed pointer to the encrypted file bytes.
        link: Link,
        /// Optional MIME type, typically inferred from a filename via
        /// [`Entry::file_from_path`].
        mime: MaybeMime,
        /// Optional caller-defined metadata.
        metadata: MaybeMetadata,
        /// `blake3` of the *plaintext* file body. Holders of the
        /// vault key can recompute this off any candidate plaintext
        /// to decide "did this file change?" without fetching or
        /// decrypting the ciphertext blob — the load-bearing
        /// optimisation for sync diffing.
        plaintext_hash: Hash,
        /// This revision's key schedule. The content key is derived from
        /// it (`ratchet.key()`); so is the entity's identity.
        ratchet: EntryRatchet,
        /// The prior **saved** revision of THIS entity (its `link` in
        /// the last manifest that carried it), so one file's history is
        /// a walk from manifest to manifest. Revisions minted and
        /// overwritten between two saves are skipped: they land in no
        /// manifest, so nothing could fetch or decrypt them.
        /// `Link::default()` means no saved predecessor — the same
        /// null-link sentinel `Manifest::previous` uses for genesis.
        /// Read it through [`Entry::previous`], which maps the sentinel
        /// to `None`.
        previous: Link,
    },
    /// A subdirectory entry. The link points at an encrypted [`Dir`] body
    /// staged in the metadata pack.
    Dir {
        /// Content-addressed pointer to the encrypted [`Dir`] body.
        link: Link,
        /// See [`Entry::File::ratchet`]. (The root has no `Entry`: its
        /// link lives on the manifest and its ratchet on the vault.)
        ratchet: EntryRatchet,
        /// See [`Entry::File::previous`].
        previous: Link,
    },
}

impl Entry {
    /// A file entry. Every field comes from the caller — there is no
    /// defaulted or placeholder form. `ratchet` is this revision's key
    /// schedule (and identity); `previous` is the prior **saved**
    /// revision's `link`, `None` when there is none (stored as the
    /// null-link sentinel); `path` drives MIME inference;
    /// `plaintext_hash` is `blake3` of the body as written.
    pub fn file(
        link: Link,
        ratchet: EntryRatchet,
        previous: Option<Link>,
        path: &Path,
        plaintext_hash: Hash,
    ) -> Self {
        Entry::File {
            link,
            mime: MaybeMime::from_path(path),
            metadata: None,
            plaintext_hash,
            ratchet,
            previous: previous.unwrap_or_default(),
        }
    }

    /// A directory entry. `previous` is the prior **saved** revision's
    /// `link`, `None` when there is none (stored as the null-link
    /// sentinel).
    pub fn dir(link: Link, ratchet: EntryRatchet, previous: Option<Link>) -> Self {
        Entry::Dir {
            link,
            ratchet,
            previous: previous.unwrap_or_default(),
        }
    }

    /// The content-addressed link to this entry's bytes.
    pub fn link(&self) -> &Link {
        match self {
            Entry::File { link, .. } => link,
            Entry::Dir { link, .. } => link,
        }
    }

    /// The content key for this revision, derived from the ratchet.
    pub fn secret(&self) -> Secret {
        self.ratchet().key()
    }

    /// Stable entity identity, derived from the ratchet's seed.
    pub fn id(&self) -> EntryId {
        self.ratchet().id()
    }

    /// This revision's key schedule.
    pub fn ratchet(&self) -> &EntryRatchet {
        match self {
            Entry::File { ratchet, .. } | Entry::Dir { ratchet, .. } => ratchet,
        }
    }

    /// The prior saved revision of this entity; `None` when there is
    /// none (the stored sentinel is `Link::default()`, never handed out).
    pub fn previous(&self) -> Option<&Link> {
        match self {
            Entry::File { previous, .. } | Entry::Dir { previous, .. } => {
                (*previous != Link::default()).then_some(previous)
            }
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
    /// directories.
    pub fn plaintext_hash(&self) -> Option<Hash> {
        match self {
            Entry::File { plaintext_hash, .. } => Some(*plaintext_hash),
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
            Entry::file(
                Link::default(),
                EntryRatchet::seed(),
                None,
                Path::new("example"),
                Hash::new(b""),
            ),
        );

        let encoded = dir.encode().unwrap();
        let decoded = Dir::decode(&encoded).unwrap();

        assert_eq!(dir, decoded);
    }

    #[test]
    fn test_entry_from_path() {
        use std::path::PathBuf;

        let link = Link::default();

        let entry = Entry::file(
            link.clone(),
            EntryRatchet::seed(),
            None,
            &PathBuf::from("/test/file.json"),
            Hash::new(b""),
        );
        assert_eq!(entry.mime().map(|m| m.as_ref()), Some("application/json"));
        assert!(entry.is_file());

        let entry = Entry::dir(link, EntryRatchet::seed(), None);
        assert!(entry.is_dir());
        assert_eq!(entry.mime(), None);
    }

    #[test]
    fn test_entry_metadata() {
        let mut entry = Entry::file(
            Link::default(),
            EntryRatchet::seed(),
            None,
            Path::new("example"),
            Hash::new(b""),
        );
        assert_eq!(entry.metadata(), None);

        entry.set_metadata("key".to_string(), LinkedData::Null);
        assert!(entry.metadata().is_some());
        assert_eq!(entry.metadata().unwrap().len(), 1);
    }
}
