//! Auth-facing surface: session introspection + daemon pairing.
//!
//! - [`me`] — `GET /api/v0/me`, who's signed in (RequireUser).
//! - [`device_code`] — `POST /api/v0/auth/device-code/{start,poll}`, the
//!   daemon side of device-code pairing (public).
//!
//! *Approving* a daemon's device code is a server-rendered hub page
//! (`/auth/device`), not a JSON route, so it has no wire types here.
//!
//! The JWT primitives (mint + verify) live in [`super::jwt`] — they're
//! auth *machinery*, not a route.

pub mod device_code;
pub mod me;

pub use device_code::{
    DeviceCodePollBody, DeviceCodePollOk, DeviceCodePollRequest, DeviceCodeStart,
    DeviceCodeStartBody, DeviceCodeStartRequest,
};
pub use me::{Me, MeRequest};
