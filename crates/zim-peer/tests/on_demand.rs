//! On-demand content: a space-constrained peer pulls the structure of
//! a vault, fetches file content on first read, and evicts it under a
//! cache budget.
//!
//! Unlike the other peer tests, Alice and Bob each own a blob store, so
//! every byte Bob reads has to cross iroh from Alice.

use std::io::Cursor;

use zim_core::blobs::BlobStore;
use zim_core::fs::{AbsPath, Entry};
use zim_core::linked_data::Hash;
use zim_crypto::PrivateKey;
use zim_peer::{BlobsProvider, Effect, MemoryVaultLog, Peer, PinPolicy, ReadError, Vault};

const BIG: usize = 256 * 1024;

async fn make_peer(policy: PinPolicy) -> Peer<MemoryVaultLog> {
    Peer::builder()
        .with_secret(PrivateKey::generate())
        .with_log(MemoryVaultLog::new())
        .with_blobs(BlobsProvider::memory().await.expect("blobs"))
        .with_sync_interval(None)
        .with_pin_policy(policy)
        .build()
        .await
        .expect("peer")
}

fn path(p: &str) -> AbsPath {
    AbsPath::new(p).unwrap()
}

async fn content_hash(vault: &Vault<MemoryVaultLog>, p: &str) -> Hash {
    match vault.fs().get_entry_at_path(&path(p)).await.unwrap() {
        Some(Entry::File { link, .. }) => link.hash(),
        other => panic!("{p} is not a file: {other:?}"),
    }
}

async fn holds(peer: &Peer<MemoryVaultLog>, hash: &Hash) -> bool {
    peer.coord().blobs().stat(hash).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn bob_reads_alices_large_files_on_demand_and_refetches_after_eviction() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("zim_peer=debug,zim_core=info")
        .with_test_writer()
        .try_init();

    // Alice mirrors everything; Bob's phone keeps room for one big file.
    let alice = make_peer(PinPolicy::Full).await;
    let bob = make_peer(PinPolicy::OnDemand {
        budget_bytes: (BIG + BIG / 2) as u64,
    })
    .await;
    alice.introduce(bob.node_addr()).unwrap();
    bob.introduce(alice.node_addr()).unwrap();

    // ── Alice writes two large files into a nested dir and shares with Bob ──
    let mut alice_vault = Vault::init(
        "photos".into(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    alice_vault.add_share(bob.id()).expect("share");
    alice_vault.fs().mkdir(&path("/trip"), false).await.unwrap();
    let beach = vec![0xb7u8; BIG];
    let hike = vec![0x4au8; BIG];
    alice_vault
        .fs()
        .add(&path("/trip/beach.raw"), Cursor::new(beach.clone()))
        .await
        .unwrap();
    alice_vault
        .fs()
        .add(&path("/trip/hike.raw"), Cursor::new(hike.clone()))
        .await
        .unwrap();
    alice_vault.save().await.expect("save");
    let beach_hash = content_hash(&alice_vault, "/trip/beach.raw").await;
    let hike_hash = content_hash(&alice_vault, "/trip/hike.raw").await;

    // ── Bob pulls: structure arrives, content doesn't ──
    bob.coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice.id(),
        })
        .await
        .expect("bob pulls");
    let bob_vault = bob.vault(vault_id).await.expect("bob opens the vault");
    let manifest = bob_vault.manifest();
    assert!(holds(&bob, &bob_vault.manifest_link().hash()).await);
    assert!(
        holds(&bob, &manifest.ops().hash()).await,
        "ops log is local"
    );
    assert!(
        bob_vault.fs().ls(&path("/trip")).await.unwrap().len() == 2,
        "dir bodies are local: Bob lists /trip without fetching"
    );
    assert!(!holds(&bob, &beach_hash).await, "beach not fetched on pull");
    assert!(!holds(&bob, &hike_hash).await, "hike not fetched on pull");

    // ── Bob reads beach: fetched from Alice on the miss ──
    let read = bob
        .coord()
        .cat(&bob_vault, &path("/trip/beach.raw"))
        .await
        .expect("bob reads beach");
    assert_eq!(read, beach);
    assert!(holds(&bob, &beach_hash).await, "beach is now cached");

    // ── Reading hike overflows the budget: beach is evicted ──
    let read = bob
        .coord()
        .cat(&bob_vault, &path("/trip/hike.raw"))
        .await
        .expect("bob reads hike");
    assert_eq!(read, hike);
    assert!(holds(&bob, &hike_hash).await);
    assert!(!holds(&bob, &beach_hash).await, "beach evicted for hike");

    // ── A second read of beach re-fetches it ──
    let read = bob
        .coord()
        .cat(&bob_vault, &path("/trip/beach.raw"))
        .await
        .expect("bob re-reads beach");
    assert_eq!(read, beach);
    assert!(holds(&bob, &beach_hash).await, "beach re-fetched");

    // ── The reconcile sweep heals structure only, never content ──
    bob.coord().reconcile_pass().await;
    assert!(
        !holds(&bob, &hike_hash).await,
        "the sweep doesn't refill hike"
    );

    alice.shutdown().await;
    bob.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn bobs_unsynced_writes_survive_cache_eviction() {
    // Bob writes a note while Alice is around but never announces it.
    // Reading Alice's large files churns the cache past its budget; the
    // note's blob exists nowhere else, so it must stay.
    let alice = make_peer(PinPolicy::Full).await;
    let bob = make_peer(PinPolicy::OnDemand { budget_bytes: 1 }).await;
    alice.introduce(bob.node_addr()).unwrap();
    bob.introduce(alice.node_addr()).unwrap();

    let mut alice_vault = Vault::init(
        "shared".into(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    alice_vault.add_share(bob.id()).expect("share");
    for name in ["/a.raw", "/b.raw"] {
        alice_vault
            .fs()
            .add(&path(name), Cursor::new(vec![7u8; BIG]))
            .await
            .unwrap();
    }
    alice_vault.save().await.expect("save");

    bob.coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice.id(),
        })
        .await
        .expect("bob pulls");

    // ── Bob authors a note locally; nobody has acknowledged it ──
    let mut bob_vault = bob.vault(vault_id).await.expect("open");
    bob_vault
        .fs()
        .add(
            &path("/note.md"),
            Cursor::new(b"remember the milk".to_vec()),
        )
        .await
        .unwrap();
    bob_vault.save().await.expect("bob saves");
    let note_hash = content_hash(&bob_vault, "/note.md").await;

    // ── Bob reads his note, then churns the cache with Alice's files ──
    let note = bob
        .coord()
        .cat(&bob_vault, &path("/note.md"))
        .await
        .unwrap();
    assert_eq!(note, b"remember the milk");
    for name in ["/a.raw", "/b.raw", "/a.raw"] {
        bob.coord().cat(&bob_vault, &path(name)).await.unwrap();
    }

    assert!(
        holds(&bob, &note_hash).await,
        "Bob's own write is never evicted"
    );
    assert_eq!(
        bob.coord()
            .cat(&bob_vault, &path("/note.md"))
            .await
            .unwrap(),
        b"remember the milk"
    );

    alice.shutdown().await;
    bob.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_with_every_provider_offline_reports_content_unavailable() {
    // Bob pulls the vault, then Alice — its only provider — goes away.
    // The read fails with a distinct "unavailable" error, not a
    // generic fs failure.
    let alice = make_peer(PinPolicy::Full).await;
    let bob = make_peer(PinPolicy::OnDemand {
        budget_bytes: BIG as u64,
    })
    .await;
    alice.introduce(bob.node_addr()).unwrap();
    bob.introduce(alice.node_addr()).unwrap();

    let mut alice_vault = Vault::init(
        "solo".into(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    alice_vault.add_share(bob.id()).expect("share");
    alice_vault
        .fs()
        .add(&path("/big.raw"), Cursor::new(vec![1u8; BIG]))
        .await
        .unwrap();
    alice_vault.save().await.expect("save");

    bob.coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice.id(),
        })
        .await
        .expect("bob pulls");
    alice.shutdown().await;

    let bob_vault = bob.vault(vault_id).await.expect("open");
    let err = bob
        .coord()
        .cat(&bob_vault, &path("/big.raw"))
        .await
        .expect_err("no provider is reachable");
    assert!(
        matches!(err, ReadError::ContentUnavailable { .. }),
        "got {err:?}"
    );

    bob.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_peer_still_downloads_content_on_pull() {
    // The default policy is unchanged: Carol mirrors content at pull time.
    let alice = make_peer(PinPolicy::Full).await;
    let carol = make_peer(PinPolicy::default()).await;
    alice.introduce(carol.node_addr()).unwrap();
    carol.introduce(alice.node_addr()).unwrap();

    let mut alice_vault = Vault::init(
        "mirror".into(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    alice_vault.add_share(carol.id()).expect("share");
    alice_vault
        .fs()
        .add(&path("/doc.txt"), Cursor::new(b"hello".to_vec()))
        .await
        .unwrap();
    alice_vault.save().await.expect("save");
    let doc_hash = content_hash(&alice_vault, "/doc.txt").await;

    carol
        .coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice.id(),
        })
        .await
        .expect("carol pulls");

    assert!(holds(&carol, &doc_hash).await, "Full pulls content eagerly");

    alice.shutdown().await;
    carol.shutdown().await;
}
