//! Hub inbound acceptance policy.
//!
//! The hub is a multi-tenant relay, so its acceptance question is
//! different from a daemon's: it can't answer "should I store this?"
//! from the sender alone. It gates on **both** ends against its
//! `user_peers` registry (a different "peer book" from the daemon's
//! contacts):
//!
//! - **recipient ∈ hosted devices** — the push is destined for a peer
//!   the hub relays for.
//! - **sender ∈ controlled devices** — the pusher is a device enrolled
//!   to the hub.
//!
//! Gating the recipient (not just the sender) is what lets same-hub
//! cross-user sharing work — Alice's enrolled daemon pushing a share to
//! Bob's enrolled browser is accepted because both are hosted, while a
//! foreign internet peer pushing to a hosted browser is rejected.
//!
//! The check runs on **every** push (the coordinator ignores
//! `known_vault` here), so a first-time share to a new recipient on an
//! *existing* vault is still seen — that arrives as an advance, not a
//! new vault.

use async_trait::async_trait;
use zim_crypto::PublicKey;
use zim_peer::{AcceptPolicy, IncomingSync};

use crate::database::models::UserPeer;
use crate::database::Database;

pub struct HubAcceptPolicy {
    db: Database,
}

impl HubAcceptPolicy {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Whether `pk` is a device the hub controls/hosts (enrolled in
    /// `user_peers`, any user). A lookup failure is treated as "no".
    async fn enrolled(&self, pk: &PublicKey) -> bool {
        match UserPeer::find_by_pubkey(pk, &self.db).await {
            Ok(found) => found.is_some(),
            Err(e) => {
                tracing::warn!(pubkey = %pk.to_hex(), "user_peers lookup failed: {e}");
                false
            }
        }
    }
}

#[async_trait]
impl AcceptPolicy for HubAcceptPolicy {
    async fn accept_blob(&self, sender: &PublicKey) -> bool {
        // Ciphertext blobs are served only to enrolled devices (any
        // user) — the mirror is for its tenants, not the open internet.
        self.enrolled(sender).await
    }

    async fn accept_sync(&self, sync: &IncomingSync) -> bool {
        // Only relay for a hosted recipient, and only from a device we
        // control. Both must be enrolled.
        let recipient_ok = self.enrolled(&sync.recipient).await;
        let sender_ok = self.enrolled(&sync.sender).await;
        let accepted = recipient_ok && sender_ok;
        if !accepted {
            tracing::info!(
                sender = %sync.sender.to_hex(),
                recipient = %sync.recipient.to_hex(),
                vault_id = %sync.vault_id,
                sender_enrolled = sender_ok,
                recipient_enrolled = recipient_ok,
                "hub dropped push (sender or recipient not enrolled)"
            );
        }
        accepted
    }
}
