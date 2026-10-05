use std::fmt::{Debug, Display};

use crate::blobs::BlobError;
use crate::fs::{FsError, ManifestError, ShareError};

use super::log::VaultLogError;

/// Top-level error type for `Vault<B, L>` operations.
///
/// Composes the lower-level error types the Vault depends on. No
/// sync- or network-level variants — sync orchestration lives in
/// `zim-peer`, not here.
#[derive(thiserror::Error, Debug)]
pub enum VaultError<L: Display + Debug> {
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    /// Access: the local key has no share, or its share can't be
    /// unsealed. `Share(ShareError::NotFound)` is how a relay (hub)
    /// learns it holds the vault without being a member.
    #[error("share: {0}")]
    Share(#[from] ShareError),
    /// Boxed: `VaultLogError<L>` is the fat variant (≥200 bytes with a
    /// typical `L`) and would otherwise push every `Result<_, VaultError>`
    /// over clippy's `result_large_err` threshold. `From` is implemented
    /// by hand below so `?` on a `VaultLogError` still works unchanged.
    #[error("log: {0}")]
    Log(Box<VaultLogError<L>>),
    #[error("blob: {0}")]
    Blob(#[from] BlobError),
    #[error("manifest: {0}")]
    Manifest(#[from] ManifestError),
}

impl<L: Display + Debug> From<VaultLogError<L>> for VaultError<L> {
    fn from(e: VaultLogError<L>) -> Self {
        Self::Log(Box::new(e))
    }
}
