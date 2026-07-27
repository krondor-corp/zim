//! Daemon inbound acceptance policy.
//!
//! Implements [`zim_peer::AcceptPolicy`] against the daemon's contacts
//! book. A push about a vault we already mirror is always accepted (the
//! sender passed the chain-validity check at our last sync); a push
//! about a **new** vault is accepted only when the sender is someone we
//! know. The recipient is trivially ourselves, so it isn't checked —
//! that distinction matters for the hub, not a single-identity daemon.

use std::sync::Arc;

use async_trait::async_trait;
use zim_crypto::PublicKey;
use zim_did::DidResolver;
use zim_peer::{AcceptPolicy, IncomingSync, PeerStore, SqlitePeerStore};

pub struct ContactsAcceptPolicy {
    contacts: SqlitePeerStore,
    resolver: Arc<dyn DidResolver>,
}

impl ContactsAcceptPolicy {
    pub fn new(contacts: SqlitePeerStore, resolver: Arc<dyn DidResolver>) -> Self {
        Self { contacts, resolver }
    }

    /// Whether `sender` is in our contacts — DID-aware.
    ///
    /// Fast path is the store's own [`PeerStore::knows`] (a direct
    /// pubkey match, covering `did:key` entries). A `did:web` entry
    /// carries no inline pubkey, so we resolve it and match `sender`
    /// against a resolved client or its `via` host — that's what lets a
    /// daemon recognise its hub (`zim hub login` records the hub as a
    /// `did:web`, and the hub dials from the key it resolves to).
    /// Resolution only fires for hosted entries, on a new-vault push, so
    /// it's cheap.
    async fn knows(&self, sender: &PublicKey) -> bool {
        match self.contacts.knows(sender).await {
            Ok(true) => return true,
            Ok(false) => {}
            Err(e) => tracing::warn!(sender = %sender.to_hex(), "contacts knows() failed: {e}"),
        }
        let entries = match self.contacts.list().await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(sender = %sender.to_hex(), "contacts list() failed: {e}");
                return false;
            }
        };
        for entry in entries {
            // did:key entries were covered by `knows()`; only hosted
            // (did:web) entries need resolution.
            if entry.identity.pubkey().is_some() {
                continue;
            }
            match zim_did::resolve_reaches(&entry.identity, &*self.resolver).await {
                Ok(reaches) => {
                    for r in &reaches {
                        if &r.client == sender || r.via.as_ref().map(|(_, h)| h) == Some(sender) {
                            return true;
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(peer = %entry.nick, "resolve contact for accept gate: {e}")
                }
            }
        }
        false
    }
}

#[async_trait]
impl AcceptPolicy for ContactsAcceptPolicy {
    async fn accept_blob(&self, sender: &PublicKey) -> bool {
        // Serve blobs only to contacts (DID-aware — a hosted contact
        // matches via its resolved host key too). Known limitation: a
        // co-shareholder we never added to the book is denied here and
        // falls back to fetching from the author or the hub — the
        // download path tries every provider and tolerates refusals.
        let accepted = self.knows(sender).await;
        if !accepted {
            tracing::info!(
                sender = %sender.to_hex(),
                "blob fetch refused (sender not in contacts)"
            );
        }
        accepted
    }

    async fn accept_sync(&self, sync: &IncomingSync) -> bool {
        // A vault we already hold fast-forwards unconditionally.
        if sync.known_vault {
            return true;
        }
        let accepted = self.knows(&sync.sender).await;
        if !accepted {
            tracing::info!(
                sender = %sync.sender.to_hex(),
                vault_id = %sync.vault_id,
                "new-vault push dropped (sender not in contacts)"
            );
        }
        accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_crypto::PrivateKey;
    use zim_did::{Did, StaticResolver};

    fn vault_id() -> zim_core::vault::VaultId {
        zim_core::vault::VaultId::from_hash(zim_core::linked_data::Hash::new(&[1u8; 32]))
    }

    fn sync(sender: PublicKey, known_vault: bool) -> IncomingSync {
        IncomingSync {
            sender,
            recipient: sender,
            vault_id: vault_id(),
            known_vault,
        }
    }

    #[tokio::test]
    async fn known_vault_is_accepted_even_from_a_stranger() {
        let policy = ContactsAcceptPolicy::new(
            SqlitePeerStore::in_memory().unwrap(),
            Arc::new(StaticResolver::default()),
        );
        let stranger = PrivateKey::generate().public();
        assert!(policy.accept_sync(&sync(stranger, true)).await);
    }

    #[tokio::test]
    async fn new_vault_accepts_a_known_contact_rejects_a_stranger() {
        let contacts = SqlitePeerStore::in_memory().unwrap();
        let friend = PrivateKey::generate().public();
        contacts
            .upsert("friend", Did::from_key(&friend), false, None)
            .await
            .unwrap();
        let policy = ContactsAcceptPolicy::new(contacts, Arc::new(StaticResolver::default()));

        assert!(
            policy.accept_sync(&sync(friend, false)).await,
            "a known contact bootstraps a new vault"
        );
        let stranger = PrivateKey::generate().public();
        assert!(
            !policy.accept_sync(&sync(stranger, false)).await,
            "a stranger is dropped"
        );
    }
}
