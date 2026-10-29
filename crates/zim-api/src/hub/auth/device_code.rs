//! `POST /api/v0/auth/device-code/{start,poll}` — the daemon side of the
//! device-code pairing flow.
//!
//! The daemon commits to its pubkey at `start`, the user approves the
//! returned code on the hub's `/auth/device` page, and the daemon's
//! signed `poll` *is* the enrollment. See the hub handler
//! (`zim-hub/src/http/api/v0/auth.rs`) for the full sequence.
//!
//! `poll` answers with its status code, not just its body: `200` +
//! [`DeviceCodePollOk`] once enrolled, `202` while pending approval, `410`
//! once the code is gone or expired. Callers that need the pending/gone
//! distinction send [`DeviceCodePollRequest::build_request`] themselves
//! and match on the status instead of going through
//! [`Client::call`](crate::Client::call), which treats any 2xx as a
//! decodable body.

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::ApiRequest;

/// `start` body: the pubkey the daemon commits to, plus a display label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeStartBody {
    /// 64-char lowercase hex of the daemon's ed25519 public key. The
    /// approve page renders it so the user can match it against what the
    /// daemon shows, and the poll signature has a fixed target.
    pub pubkey: String,
    /// Free-form display label for the device, shown on the approve page
    /// next to the pubkey. Hostname is a good default.
    #[serde(default)]
    pub label: String,
}

/// `start` reply: what the user needs to approve the pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeStart {
    /// 9-char human-typable code (`ABCD-EFGH`), shown to the user to
    /// verify on the approve page.
    pub code: String,
    /// Full URL of the hub's approve page for this code.
    pub verification_url: String,
    pub expires_at: String,
    /// Suggested poll interval. The hub doesn't rate limit today, but
    /// daemons should still honour it.
    pub poll_interval_secs: u64,
}

/// **Hub route to mirror:** `POST /api/v0/auth/device-code/start`
/// (public).
pub struct DeviceCodeStartRequest {
    pub pubkey: String,
    pub label: String,
}

impl ApiRequest for DeviceCodeStartRequest {
    type Response = DeviceCodeStart;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.post(
            base.join("/api/v0/auth/device-code/start")
                .expect("static path"),
        )
        .json(&DeviceCodeStartBody {
            pubkey: self.pubkey,
            label: self.label,
        })
    }
}

/// `poll` body: the code, plus the possession proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodePollBody {
    pub code: String,
    /// 128-char lowercase hex of the ed25519 signature over
    /// `code_bytes || pubkey_bytes`. When omitted the poll is a status
    /// check only (`202` / `410`) and never enrolls.
    #[serde(default)]
    pub signature: Option<String>,
}

/// `poll` `200` reply: the daemon is enrolled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodePollOk {
    pub ok: bool,
    /// Echoes the pubkey that was enrolled, so the daemon can confirm the
    /// hub agrees on identity before persisting a session.
    pub pubkey: String,
}

/// **Hub route to mirror:** `POST /api/v0/auth/device-code/poll`
/// (public; the signature is the auth). See the module docs for why
/// callers usually match the status themselves.
pub struct DeviceCodePollRequest {
    pub code: String,
    pub signature: Option<String>,
}

impl ApiRequest for DeviceCodePollRequest {
    type Response = DeviceCodePollOk;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.post(
            base.join("/api/v0/auth/device-code/poll")
                .expect("static path"),
        )
        .json(&DeviceCodePollBody {
            code: self.code,
            signature: self.signature,
        })
    }
}
