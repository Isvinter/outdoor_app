use thiserror::Error;
use uuid::Uuid;

/// Errors of the persistence layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("no such record: {0}")]
    NotFound(Uuid),
    #[error(
        "database has schema version {found}, this app supports up to {supported}; \
         it was written by a newer version"
    )]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
}
