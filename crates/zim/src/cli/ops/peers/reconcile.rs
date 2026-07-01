//! `zim peers reconcile` — re-share the vaults you own with every
//! trusted contact.
//!
//! Trusted contacts are your own devices (from `hub peers sync`) plus
//! anyone you `peers add --trust`. This grants each of their device keys
//! access to every vault you authored, announcing the new heads so they
//! pull. Idempotent: already-shared vaults are left untouched.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::peers::reconcile::ReconcileRequest;

#[derive(Args, Debug, Clone)]
pub struct Reconcile {}

#[derive(Debug, serde::Serialize)]
pub struct ReconcileOutput {
    pub vaults_scanned: usize,
    pub vaults_updated: usize,
    pub shares_added: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Reconcile {
    type Context = ApiContext;
    type Output = ReconcileOutput;
    type Error = ReconcileError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let r = ctx.client.call(ReconcileRequest {}).await?;
        Ok(ReconcileOutput {
            vaults_scanned: r.vaults_scanned,
            vaults_updated: r.vaults_updated,
            shares_added: r.shares_added,
        })
    }
}

impl fmt::Display for ReconcileOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.shares_added == 0 {
            write!(
                f,
                "{} ({} owned vault(s) already up to date)",
                ui::success("reconciled", ""),
                self.vaults_scanned
            )
        } else {
            write!(
                f,
                "{} {} share(s) added across {} vault(s)",
                ui::success("reconciled", ""),
                ui::num(self.shares_added.to_string()),
                ui::num(self.vaults_updated.to_string()),
            )
        }
    }
}
