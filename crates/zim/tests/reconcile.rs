//! `reconcile_trusted` folds **trusted** contacts into the vaults you
//! authored — and leaves untrusted ones alone. Daemon-side: share
//! population is orchestration above the sync protocol.

use zim_core::blobs::BlobsProvider;
use zim_crypto::PrivateKey;
use zim_did::{Identity, StaticResolver};
use zim_peer::{MemoryVaultLog, Peer, PeerStore, SqlitePeerStore, Vault};

async fn make_peer(blobs: BlobsProvider) -> anyhow::Result<Peer<MemoryVaultLog>> {
    // Default accept policy (AcceptAll) — this test exercises outbound
    // share population, not inbound acceptance.
    Peer::builder()
        .with_secret(PrivateKey::generate())
        .with_log(MemoryVaultLog::new())
        .with_blobs(blobs)
        .build()
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn reconcile_shares_owned_vaults_with_trusted_contacts_only() {
    let blobs = BlobsProvider::memory().await.expect("blobs");
    let contacts = SqlitePeerStore::in_memory().expect("contacts");
    let resolver = StaticResolver::default();

    // Alice's contacts: her phone is *trusted*; Mallory is known but
    // untrusted. Both are `did:key`, so they resolve without any network.
    let phone = PrivateKey::generate().public();
    let mallory = PrivateKey::generate().public();
    contacts
        .upsert("phone", Identity::Key(phone), true, None)
        .await
        .expect("trust phone");
    contacts
        .upsert("mallory", Identity::Key(mallory), false, None)
        .await
        .expect("know mallory");

    let alice = make_peer(blobs.clone()).await.expect("alice");

    // Alice creates a vault — owner-only at genesis.
    let vault = Vault::init(
        "journal".to_string(),
        alice.secret(),
        alice.coord().blobs().clone(),
        alice.coord().log().clone(),
    )
    .await
    .expect("init");
    let vault_id = vault.id();
    assert!(vault.manifest().get_share(&phone).is_none());

    // Reconcile folds the trusted phone in and leaves Mallory out.
    let report = zim::reconcile::reconcile_trusted(&alice, &contacts, &resolver)
        .await
        .expect("reconcile");
    assert_eq!(report.vaults_scanned, 1, "one authored vault");
    assert_eq!(report.vaults_updated, 1, "it gained a share");
    assert_eq!(report.shares_added, 1, "exactly the trusted phone");

    let reopened = alice.vault(vault_id).await.expect("reopen");
    assert!(
        reopened.manifest().get_share(&phone).is_some(),
        "trusted phone is now a shareholder"
    );
    assert!(
        reopened.manifest().get_share(&mallory).is_none(),
        "untrusted Mallory was never shared"
    );

    // Idempotent: a second pass adds nothing.
    let again = zim::reconcile::reconcile_trusted(&alice, &contacts, &resolver)
        .await
        .expect("reconcile again");
    assert_eq!(again.shares_added, 0, "second sweep is a no-op");
}
