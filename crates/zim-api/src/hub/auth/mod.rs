//! Auth-facing surface: session introspection.
//!
//! - [`me`] — `GET /api/v0/me`, who's signed in (RequireUser).
//!
//! Approving a daemon's device code is a server-rendered hub page
//! (`/auth/device`), not a JSON route, so it has no wire types here.
//!
//! The JWT primitives (mint + verify) live in [`super::jwt`] — they're
//! auth *machinery*, not a route.

pub mod me;

pub use me::{Me, MeRequest};
