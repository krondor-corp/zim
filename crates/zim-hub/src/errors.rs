use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::peer_client::PeerError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("template render failed: {0}")]
    Template(#[from] askama::Error),
    #[error("peer error: {0}")]
    Peer(#[from] PeerError),
    #[error("not found")]
    NotFound,
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Error::Template(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Peer(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        };
        (status, self.to_string()).into_response()
    }
}
