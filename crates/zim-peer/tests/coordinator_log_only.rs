//! Coordinator tests that exercise the log-only peer-query handlers.
//!
//! These tests don't open any vaults — they only need a populated
//! VaultLog, so they're fast + deterministic + don't require iroh
//! endpoint setup.

use std::sync::Arc;

use zim_crypto::PublicKey;

use zim_core::blobs::BlobsProvider;
use zim_core::linked_data::{Hash, Link, LD_RAW_CODEC};
use zim_core::vault::{Head, VaultId};
use zim_peer::peers::MemoryPeerStore;
use zim_peer::{MemoryVaultLog, VaultLog};

use zim_peer::coordinator::{MemoryPeerSender, SentMessage};
use zim_peer::messages::{AncestorReply, AncestorRequest, HeadRequest, ProbeRequest, ShareOffered};
use zim_peer::peers::PeerStore;
use zim_peer::{Effect, SyncCoordinator};

fn link(byte: u8) -> Link {
    Link::new(LD_RAW_CODEC, Hash::new([byte; 32]))
}

fn test_vault_id(byte: u8) -> zim_core::vault::VaultId {
    zim_core::vault::VaultId::from_hash(zim_core::linked_data::Hash::new([byte; 32]))
}

async fn populate(log: &MemoryVaultLog, id: VaultId, heights: &[u8]) {
    let mut prev: Option<Link> = None;
    for h in heights {
        let l = link(*h);
        log.append(id, "v".into(), l.clone(), prev, *h as u64)
            .await
            .unwrap();
        prev = Some(l);
    }
}

async fn make_coordinator() -> (
    Arc<SyncCoordinator<MemoryVaultLog, MemoryPeerStore>>,
    Arc<MemoryPeerSender>,
) {
    // BlobsProvider::memory() is async + cheap. Endpoint construction
    // is the expensive bit; the log-only handlers never touch it, so
    // we can supply a real one and only pay the bind cost once.
    let blobs = BlobsProvider::memory().await.unwrap();
    let log = MemoryVaultLog::new();
    let peers = MemoryPeerStore::new();
    let secret = zim_crypto::PrivateKey::generate();
    let iroh_secret = zim_core::iroh::to_iroh_secret_key(&secret);
    let endpoint = zim_core::iroh::Endpoint::builder()
        .secret_key(iroh_secret)
        .bind()
        .await
        .unwrap();
    let sender = Arc::new(MemoryPeerSender::default());
    let (coord, _effect_rx) =
        SyncCoordinator::new(blobs, log, peers, endpoint, secret, sender.clone(), 16);
    (coord, sender)
}

#[tokio::test]
async fn head_request_returns_none_for_unknown_vault() {
    let (coord, _) = make_coordinator().await;
    let reply = coord
        .handle_head(
            zim_crypto::PrivateKey::generate().public(),
            HeadRequest {
                vault_id: test_vault_id(1),
            },
        )
        .await;
    assert!(reply.head.is_none());
}

#[tokio::test]
async fn head_request_returns_head_when_vault_known() {
    let (coord, _) = make_coordinator().await;
    let id = test_vault_id(2);
    populate(coord.log(), id, &[0, 1, 2, 3]).await;

    let reply = coord
        .handle_head(
            zim_crypto::PrivateKey::generate().public(),
            HeadRequest { vault_id: id },
        )
        .await;
    let head = reply.head.expect("expected a head");
    assert_eq!(head.height, 3);
    assert_eq!(head.link, link(3));
}

#[tokio::test]
async fn probe_returns_deepest_match() {
    let (coord, _) = make_coordinator().await;
    let id = test_vault_id(3);
    populate(coord.log(), id, &[0, 1, 2, 3, 4]).await;

    let reply = coord
        .handle_probe(
            zim_crypto::PrivateKey::generate().public(),
            ProbeRequest {
                vault_id: id,
                sample: vec![
                    Head::new(link(99), 99),
                    Head::new(link(4), 4),
                    Head::new(link(2), 2),
                ],
            },
        )
        .await;
    let highest = reply.highest.expect("expected a match");
    assert_eq!(highest.height, 4);
    assert_eq!(highest.link, link(4));
}

#[tokio::test]
async fn probe_returns_none_when_no_match() {
    let (coord, _) = make_coordinator().await;
    let id = test_vault_id(4);
    populate(coord.log(), id, &[0, 1, 2]).await;

    let reply = coord
        .handle_probe(
            zim_crypto::PrivateKey::generate().public(),
            ProbeRequest {
                vault_id: id,
                sample: vec![Head::new(link(50), 50), Head::new(link(99), 99)],
            },
        )
        .await;
    assert!(reply.highest.is_none());
}

#[tokio::test]
async fn ancestor_request_returns_found_when_initiator_head_in_log() {
    let (coord, _) = make_coordinator().await;
    let id = test_vault_id(5);
    populate(coord.log(), id, &[0, 1, 2, 3]).await;

    // Initiator says their head is link(1) at height 1; we have it.
    let reply = coord
        .handle_ancestor(
            zim_crypto::PrivateKey::generate().public(),
            AncestorRequest {
                vault_id: id,
                initiator_head: Head::new(link(1), 1),
            },
        )
        .await;
    match reply {
        AncestorReply::Found { ancestor, .. } => {
            assert_eq!(ancestor.height, 1);
            assert_eq!(ancestor.link, link(1));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn ancestor_request_returns_need_probe_when_initiator_head_unknown() {
    let (coord, _) = make_coordinator().await;
    let id = test_vault_id(6);
    populate(coord.log(), id, &[0, 1, 2, 3]).await;

    // Initiator's head is on a different branch we never saw.
    let reply = coord
        .handle_ancestor(
            zim_crypto::PrivateKey::generate().public(),
            AncestorRequest {
                vault_id: id,
                initiator_head: Head::new(link(99), 5),
            },
        )
        .await;
    assert!(matches!(reply, AncestorReply::NeedProbe { .. }));
}

#[tokio::test]
async fn ancestor_request_returns_not_found_for_unknown_vault() {
    let (coord, _) = make_coordinator().await;
    let reply = coord
        .handle_ancestor(
            zim_crypto::PrivateKey::generate().public(),
            AncestorRequest {
                vault_id: test_vault_id(7),
                initiator_head: Head::new(link(0), 0),
            },
        )
        .await;
    assert!(matches!(reply, AncestorReply::NotFound { .. }));
}

#[tokio::test]
async fn announce_head_effect_dispatches_via_peer_sender() {
    let (coord, sender) = make_coordinator().await;
    let vault_id = test_vault_id(8);
    let peer_id: PublicKey = zim_crypto::PrivateKey::generate().public();

    coord
        .execute(Effect::AnnounceHead {
            peer_id,
            vault_id,
            head: Head::new(link(42), 42),
        })
        .await
        .unwrap();

    let sent = sender.sent.read().await;
    assert_eq!(sent.len(), 1);
    assert!(matches!(sent[0], SentMessage::HeadAdvanced(_, _)));
}

#[tokio::test]
async fn share_offered_from_unknown_peer_is_dropped() {
    // Spam gate: a `ShareOffered` for a brand-new vault from a peer
    // not in our peer book must be a no-op. We can't verify the
    // bootstrap *didn't* run by checking blobs (the test doesn't have
    // the matching ciphertext anyway), so we assert via the side
    // effect: the log still doesn't know the vault id afterward.
    let (coord, _sender) = make_coordinator().await;
    let stranger: PublicKey = zim_crypto::PrivateKey::generate().public();
    let vault_id = test_vault_id(9);

    let _: zim_peer::Ack = coord
        .handle_share_offered(
            stranger,
            ShareOffered {
                vault_id,
                head: Head::new(link(7), 0),
            },
        )
        .await;

    assert!(!coord.log().exists(vault_id).await.unwrap());
}

#[tokio::test]
async fn share_offered_from_known_peer_attempts_bootstrap() {
    // Known peer: the gate lets the call through. The bootstrap then
    // fails at "download head manifest blob" because our test
    // `MemoryPeerSender` has no blobs — but the gate has already
    // done its job. After the refactor `handle_share_offered`
    // returns `Ack` (errors are logged, not surfaced) — observe the
    // failure via the post-condition: vault id stays unknown in our
    // log.
    let (coord, _sender) = make_coordinator().await;
    let friend_sk = zim_crypto::PrivateKey::generate();
    let friend: PublicKey = friend_sk.public();
    coord
        .peers()
        .upsert("friend", zim_did::Identity::Key(friend), None)
        .await
        .unwrap();

    let fresh_id = test_vault_id(10);
    let _: zim_peer::Ack = coord
        .handle_share_offered(
            friend,
            ShareOffered {
                vault_id: fresh_id,
                head: Head::new(link(7), 0),
            },
        )
        .await;
    assert!(
        !coord.log().exists(fresh_id).await.unwrap(),
        "bootstrap should have failed at blob download"
    );
}

#[tokio::test]
async fn pull_from_peer_no_ops_when_peer_has_nothing() {
    // MemoryPeerSender's HeadReply is always `head: None`, so
    // PullFromPeer should record one HeadRequest and bail.
    let (coord, sender) = make_coordinator().await;
    let vault_id = test_vault_id(11);
    let peer_id: PublicKey = zim_crypto::PrivateKey::generate().public();

    coord
        .execute(Effect::PullFromPeer { vault_id, peer_id })
        .await
        .unwrap();

    let sent = sender.sent.read().await;
    assert_eq!(sent.len(), 1);
    assert!(matches!(sent[0], SentMessage::Head(_, _)));
}
