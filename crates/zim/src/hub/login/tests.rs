//! Device-code login against a mock hub: an axum server on
//! `127.0.0.1:0` serving `device-code/{start,poll}` and
//! `/.well-known/did.json`, with a switch standing in for Alice
//! clicking "approve" on the hub's `/auth/device` page.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use zim_api::hub::{DeviceCodePollBody, DeviceCodePollOk, DeviceCodeStart, DeviceCodeStartBody};
use zim_crypto::PrivateKey;
use zim_peer::PeerStore;

use super::{start, CompleteOptions, LoginError};
use crate::context::{paths, HubSession};

const CODE: &str = "ABCD-EFGH";

/// What the mock hub does when the daemon polls.
#[derive(Clone, Copy)]
enum Approval {
    /// Alice approves on the hub after seeing the first (pending) poll.
    AfterFirstPoll,
    /// Alice never approves; the grant expires on the hub.
    Never,
}

#[derive(Clone)]
struct MockHub {
    addr: SocketAddr,
    approval: Approval,
    /// The pubkey the daemon committed to at `start`.
    committed: Arc<Mutex<Option<String>>>,
    polls: Arc<Mutex<u32>>,
}

impl MockHub {
    async fn spawn(approval: Approval) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hub = MockHub {
            addr: listener.local_addr().unwrap(),
            approval,
            committed: Arc::default(),
            polls: Arc::default(),
        };
        let router = Router::new()
            .route("/api/v0/auth/device-code/start", post(start_code))
            .route("/api/v0/auth/device-code/poll", post(poll_code))
            .route("/.well-known/did.json", get(did_doc))
            .with_state(hub.clone());
        tokio::spawn(async move { axum::serve(listener, router).await });
        hub
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn did(&self) -> String {
        format!("did:web:127.0.0.1%3A{}", self.addr.port())
    }
}

async fn start_code(
    State(hub): State<MockHub>,
    Json(body): Json<DeviceCodeStartBody>,
) -> Json<DeviceCodeStart> {
    *hub.committed.lock().unwrap() = Some(body.pubkey);
    Json(DeviceCodeStart {
        code: CODE.to_string(),
        verification_url: format!("{}/auth/device?code={CODE}", hub.url()),
        expires_at: "2026-10-29T12:10:00Z".to_string(),
        poll_interval_secs: 1,
    })
}

async fn poll_code(State(hub): State<MockHub>, Json(body): Json<DeviceCodePollBody>) -> Response {
    assert_eq!(body.code, CODE);
    assert!(
        body.signature.is_some(),
        "daemon polls with its possession proof"
    );
    let polls = {
        let mut polls = hub.polls.lock().unwrap();
        *polls += 1;
        *polls
    };
    match hub.approval {
        Approval::AfterFirstPoll if polls > 1 => Json(DeviceCodePollOk {
            ok: true,
            pubkey: hub.committed.lock().unwrap().clone().unwrap(),
        })
        .into_response(),
        Approval::AfterFirstPoll => StatusCode::ACCEPTED.into_response(),
        Approval::Never => StatusCode::GONE.into_response(),
    }
}

async fn did_doc(State(hub): State<MockHub>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": hub.did() }))
}

/// A `$ZIM_HOME` with a fresh identity, as `zim init` leaves it.
fn alice_home() -> (tempfile::TempDir, PrivateKey) {
    let dir = tempfile::tempdir().unwrap();
    let key = PrivateKey::generate();
    std::fs::write(paths::identity_file(dir.path()), key.to_hex()).unwrap();
    (dir, key)
}

async fn peer_book(home: &Path) -> Vec<zim_peer::PeerEntry> {
    zim_peer::SqlitePeerStore::open(&paths::log_file(home))
        .unwrap()
        .list()
        .await
        .unwrap()
}

#[tokio::test]
async fn alice_pairs_laptop_after_approving_on_hub() {
    // --- Setup: Alice's laptop has an identity; the hub is up ---
    let (home, key) = alice_home();
    let hub = MockHub::spawn(Approval::AfterFirstPoll).await;

    // --- Alice starts the login and is shown what to approve ---
    let pending = start(home.path(), &format!("{}/", hub.url()), Some("laptop"))
        .await
        .unwrap();
    assert_eq!(pending.hub_url(), hub.url(), "trailing slash stripped");
    assert_eq!(pending.code(), CODE);
    assert_eq!(pending.pubkey_hex(), key.public().to_hex());
    assert!(pending.verification_url().contains(CODE));
    assert_eq!(pending.poll_interval_secs(), 1);

    // --- She approves on the hub; the laptop's poll enrolls it ---
    let out = pending
        .complete(CompleteOptions {
            timeout: Duration::from_secs(10),
            peer_nick: Some("hub".to_string()),
        })
        .await
        .unwrap();

    // --- The session marker is written for the enrolled key ---
    assert_eq!(out.enrolled_pubkey, key.public().to_hex());
    assert_eq!(out.session_path, paths::hub_session_file(home.path()));
    let session: HubSession =
        serde_json::from_str(&std::fs::read_to_string(&out.session_path).unwrap()).unwrap();
    assert_eq!(session.hub_url, hub.url());
    assert_eq!(session.enrolled_pubkey, key.public().to_hex());

    // --- …and the hub is an untrusted contact under the hub's own DID ---
    assert_eq!(out.hub_did.as_deref(), Some(hub.did().as_str()));
    assert_eq!(out.hub_nick.as_deref(), Some("hub"));
    let peers = peer_book(home.path()).await;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].nick, "hub");
    assert_eq!(peers[0].identity.to_string(), hub.did());
    assert!(
        !peers[0].trusted,
        "a hub relays; it never holds vault shares"
    );
}

#[tokio::test]
async fn login_expires_when_alice_never_approves() {
    // --- Setup: Alice starts a login but walks away ---
    let (home, _key) = alice_home();
    let hub = MockHub::spawn(Approval::Never).await;
    let pending = start(home.path(), &hub.url(), None).await.unwrap();

    // --- The hub reports the code gone; the login fails cleanly ---
    let err = pending
        .complete(CompleteOptions {
            timeout: Duration::from_secs(10),
            peer_nick: Some("hub".to_string()),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, LoginError::Expired), "got {err:?}");

    // --- Nothing is left behind in $ZIM_HOME ---
    assert!(!paths::hub_session_file(home.path()).exists());
    assert!(!paths::log_file(home.path()).exists());
}

#[tokio::test]
async fn no_peer_add_leaves_peer_book_untouched() {
    // --- Setup: Alice only wants to enroll, not route mirrors ---
    let (home, _key) = alice_home();
    let hub = MockHub::spawn(Approval::AfterFirstPoll).await;
    let pending = start(home.path(), &hub.url(), None).await.unwrap();

    // --- She approves; the login completes without a peer nick ---
    let out = pending
        .complete(CompleteOptions {
            timeout: Duration::from_secs(10),
            peer_nick: None,
        })
        .await
        .unwrap();

    // --- Session written, peer book never opened ---
    assert!(out.session_path.exists());
    assert_eq!(out.hub_did, None);
    assert_eq!(out.hub_nick, None);
    assert!(!paths::log_file(home.path()).exists());
}
