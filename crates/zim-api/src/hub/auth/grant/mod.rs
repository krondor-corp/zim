//! `/api/v0/auth/device-code/*`, browser side — the signed-in user's
//! approve page for daemon device-code login. (The daemon-side
//! start/poll flow speaks its own request types in `zim`.)

pub mod approve;
pub mod info;

pub use approve::GrantApproveRequest;
pub use info::GrantInfo;
pub use info::GrantInfoRequest;
