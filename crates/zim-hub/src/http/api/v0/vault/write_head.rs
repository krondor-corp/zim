//! `POST /api/v0/vaults/{vault_id}/head` — advance the vault head.
//!
//! The browser (or any authenticated client) submits the blake3 hash
//! of a new manifest blob it already pushed via `PUT /api/v0/blob`. The hub:
//!
//! 1. Fetches the manifest blob from the store (must already be there).
//! 2. Verifies the manifest signature.
//! 3. Verifies the author was a shareholder on the *previous* manifest
//!    (or on themselves for genesis).
//! 4. Checks chain continuity: `new.previous == current.link` and
//!    `new.height == current.height + 1`.
//!    Genesis vaults (no current head) are accepted unconditionally.
//! 5. Appends the new head to the log.
//! 6. Tags the manifest blob persistent (it would otherwise be GC'd).

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::linked_data::{Hash, Link, LD_CBOR_CODEC};
use zim_core::vault::{Head, VaultId};
use zim_peer::{Effect, VaultLog};
// Shared wire types — mirrored by `zim_api::hub::vault::WriteHeadRequest`.
use zim_api::hub::vault::{WriteHeadBody, WriteHeadResponse};

use crate::http::auth::RequireUser;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(_user): RequireUser,
    Path(vault_id): Path<VaultId>,
    Json(req): Json<WriteHeadBody>,
) -> Response {
    let hash = match Hash::from_str(&req.manifest_hash) {
        Ok(h) => h,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid manifest hash").into_response(),
    };
    let manifest_link = Link::new(LD_CBOR_CODEC, hash);

    let coord = state.peer.coord();
    let blobs = coord.blobs();
    let log = coord.log();

    // 1. Fetch the manifest blob — must already be in the store.
    let manifest: Manifest = match blobs.get_cbor(&manifest_link).await {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "manifest blob not found — push blobs first",
            )
                .into_response()
        }
    };

    // Fetch current head + manifest (if vault already exists in log).
    let current = if log.exists(vault_id).await.unwrap_or(false) {
        match log.head(vault_id, None).await {
            Ok(head) => match blobs.get_cbor::<Manifest, _>(&head.link).await {
                Ok(prev_manifest) => Some((head, prev_manifest)),
                Err(e) => {
                    tracing::error!("failed to fetch current head manifest: {e}");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "could not read current head",
                    )
                        .into_response();
                }
            },
            Err(e) => {
                tracing::error!("log.head: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "log error").into_response();
            }
        }
    } else {
        None
    };

    // 2 + 3. Signature + author-in-previous-shares.
    if let Err(e) = manifest.verify_author(current.as_ref().map(|(_, m)| m)) {
        return (
            StatusCode::BAD_REQUEST,
            format!("manifest verification failed: {e}"),
        )
            .into_response();
    }

    // 4. Chain continuity (non-genesis only).
    if let Some((ref head, ref prev_manifest)) = current {
        if manifest.previous() != &head.link {
            return (
                StatusCode::CONFLICT,
                "previous link does not match current head",
            )
                .into_response();
        }
        if manifest.height() != prev_manifest.height() + 1 {
            return (StatusCode::CONFLICT, "height must be current + 1").into_response();
        }
    }

    let previous_link = current.as_ref().map(|(head, _)| head.link.clone());

    // 5. Append to log.
    if let Err(e) = log
        .append(
            vault_id,
            manifest.name().to_string(),
            manifest_link.clone(),
            previous_link,
            manifest.height(),
        )
        .await
    {
        tracing::error!("log.append: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "log append failed").into_response();
    }

    // 6. Tag the manifest blob AND everything it pins (file-content blobs,
    //    dir bodies, the prior manifest) so none of them are GC'd. The
    //    manifest's `pins` set is exactly that closure.
    if let Err(e) = blobs.tag(hash).await {
        tracing::warn!("failed to tag manifest blob: {e}");
    }
    for pin in manifest.pins().iter() {
        if let Err(e) = blobs.tag(*pin).await {
            tracing::warn!(pin = %pin, "failed to tag pinned blob: {e}");
        }
    }

    tracing::info!(
        vault_id = %vault_id,
        height = manifest.height(),
        "head POST (vault advanced)"
    );

    // 7. Push the new head to the vault's shareholders over iroh so the
    //    user's daemons sync browser-side changes through the same protocol
    //    peers use with each other. Fire-and-forget.
    match log.head(vault_id, None).await {
        Ok(head) => announce_to_shareholders(&state, vault_id, head, &manifest).await,
        Err(e) => tracing::warn!(vault_id = %vault_id, "head fan-out skipped: {e}"),
    }

    (
        StatusCode::OK,
        Json(WriteHeadResponse {
            hash: req.manifest_hash,
            height: manifest.height(),
        }),
    )
        .into_response()
}

/// Push the freshly-advanced `head` to the vault's shareholders over iroh,
/// so the user's daemons learn about browser-side changes through the same
/// `HeadAdvanced` push peers use to sync with each other.
///
/// One message per share — dial the `via` host for a hosted client, else the
/// client itself, carrying `recipient = client` so a downstream relay knows
/// whose push it is. We skip the hub itself and the key that authored this head
/// (the browser writer already has it). Each daemon turns the push into a pull,
/// bootstrapping the vault if it's new (it accepts because the hub is in its
/// address book, via `zim hub login`). Fire-and-forget: dial failures (an
/// offline daemon) are logged, never surfaced.
async fn announce_to_shareholders(
    state: &AppState,
    vault_id: VaultId,
    head: Head,
    manifest: &Manifest,
) {
    let peer = &state.peer;
    let self_pk = peer.id();
    let author = *manifest.author();

    for (client, share) in manifest.shares().iter() {
        // `reach()` = where we dial for this share (the `via` host, else the
        // client); `client` = the recipient this push is for.
        let Some(target) = share.reach() else {
            continue;
        };
        if target == self_pk || target == author {
            continue;
        }
        if let Err(e) = peer
            .coord()
            .submit(Effect::AnnounceHead {
                peer_id: target,
                vault_id,
                head: Box::new(head.clone()),
                recipient: *client,
            })
            .await
        {
            tracing::warn!(
                peer = %target.to_hex(),
                vault_id = %vault_id,
                "failed to enqueue AnnounceHead: {e}"
            );
        }
    }
}
