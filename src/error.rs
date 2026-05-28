use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReachError {
    #[error("Path outside allowed territory: {0}")]
    PathNotAllowed(String),

    #[error("Path does not exist: {0}")]
    PathNotFound(String),

    #[error("Read-only mode — write operations are disabled")]
    ReadOnly,

    #[error("IO error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Fetch error for {url}: {message}")]
    Fetch { url: String, message: String },

    #[error("Archive error: {0}")]
    Archive(String),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("{0}")]
    Other(String),
}

impl ReachError {
    pub fn code(&self) -> &'static str {
        match self {
            ReachError::PathNotAllowed(_) => "path_not_allowed",
            ReachError::PathNotFound(_) => "path_not_found",
            ReachError::ReadOnly => "read_only",
            ReachError::Io { .. } => "io_error",
            ReachError::InvalidArgument(_) => "invalid_argument",
            ReachError::Unsupported(_) => "unsupported",
            ReachError::Fetch { .. } => "fetch_error",
            ReachError::Archive(_) => "archive_error",
            ReachError::Regex(_) => "regex_error",
            ReachError::Other(_) => "error",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": false,
            "code": self.code(),
            "error": self.to_string()
        })
    }
}

pub type ReachResult<T> = Result<T, ReachError>;
