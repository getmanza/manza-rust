use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::error::{new_api_error, Error};
use crate::resources::{
    Accounts, Beneficiaries, CheckoutSessions, Customers, Entity, Invoices, PaymentLinks,
    TransferDrafts, WebhookEndpoints,
};

/// The SDK version, sent in the `User-Agent` header.
pub const VERSION: &str = "0.2.1";

const DEFAULT_BASE_URL: &str = "https://zazu.ma";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The SDK entry point. Resources hang off it as accessor methods.
///
/// ```no_run
/// let client = zazu_sdk::Client::builder().api_key("sk_live_...").build()?;
/// let page = client.accounts().list(Default::default())?;
/// # Ok::<(), zazu_sdk::Error>(())
/// ```
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The API key is deliberately omitted.
        f.debug_struct("Client")
            .field("base_url", &self.inner.base_url)
            .field("api_version", &self.inner.api_version)
            .finish_non_exhaustive()
    }
}

struct Inner {
    api_key: String,
    base_url: String,
    api_version: Option<String>,
    agent: ureq::Agent,
}

/// Builds a [`Client`]. Every setting falls back to its environment
/// variable, then to the default.
#[derive(Default)]
pub struct ClientBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    api_version: Option<String>,
    timeout: Option<Duration>,
}

impl ClientBuilder {
    /// Sets the API key (default: the `ZAZU_API_KEY` env var).
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Sets the API base URL (default: `ZAZU_BASE_URL` or `https://zazu.ma`).
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Pins the `Zazu-Version` request header (default: `ZAZU_API_VERSION`).
    pub fn api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = Some(version.into());
        self
    }

    /// Sets the request timeout (default: 30 seconds).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Builds the [`Client`]. An API key is required — call
    /// [`api_key`](Self::api_key) or set `ZAZU_API_KEY`.
    pub fn build(self) -> Result<Client, Error> {
        let api_key = self
            .api_key
            .or_else(|| env_non_empty("ZAZU_API_KEY"))
            .ok_or_else(|| {
                Error::Configuration(
                    "missing API key: pass ClientBuilder::api_key or set ZAZU_API_KEY".to_owned(),
                )
            })?;

        let base_url = self
            .base_url
            .or_else(|| env_non_empty("ZAZU_BASE_URL"))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let base_url = base_url.trim_end_matches('/').to_owned();

        let api_version = self
            .api_version
            .or_else(|| env_non_empty("ZAZU_API_VERSION"));

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout.unwrap_or(DEFAULT_TIMEOUT)))
            .http_status_as_error(false)
            .build()
            .into();

        Ok(Client {
            inner: Arc::new(Inner {
                api_key,
                base_url,
                api_version,
                agent,
            }),
        })
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A successful (2xx) API response.
///
/// The body is returned as-is from the API — `snake_case` keys in an
/// untyped [`serde_json::Value`], no struct mapping. The same shape ships
/// across every Zazu SDK so the cassette contract is one-to-one.
#[derive(Debug, Clone)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// The `X-Request-Id` response header.
    pub request_id: Option<String>,
    /// The parsed JSON body ([`Value::Null`] when empty or non-JSON).
    pub body: Value,
    /// The raw response bytes.
    pub raw: Vec<u8>,
}

pub(crate) enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

impl Client {
    /// Starts building a [`Client`].
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Builds a [`Client`] entirely from environment variables. Shorthand
    /// for `Client::builder().build()`.
    pub fn new() -> Result<Client, Error> {
        Client::builder().build()
    }

    /// Performs an HTTP request against the API. Non-2xx responses are
    /// returned as [`Error::Api`]. `body` (when present) is JSON-encoded.
    pub(crate) fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<Response, Error> {
        let url = self.build_url(path, query);

        let result = match method {
            Method::Get => self.without_body(self.inner.agent.get(&url)).call(),
            Method::Delete => self.without_body(self.inner.agent.delete(&url)).call(),
            Method::Post => self.send(self.inner.agent.post(&url), body),
            Method::Patch => self.send(self.inner.agent.patch(&url), body),
        };

        let mut raw = result.map_err(|e| Error::Connection(e.to_string()))?;

        let status = raw.status().as_u16();
        let request_id = header(&raw, "x-request-id");
        let retry_after = header(&raw, "retry-after").and_then(|v| v.parse().ok());
        let bytes = raw
            .body_mut()
            .read_to_vec()
            .map_err(|e| Error::Connection(format!("read response body: {e}")))?;

        // Non-JSON bodies stay raw; the parsed body stays Null.
        let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        if !(200..300).contains(&status) {
            return Err(new_api_error(status, request_id, retry_after, parsed));
        }

        Ok(Response {
            status,
            request_id,
            body: parsed,
            raw: bytes,
        })
    }

    fn without_body(
        &self,
        req: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    ) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let req = req
            .header("Authorization", format!("Bearer {}", self.inner.api_key))
            .header("User-Agent", format!("zazu-rust/{VERSION}"))
            .header("Accept", "application/json");
        match &self.inner.api_version {
            Some(version) => req.header("Zazu-Version", version),
            None => req,
        }
    }

    fn send(
        &self,
        req: ureq::RequestBuilder<ureq::typestate::WithBody>,
        body: Option<&Value>,
    ) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        let req = req
            .header("Authorization", format!("Bearer {}", self.inner.api_key))
            .header("User-Agent", format!("zazu-rust/{VERSION}"))
            .header("Accept", "application/json");
        let req = match &self.inner.api_version {
            Some(version) => req.header("Zazu-Version", version),
            None => req,
        };
        match body {
            Some(value) => req.send_json(value),
            None => req.send_empty(),
        }
    }

    fn build_url(&self, path: &str, query: &[(String, String)]) -> String {
        let mut url = format!("{}/{}", self.inner.base_url, path.trim_start_matches('/'));
        for (i, (key, value)) in query.iter().enumerate() {
            url.push(if i == 0 { '?' } else { '&' });
            url.push_str(&encode_component(key));
            url.push('=');
            url.push_str(&encode_component(value));
        }
        url
    }

    pub(crate) fn get(&self, path: &str, query: &[(String, String)]) -> Result<Response, Error> {
        self.request(Method::Get, path, query, None)
    }

    pub(crate) fn post(&self, path: &str, body: Option<&Value>) -> Result<Response, Error> {
        self.request(Method::Post, path, &[], body)
    }

    pub(crate) fn patch(&self, path: &str, body: Option<&Value>) -> Result<Response, Error> {
        self.request(Method::Patch, path, &[], body)
    }

    pub(crate) fn delete(&self, path: &str) -> Result<Response, Error> {
        self.request(Method::Delete, path, &[], None)
    }

    /// Accounts and their transactions.
    pub fn accounts(&self) -> Accounts<'_> {
        Accounts { client: self }
    }

    /// Read-only directory of saved transfer recipients.
    pub fn beneficiaries(&self) -> Beneficiaries<'_> {
        Beneficiaries { client: self }
    }

    /// One-off hosted checkout sessions.
    pub fn checkout_sessions(&self) -> CheckoutSessions<'_> {
        CheckoutSessions { client: self }
    }

    /// Individuals or businesses the entity invoices.
    pub fn customers(&self) -> Customers<'_> {
        Customers { client: self }
    }

    /// The current entity (the tenant the API key belongs to).
    pub fn entity(&self) -> Entity<'_> {
        Entity { client: self }
    }

    /// Invoices and their lifecycle actions.
    pub fn invoices(&self) -> Invoices<'_> {
        Invoices { client: self }
    }

    /// Standalone payment links (not attached to an invoice).
    pub fn payment_links(&self) -> PaymentLinks<'_> {
        PaymentLinks { client: self }
    }

    /// API-initiated transfers routed into the in-app approval flow.
    pub fn transfer_drafts(&self) -> TransferDrafts<'_> {
        TransferDrafts { client: self }
    }

    /// Webhook endpoint management.
    pub fn webhook_endpoints(&self) -> WebhookEndpoints<'_> {
        WebhookEndpoints { client: self }
    }
}

fn header(res: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    res.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Percent-encodes a path segment or query component (RFC 3986 unreserved
/// characters pass through).
pub(crate) fn encode_component(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
