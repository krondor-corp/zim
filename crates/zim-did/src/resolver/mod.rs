//! [`DidResolver`] trait — the indirection between identity types
//! and the live key material those identities resolve to.
//!
//! The trait lives here; concrete implementations get a file each:
//! [`memory::StaticResolver`] (fixed map — tests, dry-runs) lives in
//! this crate; the reqwest-backed `HttpDidResolver` lives in
//! `zim-api::hub::resolver` (with the rest of the HTTP stack), so this
//! crate stays pure — types + codec + trait. Anything else that owns a
//! network stack can implement [`DidResolver`] itself.
//!
//! ## Resolving a [`Did`] to a pubkey
//!
//! The common case is "I have a `Did`, give me the ed25519 pubkey".
//! Use [`resolve_pubkey`] — it short-circuits for `did:key`
//! (zero-network, [`Did::pubkey`]) and hits the resolver only for
//! `did:web`.

pub mod memory;

pub use memory::StaticResolver;

use async_trait::async_trait;
use zim_crypto::PublicKey;

use crate::did::{Did, DidMethod};
use crate::document::DidDocument;

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

// Two spellings of one trait: native futures are `Send` (the daemon
// resolves from multi-threaded tokio); wasm is single-threaded and
// reqwest's fetch-backed futures are `!Send`, so the bound comes off.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait DidResolver: Send + Sync + 'static {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError>;
}

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait DidResolver: 'static {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError>;
}

/// Return the first decodable ed25519 pubkey in the document.
/// DIDs describe where to find a key — routing decisions belong to
/// the caller (e.g. whether this key is a relay `via` or `recipient`).
pub fn pick_pubkey(doc: &DidDocument) -> Result<PublicKey, ResolveError> {
    for vm in &doc.verification_method {
        if let Ok(pk) = vm.pubkey() {
            return Ok(pk);
        }
    }
    Err(ResolveError::InvalidDocument(
        "no decodable verification method in document".into(),
    ))
}

/// Convenience: turn a [`Did`] into a concrete pubkey,
/// short-circuiting for `did:key` and resolving `did:web` via
/// `resolver`.
pub async fn resolve_pubkey<R: DidResolver + ?Sized>(
    did: &Did,
    resolver: &R,
) -> Result<PublicKey, ResolveError> {
    match did.method() {
        DidMethod::Key => did
            .pubkey()
            .ok_or_else(|| ResolveError::InvalidDocument(format!("undecodable did:key {did}"))),
        DidMethod::Web => {
            let doc = resolver.resolve(did).await?;
            pick_pubkey(&doc)
        }
        DidMethod::Other(m) => Err(ResolveError::UnsupportedMethod(m.to_string())),
    }
}

// ─── Reach: seal target + dial target ─────────────────────────────

/// The reachability of one client behind a [`Did`]: who to seal a
/// secret **to** (`client`), and how to **reach/dial** them (`via`).
///
/// This is the unit the hosted-DID protocol shares against — one `Reach`
/// becomes one manifest share. `via` is what folds the old separate
/// `Relay` type away:
///
/// - `did:key:…` → `via = None`: the client is dialed directly (its iroh
///   `NodeId` is the key).
/// - `did:web:<host>:…` → `via = Some((did:web:<host>, host_key))`: the
///   secret is still sealed to `client` (zero-knowledge — the host never
///   gets it), but sync dials the **host**, never the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// Seal target — the verification-method key the secret is sealed to.
    pub client: PublicKey,
    /// Dial/ping target. `None` for a directly-dialable `did:key`;
    /// `Some((host_did, host_key))` for a hosted `did:web`.
    pub via: Option<(Did, PublicKey)>,
}

/// Resolve a [`Did`] to the full set of clients it names, each with
/// its dial target — the foundational hosted-DID operation.
///
/// - `did:key` → one directly-dialable client (`via: None`).
/// - `did:web` → one client **per verification method** in the resolved
///   document (the whole device set for an account DID), all reached via
///   the host: the host-only `did:web:<host>` is resolved once to its
///   peer key, shared as every client's `via`.
///
/// Each returned `Reach` maps to one manifest share, so sharing to a
/// single account DID seals the secret to every device individually
/// while routing all of them through the one host.
pub async fn resolve_reaches<R: DidResolver + ?Sized>(
    did: &Did,
    resolver: &R,
) -> Result<Vec<Reach>, ResolveError> {
    match did.method() {
        DidMethod::Key => Ok(vec![Reach {
            client: did.pubkey().ok_or_else(|| {
                ResolveError::InvalidDocument(format!("undecodable did:key {did}"))
            })?,
            via: None,
        }]),
        DidMethod::Web => {
            let doc = resolver.resolve(did).await?;
            let clients: Vec<PublicKey> = doc
                .verification_method
                .iter()
                .filter_map(|vm| vm.pubkey().ok())
                .collect();
            if clients.is_empty() {
                return Err(ResolveError::InvalidDocument(format!(
                    "no decodable verification method in {did}"
                )));
            }
            // Dial target: the host-only `did:web:<host>` (strip path +
            // fragment), resolved to its peer key. One lookup, shared by
            // every client.
            let host = host_did(did)?;
            let host_doc = resolver.resolve(&host).await?;
            let host_key = pick_pubkey(&host_doc)?;
            let via = Some((host, host_key));
            Ok(clients
                .into_iter()
                .map(|client| Reach {
                    client,
                    via: via.clone(),
                })
                .collect())
        }
        DidMethod::Other(m) => Err(ResolveError::UnsupportedMethod(m.to_string())),
    }
}

/// The host-only `did:web:<host>` for a (possibly pathed/fragmented)
/// `did:web` — strips the path segments and any `#fragment`.
fn host_did(did: &Did) -> Result<Did, ResolveError> {
    let ident = did.identifier();
    let ident = ident.split('#').next().unwrap_or(ident);
    let host = ident.split(':').next().unwrap_or(ident);
    Did::parse(&format!("did:web:{host}"))
        .map_err(|e| ResolveError::InvalidDocument(format!("host did:web:{host}: {e}")))
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
    use std::collections::HashMap;

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
    async fn resolve_pubkey_short_circuits_for_key_identity() {
        let sk = zim_crypto::PrivateKey::generate();
        let did = Did::from_key(&sk.public());
        let r = StaticResolver::default();
        let pk = resolve_pubkey(&did, &r).await.unwrap();
        assert_eq!(pk, sk.public());
    }

    #[tokio::test]
    async fn resolve_reaches_key_identity_is_direct() {
        let sk = zim_crypto::PrivateKey::generate();
        let r = StaticResolver::default();
        let reaches = resolve_reaches(&Did::from_key(&sk.public()), &r)
            .await
            .unwrap();
        assert_eq!(reaches.len(), 1);
        assert_eq!(reaches[0].client, sk.public());
        assert!(reaches[0].via.is_none(), "did:key dials directly");
    }

    #[tokio::test]
    async fn resolve_reaches_web_seals_each_device_dials_host() {
        use crate::did_key::did_key_encode;
        use crate::document::VerificationMethod;

        let vm = |pk: &PublicKey, ctrl: &str| VerificationMethod {
            id: format!("{ctrl}#k"),
            public_key_multibase: did_key_encode(pk),
        };

        let host_key = zim_crypto::PrivateKey::generate().public();
        let dev1 = zim_crypto::PrivateKey::generate().public();
        let dev2 = zim_crypto::PrivateKey::generate().public();

        let mut docs = HashMap::new();
        docs.insert(
            "did:web:hub.example.com".to_string(),
            DidDocument {
                id: "did:web:hub.example.com".into(),
                verification_method: vec![vm(&host_key, "did:web:hub.example.com")],
            },
        );
        docs.insert(
            "did:web:hub.example.com:u:alice".to_string(),
            DidDocument {
                id: "did:web:hub.example.com:u:alice".into(),
                verification_method: vec![
                    vm(&dev1, "did:web:hub.example.com:u:alice"),
                    vm(&dev2, "did:web:hub.example.com:u:alice"),
                ],
            },
        );
        let r = StaticResolver::new(docs);

        let user = Did::parse("did:web:hub.example.com:u:alice").unwrap();
        let reaches = resolve_reaches(&user, &r).await.unwrap();

        // One reach per device, each sealed to its own key…
        assert_eq!(reaches.len(), 2);
        let clients: Vec<_> = reaches.iter().map(|r| r.client).collect();
        assert!(clients.contains(&dev1) && clients.contains(&dev2));
        // …and every one dials the same host (never the client).
        for reach in &reaches {
            let (host_did, hk) = reach.via.as_ref().expect("hosted device → via Some");
            assert_eq!(host_did.as_str(), "did:web:hub.example.com");
            assert_eq!(*hk, host_key);
        }
    }
}
