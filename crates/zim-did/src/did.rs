//! [`Did`] — a parsed DID URL.
//!
//! Two methods supported by this crate today:
//!
//! - `did:key:<multibase>` — self-describing; the multibase value
//!   decodes (via the [`did_key`](crate::did_key) module) to a public
//!   key. No network resolution.
//! - `did:web:<host>[:<path-segments>]` — resolves to
//!   `https://<host>/[<path>/]\.well-known/did.json` per the did:web
//!   spec. Parsing only; resolution is out of scope.
//!
//! The `Did` type is intentionally a thin wrapper around the original
//! URL string plus a parsed method. We keep the original string so
//! round-trips through `Display` / `Serialize` are byte-identical to
//! what the user typed — important for signed DID-document
//! invariants downstream.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Parsed DID URL. Stores the canonical string form so display +
/// serialization are lossless.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did {
    /// The full DID URL exactly as parsed, e.g. `"did:key:z6Mk..."`.
    raw: String,
    /// Where the colon ends the method (`did:<method>:...`). Cached
    /// so accessors don't re-scan.
    method_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DidMethod<'a> {
    Key,
    Web,
    /// Any method we don't have first-class support for. The slice is
    /// the raw method name as it appeared in the DID string.
    Other(&'a str),
}

impl fmt::Display for DidMethod<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DidMethod::Key => f.write_str("key"),
            DidMethod::Web => f.write_str("web"),
            DidMethod::Other(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DidError {
    #[error("DID must start with `did:`")]
    MissingScheme,
    #[error("DID is missing a method")]
    MissingMethod,
    #[error("DID is missing a method-specific identifier")]
    MissingIdentifier,
    #[error("DID method contains invalid characters")]
    InvalidMethod,
    #[error("did:key: {0}")]
    DidKey(String),
    #[error("did:web: {0}")]
    DidWeb(String),
}

impl Did {
    /// Parse a DID URL.
    ///
    /// Accepts `did:<method>:<method-specific>` with no fragment or
    /// query — those land in a separate `DidUrl` type if/when we need
    /// them (the plan calls for `did:web:...#<vm-fragment>`
    /// addressing). For now the fragment must be stripped by the
    /// caller.
    pub fn parse(s: &str) -> Result<Self, DidError> {
        // Scheme.
        let rest = s.strip_prefix("did:").ok_or(DidError::MissingScheme)?;
        let method_offset = "did:".len();

        // Method.
        let colon = rest.find(':').ok_or(DidError::MissingIdentifier)?;
        if colon == 0 {
            return Err(DidError::MissingMethod);
        }
        let method = &rest[..colon];
        if !is_valid_method_name(method) {
            return Err(DidError::InvalidMethod);
        }
        let identifier = &rest[colon + 1..];
        if identifier.is_empty() {
            return Err(DidError::MissingIdentifier);
        }

        let method_end = method_offset + colon;

        // Method-specific validation. We only validate the encoding
        // shape here; semantic checks (does it decode to a valid
        // pubkey?) happen in the method's own module.
        match method {
            "key" => {
                // The validator lives in did_key; call it to surface
                // structural errors at parse time so callers don't
                // have to.
                crate::did_key::validate_did_key_identifier(identifier)
                    .map_err(DidError::DidKey)?;
            }
            "web" => validate_did_web_identifier(identifier).map_err(DidError::DidWeb)?,
            _ => {} // Unknown method — accept the structure; the
                    // caller is responsible for downstream resolution.
        }

        Ok(Self {
            raw: s.to_string(),
            method_end,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn method(&self) -> DidMethod<'_> {
        let m = &self.raw["did:".len()..self.method_end];
        match m {
            "key" => DidMethod::Key,
            "web" => DidMethod::Web,
            other => DidMethod::Other(other),
        }
    }

    /// The method-specific identifier: everything after the second
    /// colon (`did:<method>:<this>`).
    pub fn identifier(&self) -> &str {
        &self.raw[self.method_end + 1..]
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl FromStr for Did {
    type Err = DidError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Did {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Did {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Did::parse(&s).map_err(serde::de::Error::custom)
    }
}

fn is_valid_method_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn validate_did_web_identifier(s: &str) -> Result<(), String> {
    // Bare minimum: host segment must be a non-empty ASCII string
    // with no `/` (path-style colon-separated segments are how
    // did:web encodes paths).
    let host = s.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err("missing host".into());
    }
    if host.contains('/') {
        return Err("`/` not allowed in did:web (use `:` for path segments)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_did_key() {
        // Valid ed25519 did:key from the spec examples.
        let s = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let d: Did = s.parse().unwrap();
        assert_eq!(d.as_str(), s);
        assert_eq!(d.method(), DidMethod::Key);
        assert!(d.identifier().starts_with("z6Mk"));
    }

    #[test]
    fn parses_did_web() {
        let s = "did:web:hub.example.com:u:alice";
        let d: Did = s.parse().unwrap();
        assert_eq!(d.method(), DidMethod::Web);
        assert_eq!(d.identifier(), "hub.example.com:u:alice");
    }

    #[test]
    fn rejects_missing_scheme() {
        assert!(matches!(
            Did::parse("key:abc"),
            Err(DidError::MissingScheme)
        ));
    }

    #[test]
    fn rejects_missing_identifier() {
        assert!(matches!(
            Did::parse("did:key:"),
            Err(DidError::MissingIdentifier)
        ));
    }

    #[test]
    fn rejects_unknown_method_chars() {
        // Capital letters are not valid in DID method names.
        assert!(matches!(
            Did::parse("did:KEY:abc"),
            Err(DidError::InvalidMethod)
        ));
    }

    #[test]
    fn serde_roundtrip() {
        let s = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let d: Did = s.parse().unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, format!("\"{s}\""));
        let back: Did = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
