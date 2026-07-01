//! Client-side API error.

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("URL parse: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("HTTP {0}: {1}")]
    HttpStatus(StatusCode, String),
    #[error("{0}")]
    Other(String),
}
