use std::time::Duration;

/// Result alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Details of a non-success HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    /// `detail`, `message` or `error` from the JSON body, else `HTTP <status>`.
    pub message: String,
    /// Raw response body.
    pub body: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (HTTP {})", self.message, self.status)
    }
}

/// Errors returned by the SDK.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bad request: {0}")]
    BadRequest(ApiError),
    #[error("authentication failed: {0}")]
    Authentication(ApiError),
    #[error("permission denied: {0}")]
    PermissionDenied(ApiError),
    #[error("not found: {0}")]
    NotFound(ApiError),
    #[error("conflict: {0}")]
    Conflict(ApiError),
    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(ApiError),
    #[error("rate limited: {error}")]
    RateLimit {
        error: ApiError,
        retry_after: Option<Duration>,
    },
    #[error("server error: {0}")]
    Server(ApiError),
    /// Any other non-success status.
    #[error("API error: {0}")]
    Api(ApiError),
    #[error("request timed out: {0}")]
    Timeout(#[source] reqwest::Error),
    #[error("connection error: {0}")]
    Connection(#[source] reqwest::Error),
    /// The response body did not match the expected schema.
    #[error("failed to decode response: {0}")]
    Decode(#[from] serde_json::Error),
    /// Invalid client configuration (e.g. an API key that is not a valid header value).
    #[error("invalid configuration: {0}")]
    Config(String),
}

impl Error {
    /// HTTP status for API errors, `None` for transport/decode errors.
    pub fn status(&self) -> Option<u16> {
        self.api_error().map(|e| e.status)
    }

    /// The [`ApiError`] details, if this error came from an HTTP response.
    pub fn api_error(&self) -> Option<&ApiError> {
        match self {
            Error::BadRequest(e)
            | Error::Authentication(e)
            | Error::PermissionDenied(e)
            | Error::NotFound(e)
            | Error::Conflict(e)
            | Error::UnprocessableEntity(e)
            | Error::RateLimit { error: e, .. }
            | Error::Server(e)
            | Error::Api(e) => Some(e),
            _ => None,
        }
    }

    pub(crate) fn from_response(status: u16, body: String, retry_after: Option<Duration>) -> Self {
        let message = error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
        let e = ApiError {
            status,
            message,
            body,
        };
        match status {
            400 => Error::BadRequest(e),
            401 => Error::Authentication(e),
            403 => Error::PermissionDenied(e),
            404 => Error::NotFound(e),
            409 => Error::Conflict(e),
            422 => Error::UnprocessableEntity(e),
            429 => Error::RateLimit {
                error: e,
                retry_after,
            },
            500.. => Error::Server(e),
            _ => Error::Api(e),
        }
    }

    pub(crate) fn from_transport(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout(e)
        } else {
            Error::Connection(e)
        }
    }
}

fn error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let Some(obj) = value.as_object() else {
        return Some(value.to_string());
    };
    ["detail", "message", "error"]
        .iter()
        .filter_map(|k| obj.get(*k))
        .find(|v| !v.is_null())
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
}
