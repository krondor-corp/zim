mod memory;
mod sqlite;

pub use memory::MemoryVaultLog;
pub use sqlite::SqliteVaultLog;

// Trait lifted to `zim_core::vault` (same surface). Re-exported here so
// existing `use crate::log::{VaultLog, VaultLogError}` keeps
// working through the transition.
pub use zim_core::vault::{VaultLog, VaultLogError};
