//! Explicit serde for [`super::Device`] fields whose domain type has no
//! (or a different) default string form. Each module keeps the JSON the
//! all-`String` wire always had.

/// `PublicKey` as lowercase hex. The key's derived serde is a byte
/// structure, not the hex the hub has always sent.
pub mod pubkey_hex {
    use serde::{Deserialize, Deserializer, Serializer};
    use zim_crypto::PublicKey;

    pub fn serialize<S: Serializer>(key: &PublicKey, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&key.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PublicKey, D::Error> {
        let hex = String::deserialize(d)?;
        PublicKey::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

/// `Option<Did>` where `""` means `None` (older hubs send no DID).
pub mod empty_is_none {
    use serde::{Deserialize, Deserializer, Serializer};
    use zim_did::Did;

    pub fn serialize<S: Serializer>(did: &Option<Did>, s: S) -> Result<S::Ok, S::Error> {
        match did {
            Some(did) => s.serialize_str(&did.to_string()),
            None => s.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Did>, D::Error> {
        let raw = String::deserialize(d)?;
        if raw.is_empty() {
            return Ok(None);
        }
        Did::parse(&raw).map(Some).map_err(serde::de::Error::custom)
    }
}

/// `Option<DateTime<Utc>>` as RFC 3339 (`2026-01-02T03:04:05Z`, the
/// hub's SQLite `strftime` format); `""` means `None`.
pub mod timestamp {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(at: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => s.collect_str(&at.format("%Y-%m-%dT%H:%M:%SZ")),
            None => s.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
        let raw = String::deserialize(d)?;
        if raw.is_empty() {
            return Ok(None);
        }
        DateTime::parse_from_rfc3339(&raw)
            .map(|at| Some(at.with_timezone(&Utc)))
            .map_err(serde::de::Error::custom)
    }
}
