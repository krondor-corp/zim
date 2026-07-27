//! Two-peer smoke test for the sync protocol.
//!
//! Both peers share a single in-memory `BlobsProvider` so the test
//! focuses on the *sync* protocol — `HeadRequest → ProbeRequest →
//! apply_chain → merge_with` — without also exercising iroh-blobs
//! transfer (validated upstream).
//!
//! Uses the public `zim_peer::Peer` builder. Discovery is off; peers
//! learn about each other via explicit `introduce()`.

use bytes::Bytes;

use zim_core::fs::AbsPath;
use zim_crypto::PrivateKey;
use zim_peer::BlobsProvider;
use zim_peer::{MemoryVaultLog, Vault, VaultLog};

use zim_peer::{Effect, Peer};

async fn make_peer(blobs: BlobsProvider) -> anyhow::Result<Peer<MemoryVaultLog>> {
    Peer::builder()
        .with_secret(PrivateKey::generate())
        .with_log(MemoryVaultLog::new())
        .with_blobs(blobs)
        .build()
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn alice_and_bob_converge_via_pull() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("zim_peer=debug,zim_core=info")
        .with_test_writer()
        .try_init();

    let blobs = BlobsProvider::memory().await.expect("blobs");

    let alice = make_peer(blobs.clone()).await.expect("alice");
    let bob = make_peer(blobs.clone()).await.expect("bob");
    alice.introduce(bob.node_addr()).unwrap();
    bob.introduce(alice.node_addr()).unwrap();

    let alice_pk = alice.id();
    let bob_pk = bob.id();

    // ── Alice creates the vault, shares with Bob, saves L_1 ──
    let mut alice_vault = Vault::init(
        "shared".to_string(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    alice_vault.add_share(bob_pk).expect("share");
    alice_vault.save().await.expect("save L_1");

    let bootstrap = alice_vault.head().await.expect("head");

    // Seed Bob's log so he can open the vault at L_1.
    bob.coord()
        .log()
        .append(
            vault_id,
            "shared".into(),
            bootstrap.link.clone(),
            None,
            bootstrap.height,
        )
        .await
        .expect("seed bob log");
    // Sanity: bob can open the vault at L_1 with his share.
    Vault::open(
        vault_id,
        bob.coord().blobs().clone(),
        bob.coord().log().clone(),
        bob.secret(),
    )
    .await
    .expect("bob opens vault at L_1");

    // ── Alice advances to L_2 ──
    alice_vault
        .fs()
        .mkdir(&AbsPath::new("/docs").unwrap(), false)
        .await
        .unwrap();
    alice_vault
        .fs()
        .add(
            &AbsPath::new("/docs/readme.md").unwrap(),
            std::io::Cursor::new(Bytes::from_static(b"hello bob\n").to_vec()),
        )
        .await
        .unwrap();
    alice_vault.save().await.expect("save L_2");
    let alice_head = alice_vault.head().await.unwrap();
    assert!(alice_head.height > bootstrap.height);

    // ── Bob pulls from Alice ──
    bob.coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice_pk,
        })
        .await
        .expect("pull from alice");

    // ── Bob's view should reflect Alice's edits ──
    // Re-open from the log: the coordinator no longer caches `Vault`
    // by id (deep-clone of the manifest made the cache lie about
    // pulled state). After `PullFromPeer` commits, the log's head
    // advances; we read the new state by re-opening.
    let bob_vault = Vault::open(
        vault_id,
        bob.coord().blobs().clone(),
        bob.coord().log().clone(),
        bob.secret(),
    )
    .await
    .expect("bob re-opens vault at pulled head");
    let bob_head = bob_vault.head().await.unwrap();
    assert!(
        bob_head.height >= alice_head.height,
        "bob height {} should be >= alice {} after pull",
        bob_head.height,
        alice_head.height
    );

    let content = bob_vault
        .fs()
        .cat(&AbsPath::new("/docs/readme.md").unwrap())
        .await
        .expect("bob reads readme alice wrote");
    assert_eq!(content, b"hello bob\n");

    // ── Hijack rejection: vault ids are self-certifying ──
    // Mallory announces Alice's (real, validly-signed) chain under a
    // DIFFERENT vault id. The chain walks fine, but its genesis
    // hashes to Alice's id, not the claimed one — bootstrap must
    // reject before a single log append.
    let bogus_id =
        zim_core::vault::VaultId::from_hash(zim_core::linked_data::Hash::new(b"not this vault"));
    let result = bob
        .coord()
        .apply_chain(bogus_id, alice_head.clone(), None, vec![alice_pk])
        .await;
    assert!(
        result.is_err(),
        "a chain must not bootstrap under an id its genesis doesn't hash to"
    );
    assert!(
        !bob.coord().log().exists(bogus_id).await.unwrap(),
        "rejected chain must leave no log entries behind"
    );

    // Graceful teardown.
    alice.shutdown().await;
    bob.shutdown().await;
}
