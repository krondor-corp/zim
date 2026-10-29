//! App state: the connected daemon, and a hub login waiting for approval.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use zim::daemon::api::client::ApiClient;
use zim::hub::login::PendingLogin;

/// A live link to the daemon, established by `boot`.
pub struct Session {
    pub home: PathBuf,
    pub client: ApiClient,
}

#[derive(Default)]
pub struct AppState {
    session: RwLock<Option<Arc<Session>>>,
    pub pending_login: Mutex<Option<PendingLogin>>,
}

impl AppState {
    pub async fn set_session(&self, session: Session) {
        *self.session.write().await = Some(Arc::new(session));
    }

    pub async fn session(&self) -> Result<Arc<Session>, String> {
        self.session
            .read()
            .await
            .clone()
            .ok_or_else(|| "the zim daemon isn't connected yet (boot hasn't finished)".to_string())
    }
}
