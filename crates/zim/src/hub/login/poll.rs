//! Poll `device-code/poll` until the hub enrolls us, the code expires,
//! or the caller's timeout runs out.

use std::time::{Duration, Instant};

use zim_api::hub::{DeviceCodePollOk, DeviceCodePollRequest};
use zim_api::{ApiRequest, Client};
use zim_crypto::PrivateKey;

use super::LoginError;

/// Loop until the grant for `code` is approved *and* our possession
/// proof is accepted (the `200` that is the enrollment).
pub(super) async fn until_enrolled(
    client: &Client,
    code: &str,
    secret: &PrivateKey,
    interval: Duration,
    timeout: Duration,
) -> Result<(), LoginError> {
    let deadline = Instant::now() + timeout;
    let pubkey_hex = secret.public().to_hex();

    // The possession proof is fixed for the life of the loop — sign once,
    // send it on every poll. Payload is `code_bytes || pubkey_bytes`; the
    // hub rejects any approved-grant poll that doesn't carry it.
    let mut payload = Vec::with_capacity(code.len() + 32);
    payload.extend_from_slice(code.as_bytes());
    payload.extend_from_slice(&secret.public().to_bytes());
    let signature = hex::encode(secret.sign(&payload).to_bytes());

    loop {
        if Instant::now() >= deadline {
            return Err(LoginError::Timeout(timeout.as_secs()));
        }
        // Sent by hand, not via `Client::call`: `202` (pending) is a 2xx
        // with a non-JSON body, and the status is the answer.
        let res = DeviceCodePollRequest {
            code: code.to_string(),
            signature: Some(signature.clone()),
        }
        .build_request(client.base(), client.http())
        .send()
        .await
        .map_err(|e| LoginError::Hub(e.to_string()))?;
        match res.status().as_u16() {
            200 => {
                // Sanity: the hub echoes the pubkey it enrolled. Catch a
                // misaddressed login that somehow hit another daemon's grant.
                let body: DeviceCodePollOk = res
                    .json()
                    .await
                    .map_err(|e| LoginError::Hub(e.to_string()))?;
                if body.pubkey != pubkey_hex {
                    return Err(LoginError::Hub(format!(
                        "hub enrolled a different pubkey ({} vs ours {})",
                        body.pubkey, pubkey_hex
                    )));
                }
                return Ok(());
            }
            202 => tokio::time::sleep(interval).await,
            410 => return Err(LoginError::Expired),
            _ => {
                let status = res.status();
                let body = res.text().await.unwrap_or_default();
                return Err(LoginError::HubStatus(status, body));
            }
        }
    }
}
