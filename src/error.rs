use std::fmt;

/// Classification of an [`ApiError`], derived from the HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 401
    Authentication,
    /// 403
    Forbidden,
    /// 404
    NotFound,
    /// 409
    Conflict,
    /// 400, 422
    Validation,
    /// 429
    RateLimit,
    /// 5xx
    Server,
    /// Any other non-2xx status.
    Api,
}

impl ErrorKind {
    /// The kind as its wire-format string (`authentication`, `forbidden`,
    /// `not_found`, `conflict`, `validation`, `rate_limit`, `server`, `api`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::Authentication => "authentication",
            ErrorKind::Forbidden => "forbidden",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Conflict => "conflict",
            ErrorKind::Validation => "validation",
            ErrorKind::RateLimit => "rate_limit",
            ErrorKind::Server => "server",
            ErrorKind::Api => "api",
        }
    }

    fn from_status(status: u16) -> Self {
        match status {
            401 => ErrorKind::Authentication,
            403 => ErrorKind::Forbidden,
            404 => ErrorKind::NotFound,
            409 => ErrorKind::Conflict,
            400 | 422 => ErrorKind::Validation,
            429 => ErrorKind::RateLimit,
            500.. => ErrorKind::Server,
            _ => ErrorKind::Api,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The API error envelope, mirroring the other Zazu SDKs' hierarchy:
/// `{ "error": { "type": ..., "message": ..., "param": ... } }`. Match on
/// [`ApiError::kind`] instead of subclassing.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// HTTP status code.
    pub status: u16,
    /// Classification derived from the status code.
    pub kind: ErrorKind,
    /// The API's `error.type` field.
    pub error_type: Option<String>,
    /// The API's `error.message` field (falls back to the HTTP status text).
    pub message: String,
    /// The API's `error.param` field.
    pub param: Option<String>,
    /// The API's `error.payment_id` field: the existing transfer draft on a
    /// `duplicate_client_reference` conflict (409).
    pub payment_id: Option<String>,
    /// The `X-Request-Id` response header.
    pub request_id: Option<String>,
    /// Seconds from the `Retry-After` header; only set for `rate_limit`.
    pub retry_after: Option<u64>,
    /// The full parsed response body.
    pub body: serde_json::Value,
}

/// Every error the SDK returns.
#[derive(Debug)]
pub enum Error {
    /// A non-2xx API response.
    Api(Box<ApiError>),
    /// The client could not be built or the call was invalid before any
    /// request was made (missing API key, out-of-range limit, bad URL).
    Configuration(String),
    /// A transport-level failure (timeout, DNS, connection refused).
    Connection(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Api(e) => match &e.param {
                Some(param) => write!(
                    f,
                    "zazu: {} ({} {}, param {})",
                    e.message, e.status, e.kind, param
                ),
                None => write!(f, "zazu: {} ({} {})", e.message, e.status, e.kind),
            },
            Error::Configuration(message) => write!(f, "zazu: {message}"),
            Error::Connection(message) => write!(f, "zazu: connection error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn new_api_error(
    status: u16,
    request_id: Option<String>,
    retry_after: Option<u64>,
    body: serde_json::Value,
) -> Error {
    let kind = ErrorKind::from_status(status);
    let payload = body.get("error");
    let field = |name: &str| {
        payload
            .and_then(|p| p.get(name))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    };

    let message = field("message").unwrap_or_else(|| status_text(status).to_owned());

    Error::Api(Box::new(ApiError {
        status,
        kind,
        error_type: field("type"),
        message,
        param: field("param"),
        payment_id: field("payment_id"),
        request_id,
        retry_after: if kind == ErrorKind::RateLimit {
            retry_after
        } else {
            None
        },
        body,
    }))
}

fn status_text(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "HTTP Error",
    }
}
