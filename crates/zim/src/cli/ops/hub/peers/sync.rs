//! `zim hub peers sync` — fold the hub's device roster into the local
//! contacts book.
//!
//! Reads `GET /api/v0/devices` (every key enrolled to this account, via
//! `zim_api::hub`) and upserts each device other than this daemon into
//! the `contacts` table as a **trusted** contact — these are *your own*
//! devices, so they're auto-shared into the vaults you own. Idempotent:
//! devices already in the book (matched by DID) are left alone, so
//! re-running only adds what's new.

use std::collections::BTreeSet;
use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_api::hub::{device_did, device_nick};
use zim_did::Did;
use zim_peer::{PeerStore, SqlitePeerStore};

use crate::cli::op::Op;
use crate::cli::ops::hub::{load_hub_client, HubSessionError};
use crate::cli::ui;
use crate::context::paths;

#[derive(Args, Debug, Clone)]
pub struct Sync {}

#[derive(Debug, serde::Serialize)]
pub struct SyncOutput {
    pub hub_url: String,
    /// `(nick, did)` for each device freshly added this run.
    pub added: Vec<(String, String)>,
    /// Count of roster devices already present in the book.
    pub already: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Session(#[from] HubSessionError),
    #[error(transparent)]
    Api(#[from] zim_api::ApiError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("contacts: {0}")]
    Contacts(String),
}

#[async_trait]
impl Op for Sync {
    type Context = ();
    type Output = SyncOutput;
    type Error = SyncError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<SyncOutput, SyncError> {
        let client = load_hub_client()?;
        let self_hex = client.self_pubkey_hex();
        let devices = client.devices().await?;

        // The hub's own DID — the `via` we stamp on browser/web devices so
        // the daemon reaches them *through* the hub, not by dialing the
        // browser. Best-effort: if it can't be fetched, web devices fall
        // back to direct (the old behaviour).
        let hub_via: Option<Did> = client
            .did_doc("/.well-known/did.json")
            .await
            .ok()
            .and_then(|doc| Did::parse(&doc.id).ok());

        let home = paths::home_dir(None)?;
        let store = SqlitePeerStore::open(&paths::log_file(&home))
            .map_err(|e| SyncError::Contacts(e.to_string()))?;
        let known: BTreeSet<String> = store
            .list()
            .await
            .map_err(|e| SyncError::Contacts(e.to_string()))?
            .into_iter()
            .map(|e| e.identity.to_string())
            .collect();

        let mut added = Vec::new();
        let mut already = 0usize;
        for d in &devices {
            // Never add ourselves to our own address book.
            if d.pubkey == self_hex {
                continue;
            }
            let Some(did) = device_did(d) else { continue };
            let Ok(identity) = Did::parse(&did) else {
                continue;
            };
            let nick = device_nick(d);
            // Web/browser devices are reached *via the hub*; daemons are
            // dialed directly. Your own enrolled devices are trusted, so
            // they auto-share into the vaults you own.
            let via = if d.kind == "web" {
                hub_via.clone()
            } else {
                None
            };
            // Always upsert (not just for new DIDs): idempotent, and it
            // keeps `via`/`trusted` current if a device's kind changed or
            // it predates `via` support. `known` only drives the report.
            store
                .upsert_via(
                    &nick,
                    identity,
                    via,
                    true,
                    Some(format!("hub device ({})", d.kind)),
                )
                .await
                .map_err(|e| SyncError::Contacts(e.to_string()))?;
            if known.contains(&did) {
                already += 1;
            } else {
                added.push((nick, did));
            }
        }

        Ok(SyncOutput {
            hub_url: client.hub_url().to_string(),
            added,
            already,
        })
    }
}

impl fmt::Display for SyncOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {}",
            ui::success("synced", "hub peers"),
            ui::dim(&self.hub_url)
        )?;
        for (nick, did) in &self.added {
            writeln!(f, "  + {} {}", ui::ident(nick), ui::dim(did))?;
        }
        if self.added.is_empty() {
            write!(
                f,
                "  {} ({} already known)",
                ui::dim("no new devices"),
                self.already
            )?;
        } else {
            write!(
                f,
                "  {} added, {} already known",
                ui::num(self.added.len().to_string()),
                self.already
            )?;
        }
        Ok(())
    }
}
