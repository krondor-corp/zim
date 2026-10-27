//! `zim mount set <vault|path> [--auto [<bool>]] [--read-only|--ro [<bool>]]` —
//! edit a registration in place (no remove + re-add dance). A
//! `--read-only` change on a live mount remounts it.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::mounts::{MountInfo, SetRequest};

#[derive(Args, Debug, Clone)]
pub struct Set {
    /// The mount to edit — a vault name/id, or a mountpoint path.
    pub target: String,
    /// Re-mount automatically when the daemon starts. Bare `--auto`
    /// means true; pass `--auto false` to turn it off.
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub auto: Option<bool>,
    /// Mount read-only (remounts if currently mounted). Bare
    /// `--read-only` / `--ro` means true; pass `false` to turn it off.
    #[arg(
        long = "read-only",
        visible_alias = "ro",
        num_args = 0..=1,
        default_missing_value = "true"
    )]
    pub read_only: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct SetOutput(pub Vec<MountInfo>);

#[derive(Debug, thiserror::Error)]
pub enum SetError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("nothing to change — pass --auto and/or --read-only")]
    NoChanges,
}

#[async_trait]
impl Op for Set {
    type Context = ApiContext;
    type Output = SetOutput;
    type Error = SetError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        if self.auto.is_none() && self.read_only.is_none() {
            return Err(SetError::NoChanges);
        }
        let mountpoints = super::resolve_mountpoints(&ctx, &self.target).await?;
        let mut out = Vec::with_capacity(mountpoints.len());
        for mountpoint in mountpoints {
            let resp = ctx
                .client
                .call(SetRequest {
                    mountpoint,
                    auto_mount: self.auto,
                    read_only: self.read_only,
                })
                .await?;
            out.push(resp.mount);
        }
        Ok(SetOutput(out))
    }
}

impl fmt::Display for SetOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, m) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            let mut flags = Vec::new();
            if m.read_only {
                flags.push("ro");
            }
            if m.auto_mount {
                flags.push("auto");
            }
            write!(
                f,
                "{} {}",
                ui::success("updated", &m.mountpoint),
                ui::dim(if flags.is_empty() {
                    "(no flags)".to_string()
                } else {
                    flags.join(",")
                })
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Cli {
        #[command(flatten)]
        set: Set,
    }

    fn parse(args: &[&str]) -> Set {
        Cli::try_parse_from(std::iter::once("set").chain(args.iter().copied()))
            .expect("args should parse")
            .set
    }

    #[test]
    fn alice_follows_the_docs_with_bare_flags() {
        // `zim mount set notes --auto` — a bare flag turns it on.
        let set = parse(&["notes", "--auto"]);
        assert_eq!(set.auto, Some(true));
        assert_eq!(set.read_only, None);

        // `zim mount set notes --ro` — the short alias for --read-only.
        let set = parse(&["notes", "--ro"]);
        assert_eq!(set.read_only, Some(true));
        assert_eq!(set.auto, None);

        let set = parse(&["notes", "--read-only"]);
        assert_eq!(set.read_only, Some(true));
    }

    #[test]
    fn alice_turns_flags_off_with_explicit_bools() {
        let set = parse(&["notes", "--auto", "false", "--ro", "false"]);
        assert_eq!(set.auto, Some(false));
        assert_eq!(set.read_only, Some(false));

        let set = parse(&["notes", "--auto=true", "--read-only=false"]);
        assert_eq!(set.auto, Some(true));
        assert_eq!(set.read_only, Some(false));
    }

    #[test]
    fn alice_mixes_a_bare_flag_with_an_explicit_one() {
        let set = parse(&["notes", "--ro", "--auto", "false"]);
        assert_eq!(set.read_only, Some(true));
        assert_eq!(set.auto, Some(false));
    }
}
