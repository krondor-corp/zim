//! Global CLI args.

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "zim", about = "Zim — encrypted, peer-to-peer vault filesystem")]
pub struct Args {
    /// Override the data directory. Highest-priority over
    /// `$ZIM_HOME`, `$XDG_CONFIG_HOME/zim`, and `~/.config/zim`.
    #[arg(long, global = true)]
    pub config_path: Option<PathBuf>,

    /// Emit machine-readable JSON instead of the pretty terminal
    /// output. Every op's response serializes to JSON; action ops
    /// with no return data emit nothing.
    #[arg(long, global = true)]
    pub plain: bool,

    #[command(subcommand)]
    pub command: crate::Command,
}
