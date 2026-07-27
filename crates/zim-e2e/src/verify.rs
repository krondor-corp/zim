//! Cross-node verification: the invariants that make "synced" mean
//! something, not just spot-reads.
//!
//! The load-bearing check is HEAD EQUALITY: vault heads are hashes over
//! the full manifest (which pins the whole tree), so two nodes
//! reporting the same head is cryptographic proof their trees are
//! identical — every file, every directory, every byte. File reads
//! around it are sanity, not the proof.
//!
//! Beyond convergence we assert what must NOT happen (isolation of
//! unshared vaults), that deletions propagate, that concurrent forks
//! merge to one head, and that state survives a daemon restart.

use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::harness::{until, Harness, Node};

fn vault_names(node: &Node, bin: &std::path::Path) -> Vec<String> {
    // NAME is the first column of `vault list`; skip the header row.
    node.cli(bin, &["vault", "list"], None)
        .map(|out| {
            out.lines()
                .skip(1)
                .filter_map(|l| l.split_whitespace().next().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn head(node: &Node, bin: &std::path::Path, vault: &str) -> Option<String> {
    node.cli(bin, &["vault", "head", vault], None).ok()
}

/// Both nodes report the same head for `vault` — tree-equality proof.
fn heads_converge(
    a: &Node,
    b: &Node,
    bin: &std::path::Path,
    vault: &str,
    deadline: Duration,
) -> Result<()> {
    until(
        &format!("{} and {} agree on {vault}'s head", a.nick, b.nick),
        deadline,
        || match (head(a, bin, vault), head(b, bin, vault)) {
            (Some(ha), Some(hb)) => !ha.is_empty() && ha == hb,
            _ => false,
        },
    )
}

pub fn convergence(harness: &Harness, deadline: Duration) -> Result<()> {
    let bin = &harness.zim_bin;
    let a = &harness.nodes[0];
    let b = harness
        .nodes
        .get(1)
        .ok_or_else(|| anyhow!("need two nodes"))?;

    // Bob bootstraps the shared vault…
    until(&format!("{} sees demo", b.nick), deadline, || {
        vault_names(b, bin).iter().any(|n| n == "demo")
    })?;

    // …and the trees provably match.
    heads_converge(a, b, bin, "demo", deadline)?;

    // Sanity reads on top of the proof.
    until(
        &format!("{} reads {}'s /readme.md", b.nick, a.nick),
        deadline,
        || {
            b.cli(bin, &["vault", "cat", "demo", "/readme.md"], None)
                .map(|out| out.contains("hello from alice"))
                .unwrap_or(false)
        },
    )?;
    until(
        &format!("{} reads the moved /guide.md", b.nick),
        deadline,
        || {
            b.cli(bin, &["vault", "cat", "demo", "/guide.md"], None)
                .is_ok()
        },
    )?;

    // ISOLATION: `notes` was never shared — bob must not have it, and
    // must not be able to read from it. A quiet failure here would be a
    // access-control regression, the worst kind.
    if vault_names(b, bin).iter().any(|n| n == "notes") {
        return Err(anyhow!(
            "isolation violated: {} lists the unshared vault 'notes'",
            b.nick
        ));
    }
    if b.cli(bin, &["vault", "cat", "notes", "/index.md"], None)
        .is_ok()
    {
        return Err(anyhow!(
            "isolation violated: {} can read from the unshared vault 'notes'",
            b.nick
        ));
    }
    println!("  ✓ {} cannot see or read the unshared 'notes'", b.nick);
    Ok(())
}

pub fn mutations(harness: &Harness, deadline: Duration) -> Result<()> {
    let bin = &harness.zim_bin;
    let a = &harness.nodes[0];
    let b = &harness.nodes[1];

    // Round-trip: the late joiner writes, the owner reads.
    let note = format!("hi from {}", b.nick);
    b.cli(
        bin,
        &["vault", "add", "demo", "/b.md"],
        Some(note.as_bytes()),
    )?;
    until(
        &format!("{} reads {}'s /b.md", a.nick, b.nick),
        deadline,
        || {
            a.cli(bin, &["vault", "cat", "demo", "/b.md"], None)
                .map(|out| out.trim_end() == note)
                .unwrap_or(false)
        },
    )?;

    // DELETION propagates: rm on one side must become absence on the
    // other — sync that only ever adds is a different (broken) product.
    a.cli(bin, &["vault", "rm", "demo", "/b.md"], None)?;
    until(
        &format!("{}'s rm of /b.md reaches {}", a.nick, b.nick),
        deadline,
        || {
            b.cli(bin, &["vault", "cat", "demo", "/b.md"], None)
                .is_err()
        },
    )?;
    heads_converge(a, b, bin, "demo", deadline)?;

    // CONCURRENT writes: both sides commit on the same head, forking
    // the chain; the protocol must merge to one head containing both.
    a.cli(
        bin,
        &["vault", "add", "demo", "/fork-a.md"],
        Some(b"written on a"),
    )?;
    b.cli(
        bin,
        &["vault", "add", "demo", "/fork-b.md"],
        Some(b"written on b"),
    )?;
    until("both forks visible on both nodes", deadline, || {
        [a, b].iter().all(|n| {
            n.cli(bin, &["vault", "cat", "demo", "/fork-a.md"], None)
                .is_ok()
                && n.cli(bin, &["vault", "cat", "demo", "/fork-b.md"], None)
                    .is_ok()
        })
    })?;
    heads_converge(a, b, bin, "demo", deadline)?;
    Ok(())
}

pub fn durability(harness: &mut Harness, deadline: Duration) -> Result<()> {
    let bin = harness.zim_bin.clone();

    // Restart the late joiner: everything it knew must survive, and
    // sync must keep working with its fresh endpoint.
    let restarted = harness.nodes[1].nick.clone();
    println!("  restarting {restarted}…");
    harness.restart(&restarted)?;
    harness.wire_peers()?; // fresh iroh endpoint → re-introduce addrs

    let a = &harness.nodes[0];
    let b = &harness.nodes[1];

    until(
        &format!("{restarted} still has demo after restart"),
        deadline,
        || vault_names(b, &bin).iter().any(|n| n == "demo"),
    )?;
    heads_converge(a, b, &bin, "demo", deadline)?;

    // New writes still flow to the restarted node.
    a.cli(
        &bin,
        &["vault", "add", "demo", "/post-restart.md"],
        Some(b"survived"),
    )?;
    until(
        &format!("{restarted} receives post-restart writes"),
        deadline,
        || {
            b.cli(&bin, &["vault", "cat", "demo", "/post-restart.md"], None)
                .map(|out| out.trim_end() == "survived")
                .unwrap_or(false)
        },
    )?;
    heads_converge(a, b, &bin, "demo", deadline)?;
    Ok(())
}

/// FUSE across nodes: mount the synced replica on the late joiner and
/// prove reads AND writes through its mountpoint ride the same sync.
pub fn fuse_cross_node(harness: &Harness, deadline: Duration) -> Result<()> {
    let bin = &harness.zim_bin;
    let a = &harness.nodes[0];
    let b = &harness.nodes[1];

    let mp = harness.data_root.join("mnt-b-demo");
    std::fs::create_dir_all(&mp)?;
    b.cli(bin, &["mount", "add", "demo", mp.to_str().unwrap()], None)?;
    std::thread::sleep(Duration::from_secs(1));

    // Read a synced file through the replica's mount.
    let via_mount = std::fs::read_to_string(mp.join("readme.md"))?;
    if !via_mount.contains("hello from alice") {
        return Err(anyhow!("replica mount serves wrong content for readme.md"));
    }
    println!("  ✓ {}'s mount serves synced content", b.nick);

    // Write through the replica's mount; the owner must converge on it.
    std::fs::write(mp.join("from-b-mount.md"), "written through b's mount")?;
    until(
        &format!("{} receives {}'s mount-write", a.nick, b.nick),
        deadline,
        || {
            a.cli(bin, &["vault", "cat", "demo", "/from-b-mount.md"], None)
                .map(|out| out.trim_end() == "written through b's mount")
                .unwrap_or(false)
        },
    )?;
    heads_converge(a, b, bin, "demo", deadline)?;

    b.cli(bin, &["mount", "stop", "demo"], None)?;
    Ok(())
}

/// Tight repro loop for the fork-drop bug (#17): minimal setup, then
/// hammer concurrent-add rounds until one fails. On failure the
/// verdict distinguishes DIVERGED (sync incomplete — maybe timing)
/// from CONVERGED-WRONG (identical heads, missing content — a
/// correctness bug), and dumps the post-mortem: heads, tree listings,
/// and every collision/merge trace line from both daemon logs.
pub fn fork_loop(harness: &Harness, rounds: u32, deadline: Duration) -> Result<()> {
    let bin = &harness.zim_bin;
    let a = &harness.nodes[0];
    let b = &harness.nodes[1];

    // Minimal world: one shared vault, converged.
    a.cli(bin, &["vault", "create", "forky"], None)?;
    a.cli(bin, &["vault", "add", "forky", "/seed.md"], Some(b"seed"))?;
    let b_key = b.id(bin)?;
    a.cli(bin, &["vault", "shares", "add", "forky", &b_key], None)?;
    heads_converge(a, b, bin, "forky", deadline)?;
    println!("seeded + converged; starting {rounds} fork rounds");

    for round in 0..rounds {
        let fa = format!("/r{round}-a.md");
        let fb = format!("/r{round}-b.md");
        a.cli(bin, &["vault", "add", "forky", &fa], Some(b"from a"))?;
        b.cli(bin, &["vault", "add", "forky", &fb], Some(b"from b"))?;

        let both = |n: &Node| {
            n.cli(bin, &["vault", "cat", "forky", &fa], None).is_ok()
                && n.cli(bin, &["vault", "cat", "forky", &fb], None).is_ok()
        };
        let converged = until(
            &format!("round {round}: both files on both nodes"),
            deadline,
            || both(a) && both(b) && head_eq(a, b, bin, "forky"),
        );

        if let Err(e) = converged {
            let ha = head(a, bin, "forky").unwrap_or_default();
            let hb = head(b, bin, "forky").unwrap_or_default();
            let verdict = if !ha.is_empty() && ha == hb {
                "CONVERGED-WRONG: identical heads, content missing — correctness bug"
            } else {
                "DIVERGED: heads differ at deadline — sync incomplete (timing?)"
            };
            println!(
                "
================ POST-MORTEM (round {round}) ================"
            );
            println!("verdict: {verdict}");
            for n in [a, b] {
                println!("--- {}:", n.nick);
                println!("  head: {}", head(n, bin, "forky").unwrap_or_default());
                println!(
                    "  ls /: {}",
                    n.cli(bin, &["vault", "ls", "forky", "/"], None)
                        .unwrap_or_default()
                        .replace('\n', "  ")
                );
                let log = std::fs::read_to_string(n.home.join("daemon.log")).unwrap_or_default();
                let interesting: Vec<&str> = log
                    .lines()
                    .filter(|l| {
                        l.contains("OP ID COLLISION")
                            || l.contains("conflict resolved")
                            || l.contains("windows collected")
                    })
                    .collect();
                let tail = interesting.len().saturating_sub(30);
                println!("  merge trace ({} lines, last 30):", interesting.len());
                for l in &interesting[tail..] {
                    println!("    {l}");
                }
            }
            return Err(e).map_err(|e| anyhow!("{e} [{verdict}]"));
        }
    }
    println!(
        "
{rounds} fork rounds clean"
    );
    Ok(())
}

fn head_eq(a: &Node, b: &Node, bin: &std::path::Path, vault: &str) -> bool {
    match (head(a, bin, vault), head(b, bin, vault)) {
        (Some(ha), Some(hb)) => !ha.is_empty() && ha == hb,
        _ => false,
    }
}
