//! Readiness data source — a trait so the readiness handler can be
//! exercised against mocks. Mirrors `_zim-peer/.../health/data_source`.
//!
//! For zim v1 the data source just checks that the service hasn't
//! signalled shutdown; future versions will add dependency probes
//! (sqlite reachability, vault loadability, etc.).
#![allow(dead_code)]

use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::daemon::state::ServiceState;

#[async_trait]
pub trait DataSource {
    async fn is_ready(&self) -> Result<(), DataSourceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DataSourceError {
    #[error("one or more dependent services aren't available")]
    DependencyFailure,
    #[error("service has received a shutdown signal")]
    ShuttingDown,
}

pub type DynDataSource = Arc<dyn DataSource + Send + Sync>;

pub struct StateDataSource(DynDataSource);

impl Debug for StateDataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateDataSource").finish()
    }
}

impl StateDataSource {
    #[cfg(test)]
    pub fn new(dds: DynDataSource) -> Self {
        Self(dds)
    }
}

impl Deref for StateDataSource {
    type Target = DynDataSource;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Always-ready source backed by the live `ServiceState`. Future
/// versions can probe sqlite, blobs, etc.
struct PeerSource;

#[async_trait]
impl DataSource for PeerSource {
    async fn is_ready(&self) -> Result<(), DataSourceError> {
        Ok(())
    }
}

#[async_trait]
impl FromRequestParts<ServiceState> for StateDataSource {
    type Rejection = ();

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &ServiceState,
    ) -> Result<Self, Self::Rejection> {
        Ok(StateDataSource(Arc::new(PeerSource)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[derive(Clone)]
    pub(crate) enum MockReadiness {
        DependencyFailure,
        Ready,
        ShuttingDown,
    }

    #[async_trait]
    impl DataSource for MockReadiness {
        async fn is_ready(&self) -> Result<(), DataSourceError> {
            match self {
                MockReadiness::DependencyFailure => Err(DataSourceError::DependencyFailure),
                MockReadiness::Ready => Ok(()),
                MockReadiness::ShuttingDown => Err(DataSourceError::ShuttingDown),
            }
        }
    }
}
