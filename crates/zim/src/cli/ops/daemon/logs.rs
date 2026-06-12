//! `zim daemon logs` — show the daemon's log file.
//!
//! The daemon writes tracing output to `$ZIM_HOME/state/daemon.log`
//! whenever it runs (foreground or service-managed). This op reads
//! that file and prints the tail. No `--follow` yet; rerun for fresh
//! output, or just `tail -f` the file directly.

use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{paths, ContextError};

/// Default number of lines if `--lines` is omitted. Matches `tail`'s
/// default well enough for ad-hoc inspection.
const DEFAULT_LINES: usize = 200;

#[derive(Args, Debug, Clone)]
pub struct Logs {
    /// Show only the last N lines. Use 0 for the whole file.
    #[arg(short = 'n', long, default_value_t = DEFAULT_LINES)]
    pub lines: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct LogsOutput {
    pub path: PathBuf,
    pub lines: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LogsError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error("log file not found at {0} — has the daemon ever started?")]
    NotFound(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[async_trait]
impl Op for Logs {
    type Context = ();
    type Output = LogsOutput;
    type Error = LogsError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let home = paths::home_dir(None)?;
        let path = paths::daemon_log_path(&home);
        if !path.exists() {
            return Err(LogsError::NotFound(path));
        }

        let file = tokio::fs::File::open(&path).await?;
        let mut reader = BufReader::new(file).lines();
        let lines = if self.lines == 0 {
            let mut all = Vec::new();
            while let Some(l) = reader.next_line().await? {
                all.push(l);
            }
            all
        } else {
            // Bounded ring of the last N lines — avoids reading the
            // whole file into memory on log files that have grown.
            let mut tail: VecDeque<String> = VecDeque::with_capacity(self.lines);
            while let Some(l) = reader.next_line().await? {
                if tail.len() == self.lines {
                    tail.pop_front();
                }
                tail.push_back(l);
            }
            tail.into_iter().collect()
        };

        Ok(LogsOutput { path, lines })
    }
}

impl fmt::Display for LogsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.lines.is_empty() {
            return write!(
                f,
                "{}",
                ui::dim(format!("{} is empty", self.path.display()))
            );
        }
        for line in &self.lines {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}
