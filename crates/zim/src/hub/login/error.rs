//! Failure modes of the device-code login flow.

/// Why [`start`](super::start) or
/// [`PendingLogin::complete`](super::PendingLogin::complete) failed.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("identity decode: {0}")]
    Identity(String),
    #[error("hub request: {0}")]
    Hub(String),
    #[error("hub returned {0}: {1}")]
    HubStatus(reqwest::StatusCode, String),
    #[error("device code expired before approval")]
    Expired,
    #[error("login timed out after {0}s")]
    Timeout(u64),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("peer book: {0}")]
    PeerBook(String),
    #[error("invalid hub url: {0}")]
    BadHubUrl(String),
}

impl From<zim_api::ApiError> for LoginError {
    fn from(e: zim_api::ApiError) -> Self {
        match e {
            zim_api::ApiError::HttpStatus(status, body) => LoginError::HubStatus(status, body),
            other => LoginError::Hub(other.to_string()),
        }
    }
}
