use clap::Args;

use zim_peer::spawn_service;
use zim_peer::state::AppState;

#[derive(Args, Debug, Clone)]
pub struct Daemon {
    /// Override API server port (default from config)
    #[arg(long)]
    pub api_port: Option<u16>,

    /// Override gateway server port (default from config)
    #[arg(long)]
    pub gateway_port: Option<u16>,

    /// Gateway URL for share/download links (e.g., https://gateway.example.com)
    #[arg(long)]
    pub gateway_url: Option<String>,

    /// Directory for log files (logs to stdout only if not set)
    #[arg(long)]
    pub log_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("state error: {0}")]
    StateError(#[from] zim_peer::state::StateError),

    #[error("daemon failed: {0}")]
    Failed(String),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Daemon {
    type Error = DaemonError;
    type Output = String;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        // Load state from config path (or default ~/.jax)
        let state = AppState::load(ctx.config_path.clone())?;

        // Project AppState → runtime ServiceConfig (single projection helper).
        let config = state.build_service_config(
            self.api_port,
            self.gateway_port,
            tracing::Level::DEBUG,
            self.log_dir.clone(),
            self.gateway_url.clone(),
        )?;

        spawn_service(&config).await;
        Ok("daemon ended".to_string())
    }
}
