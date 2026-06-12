//! HTTP-backed [`DidResolver`] for `did:web`.
//!
//! Wraps a `reqwest::Client` and a small in-memory cache. Each
//! resolve:
//!
//! 1. Cache hit + fresh → return immediately.
//! 2. Cache miss / stale → GET `<scheme>://<host>[/<path>]/did.json`,
//!    decode the JSON, store, return.
//!
//! Production callers leave HTTPS on; the dev/loopback path flips
//! `allow_http` so a hub running on `http://127.0.0.1:18082` can be
//! resolved without standing up TLS.
//!
//! The cache has a single TTL (5 min by default) and is unbounded in
//! size — fine for the daemon, which only sees a handful of distinct
//! DIDs (its known peers + relays). Re-evaluate when we add public
//! browsing or any path that resolves arbitrary user DIDs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{did_web_url, Did, DidDocument, DidMethod, DidResolver, ResolveError};

/// Default freshness window for cached DID documents. The hub
/// rotates infrequently in practice — 5 min keeps spam-storm
/// resilience without making every share-add hit the network.
const DEFAULT_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct HttpDidResolver {
    client: reqwest::Client,
    /// Allow `http://` URLs. False in production; flipped on by
    /// callers that need to point at a dev hub on localhost.
    allow_http: bool,
    ttl: Duration,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

#[derive(Clone)]
struct CacheEntry {
    doc: DidDocument,
    stored_at: Instant,
}

impl HttpDidResolver {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            allow_http: false,
            ttl: DEFAULT_TTL,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Permit `http://` resolution for `did:web:` URLs. Use only on
    /// loopback / explicitly-dev hubs.
    pub fn with_allow_http(mut self, allow: bool) -> Self {
        self.allow_http = allow;
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
}

impl Default for HttpDidResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DidResolver for HttpDidResolver {
    async fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError> {
        if did.method() != DidMethod::Web {
            return Err(ResolveError::UnsupportedMethod(did.method().to_string()));
        }
        let key = did.as_str().to_string();

        // Cache hit?
        if let Some(entry) = self.cache.read().await.get(&key) {
            if entry.stored_at.elapsed() < self.ttl {
                return Ok(entry.doc.clone());
            }
        }

        let url = did_web_url(did, self.allow_http).map_err(ResolveError::InvalidDocument)?;
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ResolveError::Network(format!("GET {url}: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ResolveError::NotFound(key));
        }
        if !resp.status().is_success() {
            return Err(ResolveError::Network(format!(
                "GET {url} returned {}",
                resp.status()
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ResolveError::Network(format!("read body: {e}")))?;
        let doc: DidDocument = serde_json::from_slice(&bytes)
            .map_err(|e| ResolveError::InvalidDocument(format!("parse JSON: {e}")))?;

        // The document's `id` should equal what we asked for —
        // mismatch suggests a misconfigured hub serving the wrong
        // identity. We refuse rather than silently trust it.
        if doc.id != did.as_str() {
            return Err(ResolveError::InvalidDocument(format!(
                "document id `{}` does not match resolved DID `{}`",
                doc.id,
                did.as_str()
            )));
        }

        self.cache.write().await.insert(
            key,
            CacheEntry {
                doc: doc.clone(),
                stored_at: Instant::now(),
            },
        );
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_did_key() {
        let resolver = HttpDidResolver::new();
        let did = Did::parse("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let err = resolver.resolve(&did).await.unwrap_err();
        assert!(matches!(err, ResolveError::UnsupportedMethod(_)));
    }
}
