//! `zim hub register` — generate a device keypair, cache it locally, print
//! the pubkey for manual registration with the hub operator.
//!
//! Full browser-approval flow (auto-register via `/auth/cli-register`) requires
//! T-017a's hub-side surface. Until then, the operator copies the printed
//! pubkey and adds the device via the hub's admin UI or `zim bucket viewer
//! authorize`.

use std::fmt;
use std::path::PathBuf;

use clap::Args;

use crate::cli::ui;
use zim_crypto::PrivateKey;

fn hub_config_dir(hub_url: &str) -> PathBuf {
    let sanitised = hub_url
        .replace("https://", "")
        .replace("http://", "")
        .replace(['/', ':'], "_");
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zim")
        .join("hubs")
        .join(sanitised)
}

#[derive(Args, Debug, Clone)]
pub struct Register {
    /// Hub URL (e.g. https://hub.example.com)
    #[arg(long)]
    pub hub: String,

    /// Human-readable label for this device (e.g. "MacBook Pro")
    #[arg(long, default_value = "CLI device")]
    pub label: String,
}

#[derive(Debug)]
pub struct RegisterOutput {
    pub hub: String,
    pub public_key: String,
    pub key_path: PathBuf,
}

impl fmt::Display for RegisterOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", ui::success("Registered", &self.hub))?;
        writeln!(f, "{}", ui::label("pubkey", &self.public_key))?;
        writeln!(
            f,
            "{}",
            ui::label("key", &self.key_path.display().to_string())
        )?;
        write!(
            f,
            "\n  Give the pubkey above to the bucket owner so they can run:\n  \
             zim bucket viewer authorize <BUCKET> {}",
            self.public_key
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Device key already exists at {0}. Use --force to overwrite (not implemented).")]
    AlreadyRegistered(PathBuf),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Register {
    type Error = RegisterError;
    type Output = RegisterOutput;

    async fn execute(&self, _ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let config_dir = hub_config_dir(&self.hub);
        let key_path = config_dir.join("device.key.pem");

        if key_path.exists() {
            return Err(RegisterError::AlreadyRegistered(key_path));
        }

        std::fs::create_dir_all(&config_dir)?;

        let secret = PrivateKey::generate();
        let public_key = secret.public().to_hex();

        std::fs::write(&key_path, secret.to_pem())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
        }

        // Write label + hub metadata for future `login`.
        let meta_path = config_dir.join("device.json");
        let meta = serde_json::json!({
            "hub": self.hub,
            "label": self.label,
            "public_key": public_key,
        });
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap())?;

        Ok(RegisterOutput {
            hub: self.hub.clone(),
            public_key,
            key_path,
        })
    }
}
