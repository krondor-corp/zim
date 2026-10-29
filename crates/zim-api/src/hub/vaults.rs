//! `GET /api/v0/vaults` — the vaults shared to the account's keys.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use zim_core::vault::VaultId;

use crate::ApiRequest;

/// One vault the hub mirrors for this account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultItem {
    pub vault_id: VaultId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultsResponse {
    pub vaults: Vec<VaultItem>,
}

/// **Hub route to mirror:** `GET /api/v0/vaults` (RequireOnboardedUser).
pub struct VaultsRequest;

impl ApiRequest for VaultsRequest {
    type Response = VaultsResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/vaults").expect("static path"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_HEX: &str = "0a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728ff";

    #[test]
    fn alices_vault_list_keeps_its_hex_id_on_the_wire() {
        let wire = format!(r#"{{"vaults":[{{"vault_id":"{ID_HEX}","name":"notes"}}]}}"#);

        let parsed: VaultsResponse = serde_json::from_str(&wire).unwrap();

        assert_eq!(parsed.vaults[0].vault_id.to_string(), ID_HEX);
        assert_eq!(parsed.vaults[0].name, "notes");
        assert_eq!(serde_json::to_string(&parsed).unwrap(), wire);
    }

    #[test]
    fn a_malformed_vault_id_is_rejected() {
        let wire = r#"{"vault_id":"not-hex","name":"x"}"#;
        assert!(serde_json::from_str::<VaultItem>(wire).is_err());
    }
}
