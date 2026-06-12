use std::fmt::{Debug, Display};

use crate::blobs::BlobError;
use crate::fs::{FsError, ManifestError};

use super::log::VaultLogError;

/// Top-level error type for `Vault<B, L>` operations.
///
/// Composes the four lower-level error types the Vault depends on. No
/// sync- or network-level variants — sync orchestration lives in
/// `zim-peer`, not here.
#[derive(thiserror::Error, Debug)]
pub enum VaultError<L: Display + Debug> {
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    #[error("log: {0}")]
    Log(#[from] VaultLogError<L>),
    #[error("blob: {0}")]
    Blob(#[from] BlobError),
    #[error("manifest: {0}")]
    Manifest(#[from] ManifestError),
}
