//! `zim-hub-devseed` — dev-only hub state seeder.
//!
//! Skips the interactive device-code login dance. Instead it writes
//! the rows that a successful `zim hub login` would have produced on the
//! hub side: one authorized+admin [`User`] for a fixed email, and a
//! `Daemon`-kind [`UserPeer`] row per daemon pubkey passed on the
//! command line. After this runs, those daemons authenticate to the
//! hub's HTTP API (bearer JWT, verified against `user_peers`) as that
//! user — exactly as if they'd been approved through the browser.
//!
//! It does NOT mint a web key — that's a browser-resident keypair the
//! user still onboards manually (the workspace UI gates on it). This
//! only stands in for the daemon enrollments.
//!
//! Usage:
//!
//! ```text
//! ZIM_HUB_HOME=data/zim-hub \
//! ZIM_DEV_SEED_EMAIL=al@krondor.org \
//!   zim-hub-devseed alice=<pubkey-hex> bob=<pubkey-hex>
//! ```
//!
//! Each positional arg is `LABEL=PUBKEYHEX` (label optional — a bare
//! `PUBKEYHEX` enrolls with no label). Re-running is idempotent: an
//! already-enrolled pubkey is left alone, and the user is upgraded to
//! admin+authorized if it wasn't already.
//!
//! Resolves the hub DB the same way `zim-hub` itself does:
//! `$ZIM_HUB_HOME/state/hub.db` (default home `./data/zim-hub`).

use std::path::PathBuf;

use anyhow::{anyhow, Context};
use zim_crypto::PublicKey;
use zim_hub::database::models::{PeerKind, User, UserPeer};
use zim_hub::Database;

/// Mirror `zim-hub`'s home resolution (see `config.rs` `DEFAULT_HOME`).
const DEFAULT_HOME: &str = "./data/zim-hub";
const DEFAULT_EMAIL: &str = "al@krondor.org";

struct Peer {
    label: Option<String>,
    pubkey: PublicKey,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("devseed: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let email = std::env::var("ZIM_DEV_SEED_EMAIL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_EMAIL.to_string());

    let peers = parse_peers(std::env::args().skip(1))?;
    if peers.is_empty() {
        return Err(anyhow!(
            "no peers given\n\
             usage: zim-hub-devseed LABEL=PUBKEYHEX [LABEL=PUBKEYHEX ...]"
        ));
    }

    let db = connect_hub_db().await?;

    // 1. Ensure the user exists and is usable (admin + authorized).
    let user = match User::find_by_email(&email, &db)
        .await
        .context("look up user by email")?
    {
        Some(u) => {
            if !(u.is_admin() && u.is_authorized()) {
                let patched = u
                    .patch(
                        zim_hub::database::models::UserPatch {
                            is_admin: Some(true),
                            is_authorized: Some(true),
                            ..Default::default()
                        },
                        &db,
                    )
                    .await
                    .context("upgrade existing user to admin+authorized")?;
                println!("user  {email}  (upgraded → admin+authorized)");
                patched
            } else {
                println!("user  {email}  (exists, admin+authorized)");
                u
            }
        }
        None => {
            // Name = local-part of the email; purely cosmetic.
            let name = email.split('@').next().unwrap_or(&email);
            let u = User::create(&email, name, true, true, &db)
                .await
                .context("create user")?;
            println!("user  {email}  (created → admin+authorized)");
            u
        }
    };

    // 2. Enroll each daemon pubkey as a Daemon-kind device.
    for peer in &peers {
        let label = peer.label.as_deref();
        match UserPeer::find_by_pubkey(&peer.pubkey, &db)
            .await
            .context("look up peer by pubkey")?
        {
            Some(existing) if existing.user_id() == user.id() => {
                println!(
                    "peer  {}{}  (already enrolled)",
                    peer.pubkey.to_hex(),
                    label.map(|l| format!("  [{l}]")).unwrap_or_default()
                );
            }
            Some(_) => {
                eprintln!(
                    "peer  {}  SKIPPED — pubkey already belongs to a different user",
                    peer.pubkey.to_hex()
                );
            }
            None => {
                UserPeer::create(user.id(), &peer.pubkey, label, PeerKind::Daemon, &db)
                    .await
                    .context("create user_peer row")?;
                println!(
                    "peer  {}{}  (enrolled)",
                    peer.pubkey.to_hex(),
                    label.map(|l| format!("  [{l}]")).unwrap_or_default()
                );
            }
        }
    }

    println!("\nseeded {} daemon(s) for {email}", peers.len());
    Ok(())
}

/// Parse `LABEL=PUBKEYHEX` (or bare `PUBKEYHEX`) positional args.
fn parse_peers(args: impl Iterator<Item = String>) -> anyhow::Result<Vec<Peer>> {
    let mut out = Vec::new();
    for arg in args {
        let (label, hex) = match arg.split_once('=') {
            Some((l, h)) => (Some(l.to_string()), h.to_string()),
            None => (None, arg),
        };
        let pubkey =
            PublicKey::from_hex(hex.trim()).map_err(|e| anyhow!("bad pubkey hex `{hex}`: {e}"))?;
        out.push(Peer { label, pubkey });
    }
    Ok(out)
}

/// Open the hub DB at `$ZIM_HUB_HOME/state/hub.db`, matching the URL
/// construction in `zim-hub`'s `main.rs`. Runs migrations, so it works
/// against a fresh home that the hub hasn't booted yet.
async fn connect_hub_db() -> anyhow::Result<Database> {
    let data_dir = std::env::var("ZIM_HUB_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_HOME));
    let state_dir = data_dir.join("state");
    std::fs::create_dir_all(&state_dir)
        .with_context(|| format!("create state dir {}", state_dir.display()))?;
    let abs = std::fs::canonicalize(&state_dir)
        .with_context(|| format!("canonicalize {}", state_dir.display()))?
        .join("hub.db");
    let url = url::Url::parse(&format!("sqlite://{}", abs.display()))
        .with_context(|| format!("build sqlite url for {}", abs.display()))?;
    println!("hub db  {}", abs.display());
    Database::connect(&url)
        .await
        .map_err(|e| anyhow!("connect hub db: {e}"))
}
