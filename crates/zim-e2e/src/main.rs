//! `zim-e2e` — the end-to-end verdict tool.
//!
//! Boots a hermetic throwaway environment (real daemons on the 1722x
//! band, fresh homes under data/e2e/), wires peers deterministically,
//! applies `bin/dev_/fixtures.toml` through the real CLI, then verifies
//! cross-node sync convergence with poll-until-deadline assertions.
//! Exit code is the verdict.
//!
//! This deliberately does NOT touch the interactive dev environment
//! (`./bin/dev`, ports 1717x) — the two can run side by side.
//!
//! Usage:
//!   zim-e2e [--keep] [--skip-fuse] [--deadline SECS]
//!
//! Env:
//!   ZIM_BIN   path to the zim binary (default target/debug/zim)

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};

mod apply;
mod fixtures;
mod harness;

use harness::{until, Harness};

struct Opts {
    keep: bool,
    skip_fuse: bool,
    deadline: Duration,
}

fn parse_args() -> Result<Opts> {
    let mut opts = Opts {
        keep: false,
        skip_fuse: false,
        deadline: Duration::from_secs(60),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--keep" => opts.keep = true,
            "--skip-fuse" => opts.skip_fuse = true,
            "--deadline" => {
                let secs: u64 = args
                    .next()
                    .ok_or_else(|| anyhow!("--deadline needs seconds"))?
                    .parse()?;
                opts.deadline = Duration::from_secs(secs);
            }
            "--help" | "-h" => {
                println!("zim-e2e [--keep] [--skip-fuse] [--deadline SECS]");
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown arg: {other}")),
        }
    }
    Ok(opts)
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Node nicks from bin/dev_/nodes.toml section headers — the single
/// source of truth for who the dev peers are. Ports are NOT taken from
/// there: the harness maps nicks onto its own 1722x band.
fn node_nicks(root: &std::path::Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(root.join("bin/dev_/nodes.toml"))?;
    let table: toml::Table = text.parse()?;
    let nicks: Vec<String> = table.keys().cloned().collect();
    if nicks.is_empty() {
        return Err(anyhow!("no nodes defined in bin/dev_/nodes.toml"));
    }
    Ok(nicks)
}

fn main() {
    match run() {
        Ok(()) => println!("\ne2e PASS"),
        Err(e) => {
            eprintln!("\ne2e FAIL — {e:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<()> {
    let opts = parse_args()?;
    let root = project_root();
    let zim_bin = std::env::var("ZIM_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target/debug/zim"));
    if !zim_bin.exists() {
        return Err(anyhow!(
            "zim binary not found at {} — build it first (make e2e does)",
            zim_bin.display()
        ));
    }

    let nicks = node_nicks(&root)?;
    let fixtures = fixtures::load(&root.join("bin/dev_/fixtures.toml"))?;

    println!("[1/4] boot: {} daemons on the 1722x band", nicks.len());
    let harness = Harness::boot(&zim_bin, &nicks, &root.join("data/e2e"), opts.keep)?;

    println!("[2/4] wire: address books + direct NodeAddr introduction");
    harness.wire_peers()?;

    let fuse_ok = !opts.skip_fuse && harness.fuse_available();
    println!(
        "[3/4] fixtures ({} entries, FUSE: {})",
        fixtures.len(),
        if fuse_ok { "enabled" } else { "skipped" }
    );
    let applied = apply::apply(&harness, &fixtures, fuse_ok)?;
    println!("  {} fixtures applied", applied.ran);
    if applied.skipped_fuse > 0 {
        println!(
            "  ({} FUSE fixtures skipped — not a failure)",
            applied.skipped_fuse
        );
    }

    println!("[4/4] cross-node sync convergence");
    let a = &harness.nodes[0];
    let b = harness.nodes.get(1).unwrap_or(a);
    let bin = &harness.zim_bin;

    until(&format!("{} sees demo", b.nick), opts.deadline, || {
        b.cli(bin, &["vault", "list"], None)
            .map(|out| out.split_whitespace().any(|w| w == "demo"))
            .unwrap_or(false)
    })?;
    until(
        &format!("{} reads {}'s /readme.md", b.nick, a.nick),
        opts.deadline,
        || {
            b.cli(bin, &["vault", "cat", "demo", "/readme.md"], None)
                .map(|out| out.contains("hello from alice"))
                .unwrap_or(false)
        },
    )?;
    until(
        &format!("{} reads the moved /guide.md", b.nick),
        opts.deadline,
        || {
            b.cli(bin, &["vault", "cat", "demo", "/guide.md"], None).is_ok()
        },
    )?;
    // Round-trip the other way — the one check fixtures can't express.
    let note = format!("hi from {}", b.nick);
    b.cli(bin, &["vault", "add", "demo", "/b.md"], Some(note.as_bytes()))?;
    until(
        &format!("{} reads {}'s /b.md", a.nick, b.nick),
        opts.deadline,
        || {
            a.cli(bin, &["vault", "cat", "demo", "/b.md"], None)
                .map(|out| out.trim_end() == note)
                .unwrap_or(false)
        },
    )?;

    if opts.keep {
        println!("\n--keep: environment left running under {}", harness.data_root.display());
    }
    Ok(())
}
