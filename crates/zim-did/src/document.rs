//! DID document types — the shape returned by a [`DidResolver`].
//!
//! **Deliberately minimal, and NOT a claim of W3C DID Core
//! conformance.** Zim uses a DID document for exactly one thing:
//! *identifier → current key list*. We keep the spec's field names
//! (`verificationMethod`, `publicKeyMultibase`) so the shape stays
//! recognizable, but we neither emit nor honor the spec's semantic
//! surface — no `controller` (delegation), no `type` (suite
//! conformance), no `authentication`/`assertionMethod` (verification
//! relationships), no `@context` (JSON-LD). Those imply security
//! guarantees zim does not ship: the trust model is simply "the host
//! serving the document is trusted for the roster" (see
//! `docs/product/security.md`). Spec-shaped documents from other
//! producers still parse — unknown fields are ignored.
//!
//! [`DidResolver`]: crate::DidResolver

use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;

use crate::did_key::did_key_decode;

/// Resolved DID document: the identifier + its current keys. Every
/// other document-level key is ignored on parse and never emitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
    /// The document's canonical DID. Should equal the URL the caller
    /// resolved; mismatch is a useful sanity check the resolver can
    /// run.
    pub id: String,

    #[serde(rename = "verificationMethod", default)]
    pub verification_method: Vec<VerificationMethod>,
}

/// One key. Just a human-addressable name and the key material —
/// no controller/type/relationship semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    /// e.g. `"did:web:hub.example.com#key-0"` — a display/reference
    /// handle, nothing more.
    #[serde(default)]
    pub id: String,

    /// Multibase-encoded public key. Format matches the bytes after
    /// `did:key:` (multibase-prefixed multicodec + key bytes), so
    /// [`did_key_decode`] is the bridge — the multicodec prefix already
    /// says "ed25519", which is why there's no `type` field.
    #[serde(rename = "publicKeyMultibase")]
    pub public_key_multibase: String,
}

impl VerificationMethod {
    /// Decode `public_key_multibase` to an ed25519 pubkey. Errors
    /// surface as a human-readable string suitable for logging.
    pub fn pubkey(&self) -> Result<PublicKey, String> {
        did_key_decode(&self.public_key_multibase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_document() {
        let body = r#"{
            "id": "did:web:hub.example.com",
            "verificationMethod": [{
                "id": "did:web:hub.example.com#key-0",
                "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
            }]
        }"#;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert_eq!(doc.id, "did:web:hub.example.com");
        assert_eq!(doc.verification_method.len(), 1);
        assert!(doc.verification_method[0].pubkey().is_ok());
    }

    #[test]
    fn spec_shaped_documents_still_parse_with_extras_ignored() {
        // A W3C-conformant producer's doc: @context, controller, type,
        // authentication all present — and all ignored.
        let body = r##"{
            "@context": ["https://www.w3.org/ns/did/v1"],
            "id": "did:web:x",
            "verificationMethod": [{
                "id": "#a",
                "controller": "did:web:x",
                "type": "Ed25519VerificationKey2020",
                "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
            }],
            "authentication": ["#a"]
        }"##;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert!(doc.verification_method[0].pubkey().is_ok());
    }

    #[test]
    fn missing_verification_method_defaults_to_empty() {
        let body = r#"{"id":"did:web:x"}"#;
        let doc: DidDocument = serde_json::from_str(body).unwrap();
        assert!(doc.verification_method.is_empty());
    }
}
