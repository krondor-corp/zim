#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("{0}")]
    Client(#[from] rusqlite::Error),
    #[error("{0}")]
    Deserialize(#[from] anyhow::Error),
}

pub(crate) type Result<T> = std::result::Result<T, DatabaseError>;
