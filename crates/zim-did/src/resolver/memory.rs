//! In-memory [`DidResolver`] — a fixed `did → document` map.
//!
//! Tests build one instead of standing up an HTTP server; also fits
//! dry-runs. Production code never uses this directly.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::did::Did;
use crate::document::DidDocument;

use super::{DidResolver, ResolveError};

#[derive(Debug, Clone, Default)]
pub struct StaticResolver {
    docs: Arc<HashMap<String, DidDocument>>,
}

impl StaticResolver {
    pub fn new(docs: HashMap<String, DidDocument>) -> Self {
        Self {
            docs: Arc::new(docs),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DidResolver for StaticResolver {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError> {
        self.docs
            .get(did.as_str())
            .cloned()
            .ok_or_else(|| ResolveError::NotFound(did.as_str().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_the_stored_document() {
        // A spec-shaped doc (controller/type/purpose extras) — parses
        // fine, extras ignored.
        let doc: DidDocument = serde_json::from_str(
            r##"{
                "id": "did:web:hub.example.com",
                "verificationMethod": [{
                    "id": "#peer",
                    "controller": "did:web:hub.example.com",
                    "type": "Ed25519VerificationKey2020",
                    "publicKeyMultibase": "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                    "purpose": "peer"
                }]
            }"##,
        )
        .unwrap();
        let mut docs = HashMap::new();
        docs.insert("did:web:hub.example.com".into(), doc);
        let r = StaticResolver::new(docs);
        let did = Did::parse("did:web:hub.example.com").unwrap();
        let got = r.resolve(&did).await.unwrap();
        assert_eq!(got.verification_method.len(), 1);
    }

    #[tokio::test]
    async fn unknown_did_is_not_found() {
        let r = StaticResolver::default();
        let did = Did::parse("did:web:nowhere.example.com").unwrap();
        assert!(matches!(
            r.resolve(&did).await.unwrap_err(),
            ResolveError::NotFound(_)
        ));
    }
}
