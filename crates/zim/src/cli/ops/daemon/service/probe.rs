//! Liveness probes against the daemon's `/_status/livez`.
//!
//! The OS service manager only tells us what it *asked* for; these
//! probes tell us whether a daemon is actually answering on the
//! configured API port.

use std::time::Duration;

use crate::daemon::api::client::ApiClient;
use crate::daemon::health::liveness::LivezRequest;

/// Same short timeout the `health` dashboard uses for its livez
/// probe. If the daemon doesn't answer within this window we treat
/// the port as free.
const PROBE_TIMEOUT: Duration = Duration::from_millis(750);

/// Pause between probes while waiting for the daemon to go down.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Whether a daemon answers on the configured API port.
pub async fn is_up(client: &ApiClient) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, client.call(LivezRequest {})).await,
        Ok(Ok(_))
    )
}

/// Poll until the daemon stops answering. `false` if it's still up
/// after `timeout`.
pub async fn wait_until_down(client: &ApiClient, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while is_up(client).await {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    true
}
