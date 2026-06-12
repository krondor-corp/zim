use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BucketStatus {
    Pending,
    Active,
    Ignored,
}

impl BucketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BucketStatus::Pending => "pending",
            BucketStatus::Active => "active",
            BucketStatus::Ignored => "ignored",
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown bucket status: {0}")]
pub struct ParseBucketStatusError(String);

impl std::str::FromStr for BucketStatus {
    type Err = ParseBucketStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(BucketStatus::Pending),
            "active" => Ok(BucketStatus::Active),
            "ignored" => Ok(BucketStatus::Ignored),
            _ => Err(ParseBucketStatusError(s.to_string())),
        }
    }
}

impl std::fmt::Display for BucketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl rusqlite::types::ToSql for BucketStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

impl rusqlite::types::FromSql for BucketStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        s.parse()
            .map_err(|e| rusqlite::types::FromSqlError::Other(Box::new(e)))
    }
}

impl BucketStatus {
    pub fn get(bucket_id: &Uuid, db: &Database) -> crate::database::Result<Option<BucketStatus>> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT status FROM bucket_status WHERE bucket_id = ?1",
                rusqlite::params![bucket_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    pub fn set(
        bucket_id: &Uuid,
        status: BucketStatus,
        shared_by: Option<&str>,
        db: &Database,
    ) -> crate::database::Result<()> {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO bucket_status (bucket_id, status, shared_by, updated_at, created_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
             ON CONFLICT(bucket_id) DO UPDATE SET
                 status = ?2,
                 updated_at = CURRENT_TIMESTAMP",
            rusqlite::params![bucket_id.to_string(), status, shared_by],
        )?;
        Ok(())
    }

    pub fn get_effective(bucket_id: &Uuid, db: &Database) -> crate::database::Result<BucketStatus> {
        Ok(Self::get(bucket_id, db)?.unwrap_or(BucketStatus::Active))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_statuses() {
        assert_eq!(
            "pending".parse::<BucketStatus>().unwrap(),
            BucketStatus::Pending
        );
        assert_eq!(
            "active".parse::<BucketStatus>().unwrap(),
            BucketStatus::Active
        );
        assert_eq!(
            "ignored".parse::<BucketStatus>().unwrap(),
            BucketStatus::Ignored
        );
    }

    #[test]
    fn parse_unknown_status_returns_error() {
        assert!("unknown".parse::<BucketStatus>().is_err());
        assert!("".parse::<BucketStatus>().is_err());
        assert!("ACTIVE".parse::<BucketStatus>().is_err());
    }

    #[test]
    fn display_roundtrip() {
        for status in [
            BucketStatus::Pending,
            BucketStatus::Active,
            BucketStatus::Ignored,
        ] {
            let s = status.to_string();
            let parsed: BucketStatus = s.parse().unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn serde_roundtrip() {
        for status in [
            BucketStatus::Pending,
            BucketStatus::Active,
            BucketStatus::Ignored,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: BucketStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }
}
