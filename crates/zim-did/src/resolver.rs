//! [`DidResolver`] trait — the indirection between identity types
//! and the live key material those identities resolve to.
//!
//! This crate keeps the trait but **provides no network-backed
//! implementation** (see the crate-level doc). Downstream crates
//! that own the network stack (`zim` today, eventually `zim-hub`)
//! implement it. Tests use [`StaticResolver`] to plug in a fixed
//! mapping without standing up an HTTP server.
//!
//! ## Resolving an [`Identity`] to a dialable pubkey
//!
//! The common case is "I have an `Identity`, give me the ed25519
//! pubkey I should use to dial / encrypt-to". Use
//! [`resolve_pubkey`] — it short-circuits for `Identity::Key`
//! (zero-network) and falls through to the resolver only for
//! `Identity::Web`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use zim_crypto::PublicKey;

use crate::did::Did;
use crate::document::{DidDocument, VmPurpose};
use crate::identity::Identity;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("DID not found: {0}")]
    NotFound(String),
    #[error("network: {0}")]
    Network(String),
    #[error("invalid DID document: {0}")]
    InvalidDocument(String),
    #[error("unsupported method `{0}` for resolution")]
    UnsupportedMethod(String),
}

#[async_trait]
pub trait DidResolver: Send + Sync + 'static {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError>;
}

/// Walk a resolved document's verification methods, return the first
/// dialable ed25519 pubkey. "Dialable" = `purpose: peer` OR
/// `purpose: unknown` (the latter so docs that pre-date the
/// `purpose` extension still work).
///
/// `Web`-purpose methods are skipped — they're browser-only keys
/// and dialing them is guaranteed to time out.
pub fn pick_peer_pubkey(doc: &DidDocument) -> Result<PublicKey, ResolveError> {
    for vm in &doc.verification_method {
        if matches!(vm.purpose, VmPurpose::Peer | VmPurpose::Unknown) {
            if let Ok(pk) = vm.pubkey() {
                return Ok(pk);
            }
        }
    }
    Err(ResolveError::InvalidDocument(
        "no peer-dialable verification method in document".into(),
    ))
}

/// Convenience: turn an [`Identity`] into a concrete pubkey,
/// short-circuiting for the `Key` case and resolving the `Web` case
/// via `resolver`.
pub async fn resolve_pubkey<R: DidResolver + ?Sized>(
    identity: &Identity,
    resolver: &R,
) -> Result<PublicKey, ResolveError> {
    match identity {
        Identity::Key(pk) => Ok(*pk),
        Identity::Web { did, .. } => {
            let doc = resolver.resolve(did).await?;
            pick_peer_pubkey(&doc)
        }
    }
}

// ─── Static (tests, dry-run) ──────────────────────────────────────

/// In-memory resolver. Tests build one with a fixed `did → doc` map;
/// production code never uses this directly.
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

#[async_trait]
impl DidResolver for StaticResolver {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError> {
        self.docs
            .get(did.as_str())
            .cloned()
            .ok_or_else(|| ResolveError::NotFound(did.as_str().to_string()))
    }
}

// ─── did:web URL construction ─────────────────────────────────────

/// Build the HTTPS URL where a `did:web:` document lives.
///
/// Per spec:
/// - `did:web:<host>` → `https://<host>/.well-known/did.json`
/// - `did:web:<host>:<path…>` → `https://<host>/<path…>/did.json`
///   (with `:` in the method-specific id replaced by `/`)
///
/// `allow_http` is the dev-mode escape hatch: when true, the URL uses
/// `http://` instead of `https://`. Production callers leave it
/// false. Percent-encoded characters in the host (e.g. `%3A` for
/// port) are passed through unchanged — reqwest decodes them.
pub fn did_web_url(did: &Did, allow_http: bool) -> Result<String, String> {
    use crate::did::DidMethod;
    if did.method() != DidMethod::Web {
        return Err(format!("not a did:web: {did}"));
    }
    let scheme = if allow_http { "http" } else { "https" };
    let identifier = did.identifier();
    // Strip any DID-URL fragment (`#vm-id`) — only the document URL
    // is what we want; the fragment selects within the doc.
    let identifier = identifier.split('#').next().unwrap_or(identifier);

    let mut segments = identifier.split(':');
    let host = segments
        .next()
        .ok_or_else(|| "did:web missing host".to_string())?;
    // W3C did:web §3.1.2: percent-decode each part before constructing
    // the URL — chiefly so a port-bearing identifier like
    // `127.0.0.1%3A8080` becomes `127.0.0.1:8080` in the URL netloc.
    // Without this, reqwest's URL parser rejects `%3A` in the host.
    let host = percent_decode_segment(host);
    let rest: Vec<String> = segments.map(percent_decode_segment).collect();
    if rest.is_empty() {
        Ok(format!("{scheme}://{host}/.well-known/did.json"))
    } else {
        Ok(format!("{scheme}://{host}/{}/did.json", rest.join("/")))
    }
}

fn percent_decode_segment(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_for_root_did_web_uses_well_known() {
        let did = Did::parse("did:web:hub.example.com").unwrap();
        assert_eq!(
            did_web_url(&did, false).unwrap(),
            "https://hub.example.com/.well-known/did.json"
        );
    }

    #[test]
    fn url_for_pathed_did_web_drops_well_known() {
        let did = Did::parse("did:web:hub.example.com:u:alice").unwrap();
        assert_eq!(
            did_web_url(&did, false).unwrap(),
            "https://hub.example.com/u/alice/did.json"
        );
    }

    #[test]
    fn url_for_port_encoded_host_decodes_percent() {
        // W3C did:web §3.1.2: percent-decode each segment. The port
        // colon comes through as a literal `:` in the URL — `%3A`
        // would make reqwest's URL parser reject the netloc.
        let did = Did::parse("did:web:127.0.0.1%3A18082").unwrap();
        assert_eq!(
            did_web_url(&did, true).unwrap(),
            "http://127.0.0.1:18082/.well-known/did.json"
        );
    }

    #[test]
    fn rejects_did_key_input() {
        let did = Did::parse("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        assert!(did_web_url(&did, false).is_err());
    }

    #[tokio::test]
    async fn static_resolver_returns_stored_document() {
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
    async fn resolve_pubkey_short_circuits_for_key_identity() {
        let sk = zim_crypto::PrivateKey::generate();
        let id = Identity::Key(sk.public());
        let r = StaticResolver::default();
        let pk = resolve_pubkey(&id, &r).await.unwrap();
        assert_eq!(pk, sk.public());
    }
}
