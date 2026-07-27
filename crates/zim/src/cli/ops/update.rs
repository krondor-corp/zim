//! `zim update` — self-update from GitHub releases.
//!
//! Queries the repo's releases for the newest `zim-v*` tag, compares it
//! against the running version, downloads the platform artifact
//! (`zim-{darwin-arm64|linux-x64}[-fuse]-<version>`, matching what
//! `.github/workflows/release-cli.yml` uploads), and atomically replaces
//! the current executable. If the daemon is installed as a system
//! service (`zim daemon service install`), it is restarted afterwards so
//! the running daemon picks up the new binary — a CLI newer than its
//! daemon is a version-skew bug waiting to happen.

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;
use serde::Deserialize;

use crate::cli::op::Op;
use crate::cli::ui;

const REPO: &str = "krondor-corp/zim";
const TAG_PREFIX: &str = "zim-v";

#[derive(Args, Debug, Clone)]
pub struct Update {
    /// Check and report only; don't install.
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateOutput {
    pub current: String,
    pub latest: Option<String>,
    pub updated: bool,
    pub binary: Option<String>,
    pub service_restarted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("release lookup: {0}")]
    Lookup(String),
    #[error("no release asset `{0}` — this platform/feature combo may not be published")]
    NoAsset(String),
    #[error("download: {0}")]
    Download(String),
    #[error("install: {0}")]
    Install(String),
    #[error("service restart: {0} — run `zim daemon service start` manually")]
    ServiceRestart(String),
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// `(major, minor, patch)` from `x.y.z`; `None` on anything else.
fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').splitn(3, '.');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

/// The release-asset name for this binary: platform + whether this
/// build carries FUSE support.
fn asset_name(version: &str) -> Option<String> {
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "zim-darwin-arm64",
        ("linux", "x86_64") => "zim-linux-x64",
        _ => return None,
    };
    let fuse = if cfg!(feature = "fuse") { "-fuse" } else { "" };
    Some(format!("{platform}{fuse}-{version}"))
}

async fn latest_release(http: &reqwest::Client) -> Result<Option<Release>, UpdateError> {
    let mut req = http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases?per_page=20"
        ))
        .header("User-Agent", "zim-update");
    // Private repo / rate-limit escape hatch.
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| UpdateError::Lookup(e.to_string()))?;
    // 404 = repo private (unauthenticated) or no releases — not an error,
    // just nothing to update to.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let releases: Vec<Release> = resp
        .error_for_status()
        .map_err(|e| UpdateError::Lookup(e.to_string()))?
        .json()
        .await
        .map_err(|e| UpdateError::Lookup(e.to_string()))?;
    Ok(releases
        .into_iter()
        .filter(|r| r.tag_name.starts_with(TAG_PREFIX))
        .max_by_key(|r| parse_semver(r.tag_name.trim_start_matches(TAG_PREFIX))))
}

/// Restart the installed daemon service, if there is one. `Ok(false)`
/// when no service is installed/running — not an error.
fn restart_service_if_running() -> Result<bool, UpdateError> {
    use crate::cli::ops::daemon::service as svc;
    let Ok(manager) = svc::manager() else {
        return Ok(false);
    };
    let label = svc::label();
    let status = manager
        .status(service_manager::ServiceStatusCtx {
            label: label.clone(),
        })
        .map_err(|e| UpdateError::ServiceRestart(e.to_string()))?;
    match status {
        service_manager::ServiceStatus::Running => {
            manager
                .stop(service_manager::ServiceStopCtx {
                    label: label.clone(),
                })
                .map_err(|e| UpdateError::ServiceRestart(e.to_string()))?;
            manager
                .start(service_manager::ServiceStartCtx { label })
                .map_err(|e| UpdateError::ServiceRestart(e.to_string()))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[async_trait]
impl Op for Update {
    type Context = ();
    type Output = UpdateOutput;
    type Error = UpdateError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let current = env!("CARGO_PKG_VERSION").to_string();
        let http = reqwest::Client::new();

        let Some(release) = latest_release(&http).await? else {
            return Ok(UpdateOutput {
                current,
                latest: None,
                updated: false,
                binary: None,
                service_restarted: false,
            });
        };
        let latest = release.tag_name.trim_start_matches(TAG_PREFIX).to_string();

        let newer = match (parse_semver(&latest), parse_semver(&current)) {
            (Some(l), Some(c)) => l > c,
            _ => latest != current,
        };
        if !newer || self.check {
            return Ok(UpdateOutput {
                current,
                latest: Some(latest),
                updated: false,
                binary: None,
                service_restarted: false,
            });
        }

        let wanted = asset_name(&latest).ok_or_else(|| {
            UpdateError::NoAsset(format!(
                "{}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        })?;
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == wanted)
            .ok_or(UpdateError::NoAsset(wanted))?;

        // Download beside the current binary so the final rename is
        // atomic (same filesystem), then swap it in. On unix, renaming
        // over a running executable is fine — the old inode lives on
        // until the process exits.
        let exe = std::env::current_exe().map_err(|e| UpdateError::Install(e.to_string()))?;
        let staging: PathBuf = exe.with_extension("update-staging");
        let bytes = http
            .get(&asset.browser_download_url)
            .header("User-Agent", "zim-update")
            .send()
            .await
            .map_err(|e| UpdateError::Download(e.to_string()))?
            .error_for_status()
            .map_err(|e| UpdateError::Download(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        std::fs::write(&staging, &bytes).map_err(|e| UpdateError::Install(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| UpdateError::Install(e.to_string()))?;
        }
        std::fs::rename(&staging, &exe).map_err(|e| UpdateError::Install(e.to_string()))?;

        // A running managed daemon is still executing the old binary —
        // bounce it so daemon and CLI stay in lockstep.
        let service_restarted = restart_service_if_running()?;

        Ok(UpdateOutput {
            current,
            latest: Some(latest),
            updated: true,
            binary: Some(exe.display().to_string()),
            service_restarted,
        })
    }
}

impl fmt::Display for UpdateOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.latest, self.updated) {
            (None, _) => write!(
                f,
                "{}",
                ui::dim("no releases found (repo private without GITHUB_TOKEN, or none published)")
            ),
            (Some(l), false) if *l == self.current => {
                write!(
                    f,
                    "{} ({})",
                    ui::success("up to date", &self.current),
                    ui::dim("latest")
                )
            }
            (Some(l), false) => write!(
                f,
                "update available: {} → {} ({})",
                self.current,
                l,
                ui::dim("run `zim update` to install")
            ),
            (Some(l), true) => {
                write!(f, "{} {} → {}", ui::success("updated", ""), self.current, l)?;
                if let Some(b) = &self.binary {
                    write!(f, "\n  {}", ui::dim(b))?;
                }
                if self.service_restarted {
                    write!(f, "\n  {}", ui::dim("daemon service restarted"))?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_ordering_behaves() {
        assert!(parse_semver("0.2.0") > parse_semver("0.1.9"));
        assert!(parse_semver("1.0.0") > parse_semver("0.9.9"));
        assert_eq!(parse_semver("0.1.0"), parse_semver("v0.1.0"));
        assert!(parse_semver("nonsense").is_none());
    }

    #[test]
    fn asset_names_match_release_workflow() {
        // Keep in lockstep with .github/workflows/release-cli.yml's
        // matrix `artifact:` names.
        if let Some(name) = asset_name("0.2.0") {
            assert!(name.starts_with("zim-darwin-") || name.starts_with("zim-linux-"));
            assert!(name.ends_with("-0.2.0"));
            if cfg!(feature = "fuse") {
                assert!(name.contains("-fuse-"));
            }
        }
    }
}
