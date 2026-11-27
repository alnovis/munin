use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// Input that an agent or a user can fix; the message says what is allowed.
    #[error("invalid {field}: {message}")]
    Invalid {
        field: &'static str,
        message: String,
    },

    #[error("entry {0} not found")]
    NotFound(i64),

    #[error("group {0} not found")]
    GroupNotFound(String),

    /// The request is well formed but contradicts the current state.
    #[error("{0}")]
    Conflict(String),

    #[error("database schema {found} is newer than this build of munin (knows up to {known})")]
    NewerSchema { found: i64, known: i64 },

    #[error("migration to schema {version} failed: {detail}")]
    Migration { version: i64, detail: String },

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub(crate) fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Error::Invalid {
            field,
            message: message.into(),
        }
    }
}
