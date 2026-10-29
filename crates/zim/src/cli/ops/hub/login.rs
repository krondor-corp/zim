//! `zim hub login --hub <url>` — pair this daemon with a hub.
//!
//! A thin front end over the library flow in [`crate::hub::login`]:
//! [`start`](crate::hub::login::start) the device-code grant, show the
//! user where to approve it, then
//! [`complete`](crate::hub::login::PendingLogin::complete) the login.
//!
//! The pairing instructions have to reach the user *before* polling
//! finishes, so they can't ride in the op's final output. They're the
//! one mid-run message this op emits: a typed [`PairInstructions`] whose
//! `Display` owns the formatting, written once to stderr (stdout stays
//! the op's result — pretty, or JSON under `--plain`). See "The Progress
//! Exception" in `docs/patterns/cli.md`.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{paths, ContextError};
use crate::hub::login::{self, CompleteOptions, LoginError, LoginOutput, PendingLogin};

/// Default `--hub` target. Debug builds point at the local dev hub
/// (`./bin/dev hub` serves `127.0.0.1:17190`); release points at the
/// hosted hub. Passing `--hub` explicitly always wins.
pub const DEFAULT_HUB: &str = if cfg!(debug_assertions) {
    "http://127.0.0.1:17190"
} else {
    "https://hub.zim.krondor.org"
};

#[derive(Args, Debug, Clone)]
pub struct Login {
    /// Hub base URL, e.g. `https://hub.example.com`. No trailing
    /// slash; we strip one if you pass one. Defaults to the local dev
    /// hub in debug builds, the hosted hub in release.
    #[arg(long, default_value_t = DEFAULT_HUB.to_string())]
    pub hub: String,

    /// Override the device label shown on the approval page. Defaults
    /// to the machine's hostname.
    #[arg(long)]
    pub label: Option<String>,

    /// Nickname for the hub in the local peer book. Existing entry
    /// is overwritten so a re-login redirects future `relays add
    /// <nick>` calls.
    #[arg(long, default_value = "hub")]
    pub nick: String,

    /// Skip the peer-book update step. Useful if you only want to
    /// enroll without affecting how vaults route mirrors.
    #[arg(long)]
    pub no_peer_add: bool,

    /// Cap on how long we'll poll before giving up. Default 10 min,
    /// matches the hub's grant TTL.
    #[arg(long, default_value_t = 600)]
    pub timeout_secs: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HubLoginError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Login(#[from] LoginError),
}

#[async_trait]
impl Op for Login {
    type Context = ();
    type Output = LoginOutput;
    type Error = HubLoginError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let home = paths::home_dir(None)?;
        let pending = login::start(&home, &self.hub, self.label.as_deref()).await?;

        eprintln!("{}", PairInstructions(&pending));

        let opts = CompleteOptions {
            timeout: Duration::from_secs(self.timeout_secs),
            peer_nick: (!self.no_peer_add).then(|| self.nick.clone()),
        };
        Ok(pending.complete(opts).await?)
    }
}

/// What the user needs to approve the pairing in their browser.
pub struct PairInstructions<'a>(pub &'a PendingLogin);

impl fmt::Display for PairInstructions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.0;
        writeln!(f)?;
        writeln!(f, "  Open this URL in your browser:")?;
        writeln!(f, "    {}", ui::ident(p.verification_url()))?;
        writeln!(f)?;
        writeln!(f, "  Verify the code on the approve page matches:")?;
        writeln!(f, "    {}", ui::ident(p.code()))?;
        writeln!(f)?;
        writeln!(f, "  …and the identity key matches:")?;
        writeln!(f, "    {}", ui::ident(p.pubkey_hex()))?;
        writeln!(f)?;
        write!(
            f,
            "  Code expires {} · polling every {}s …",
            ui::dim(p.expires_at()),
            p.poll_interval_secs()
        )
    }
}

impl fmt::Display for LoginOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {}",
            ui::success("logged in", ""),
            ui::dim(&self.hub_url)
        )?;
        writeln!(f, "  pubkey:  {}", ui::ident(&self.enrolled_pubkey))?;
        writeln!(
            f,
            "  session: {}",
            ui::dim(self.session_path.display().to_string())
        )?;
        match (&self.hub_nick, &self.hub_did) {
            (Some(nick), Some(did)) => {
                writeln!(
                    f,
                    "  peer:    {} {}",
                    ui::ident(nick),
                    ui::dim(format!("→ {did}"))
                )?;
                write!(f, "  next:    {}", ui::ident("zim hub peers sync"))
            }
            _ => write!(f, "  peer:    {}", ui::dim("(skipped via --no-peer-add)")),
        }
    }
}
