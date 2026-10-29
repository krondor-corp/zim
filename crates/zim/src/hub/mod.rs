//! Hub integration as a library — the parts of `zim hub` a non-CLI
//! front end (e.g. a desktop app sharing the same `$ZIM_HOME`) drives
//! with its own UI.
//!
//! - [`login`] — device-code pairing: [`login::start`] then
//!   [`login::PendingLogin::complete`].

pub mod login;
