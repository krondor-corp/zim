//! Fixture execution: each fixture drives the real CLI (or the real
//! mountpoint, for FUSE ops) against a harness node — the same paths a
//! user takes, so a green apply is an end-to-end exercise of the
//! stack, not a simulation of one.

use std::path::PathBuf;

use anyhow::{anyhow, Result};

use crate::fixtures::Fixture;
use crate::harness::Harness;

/// Compare file-ish content the way shells do: trailing-newline
/// differences are not disagreements.
fn content_eq(actual: &str, expected: &str) -> bool {
    actual.trim_end_matches('\n') == expected.trim_end_matches('\n')
}

pub struct Applied {
    pub ran: usize,
    pub skipped_fuse: usize,
}

pub fn apply(harness: &Harness, fixtures: &[Fixture], fuse_ok: bool) -> Result<Applied> {
    let mut ran = 0;
    let mut skipped_fuse = 0;
    let default_nick = harness
        .nodes
        .first()
        .map(|n| n.nick.clone())
        .ok_or_else(|| anyhow!("harness has no nodes"))?;

    for fixture in fixtures {
        if fixture.is_fuse() && !fuse_ok {
            skipped_fuse += 1;
            continue;
        }
        apply_one(harness, fixture, &default_nick)?;
        ran += 1;
    }
    Ok(Applied { ran, skipped_fuse })
}

/// Relative mount points land in the harness data root (mirrors the
/// bash harness resolving them against data/).
fn mount_point(harness: &Harness, mp: &str) -> PathBuf {
    let p = PathBuf::from(mp);
    if p.is_absolute() {
        p
    } else {
        harness.data_root.join(mp)
    }
}

fn apply_one(harness: &Harness, fixture: &Fixture, default_nick: &str) -> Result<()> {
    let bin = &harness.zim_bin;
    let nick = |n: &Option<String>| n.clone().unwrap_or_else(|| default_nick.to_string());

    match fixture {
        Fixture::Vault { name, node } => {
            let node = harness.node(&nick(node))?;
            // Idempotent: vault names aren't unique, so only create when
            // the name isn't already listed (re-runs must not mint dupes).
            let listed = node.cli(bin, &["vault", "list"], None).unwrap_or_default();
            if !listed.split_whitespace().any(|w| w == name) {
                node.cli(bin, &["vault", "create", name], None)?;
            }
            println!("  vault {name}: ok");
        }
        Fixture::File {
            vault,
            path,
            content,
            source,
            node,
        } => {
            let node = harness.node(&nick(node))?;
            let bytes = match (content, source) {
                (Some(c), _) => c.clone().into_bytes(),
                (None, Some(src)) => std::fs::read(src)?,
                (None, None) => return Err(anyhow!("file fixture {path}: no content or source")),
            };
            node.cli(bin, &["vault", "add", vault, path], Some(&bytes))?;
            println!("  file {vault}:{path}: written");
        }
        Fixture::Dir { vault, path, node } => {
            let node = harness.node(&nick(node))?;
            node.cli(bin, &["vault", "mkdir", vault, path], None)?;
            println!("  dir {vault}:{path}: created");
        }
        Fixture::Share { vault, peer, node } => {
            let node = harness.node(&nick(node))?;
            let peer_key = harness.node(peer)?.id(bin)?;
            node.cli(bin, &["vault", "shares", "add", vault, &peer_key], None)?;
            println!("  share {vault} → {peer}: ok");
        }
        Fixture::Mv {
            vault,
            from,
            to,
            node,
        } => {
            let node = harness.node(&nick(node))?;
            node.cli(bin, &["vault", "mv", vault, from, to], None)?;
            println!("  mv {vault}:{from} → {to}: ok");
        }
        Fixture::VaultRead {
            vault,
            path,
            content,
            node,
        } => {
            let node = harness.node(&nick(node))?;
            let actual = node.cli(bin, &["vault", "cat", vault, path], None)?;
            if let Some(expected) = content {
                if !content_eq(&actual, expected) {
                    return Err(anyhow!(
                        "vault_read {vault}:{path}: content mismatch\n  expected: {expected:?}\n  actual:   {actual:?}"
                    ));
                }
                println!("  vault_read {vault}:{path}: content verified");
            } else {
                println!("  vault_read {vault}:{path}: ok");
            }
        }
        Fixture::Mount {
            vault,
            mount_point: mp,
            node,
        } => {
            let node = harness.node(&nick(node))?;
            let mp = mount_point(harness, mp);
            std::fs::create_dir_all(&mp)?;
            node.cli(bin, &["mount", "add", vault, mp.to_str().unwrap()], None)?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            println!("  mount {vault} at {}: ok", mp.display());
        }
        Fixture::MountVerify { mount_point: mp } => {
            let mp = mount_point(harness, mp);
            std::fs::read_dir(&mp)
                .map_err(|e| anyhow!("mount_verify {}: {e}", mp.display()))?;
            println!("  mount_verify {}: accessible", mp.display());
        }
        Fixture::Unmount { vault, node } => {
            let node = harness.node(&nick(node))?;
            node.cli(bin, &["mount", "stop", vault], None)?;
            println!("  unmount {vault}: ok");
        }
        Fixture::FuseLs { mount_point: mp, path } => {
            let full = mount_point(harness, mp).join(path);
            std::fs::read_dir(&full).map_err(|e| anyhow!("fuse_ls {}: {e}", full.display()))?;
            println!("  fuse_ls {path}: ok");
        }
        Fixture::FuseRead {
            mount_point: mp,
            path,
            content,
        } => {
            let full = mount_point(harness, mp).join(path);
            let actual = std::fs::read_to_string(&full)
                .map_err(|e| anyhow!("fuse_read {}: {e}", full.display()))?;
            if let Some(expected) = content {
                if !content_eq(&actual, expected) {
                    return Err(anyhow!(
                        "fuse_read {path}: content mismatch\n  expected: {expected:?}\n  actual:   {actual:?}"
                    ));
                }
                println!("  fuse_read {path}: content verified");
            } else {
                println!("  fuse_read {path}: ok");
            }
        }
        Fixture::FuseWrite {
            mount_point: mp,
            path,
            content,
        } => {
            let full = mount_point(harness, mp).join(path);
            std::fs::write(&full, content)
                .map_err(|e| anyhow!("fuse_write {}: {e}", full.display()))?;
            println!("  fuse_write {path}: ok");
        }
        Fixture::FuseMv {
            mount_point: mp,
            from,
            to,
        } => {
            let root = mount_point(harness, mp);
            std::fs::rename(root.join(from), root.join(to))
                .map_err(|e| anyhow!("fuse_mv {from} → {to}: {e}"))?;
            println!("  fuse_mv {from} → {to}: ok");
        }
        Fixture::FuseMvIn {
            mount_point: mp,
            path,
            content,
        } => {
            // A rename INTO the mount crosses filesystems, which is
            // exactly what this exercises: write outside, move in.
            let outside = harness.data_root.join(".fuse-mv-in.tmp");
            std::fs::write(&outside, content)?;
            let dest = mount_point(harness, mp).join(path);
            move_across(&outside, &dest).map_err(|e| anyhow!("fuse_mv_in {path}: {e}"))?;
            println!("  fuse_mv_in {path}: ok");
        }
        Fixture::FuseMvOut {
            mount_point: mp,
            path,
        } => {
            let src = mount_point(harness, mp).join(path);
            let outside = harness.data_root.join(".fuse-mv-out.tmp");
            move_across(&src, &outside).map_err(|e| anyhow!("fuse_mv_out {path}: {e}"))?;
            if src.exists() {
                return Err(anyhow!("fuse_mv_out {path}: source still present"));
            }
            std::fs::remove_file(&outside).ok();
            println!("  fuse_mv_out {path}: ok");
        }
        Fixture::FuseRm {
            mount_point: mp,
            path,
        } => {
            let full = mount_point(harness, mp).join(path);
            std::fs::remove_file(&full).map_err(|e| anyhow!("fuse_rm {path}: {e}"))?;
            if full.exists() {
                return Err(anyhow!("fuse_rm {path}: still present after rm"));
            }
            println!("  fuse_rm {path}: ok");
        }
    }
    Ok(())
}

/// `mv` across filesystem boundaries: rename when possible, else
/// copy+delete (what coreutils mv does — and what crossing in/out of a
/// FUSE mount always needs).
fn move_across(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(src, dest)?;
            std::fs::remove_file(src)
        }
    }
}
