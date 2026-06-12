//! Build-time information for the zim daemon.
//!
//! Populated by `build.rs` via `cargo:rustc-env=` directives. Surfaced
//! over the wire via `GET /_status/version` and to the CLI via
//! `zim version`.

use std::fmt;

use serde::{Deserialize, Serialize};

pub type Version = String;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildInfo {
    pub version: Version,
    pub git_hash: String,
    pub build_profile: String,
    pub build_timestamp: String,
    pub rust_version: String,
    pub target: String,
    pub host: String,
}

impl BuildInfo {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_hash: option_env!("REPO_VERSION").unwrap_or("unknown").to_string(),
            build_profile: option_env!("BUILD_PROFILE")
                .unwrap_or("unknown")
                .to_string(),
            build_timestamp: option_env!("BUILD_TIMESTAMP")
                .unwrap_or("unknown")
                .to_string(),
            rust_version: option_env!("RUST_VERSION").unwrap_or("unknown").to_string(),
            target: option_env!("BUILD_TARGET").unwrap_or("unknown").to_string(),
            host: option_env!("BUILD_HOST").unwrap_or("unknown").to_string(),
        }
    }

    pub fn is_debug(&self) -> bool {
        self.build_profile == "debug"
    }

    pub fn is_release(&self) -> bool {
        self.build_profile == "release"
    }

    pub fn short_hash(&self) -> &str {
        if self.git_hash.len() > 7 {
            &self.git_hash[..7]
        } else {
            &self.git_hash
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.git_hash.contains("-dirty")
    }
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) built with {} on {} for {}",
            self.version,
            self.short_hash(),
            self.rust_version,
            self.build_timestamp,
            self.target
        )
    }
}

pub fn build_info() -> BuildInfo {
    BuildInfo::new()
}

pub fn version() -> String {
    format!(
        "{} ({})",
        env!("CARGO_PKG_VERSION"),
        option_env!("REPO_VERSION").unwrap_or("unknown")
    )
}
