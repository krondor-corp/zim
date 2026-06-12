use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl MountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MountStatus::Stopped => "stopped",
            MountStatus::Starting => "starting",
            MountStatus::Running => "running",
            MountStatus::Stopping => "stopping",
            MountStatus::Error => "error",
        }
    }
}

impl std::str::FromStr for MountStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "stopped" => MountStatus::Stopped,
            "starting" => MountStatus::Starting,
            "running" => MountStatus::Running,
            "stopping" => MountStatus::Stopping,
            "error" => MountStatus::Error,
            _ => MountStatus::Stopped,
        })
    }
}

impl std::fmt::Display for MountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl rusqlite::types::ToSql for MountStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

impl rusqlite::types::FromSql for MountStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        Ok(s.parse().unwrap())
    }
}
