use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::sqlite::{SqliteArgumentValue, SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, Sqlite, Type};

/// Which sign-in provider an account belongs to (`users.auth_provider`).
/// Stored as its `ZIM_HUB_AUTH` name (`google`, `dev-trust`).
///
/// Both variants exist in every build — unlike the config-side
/// `AuthProvider`, whose dev-trust variant is behind `dev-login` — so a
/// release hub can still read rows a dev build wrote.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DbAuthProvider {
    Google,
    DevTrust,
}

impl DbAuthProvider {
    pub const ALL: [Self; 2] = [Self::Google, Self::DevTrust];

    /// The `ZIM_HUB_AUTH` / column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::DevTrust => "dev-trust",
        }
    }

    /// How user-facing copy names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::DevTrust => "Dev login",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown auth provider `{0}`")]
pub struct UnknownAuthProvider(pub String);

impl FromStr for DbAuthProvider {
    type Err = UnknownAuthProvider;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| UnknownAuthProvider(s.to_string()))
    }
}

impl fmt::Display for DbAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Decode<'_, Sqlite> for DbAuthProvider {
    fn decode(value: SqliteValueRef<'_>) -> Result<Self, BoxDynError> {
        let s = <&str as Decode<Sqlite>>::decode(value)?;
        Ok(s.parse()?)
    }
}

impl Encode<'_, Sqlite> for DbAuthProvider {
    fn encode_by_ref(
        &self,
        args: &mut Vec<SqliteArgumentValue<'_>>,
    ) -> Result<IsNull, BoxDynError> {
        args.push(SqliteArgumentValue::Text(self.as_str().into()));
        Ok(IsNull::No)
    }
}

impl Type<Sqlite> for DbAuthProvider {
    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <String as Type<Sqlite>>::compatible(ty)
    }

    fn type_info() -> SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_round_trips_through_its_column_value() {
        for p in DbAuthProvider::ALL {
            assert_eq!(p.as_str().parse::<DbAuthProvider>().unwrap(), p);
        }
        assert!("github".parse::<DbAuthProvider>().is_err());
    }
}
