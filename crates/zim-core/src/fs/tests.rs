//! Tree-only Fs tests.
//!
//! After the Vault-split refactor, `Fs` is a pure tree handle: it
//! decrypts + walks + mutates the file tree but doesn't carry the
//! manifest, the local peer's private key, or the chain log. Tests
//! that used to exercise `Fs::init` / `Fs::load` / `Fs::save` /
//! `Fs::publish` / `Fs::add_share` / etc. have been moved to
//! `crates/zim-core/src/vault/tests.rs` because they belong to the
//! `Vault` layer now.
//!
//! What remains in this file: tree mutations (`add`, `mkdir`, `rm`,
//! `mv`), reads (`cat`, `ls`, `get_entry_at_path`), and the CRDT
//! ops-log handoff (`apply_ops`). All of these are independent of
//! the manifest — they operate on the decrypted root dir, the
//! pending ops log, and the in-memory pin set.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;

use crate::blobs::{BlobError, BlobStore};
use crate::linked_data::Hash;
use zim_crypto::{EntryRatchet, PrivateKey};

use super::abs_path::AbsPath;
use super::fs_inner::Fs;

/// In-memory blob store for testing. No disk, no iroh.
#[derive(Clone, Default)]
struct MemBlobs(Arc<StdMutex<HashMap<Hash, Vec<u8>>>>);

#[async_trait]
impl BlobStore for MemBlobs {
    async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError> {
        let store = self.0.lock().unwrap();
        store
            .get(hash)
            .map(|v| Bytes::from(v.clone()))
            .ok_or_else(|| BlobError::NotFound(*hash))
    }

    async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError> {
        let hash = Hash::new(&data);
        self.0.lock().unwrap().insert(hash, data);
        Ok(hash)
    }

    async fn put_reader(
        &self,
        mut reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<Hash, BlobError> {
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| BlobError::Store(e.into()))?;
        self.put(buf).await
    }

    async fn stat(&self, hash: &Hash) -> Result<bool, BlobError> {
        Ok(self.0.lock().unwrap().contains_key(hash))
    }
}

async fn setup() -> (Fs<MemBlobs>, PrivateKey) {
    let blobs = MemBlobs::default();
    let owner = PrivateKey::generate();
    let root = EntryRatchet::seed();
    let (fs, _root_link) = Fs::init_tree(owner.public(), &root, blobs)
        .await
        .expect("init_tree");
    (fs, owner)
}

/// Stand in for a `Vault::save`: persist the tree so everything minted
/// so far counts as a saved revision. The root ratchet and prior root
/// hash don't matter to these tests.
async fn checkpoint(fs: &Fs<MemBlobs>) {
    fs.save_tree(Hash::new(b""), &EntryRatchet::seed())
        .await
        .expect("save_tree");
}

#[tokio::test]
async fn alice_adds_a_file_and_reads_it_back() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/hello.txt").unwrap();

    fs.add(&path, Cursor::new(b"hello world")).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    assert!(leaf.is_file());
}

#[tokio::test]
async fn add_records_blake3_of_plaintext_on_the_leaf() {
    // The load-bearing property for sync diff optimisation: the
    // `Entry::File` left behind by `add` carries `blake3(plaintext)`
    // so a sync engine can answer "did this file change?" against a
    // local copy without fetching or decrypting the ciphertext blob.
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/recipe.md").unwrap();
    let body = b"hash me before encryption";

    fs.add(&path, Cursor::new(body)).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    let recorded = leaf
        .plaintext_hash()
        .expect("fresh writes populate plaintext_hash");
    let expected = blake3::hash(body);
    assert_eq!(
        recorded.as_bytes(),
        expected.as_bytes(),
        "plaintext_hash on Entry::File should equal blake3 of the input body"
    );
}

#[tokio::test]
async fn get_returns_same_leaf_for_repeated_lookups() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/data.json").unwrap();
    fs.add(&path, Cursor::new(b"{}")).await.unwrap();

    let first = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    let second = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    assert_eq!(first, second);
}

#[tokio::test]
async fn mkdir_then_add_inside_lists_the_file() {
    let (fs, _owner) = setup().await;
    fs.mkdir(&AbsPath::new("/docs").unwrap(), false)
        .await
        .unwrap();
    fs.add(
        &AbsPath::new("/docs/readme.md").unwrap(),
        Cursor::new(b"hello"),
    )
    .await
    .unwrap();

    let entries = fs.ls(&AbsPath::new("/docs").unwrap()).await.unwrap();
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn cat_returns_the_bytes_we_added() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/greeting.txt").unwrap();
    fs.add(&path, Cursor::new(b"hello from the test"))
        .await
        .unwrap();

    let bytes = fs.cat(&path).await.unwrap();
    assert_eq!(bytes, b"hello from the test");
}

#[tokio::test]
async fn rm_removes_the_file() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/doomed.txt").unwrap();
    fs.add(&path, Cursor::new(b"goodbye")).await.unwrap();

    fs.rm(&path).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap();
    assert!(leaf.is_none(), "file should be gone after rm");
}

#[tokio::test]
async fn mv_moves_a_file_to_a_new_path() {
    let (fs, _owner) = setup().await;
    let src = AbsPath::new("/orig.txt").unwrap();
    let dst = AbsPath::new("/moved.txt").unwrap();
    fs.add(&src, Cursor::new(b"contents")).await.unwrap();

    fs.mv(&src, &dst).await.unwrap();

    assert!(fs.get_entry_at_path(&src).await.unwrap().is_none());
    let moved = fs.get_entry_at_path(&dst).await.unwrap().unwrap();
    assert!(moved.is_file());
    assert_eq!(fs.cat(&dst).await.unwrap(), b"contents");
}

// ── Per-entry identity + ratchets ───────────────────────────────────────

#[tokio::test]
async fn rewriting_a_file_keeps_its_identity_and_advances_its_key() {
    // Alice writes a note and saves, then writes a second draft over it.
    let (fs, _) = setup().await;
    let path = AbsPath::new("/note.md").unwrap();
    fs.add(&path, Cursor::new(b"first draft")).await.unwrap();
    let v1 = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    checkpoint(&fs).await;

    fs.add(&path, Cursor::new(b"second draft, longer"))
        .await
        .unwrap();
    let v2 = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    // Same entity…
    assert_eq!(v1.id(), v2.id(), "a rewrite keeps the entity id");
    // …next revision: the ratchet advanced, so the key and link moved on…
    assert_ne!(v1.secret(), v2.secret(), "the content key ratchets forward");
    assert_ne!(v1.link(), v2.link());
    assert_eq!(
        v1.ratchet().advanced(),
        *v2.ratchet(),
        "v2's ratchet is exactly v1's advanced once"
    );
    // …and history chains: v2 points back at the saved v1.
    assert_eq!(v1.previous(), None, "the first version has no predecessor");
    assert_eq!(v2.previous(), Some(v1.link()));
}

#[tokio::test]
async fn previous_skips_revisions_that_were_never_saved() {
    // Alice saves v1, then writes v2 and v3 without saving in between.
    // v2 lands in no manifest — nobody could fetch or decrypt it — so
    // v3's lineage points at v1, and v2 (while it lived) did too.
    let (fs, _) = setup().await;
    let path = AbsPath::new("/note.md").unwrap();
    fs.add(&path, Cursor::new(b"v1")).await.unwrap();
    let v1 = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    checkpoint(&fs).await;

    fs.add(&path, Cursor::new(b"v2")).await.unwrap();
    let v2 = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    assert_eq!(v2.previous(), Some(v1.link()));

    fs.add(&path, Cursor::new(b"v3")).await.unwrap();
    let v3 = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    assert_eq!(
        v3.previous(),
        Some(v1.link()),
        "an unsaved revision is not a predecessor"
    );
    // The key schedule counts saved revisions too: v2 never became one,
    // so v3 is the revision v2 was going to be — v1 advanced ONCE, and
    // v2 and v3 share a key (each encryption drew its own nonce).
    assert_eq!(v1.ratchet().advanced(), *v3.ratchet());
    assert_eq!(v2.ratchet(), v3.ratchet());
    assert_ne!(v2.link(), v3.link(), "different bytes, different blob");

    // Two unsaved writes with nothing saved before them have no lineage.
    let fresh = AbsPath::new("/scratch.md").unwrap();
    fs.add(&fresh, Cursor::new(b"a")).await.unwrap();
    fs.add(&fresh, Cursor::new(b"b")).await.unwrap();
    let b = fs.get_entry_at_path(&fresh).await.unwrap().unwrap();
    assert_eq!(b.previous(), None);
}

#[tokio::test]
async fn a_directory_s_previous_also_skips_unsaved_bodies() {
    // Alice saves a folder, then drops two files into it before saving
    // again. Each add rewrites the folder body; the second rewrite must
    // chain to the saved body, not to the evicted intermediate one.
    let (fs, _) = setup().await;
    let docs = AbsPath::new("/docs").unwrap();
    fs.mkdir(&docs, false).await.unwrap();
    let saved = fs.get_entry_at_path(&docs).await.unwrap().unwrap();
    checkpoint(&fs).await;

    fs.add(&AbsPath::new("/docs/a.md").unwrap(), Cursor::new(b"a"))
        .await
        .unwrap();
    fs.add(&AbsPath::new("/docs/b.md").unwrap(), Cursor::new(b"b"))
        .await
        .unwrap();
    let now = fs.get_entry_at_path(&docs).await.unwrap().unwrap();

    assert_eq!(now.previous(), Some(saved.link()));
    assert_eq!(
        saved.ratchet().advanced(),
        *now.ratchet(),
        "two unsaved rewrites are one revision"
    );
}

#[tokio::test]
async fn renaming_a_file_moves_the_same_entity_unchanged() {
    // Alice renames a note; nothing about its content or key changes.
    let (fs, _) = setup().await;
    let from = AbsPath::new("/draft.md").unwrap();
    let to = AbsPath::new("/final.md").unwrap();
    fs.add(&from, Cursor::new(b"same bytes")).await.unwrap();
    let before = fs.get_entry_at_path(&from).await.unwrap().unwrap();

    fs.mv(&from, &to).await.unwrap();

    assert!(fs.get_entry_at_path(&from).await.unwrap().is_none());
    let after = fs.get_entry_at_path(&to).await.unwrap().unwrap();
    assert_eq!(before.id(), after.id(), "a rename is the SAME entity");
    assert_eq!(before.ratchet(), after.ratchet(), "no revision happened");
    assert_eq!(before.secret(), after.secret());
    assert_eq!(before.link(), after.link());
}

#[tokio::test]
async fn a_directory_has_an_identity_and_advances_when_its_contents_change() {
    // Alice makes a folder and saves, then adds a file inside it — which
    // rewrites the folder's body.
    let (fs, _) = setup().await;
    let docs = AbsPath::new("/docs").unwrap();
    fs.mkdir(&docs, false).await.unwrap();
    let d1 = fs.get_entry_at_path(&docs).await.unwrap().unwrap();
    assert!(d1.is_dir());
    checkpoint(&fs).await;

    fs.add(&AbsPath::new("/docs/a.md").unwrap(), Cursor::new(b"inside"))
        .await
        .unwrap();
    let d2 = fs.get_entry_at_path(&docs).await.unwrap().unwrap();

    assert_eq!(d1.id(), d2.id(), "the folder is the same entity");
    assert_ne!(d1.secret(), d2.secret(), "its key ratcheted on rewrite");
    assert_eq!(d1.ratchet().advanced(), *d2.ratchet());
    assert_eq!(d2.previous(), Some(d1.link()));
}

#[tokio::test]
async fn replaying_ops_rebuilds_the_writers_exact_entities() {
    // Alice writes a note and a folder. Bob — a different peer with his
    // own key and store — replays her ops. Because the writer ships the
    // ratchet state in the op, Bob must end up with the SAME entity ids
    // and the SAME content keys; peers never advance independently.
    let (alice, _) = setup().await;
    let (bob, _) = setup().await;
    let note = AbsPath::new("/note.md").unwrap();
    let docs = AbsPath::new("/docs").unwrap();

    alice.add(&note, Cursor::new(b"hello bob")).await.unwrap();
    alice.mkdir(&docs, false).await.unwrap();
    // A second revision, so the replayed ratchet is an ADVANCED one.
    alice.add(&note, Cursor::new(b"hello again")).await.unwrap();

    let ops = alice.inner().await.ops_log.clone();
    bob.apply_ops(&ops).await.unwrap();

    let a_note = alice.get_entry_at_path(&note).await.unwrap().unwrap();
    let b_note = bob.get_entry_at_path(&note).await.unwrap().unwrap();
    assert_eq!(a_note.id(), b_note.id(), "Bob holds Alice's entity id");
    assert_eq!(
        a_note.secret(),
        b_note.secret(),
        "…and derives her exact key"
    );
    assert_eq!(a_note.link(), b_note.link());

    let a_docs = alice.get_entry_at_path(&docs).await.unwrap().unwrap();
    let b_docs = bob.get_entry_at_path(&docs).await.unwrap().unwrap();
    assert_eq!(a_docs.id(), b_docs.id(), "directory identity ships too");
}

// ── Rename-aware merge: an edit follows a concurrent rename ──────────────
//
// Alice edits /a.md while Bob renames it. Ops name ENTITIES, so on
// merge Alice's edit must land on the entity's new path — never
// resurrect the old name. The Lamport tie-break between the two
// concurrent ops is decided by peer key, so each test forces one
// ordering by advancing one side's clock first; together they cover
// both replay paths (redirect-then-apply, and apply-then-move-along).

/// A second peer sharing `alice`'s blob store — content-addressed
/// blobs sync between real peers, so a shared store is the faithful
/// stand-in for the fs-level test (only entries are being exercised).
async fn peer_sharing_blobs_of(alice: &Fs<MemBlobs>) -> Fs<MemBlobs> {
    let blobs = alice.blobs().inner().clone();
    let (fs, _) = Fs::init_tree(
        PrivateKey::generate().public(),
        &EntryRatchet::seed(),
        blobs,
    )
    .await
    .unwrap();
    fs
}

/// Both sides start from the same tree: Alice creates /a.md, Bob
/// replays it. Returns the entity id.
async fn shared_file(alice: &Fs<MemBlobs>, bob: &Fs<MemBlobs>) -> zim_crypto::EntryId {
    let a = AbsPath::new("/a.md").unwrap();
    alice.add(&a, Cursor::new(b"v1")).await.unwrap();
    let ops = alice.inner().await.ops_log.clone();
    bob.apply_ops(&ops).await.unwrap();
    alice.get_entry_at_path(&a).await.unwrap().unwrap().id()
}

/// Exchange full logs the way `chain::merge` does: each side replays
/// the merged window (its own ops ∪ the other's).
async fn exchange(alice: &Fs<MemBlobs>, bob: &Fs<MemBlobs>) {
    let a_log = alice.inner().await.ops_log.clone();
    let b_log = bob.inner().await.ops_log.clone();
    let mut for_bob = b_log.clone();
    for_bob.merge(&a_log);
    let mut for_alice = a_log;
    for_alice.merge(&b_log);
    bob.apply_ops(&for_bob).await.unwrap();
    alice.apply_ops(&for_alice).await.unwrap();
}

async fn assert_edit_followed_rename(
    fs: &Fs<MemBlobs>,
    who: &str,
    to: &str,
    id: zim_crypto::EntryId,
) {
    let old = AbsPath::new("/a.md").unwrap();
    let new = AbsPath::new(to).unwrap();
    assert!(
        fs.get_entry_at_path(&old).await.unwrap().is_none(),
        "{who}: the old name must not be resurrected"
    );
    let e = fs
        .get_entry_at_path(&new)
        .await
        .unwrap()
        .expect("renamed file present");
    assert_eq!(e.id(), id, "{who}: same entity at the new path");
    assert_eq!(
        fs.cat(&new).await.unwrap(),
        b"v2 edited",
        "{who}: carries the edit"
    );
}

#[tokio::test]
async fn an_edit_follows_a_concurrent_rename_when_the_rename_replays_first() {
    let (alice, _) = setup().await;
    let bob = peer_sharing_blobs_of(&alice).await;
    let id = shared_file(&alice, &bob).await;

    // Bob renames to a name that sorts BEFORE /a.md — the ordering that
    // broke under per-path replay. Alice bumps her clock first so her
    // edit carries the later OpId: the rename replays first, and the
    // edit must be redirected onto /0.md.
    alice
        .mkdir(&AbsPath::new("/scratch").unwrap(), false)
        .await
        .unwrap();
    alice
        .add(&AbsPath::new("/a.md").unwrap(), Cursor::new(b"v2 edited"))
        .await
        .unwrap();
    bob.mv(
        &AbsPath::new("/a.md").unwrap(),
        &AbsPath::new("/0.md").unwrap(),
    )
    .await
    .unwrap();

    exchange(&alice, &bob).await;
    assert_edit_followed_rename(&alice, "alice", "/0.md", id).await;
    assert_edit_followed_rename(&bob, "bob", "/0.md", id).await;
}

#[tokio::test]
async fn an_edit_follows_a_concurrent_rename_when_the_edit_replays_first() {
    let (alice, _) = setup().await;
    let bob = peer_sharing_blobs_of(&alice).await;
    let id = shared_file(&alice, &bob).await;

    // Bob bumps his clock first so the RENAME carries the later OpId:
    // the edit replays first (at /a.md), then the rename must carry the
    // edited content along to /0.md.
    bob.mkdir(&AbsPath::new("/scratch").unwrap(), false)
        .await
        .unwrap();
    bob.mv(
        &AbsPath::new("/a.md").unwrap(),
        &AbsPath::new("/0.md").unwrap(),
    )
    .await
    .unwrap();
    alice
        .add(&AbsPath::new("/a.md").unwrap(), Cursor::new(b"v2 edited"))
        .await
        .unwrap();

    exchange(&alice, &bob).await;
    assert_edit_followed_rename(&alice, "alice", "/0.md", id).await;
    assert_edit_followed_rename(&bob, "bob", "/0.md", id).await;
}
