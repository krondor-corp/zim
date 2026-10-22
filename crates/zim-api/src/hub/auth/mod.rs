//! Auth-facing surface: session introspection + device-code approval.
//!
//! - [`me`] — `GET /api/v0/me`, who's signed in (RequireUser).
//! - [`grant`] — `/api/v0/auth/device-code/*`, the browser side of
//!   daemon device-code login (behind the `grant` capability).
//!
//! The JWT primitives (mint + verify) live in [`super::jwt`] — they're
//! auth *machinery*, not a route.

#[cfg(feature = "grant")]
pub mod grant;
pub mod me;

#[cfg(feature = "grant")]
pub use grant::{GrantApproveRequest, GrantInfo, GrantInfoRequest};
pub use me::{Me, MeRequest};
