//! The desktop shell's IPC contract: what the SPA sends to, and gets
//! back from, the Tauri commands in `zim-desktop`.
//!
//! One definition, compiled by both ends — the shell (native) and the
//! SPA (wasm) — so ids, paths, keys and errors cross as domain types
//! ([`VaultId`], [`AbsPath`], [`PublicKey`], [`CommandError`]), never as
//! hand-matched JSON. Each command takes a single `args` struct.
//!
//! The one exception is file bytes on `vault_add`: a raw IPC body can't
//! carry a struct, so the vault and path ride in headers, built and
//! parsed in one place ([`AddHeaders`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;
use zim_crypto::PublicKey;

use crate::Url;

/// Command names — the shell registers them, the SPA invokes them.
pub mod cmd {
    pub const BOOT: &str = "boot";
    pub const DEFAULT_HUB: &str = "default_hub";
    pub const HUB_LOGIN_START: &str = "hub_login_start";
    pub const HUB_LOGIN_COMPLETE: &str = "hub_login_complete";
    pub const VAULT_LIST: &str = "vault_list";
    pub const VAULT_CREATE: &str = "vault_create";
    pub const VAULT_HEAD: &str = "vault_head";
    pub const VAULT_LS: &str = "vault_ls";
    pub const VAULT_CAT: &str = "vault_cat";
    pub const VAULT_ADD: &str = "vault_add";
    pub const VAULT_MKDIR: &str = "vault_mkdir";
    pub const VAULT_RM: &str = "vault_rm";
    pub const VAULT_MV: &str = "vault_mv";
}

/// Why a command failed. The SPA shows it ([`Display`](std::fmt::Display))
/// and can match on the variant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum CommandError {
    /// The IPC call itself failed — no desktop shell, or it didn't answer.
    #[error("the desktop shell didn't answer: {0}")]
    Ipc(String),
    /// A vault command ran before `boot` connected to the daemon.
    #[error("the zim daemon isn't connected yet")]
    NotConnected,
    /// Launching or connecting to the daemon failed.
    #[error("{0}")]
    Boot(String),
    /// The daemon answered with an error status.
    #[error("the zim daemon refused ({status}): {message}")]
    Daemon { status: u16, message: String },
    /// The daemon couldn't be reached at all.
    #[error("could not reach the zim daemon: {0}")]
    Unreachable(String),
    /// `hub_login_complete` without a `hub_login_start`.
    #[error("no hub login in progress")]
    NoLoginInProgress,
    /// The device-code login failed (hub unreachable, denied, expired).
    #[error("hub login failed: {0}")]
    Login(String),
    /// An argument the shell couldn't use.
    #[error("bad request: {0}")]
    BadRequest(String),
}

/// What `boot` learned. Safe to call again after a reload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootInfo {
    pub home: PathBuf,
    /// This device's identity — the same key the CLI shows.
    pub peer_id: PublicKey,
    pub port: u16,
    /// The identity was minted by this launch.
    pub first_run: bool,
    /// The app started the daemon (vs. attaching to a running one).
    pub started_daemon: bool,
    /// A hub session is already on disk.
    pub hub_linked: bool,
}

/// A vault in the daemon's listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSummary {
    pub id: VaultId,
    pub name: String,
}

/// A vault's current head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub id: VaultId,
    pub height: u64,
    pub link: Link,
}

/// A device-code login waiting for approval on the hub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairInfo {
    pub hub_url: Url,
    pub verification_url: Url,
    pub code: String,
    pub pubkey: PublicKey,
}

/// A finished hub login.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginInfo {
    pub hub_url: Url,
}

/// `hub_login_start`'s argument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginStart {
    pub hub_url: Url,
}

/// `vault_create`'s argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateVault {
    pub name: String,
}

/// An argument naming just a vault (`vault_head`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRef {
    pub vault: VaultId,
}

/// An argument naming a path in a vault (`vault_ls`, `vault_cat`,
/// `vault_mkdir`, `vault_rm`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultPath {
    pub vault: VaultId,
    pub path: AbsPath,
}

/// `vault_mv`'s argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultMove {
    pub vault: VaultId,
    pub from: AbsPath,
    pub to: AbsPath,
}

/// Where `vault_add` carries its target, since the body is the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddHeaders {
    pub vault: VaultId,
    pub path: AbsPath,
}

const VAULT_HEADER: &str = "x-zim-vault";
const PATH_HEADER: &str = "x-zim-path";

impl AddHeaders {
    /// `(name, value)` pairs to put on the request. The path is
    /// percent-encoded: header values can't carry raw UTF-8.
    pub fn to_pairs(&self) -> [(&'static str, String); 2] {
        [
            (VAULT_HEADER, self.vault.to_string()),
            (
                PATH_HEADER,
                percent_encode(&self.path.as_ref().to_string_lossy()),
            ),
        ]
    }

    /// Parse them back, looking headers up with `get`.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, CommandError> {
        let bad = |m: &str| CommandError::BadRequest(m.to_string());
        let vault = get(VAULT_HEADER)
            .ok_or_else(|| bad("missing vault header"))?
            .parse()
            .map_err(|e| CommandError::BadRequest(format!("bad vault id: {e}")))?;
        let raw = get(PATH_HEADER).ok_or_else(|| bad("missing path header"))?;
        let path = percent_decode(&raw)
            .and_then(AbsPath::new)
            .ok_or_else(|| bad("path header isn't an absolute path"))?;
        Ok(Self { vault, path })
    }
}

/// Unreserved characters and `/` pass through; everything else is `%XX`.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_notes_with_spaces_and_unicode_survive_the_header_round_trip() {
        // Alice adds "/Notes/café plan.md" to her vault.
        let sent = AddHeaders {
            vault: VaultId::default(),
            path: AbsPath::new("/Notes/café plan.md").unwrap(),
        };

        let pairs = sent.to_pairs();
        let got = AddHeaders::from_lookup(|name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
        })
        .unwrap();

        assert_eq!(got, sent);
        // Headers stay plain ASCII.
        assert!(pairs[1].1.is_ascii());
    }

    #[test]
    fn bob_gets_a_typed_error_for_a_relative_path_header() {
        let err = AddHeaders::from_lookup(|name| match name {
            VAULT_HEADER => Some(VaultId::default().to_string()),
            _ => Some("notes.md".to_string()),
        })
        .unwrap_err();

        assert!(matches!(err, CommandError::BadRequest(_)));
    }

    #[test]
    fn errors_keep_their_variant_across_the_ipc_boundary() {
        let sent = CommandError::Daemon {
            status: 404,
            message: "no such vault".into(),
        };

        let wire = serde_json::to_string(&sent).unwrap();
        let got: CommandError = serde_json::from_str(&wire).unwrap();

        assert_eq!(got, sent);
    }
}
