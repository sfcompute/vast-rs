use thiserror::Error;

/// All errors that can be produced by this library.
#[derive(Error, Debug)]
pub enum Error {
    /// An error returned by the VAST VMS API (non-2xx HTTP status).
    #[error("VAST API error {status}: {message}")]
    Api { status: u16, message: String },

    /// An HTTP transport-level error (connection refused, timeout, TLS, etc.).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Failed to parse a URL.
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// Failed to (de)serialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Authentication failed or credentials were rejected.
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// The client was misconfigured (e.g., missing address or credentials).
    #[error("Configuration error: {0}")]
    Config(String),
}

/// A convenient alias for `Result<T, vast::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Returns the HTTP status code, if this error originated from the API.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Error::Api { status, .. } => Some(*status),
            Error::Http(e) => e.status().map(|s| s.as_u16()),
            _ => None,
        }
    }

    /// Returns `true` if the error represents a 404 Not Found response.
    pub fn is_not_found(&self) -> bool {
        self.status_code() == Some(404)
    }

    /// Returns `true` if the error is an authentication / authorization failure.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self.status_code(), Some(401) | Some(403))
    }
}
