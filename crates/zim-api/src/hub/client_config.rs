//! Client timing config the hub injects into the SPA's `index.html`.
//!
//! The hub serves the web app, so instead of an endpoint (and a round
//! trip at boot) it writes this struct as a JSON `<script>` into the page
//! it hands out (see `zim-hub`'s `http::spa`). The SPA reads it once at
//! startup and falls back to [`ClientConfig::default`] when the tag is
//! absent — the desktop app loads the SPA from local files, not a hub.

use serde::{Deserialize, Serialize};

/// The `id` of the `<script type="application/json">` tag carrying the
/// config. A JSON script is data, never executed, so the hub's CSP holds.
pub const CLIENT_CONFIG_ELEMENT_ID: &str = "zim-config";

/// Timings the SPA uses for saving and following remote edits.
/// Missing fields take their default, so an older hub's page still parses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// How long an edit may sit unsaved before autosave writes it. Every
    /// write is a version, so this bounds history density.
    pub autosave_ms: u32,
    /// How often an open vault re-checks the hub's head for other
    /// devices' edits.
    pub follow_ms: u32,
}

impl ClientConfig {
    /// Production autosave delay.
    pub const DEFAULT_AUTOSAVE_MS: u32 = 30_000;
    /// Production follow-poll interval.
    pub const DEFAULT_FOLLOW_MS: u32 = 3_000;
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            autosave_ms: Self::DEFAULT_AUTOSAVE_MS,
            follow_ms: Self::DEFAULT_FOLLOW_MS,
        }
    }
}
