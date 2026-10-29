//! The authorizer process (key B). Answers `payment.authorization_requested`
//! webhooks:
//!
//!   1. Verify the delivery (401 when the signature or timestamp is bad).
//!   2. Find the draft in your own ledger by its `client_reference`.
//!   3. Missing or different from what you created: decline with a reason.
//!   4. A payee not trusted yet (`payee_trusted: false`, only with "Approve
//!      transfers to new payees" on) is trusted by the authorize, unless
//!      `ALLOW_NEW_PAYEES=false` declines it.
//!   5. Otherwise sign the challenge from the ledger and authorize it.
//!
//! It answers 200 first and does steps 2-5 on a background thread: the
//! delivery has a timeout, and a retried webhook is safe because authorize
//! replays idempotently.
//!
//!   cargo run --example transfer_authorizer      # listens on PORT (4599)

#[path = "dotenv.rs"]
pub mod dotenv;
#[path = "ledger.rs"]
pub mod ledger;

#[cfg(test)]
#[path = "test_api.rs"]
pub mod test_api;
#[cfg(test)]
#[path = "authorizer_tests.rs"]
mod tests;

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manza::transfer_authorization::{sign, signature_input};
use manza::{Client, Error, ErrorKind, Response};
use serde_json::Value;

use ledger::{Entry, Ledger};

const EVENT: &str = "payment.authorization_requested";

/// Ledger field and webhook `data.payment` field. All must match.
const CHECKED_FIELDS: [(&str, &str); 5] = [
    ("payment_id", "id"),
    ("amount", "amount"),
    ("currency_code", "currency_code"),
    ("account_id", "account_id"),
    ("payee", "payee"),
];

/// A 409 worth another try: a bank-detail edit, trust or revocation on the
/// same payee was in flight.
const BUSY: &str = "authorization_busy";
const ATTEMPTS: u64 = 3;
/// Seconds; a challenge expires, so never park the thread for long.
const MAX_RETRY_AFTER: u64 = 30;
/// The largest webhook body the server reads (a challenge is well under 4 KiB).
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub enum Level {
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        })
    }
}

pub type Log = Arc<dyn Fn(Level, &str) + Send + Sync>;

pub struct App {
    pub client: Client,
    pub secret: String,
    pub ledger: Ledger,
    pub log: Log,
    /// Answer on a background thread (the server) or inline (the tests).
    pub background: bool,
    /// How long to wait for the ledger entry: the webhook can arrive
    /// before `create_transfer` has written it from the create response.
    pub ledger_wait: Duration,
    /// Base seconds between attempts.
    pub retry_delay: u64,
    /// `false` declines transfers to payees not trusted yet.
    pub allow_new_payees: bool,
}

/// Why a challenge could not be answered.
enum Failure {
    Api(Error),
    Other(String),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Failure {
        Failure::Api(error)
    }
}

impl App {
    /// Handles one delivery and returns the HTTP status to answer with.
    pub fn call(self: &Arc<Self>, body: &[u8], headers: &[(String, String)]) -> u16 {
        let Some(event) = self.verify(body, headers) else {
            return 401;
        };

        let name = event["event"].as_str().unwrap_or_default();
        if name != EVENT {
            self.info(&format!("ignoring {name}: this app only answers {EVENT}"));
            return 200;
        }

        let data = event["data"].clone();
        if self.background {
            let app = Arc::clone(self);
            std::thread::spawn(move || app.answer(&data));
        } else {
            self.answer(&data);
        }
        200
    }

    fn verify(&self, body: &[u8], headers: &[(String, String)]) -> Option<Value> {
        let tolerance = Some(manza::webhook::DEFAULT_TOLERANCE);
        let pairs = headers.iter().map(|(k, v)| (k.as_str(), v.as_str()));
        match manza::webhook::construct_event(body, pairs, &self.secret, tolerance) {
            Ok(event) => {
                self.info(&format!(
                    "received {} ({}), signature verified",
                    text(&event["event"]),
                    text(&event["event_id"])
                ));
                Some(event)
            }
            Err(Error::Configuration(message)) => {
                self.warn(&format!("rejected a delivery: {message}"));
                None
            }
            Err(other) => {
                self.warn(&format!("rejected a delivery: {other}"));
                None
            }
        }
    }

    fn answer(&self, data: &Value) {
        match self.try_answer(data) {
            Ok(()) => {}
            Err(Failure::Api(error)) => self.error(&format!("API error {}", describe(&error))),
            Err(Failure::Other(message)) => {
                self.error(&format!("could not answer the challenge: {message}"))
            }
        }
    }

    fn try_answer(&self, data: &Value) -> Result<(), Failure> {
        let payment = &data["payment"];
        let authorization = &data["authorization"];
        if !payment.is_object() || !authorization.is_object() {
            return Err(Failure::Other(
                "the webhook has no data.payment or data.authorization".to_owned(),
            ));
        }
        let payment_id = text(&payment["id"]);
        let authorization_id = text(&authorization["id"]);
        let client_reference = payment["client_reference"].as_str();
        let entry = self.wait_for_entry(client_reference)?;

        let Some(entry) = entry else {
            let reason = format!(
                "client_reference {} is not in the ledger",
                payment["client_reference"]
            );
            return self.decline(payment_id, authorization_id, &reason);
        };
        if let Some(reason) = mismatch_reason(&entry, payment) {
            return self.decline(payment_id, authorization_id, &reason);
        }

        // Anything but true counts as new, so a webhook without the field is
        // declined under ALLOW_NEW_PAYEES=false rather than trusted blindly.
        let payee = text(&payment["payee"]);
        if payment["payee_trusted"] != Value::Bool(true) {
            if !self.allow_new_payees {
                return self.decline(
                    payment_id,
                    authorization_id,
                    &format!(
                        "new payee {payee}: ALLOW_NEW_PAYEES=false declines payees not trusted yet"
                    ),
                );
            }
            self.info(&format!("new payee {payee}, will be trusted on authorize"));
        }

        self.info(&format!(
            "ledger match for {}: {} {} to {payee}",
            client_reference.unwrap_or_default(),
            text(&payment["amount"]),
            text(&payment["currency_code"])
        ));
        self.authorize(
            &entry,
            authorization,
            client_reference,
            &data["signature_input"],
        )
    }

    fn wait_for_entry(&self, client_reference: Option<&str>) -> Result<Option<Entry>, Failure> {
        let Some(client_reference) = client_reference else {
            return Ok(None);
        };
        let deadline = Instant::now() + self.ledger_wait;
        loop {
            let entry = self
                .ledger
                .find(client_reference)
                .map_err(|e| Failure::Other(format!("could not read the ledger: {e}")))?;
            if entry.is_some() || Instant::now() >= deadline {
                return Ok(entry);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Signs from the ledger, never from the webhook: `signature_input` in
    /// the webhook is only there to compare against.
    fn authorize(
        &self,
        entry: &Entry,
        authorization: &Value,
        client_reference: Option<&str>,
        webhook_input: &Value,
    ) -> Result<(), Failure> {
        let field = |name: &str| entry.get(name).and_then(Value::as_str).unwrap_or_default();
        let payment_id = field("payment_id");
        let input = signature_input(
            payment_id,
            text(&authorization["nonce"]),
            field("amount"),
            field("currency_code"),
            field("account_id"),
            field("payee"),
            client_reference,
        );
        // A signature over a different input is sure to fail, and failed
        // attempts count toward suspending the authorizer. Leave the
        // challenge unanswered instead: it expires and goes to your
        // approvers.
        if webhook_input.as_str() != Some(input.as_str()) {
            self.warn(&format!(
                "signature input differs from the webhook's: ledger {input:?}, webhook {webhook_input}; \
                 not answering, the challenge goes to your approvers when it expires"
            ));
            return Ok(());
        }

        let signature = sign(&self.secret, &input);
        self.info(&format!("signed the challenge for payment {payment_id}"));

        let authorization_id = text(&authorization["id"]);
        let response = self.with_retries(|| {
            self.client
                .payments()
                .authorize(payment_id, authorization_id, &signature)
        })?;
        self.info(&format!(
            "authorized payment {payment_id} (status {})",
            response.body["status"]
        ));
        Ok(())
    }

    fn decline(
        &self,
        payment_id: &str,
        authorization_id: &str,
        reason: &str,
    ) -> Result<(), Failure> {
        self.with_retries(|| {
            self.client
                .payments()
                .decline(payment_id, authorization_id, Some(reason))
        })?;
        self.warn(&format!("declined payment {payment_id}: {reason}"));
        Ok(())
    }

    /// Retries what is worth another try: the API did not answer, asked us
    /// to slow down, or answered 409 `authorization_busy`. Authorize and
    /// decline replay idempotently, so a retry is safe.
    fn with_retries(&self, call: impl Fn() -> Result<Response, Error>) -> Result<Response, Error> {
        let mut attempt = 1;
        loop {
            let error = match call() {
                Ok(response) => return Ok(response),
                Err(error) => error,
            };
            let (transient, retry_after, request_id) = match &error {
                Error::Connection(_) => (true, None, None),
                Error::Api(e) => (
                    matches!(e.kind, ErrorKind::Server | ErrorKind::RateLimit)
                        || (e.kind == ErrorKind::Conflict && e.error_type.as_deref() == Some(BUSY)),
                    e.retry_after,
                    e.request_id.clone(),
                ),
                Error::Configuration(_) => (false, None, None),
            };
            if !transient || attempt >= ATTEMPTS {
                return Err(error);
            }

            let delay = match retry_after {
                Some(seconds) => seconds.min(MAX_RETRY_AFTER),
                None => self.retry_delay * attempt,
            };
            self.warn(&format!(
                "{} (request_id {}), retrying in {delay}s",
                kind(&error),
                inspect(request_id.as_deref())
            ));
            std::thread::sleep(Duration::from_secs(delay));
            attempt += 1;
        }
    }

    fn info(&self, message: &str) {
        (self.log)(Level::Info, message);
    }

    fn warn(&self, message: &str) {
        (self.log)(Level::Warn, message);
    }

    fn error(&self, message: &str) {
        (self.log)(Level::Error, message);
    }
}

fn mismatch_reason(entry: &Entry, payment: &Value) -> Option<String> {
    CHECKED_FIELDS
        .iter()
        .find_map(|(ledger_field, webhook_field)| {
            let recorded = entry.get(*ledger_field).unwrap_or(&Value::Null);
            let received = &payment[*webhook_field];
            (recorded != received).then(|| {
                format!(
                "{ledger_field} differs from the ledger (ledger {recorded}, webhook {received})"
            )
            })
        })
}

/// A JSON string's contents, or `""`.
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn kind(error: &Error) -> String {
    match error {
        Error::Api(e) => e.kind.to_string(),
        Error::Connection(_) => "connection".to_owned(),
        Error::Configuration(_) => "configuration".to_owned(),
    }
}

/// `server (503): message (type "x", request_id "y")`.
fn describe(error: &Error) -> String {
    match error {
        Error::Api(e) => format!(
            "{} ({}): {} (type {}, request_id {})",
            e.kind,
            e.status,
            e.message,
            inspect(e.error_type.as_deref()),
            inspect(e.request_id.as_deref())
        ),
        Error::Connection(message) => format!("connection: {message}"),
        Error::Configuration(message) => format!("configuration: {message}"),
    }
}

fn inspect(value: Option<&str>) -> String {
    value.map_or_else(|| "none".to_owned(), |v| format!("{v:?}"))
}

/// The request body, or `None` when it is over [`MAX_BODY_BYTES`]. A read
/// error yields what was read, which then fails verification.
fn read_body(reader: impl std::io::Read) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut body = Vec::new();
    let _ = reader
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body);
    (body.len() <= MAX_BODY_BYTES).then_some(body)
}

fn main() {
    dotenv::load();
    let missing: Vec<&str> = ["MANZA_AUTHORIZER_API_KEY", "MANZA_AUTHORIZER_SECRET"]
        .into_iter()
        .filter(|name| dotenv::var(name).is_empty())
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "Set {} in {}",
            missing.join(" and "),
            dotenv::env_path().display()
        );
        std::process::exit(1);
    }

    let client = Client::builder()
        .api_key(dotenv::var("MANZA_AUTHORIZER_API_KEY"))
        .build()
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
    let allow_new_payees = !["false", "0", "no"].contains(
        &dotenv::var("ALLOW_NEW_PAYEES")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
    );
    let app = Arc::new(App {
        client,
        secret: dotenv::var("MANZA_AUTHORIZER_SECRET"),
        ledger: Ledger::new(dotenv::dir().join("ledger.json")),
        log: Arc::new(|level, message| println!("{level} {message}")),
        background: true,
        ledger_wait: Duration::from_secs(10),
        retry_delay: 2,
        allow_new_payees,
    });

    let port = dotenv::var("PORT");
    let port = if port.is_empty() {
        "4599".to_owned()
    } else {
        port
    };
    let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).unwrap_or_else(|e| {
        eprintln!("could not listen on 127.0.0.1:{port}: {e}");
        std::process::exit(1);
    });
    println!("listening on http://127.0.0.1:{port} (point your tunnel here)");

    for mut request in server.incoming_requests() {
        // Read before verifying, so cap it: an oversized body gets 413 and
        // calls nothing. An unreadable one fails verification: 401.
        let Some(body) = read_body(request.as_reader()) else {
            let _ = request.respond(
                tiny_http::Response::from_string("payload too large").with_status_code(413),
            );
            continue;
        };
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|h| (h.field.to_string(), h.value.to_string()))
            .collect();
        let status = app.call(&body, &headers);
        let text = if status == 200 { "ok" } else { "unauthorized" };
        let _ = request.respond(tiny_http::Response::from_string(text).with_status_code(status));
    }
}
