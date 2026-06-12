//! `zim version` — prints local `BuildInfo`. No network.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::version::{build_info, BuildInfo};

#[derive(Args, Debug, Clone)]
pub struct Version;

#[derive(Debug, serde::Serialize)]
pub struct VersionOutput(BuildInfo);

#[derive(Debug, thiserror::Error)]
pub enum VersionError {}

#[async_trait]
impl Op for Version {
    type Context = ();
    type Output = VersionOutput;
    type Error = VersionError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(VersionOutput(build_info()))
    }
}

impl fmt::Display for VersionOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
