//! Relay-pull smoke test.
//!
//! Locks in the hub-revival's load-bearing flow: a peer that's
//! registered as a vault *relay* (no `Share`, no vault secret) can
//! still mirror the manifest chain + ciphertext blobs via the
//! `Effect::PullFromPeer` path. Post-refactor the relay branch lives
//! in `zim_peer::relay_pull` — it walks the chain by raw blob fetches
//! and appends to the log, without ever constructing a `Vault`.
//!
//! Assertions go against the hub's raw `log` + `blobs`, not against a
//! `Vault` (the hub doesn't have a Share, so it can't construct one).

use bytes::Bytes;

use zim_core::blobs::{BlobStore, BlobsProvider};
use zim_core::fs::AbsPath;
use zim_core::vault::VaultLog;
use zim_crypto::PrivateKey;
use zim_did::Identity;
use zim_peer::{MemoryVaultLog, Vault};

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
async fn hub_mirrors_chain_without_a_share() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("zim_peer=debug,zim_core=info")
        .with_test_writer()
        .try_init();

    let blobs = BlobsProvider::memory().await.expect("blobs");

    let alice = make_peer(blobs.clone()).await.expect("alice");
    let hub = make_peer(blobs.clone()).await.expect("hub");
    alice.introduce(hub.node_addr()).unwrap();
    hub.introduce(alice.node_addr()).unwrap();

    let alice_pk = alice.id();
    let hub_pk = hub.id();

    // ── Alice creates the vault. Hub is registered as a Relay only —
    // never gets a Share, never holds a secret. ──
    let mut alice_vault = Vault::init(
        "mirror-me".to_string(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = alice_vault.id();
    // A browser client is granted the vault *via* the hub: the secret is
    // sealed to the browser key, the hub is only the `via` host — so it
    // mirrors the chain without ever holding a Share or the vault secret.
    let browser = PrivateKey::generate().public();
    alice_vault
        .add_share_via(browser, Some(Identity::Key(hub_pk)))
        .expect("share via hub");

    // Some real content so the chain isn't trivial.
    alice_vault
        .fs()
        .mkdir(&AbsPath::new("/docs").unwrap(), false)
        .await
        .unwrap();
    alice_vault
        .fs()
        .add(
            &AbsPath::new("/docs/readme.md").unwrap(),
            std::io::Cursor::new(Bytes::from_static(b"mirror me\n").to_vec()),
        )
        .await
        .unwrap();
    let alice_head_link = alice_vault.save().await.expect("save");
    let alice_head = alice_vault.head().await.unwrap();

    // ── Pre-flight: hub has nothing about this vault yet. ──
    assert!(
        !hub.coord().log().exists(vault_id).await.unwrap(),
        "hub shouldn't know about this vault before pulling"
    );

    // ── Trigger the pull. The relay branch in `pull_from_peer`
    // detects `ShareNotFound` and delegates to
    // `zim_peer::relay_pull::apply_chain_log_only`. ──
    hub.coord()
        .execute(Effect::PullFromPeer {
            vault_id,
            peer_id: alice_pk,
        })
        .await
        .expect("hub pulls from alice");

    // ── Hub now has the vault in its log + the manifest blob in
    // its store. We assert against the raw log + blob store (no
    // Vault — by design, the hub can't construct one). ──
    assert!(
        hub.coord().log().exists(vault_id).await.unwrap(),
        "hub log should have the vault after pull"
    );
    let hub_head = hub
        .coord()
        .log()
        .head(vault_id, None)
        .await
        .expect("hub log head");
    assert_eq!(
        hub_head.height, alice_head.height,
        "hub head height matches alice"
    );
    assert_eq!(
        hub_head.link, alice_head_link,
        "hub head link matches alice"
    );
    assert!(
        hub.coord()
            .blobs()
            .stat(&hub_head.link.hash())
            .await
            .unwrap(),
        "hub blob store should contain the manifest blob"
    );

    // Alice still reads her own content fine through the same shared
    // blob store. Confirms we didn't corrupt anything by routing the
    // pull through a Share-less path.
    let content = alice_vault
        .fs()
        .cat(&AbsPath::new("/docs/readme.md").unwrap())
        .await
        .expect("alice reads her own content");
    assert_eq!(content, b"mirror me\n");

    alice.shutdown().await;
    hub.shutdown().await;
}
