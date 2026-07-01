//! Daemon-side SQLite store shared by every table the peer keeps in
//! `log.sqlite`: the vault log ([`crate::log::SqliteVaultLog`]) and the
//! contacts address book ([`crate::peers::SqlitePeerStore`]). One
//! connection, one migration set — adding a table is a new `M::up` in
//! [`migrations`], not a second database.

mod client;
mod error;
mod migrations;

pub(crate) use client::Database;
pub use error::DatabaseError;
pub(crate) use error::Result;
