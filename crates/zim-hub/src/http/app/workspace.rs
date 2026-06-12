//! `GET /app` — authenticated workspace landing.
//!
//! Wraps the existing index handler so all user-touching rendering
//! sits under `/app`. The hero on `/` is pure marketing; everything
//! that mentions an email, a vault, or admin links lives here.

pub use crate::http::html::index::handler;
