//! FUSE mount support for the daemon (behind the `fuse` feature).
//!
//! [`MountManager`] owns long-lived vault handles + their FUSE sessions;
//! [`MountStore`] persists the registrations. The CLI `mount` group and the
//! `/api/v0/mounts` endpoints drive this.

mod manager;
mod store;

pub use manager::{MountManager, MountStatus};
pub use store::{MountRecord, MountStore};
