//! CLI module — args, op trait, ops, presentation.

pub mod args;
pub mod op;
pub mod ops;
pub mod ui;

pub use ops::{Daemon, Health, Id, Init, Peers, Vault, Version};
