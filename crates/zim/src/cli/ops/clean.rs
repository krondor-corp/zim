//! `zim clean` — wipe the local data directory.
//!
//! **Debug-only** — the subcommand doesn't exist in `--release` builds
//! (gated in the `command_enum!` invocation), so a release binary can't
//! nuke real user data. In a debug build with no override the resolved
//! home is the `debug/` nested dir (see [`paths`]), so `zim clean`
//! resets local dev state in one shot.
//!
//! Defaults to a dry run: it reports the directory and its top-level
//! entries but removes nothing. Pass `--yes` to actually delete.

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::paths;

/// Wipe the local data directory (debug builds only).
#[derive(Args, Debug, Clone)]
pub struct Clean {
    /// Actually remove the directory. Without it, this is a dry run
    /// that only reports what would be deleted.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct CleanOutput {
    pub home: PathBuf,
    pub existed: bool,
    pub removed: bool,
    /// Top-level entries under `home` (for the dry-run preview).
    pub entries: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CleanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[async_trait]
impl Op for Clean {
    type Context = ();
    type Output = CleanOutput;
    type Error = CleanError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let home = paths::home_dir(None)?;
        let existed = home.exists();

        let mut entries = Vec::new();
        if existed {
            let mut rd = tokio::fs::read_dir(&home).await?;
            while let Some(e) = rd.next_entry().await? {
                entries.push(e.file_name().to_string_lossy().into_owned());
            }
            entries.sort();
        }

        let removed = if self.yes && existed {
            tokio::fs::remove_dir_all(&home).await?;
            true
        } else {
            false
        };

        Ok(CleanOutput {
            home,
            existed,
            removed,
            entries,
        })
    }
}

impl fmt::Display for CleanOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let home = self.home.display().to_string();

        if !self.existed {
            return write!(f, "{} {}", ui::dim("nothing to clean —"), ui::dim(home));
        }

        if self.removed {
            return write!(f, "{} {}", ui::success("removed", ""), ui::dim(home));
        }

        // Dry run: show what would go.
        writeln!(f, "{} {}", ui::warning("would remove", ""), ui::dim(&home))?;
        for entry in &self.entries {
            writeln!(f, "  {}", ui::dim(entry))?;
        }
        write!(f, "{}", ui::dim("re-run with --yes to delete"))
    }
}
