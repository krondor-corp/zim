//! The node harness: spawns real `zim` daemons as subprocesses, each
//! with its own home + port, waits for health, and wires the peers
//! together deterministically (address-book cross-add + direct
//! NodeAddr introduction — no DHT in the dial path).
//!
//! Ports live in the 1722x band (see bin/dev_/nodes.toml's band map)
//! so a harness run never collides with the interactive dev
//! environment on 1717x.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

pub const E2E_PORT_BASE: u16 = 17220;

pub struct Node {
    pub nick: String,
    pub port: u16,
    pub home: PathBuf,
    child: Option<Child>,
}

impl Node {
    pub fn api(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    /// Run the real `zim` CLI against this node. stdin is fed when
    /// given (vault add reads content from stdin).
    pub fn cli(&self, zim_bin: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<String> {
        let mut cmd = Command::new(zim_bin);
        cmd.env("ZIM_HOME", &self.home)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let mut child = cmd.spawn().context("spawn zim CLI")?;
        if let Some(bytes) = stdin {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("stdin piped")
                .write_all(bytes)?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "zim {:?} on {} failed: {}",
                args,
                self.nick,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }

    /// The node's raw hex pubkey (`zim id`).
    pub fn id(&self, zim_bin: &Path) -> Result<String> {
        let out = self.cli(zim_bin, &["id"], None)?;
        Ok(out.lines().last().unwrap_or_default().trim().to_string())
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // SIGKILL is fine here: fixture runs end with `unmount`, and
            // the throwaway data dir is wiped on the next run anyway.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct Harness {
    pub zim_bin: PathBuf,
    pub nodes: Vec<Node>,
    pub data_root: PathBuf,
    /// When set, nodes are left running on drop for inspection.
    pub keep: bool,
}

impl Harness {
    /// Boot one daemon per nick under `data_root`, wiping any previous
    /// run's state first.
    pub fn boot(zim_bin: &Path, nicks: &[String], data_root: &Path, keep: bool) -> Result<Self> {
        if data_root.exists() {
            std::fs::remove_dir_all(data_root).ok();
        }
        let mut nodes = Vec::new();
        for (i, nick) in nicks.iter().enumerate() {
            let port = E2E_PORT_BASE + i as u16;
            let home = data_root.join(nick);
            std::fs::create_dir_all(&home)?;
            std::fs::write(
                home.join("config.toml"),
                format!("api_port = {port}\nlog_level = \"info\"\n"),
            )?;
            let log = std::fs::File::create(home.join("daemon.log"))?;
            let child = Command::new(zim_bin)
                .env("ZIM_HOME", &home)
                .args(["daemon", "run", "--port", &port.to_string()])
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()
                .with_context(|| format!("spawn daemon {nick}"))?;
            nodes.push(Node {
                nick: nick.clone(),
                port,
                home,
                child: Some(child),
            });
        }
        let harness = Self {
            zim_bin: zim_bin.to_path_buf(),
            nodes,
            data_root: data_root.to_path_buf(),
            keep,
        };
        harness.wait_healthy(Duration::from_secs(30))?;
        Ok(harness)
    }

    fn wait_healthy(&self, deadline: Duration) -> Result<()> {
        let start = Instant::now();
        for node in &self.nodes {
            loop {
                if reqwest::blocking::get(node.api("/_status/livez"))
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
                {
                    break;
                }
                if start.elapsed() > deadline {
                    let log = std::fs::read_to_string(node.home.join("daemon.log"))
                        .unwrap_or_default();
                    let tail: Vec<_> = log.lines().rev().take(10).collect();
                    return Err(anyhow!(
                        "daemon {} never became healthy; log tail:\n{}",
                        node.nick,
                        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                    ));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        Ok(())
    }

    pub fn node(&self, nick: &str) -> Result<&Node> {
        self.nodes
            .iter()
            .find(|n| n.nick == nick)
            .ok_or_else(|| anyhow!("fixture references unknown node '{nick}'"))
    }

    /// Peer plumbing: init each node, cross-add address books, and
    /// cross-introduce direct NodeAddrs so local dials skip discovery.
    pub fn wire_peers(&self) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        for node in &self.nodes {
            let _ = node.cli(&self.zim_bin, &["init"], None);
        }
        for a in &self.nodes {
            for b in &self.nodes {
                if a.nick == b.nick {
                    continue;
                }
                let b_id = b.id(&self.zim_bin)?;
                let _ = a.cli(&self.zim_bin, &["peers", "add", &b.nick, &b_id], None);
            }
        }
        // Direct introduction: fetch each node's NodeAddr and hand it to
        // every other node (the hermetic-dial path — see
        // /api/v0/peers/{addr,introduce}).
        for a in &self.nodes {
            let addr: serde_json::Value = client
                .post(a.api("/api/v0/peers/addr"))
                .json(&serde_json::json!({}))
                .send()?
                .json()?;
            for b in &self.nodes {
                if a.nick == b.nick {
                    continue;
                }
                client
                    .post(b.api("/api/v0/peers/introduce"))
                    .json(&addr)
                    .send()?
                    .error_for_status()
                    .with_context(|| format!("introduce {} to {}", a.nick, b.nick))?;
            }
        }
        Ok(())
    }

    /// FUSE availability: platform library + daemon built with the
    /// `fuse` feature (from `/_status/version` build_features).
    pub fn fuse_available(&self) -> bool {
        let lib = if cfg!(target_os = "macos") {
            Path::new("/Library/Filesystems/macfuse.fs").is_dir()
        } else {
            Path::new("/dev/fuse").exists()
        };
        if !lib {
            return false;
        }
        let Some(node) = self.nodes.first() else {
            return false;
        };
        reqwest::blocking::get(node.api("/_status/version"))
            .ok()
            .and_then(|r| r.json::<serde_json::Value>().ok())
            .map(|v| {
                v["build_features"]
                    .as_array()
                    .map(|a| a.iter().any(|f| f == "fuse"))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if self.keep {
            // Disarm the node killers.
            for node in &mut self.nodes {
                if let Some(child) = node.child.take() {
                    eprintln!(
                        "  --keep: {} left running (pid {}, port {})",
                        node.nick,
                        child.id(),
                        node.port
                    );
                }
            }
        }
    }
}

/// Poll `check` until it returns true or the deadline passes. The
/// engine of outcome-determinism: sync convergence varies in timing,
/// never in verdict.
pub fn until(label: &str, deadline: Duration, mut check: impl FnMut() -> bool) -> Result<()> {
    let start = Instant::now();
    loop {
        if check() {
            println!("  ✓ {label} ({}s)", start.elapsed().as_secs());
            return Ok(());
        }
        if start.elapsed() > deadline {
            return Err(anyhow!("{label}: not converged within {deadline:?}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
