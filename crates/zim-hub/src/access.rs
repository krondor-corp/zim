//! Per-user access predicates.
//!
//! The hub serves one shared peer's blob store + log, so every
//! read-side handler asks: "does the requesting user own this
//! resource?" The answers live here so handlers stay one line.
//!
//! - [`can_access_vault`] — owner iff one of the user's registered
//!   peers is a shareholder on the vault's head manifest. Admin
//!   bypasses.
//! - [`can_access_escrow_did`] — owner iff the DID fragment's
//!   `u:<user_uuid>` segment matches the requesting user.

use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::vault::VaultId;
use zim_crypto::PublicKey;
use zim_peer::VaultLog;

use crate::database::models::{User, UserPeer};
use crate::database::Database;
use crate::state::AppState;

/// True iff the user owns the vault.
///
/// Reads the head manifest blob *directly* — the hub is a relay,
/// not a shareholder, so `Peer::vault(id)` would fail with
/// `ShareNotFound`. The manifest's `shares` map keys are
/// [`PublicKey`]s (the addressing for who can decrypt), not the
/// `SecretShare` payloads, so the JOIN against `user_peers` works
/// without decrypting anything. Lookup errors fall through to
/// `false` — the caller surfaces 404 to avoid leaking existence.
pub async fn can_access_vault(state: &AppState, user: &User, vault_id: VaultId) -> bool {
    if user.is_admin() {
        return true;
    }
    let shareholders = match read_manifest_shareholders(state, vault_id).await {
        Some(v) => v,
        None => return false,
    };
    can_access_vault_via_db(&state.db, user, &shareholders).await
}

/// Read the shareholder pubkey list from the vault's current head
/// manifest, without needing a `Share`. Used by access predicates +
/// the index handler's per-row filter; returns `None` on any lookup
/// error (the row is treated as "not visible").
pub async fn read_manifest_shareholders(
    state: &AppState,
    vault_id: VaultId,
) -> Option<Vec<PublicKey>> {
    read_manifest_meta(state, vault_id)
        .await
        .map(|m| m.shareholders)
}

/// Compact summary of a relay-mirrored vault's head manifest. Used
/// by the index handler so it can both filter by ownership AND show
/// the human-readable name without needing a `Share` to decrypt.
pub struct ManifestMeta {
    pub name: String,
    pub shareholders: Vec<PublicKey>,
}

/// Read `(name, shareholders)` from the vault's current head
/// manifest. Returns `None` if the vault isn't in the log, the head
/// link can't be looked up, or the manifest blob isn't local.
pub async fn read_manifest_meta(state: &AppState, vault_id: VaultId) -> Option<ManifestMeta> {
    let coord = state.peer.coord();
    let log = coord.log();
    if !log.exists(vault_id).await.unwrap_or(false) {
        return None;
    }
    let head_link = log.head(vault_id, None).await.ok()?.link;
    let manifest: Manifest = coord.blobs().get_cbor(&head_link).await.ok()?;
    Some(ManifestMeta {
        name: manifest.name().to_string(),
        shareholders: manifest.shares().iter().map(|(pk, _)| *pk).collect(),
    })
}

/// True iff the DID fragment belongs to the user. Admin bypasses.
///
/// Convention: every browser-side DID the hub hosts has the shape
/// `did:web:<hub_host>:u:<user_uuid>#<device_label>`. The middle
/// `u:<user_uuid>` segment is the authorization marker.
pub fn can_access_escrow_did(user: &User, did_fragment: &str) -> bool {
    if user.is_admin() {
        return true;
    }
    let Some(u_segment) = extract_user_segment(did_fragment) else {
        return false;
    };
    u_segment == user.id().to_string()
}

/// Variant for callers that already know the shareholder list and
/// have a [`Database`] handle — same logic minus the manifest read.
pub async fn can_access_vault_via_db(
    db: &Database,
    user: &User,
    shareholders: &[PublicKey],
) -> bool {
    if user.is_admin() {
        return true;
    }
    for pk in shareholders {
        match UserPeer::user_owns_pubkey(user.id(), pk, db).await {
            Ok(true) => return true,
            Ok(false) => continue,
            Err(e) => {
                tracing::warn!("user_owns_pubkey lookup failed: {e}");
                return false;
            }
        }
    }
    false
}

/// Pull the `u:<value>` segment out of `did:web:host:u:<value>#frag`.
fn extract_user_segment(did: &str) -> Option<&str> {
    let base = did.split('#').next()?;
    let parts: Vec<&str> = base.split(':').collect();
    if parts.len() < 5 {
        return None;
    }
    if parts[0] != "did" || parts[1] != "web" || parts[3] != "u" {
        return None;
    }
    Some(parts[4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_user_segment_with_fragment() {
        assert_eq!(
            extract_user_segment(
                "did:web:hub.example.com:u:11111111-1111-4111-a111-111111111111#laptop"
            ),
            Some("11111111-1111-4111-a111-111111111111")
        );
    }

    #[test]
    fn rejects_did_key() {
        assert!(
            extract_user_segment("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_did_web() {
        assert!(extract_user_segment("did:web:hub.example.com#nope").is_none());
        assert!(extract_user_segment("did:web:hub:other:value").is_none());
    }
}
