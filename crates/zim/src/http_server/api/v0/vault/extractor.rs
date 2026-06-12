//! `VaultHandle` — axum extractor that resolves `/vault/:vault_id/...`
//! to an open [`Vault`]. Every per-vault handler takes a
//! `VaultHandle` in its signature; the path → open dance happens
//! once here, not in every handler body.

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use zim_core::vault::VaultId;
use zim_peer::{SqliteVaultLog, Vault};

use crate::service_state::vault_lookup_response;
use crate::ServiceState;

#[derive(Deserialize)]
struct VaultIdPath {
    vault_id: VaultId,
}

/// An open vault, plus its id, extracted from the URL path.
pub struct VaultHandle {
    pub id: VaultId,
    pub vault: Vault<SqliteVaultLog>,
}

#[async_trait]
impl FromRequestParts<ServiceState> for VaultHandle {
    /// Sum of every failure mode this extractor can produce: the
    /// `Path<VaultId>` extractor's own rejection (malformed segment →
    /// 400 with axum's own message) and `Peer::vault`'s lookup
    /// failures (`NotFound` → 404, `Backing` → 500). Both already
    /// know how to render themselves; the extractor just routes.
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServiceState,
    ) -> Result<Self, Self::Rejection> {
        let Path(VaultIdPath { vault_id }) = Path::<VaultIdPath>::from_request_parts(parts, state)
            .await
            .map_err(|e| e.into_response())?;
        let vault = state
            .peer()
            .vault(vault_id)
            .await
            .map_err(vault_lookup_response)?;
        Ok(VaultHandle {
            id: vault_id,
            vault,
        })
    }
}
