//! DID document types — the shape returned by a [`DidResolver`].
//!
//! Matches the W3C DID Core spec at the field level for the bits we
//! care about. Unknown top-level keys deserialize unchanged but get
//! dropped; that's fine for our needs (we only inspect
//! `verificationMethod`).
//!
//! Field naming follows the spec: `verificationMethod`,
//! `publicKeyMultibase`. `type` is renamed to `vm_type` in Rust to
//! dodge the keyword conflict.

use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;

use crate::did_key::did_key_decode;

/// Resolved DID document. Only the fields we actually consume are
/// modeled — the spec allows many more keys at the document level
/// (`@context`, `controller`, `authentication`, etc.) which we
/// happily ignore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
    /// The document's canonical DID. Should equal the URL the caller
    /// resolved; mismatch is a useful sanity check the resolver can
    /// run.
    pub id: String,

    #[serde(rename = "verificationMethod", default)]
    pub verification_method: Vec<VerificationMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    /// e.g. `"did:web:hub.example.com#peer"`.
    pub id: String,

    /// Controller DID — the entity allowed to update the key.
    pub controller: String,

    #[serde(rename = "type")]
    pub vm_type: String,

    /// Multibase-encoded public key. Format matches the bytes after
    /// `did:key:` (multibase-prefixed multicodec + key bytes), so
    /// [`did_key_decode`] is the bridge.
    #[serde(rename = "publicKeyMultibase")]
    pub public_key_multibase: String,

    /// Custom field describing what this verification method is FOR.
    /// Not in the DID-core spec — added so the dial loop can tell a
    /// browser-resident `web` key from a server-runnable `peer` key
    /// without dialing every method and discovering by timeout.
    ///
    /// Missing/unknown → [`VmPurpose::Unknown`]. Treat unknown as
    /// peer-capable for now; revisit if it causes wasted dials.
    #[serde(default)]
    pub purpose: VmPurpose,
}

impl VerificationMethod {
    /// Decode `public_key_multibase` to an ed25519 pubkey. Errors
    /// surface as a human-readable string suitable for logging.
    pub fn pubkey(&self) -> Result<PublicKey, String> {
        did_key_decode(&self.public_key_multibase)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VmPurpose {
    /// Dialable as an iroh peer (daemon / hub backend).
    Peer,
    /// Browser-resident key (`zim-wasm`) — signs and decrypts but
    /// never runs an endpoint. Dial loop skips these.
    Web,
    #[default]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_document() {
        let body = r#"{
            "id": "did:web:hub.example.com",
            "verificationMethod": [{
                "id": "did:web:hub.example.com#peer",
                "controller": "did:web:hub.example.com",
                "type": "Ed25519VerificationKey2020",
                "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                "purpose": "peer"
            }]
        }"#;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert_eq!(doc.id, "did:web:hub.example.com");
        assert_eq!(doc.verification_method.len(), 1);
        let vm = &doc.verification_method[0];
        assert_eq!(vm.vm_type, "Ed25519VerificationKey2020");
        assert_eq!(vm.purpose, VmPurpose::Peer);
        assert!(vm.pubkey().is_ok());
    }

    #[test]
    fn unknown_purpose_falls_through() {
        // Two hashes on the raw string so JSON fragments like `"#a"`
        // don't close it early.
        let body = r##"{
            "id": "did:web:x",
            "verificationMethod": [{
                "id": "#a",
                "controller": "did:web:x",
                "type": "Ed25519VerificationKey2020",
                "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
            }]
        }"##;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert_eq!(doc.verification_method[0].purpose, VmPurpose::Unknown);
    }

    #[test]
    fn missing_verification_method_defaults_to_empty() {
        let body = r#"{"id":"did:web:x"}"#;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert!(doc.verification_method.is_empty());
    }
}
