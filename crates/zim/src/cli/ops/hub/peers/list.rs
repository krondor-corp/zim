//! `zim hub peers ls` — show the hub's device roster, marking which
//! devices are already in the local address book.
//!
//! Read-only counterpart to `sync`: it never writes the contacts table,
//! just reports the roster (from `zim_api::hub`) and whether you'd pick
//! each device up.

use std::collections::BTreeSet;
use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_api::hub::device_did;
use zim_peer::{PeerStore, SqlitePeerStore};

use crate::cli::op::Op;
use crate::cli::ops::hub::{load_hub_client, HubSessionError};
use crate::cli::ui;
use crate::context::paths;

#[derive(Args, Debug, Clone)]
pub struct List {}

#[derive(Debug, serde::Serialize)]
pub struct ListedDevice {
    pub label: Option<String>,
    pub kind: String,
    pub did: String,
    pub pubkey: String,
    /// Already present in the local address book (matched by DID).
    pub in_book: bool,
    /// This daemon itself.
    pub is_self: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct ListOutput {
    pub hub_url: String,
    pub devices: Vec<ListedDevice>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
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
impl Op for List {
    type Context = ();
    type Output = ListOutput;
    type Error = ListError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<ListOutput, ListError> {
        let client = load_hub_client()?;
        let self_hex = client.self_pubkey_hex();
        let devices = client.devices().await?;

        let home = paths::home_dir(None)?;
        let store = SqlitePeerStore::open(&paths::log_file(&home))
            .map_err(|e| ListError::Contacts(e.to_string()))?;
        let known: BTreeSet<String> = store
            .list()
            .await
            .map_err(|e| ListError::Contacts(e.to_string()))?
            .into_iter()
            .map(|e| e.identity.to_string())
            .collect();

        let listed = devices
            .iter()
            .map(|d| {
                let did = device_did(d).unwrap_or_default();
                ListedDevice {
                    label: d.label.clone(),
                    kind: d.kind.clone(),
                    in_book: known.contains(&did),
                    is_self: d.pubkey == self_hex,
                    did,
                    pubkey: d.pubkey.clone(),
                }
            })
            .collect();

        Ok(ListOutput {
            hub_url: client.hub_url().to_string(),
            devices: listed,
        })
    }
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", ui::ident("hub devices"), ui::dim(&self.hub_url))?;
        if self.devices.is_empty() {
            return write!(f, "  {}", ui::dim("(none enrolled)"));
        }
        for d in &self.devices {
            let status = if d.is_self {
                ui::dim("· self    ")
            } else if d.in_book {
                ui::success("✓ in book", "")
            } else {
                ui::warning("✗ missing", "")
            };
            let name = d.label.as_deref().unwrap_or("(unlabeled)");
            let short = &d.pubkey[..d.pubkey.len().min(12)];
            writeln!(
                f,
                "  {}  {} {} {}",
                status,
                ui::ident(name),
                ui::dim(&d.kind),
                ui::dim(short)
            )?;
        }
        Ok(())
    }
}
