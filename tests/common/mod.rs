//! Reads VCR YAML cassettes (recorded by manza-ruby) and serves them from a
//! local `tiny_http` server so identical interactions replay against this
//! SDK. The contract is enforced cross-language: every SDK that consumes the
//! cassette tarball must replay the exact request shape.
//!
//! Matching is method + path+query + semantic JSON body (serde_json maps
//! don't preserve Ruby's insertion order, so byte-equality against the
//! recorded bodies would never hold). The recorded host is ignored.
//! `ReplayServer::start_ignoring_signature` is the one exception: the
//! authorize cassettes compare the body with `signature` removed, because
//! the recorded HMAC cannot be reproduced on replay.

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use base64::Engine;

/// Mirror of spec/support/fixture_ids.rb in manza-ruby. The placeholders must
/// exactly match what VCR scrubbed the real staging UUIDs to when the
/// cassettes were recorded — otherwise the request URI won't match.
const FIXTURE_IDS: &[(&str, &str)] = &[
    ("MANZA_FIXTURE_ACCOUNT_ID", "fixture-account-id"),
    ("MANZA_FIXTURE_TRANSACTION_ID", "fixture-transaction-id"),
    ("MANZA_FIXTURE_CUSTOMER_ID", "fixture-customer-id"),
    (
        "MANZA_FIXTURE_DELETABLE_CUSTOMER_ID",
        "fixture-deletable-customer-id",
    ),
    ("MANZA_FIXTURE_INVOICE_ID", "fixture-invoice-id"),
    (
        "MANZA_FIXTURE_DELETABLE_INVOICE_ID",
        "fixture-deletable-invoice-id",
    ),
    ("MANZA_FIXTURE_PAYMENT_LINK_ID", "fixture-payment-link-id"),
    (
        "MANZA_FIXTURE_CANCELLABLE_PAYMENT_LINK_ID",
        "fixture-cancellable-payment-link-id",
    ),
    ("MANZA_FIXTURE_WEBHOOK_ID", "fixture-webhook-id"),
    (
        "MANZA_FIXTURE_ENABLED_WEBHOOK_ID",
        "fixture-enabled-webhook-id",
    ),
    (
        "MANZA_FIXTURE_DISABLED_WEBHOOK_ID",
        "fixture-disabled-webhook-id",
    ),
    (
        "MANZA_FIXTURE_DELETABLE_WEBHOOK_ID",
        "fixture-deletable-webhook-id",
    ),
    (
        "MANZA_FIXTURE_CHECKOUT_SESSION_ID",
        "fixture-checkout-session-id",
    ),
    ("MANZA_FIXTURE_BENEFICIARY_ID", "fixture-beneficiary-id"),
    (
        "MANZA_FIXTURE_TRANSFER_DRAFT_ID",
        "fixture-transfer-draft-id",
    ),
    // The discovered beneficiary's default bank account, marked a trusted
    // payee once by hand so machine-authorized drafts can target it.
    (
        "MANZA_FIXTURE_TRUSTED_EXTERNAL_ACCOUNT_ID",
        "fixture-trusted-external-account-id",
    ),
    (
        "MANZA_FIXTURE_CREATED_BENEFICIARY_ID",
        "fixture-created-beneficiary-id",
    ),
    (
        "MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID",
        "fixture-external-account-id",
    ),
    // Not an ID, but fresh per seed (bank account numbers are unique per
    // entity) and scrubbed the same way.
    (
        "MANZA_FIXTURE_NEW_ACCOUNT_NUMBER",
        "fixture-new-account-number",
    ),
    (
        "MANZA_FIXTURE_PAYEE_TRUST_REQUEST_ID",
        "fixture-payee-trust-request-id",
    ),
    // client_reference is unique per entity, so these are fresh per seed too.
    ("MANZA_FIXTURE_CLIENT_REFERENCE", "fixture-client-reference"),
    (
        "MANZA_FIXTURE_AUTHORIZABLE_CLIENT_REFERENCE",
        "fixture-authorizable-client-reference",
    ),
    (
        "MANZA_FIXTURE_AUTHORIZABLE_DRAFT_ID",
        "fixture-authorizable-draft-id",
    ),
    (
        "MANZA_FIXTURE_DECLINABLE_DRAFT_ID",
        "fixture-declinable-draft-id",
    ),
    (
        "MANZA_FIXTURE_BAD_SIGNATURE_DRAFT_ID",
        "fixture-bad-signature-draft-id",
    ),
    (
        "MANZA_FIXTURE_AUTHORIZABLE_AUTHORIZATION_ID",
        "fixture-authorizable-authorization-id",
    ),
    (
        "MANZA_FIXTURE_DECLINABLE_AUTHORIZATION_ID",
        "fixture-declinable-authorization-id",
    ),
    (
        "MANZA_FIXTURE_BAD_SIGNATURE_AUTHORIZATION_ID",
        "fixture-bad-signature-authorization-id",
    ),
    // One-time nonce from the authorization webhook. Only needed while
    // recording; it never appears in a request or response.
    (
        "MANZA_FIXTURE_AUTHORIZABLE_NONCE",
        "fixture-authorizable-nonce",
    ),
];

pub fn fixture_id(env_var: &str) -> &'static str {
    FIXTURE_IDS
        .iter()
        .find(|(name, _)| *name == env_var)
        .map(|(_, placeholder)| *placeholder)
        .unwrap_or_else(|| panic!("unknown fixture env var {env_var:?} — add it to FIXTURE_IDS"))
}

struct Interaction {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: String,
    status: u16,
    response_body: String,
}

pub struct ReplayServer {
    pub url: String,
    server: Arc<tiny_http::Server>,
    handle: Option<JoinHandle<()>>,
}

impl ReplayServer {
    /// Loads the named cassettes (e.g. `"payment_links/list"`) and serves
    /// their interactions. Unmatched requests get a 501 whose error message
    /// surfaces in the test failure.
    pub fn start(names: &[&str]) -> ReplayServer {
        Self::start_with(names, false)
    }

    /// Like [`start`](Self::start), but compares request bodies with the
    /// `signature` key removed (the authorize cassettes).
    pub fn start_ignoring_signature(names: &[&str]) -> ReplayServer {
        Self::start_with(names, true)
    }

    fn start_with(names: &[&str], ignore_signature: bool) -> ReplayServer {
        let mut interactions = Vec::new();
        for name in names {
            let path = Path::new("testdata/cassettes").join(format!("{name}.yml"));
            let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "read cassette {}: {e} (run scripts/fetch-cassettes.sh first)",
                    path.display()
                )
            });
            interactions.extend(parse_cassette(&raw, name));
        }

        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").expect("bind replay server"));
        let port = server
            .server_addr()
            .to_ip()
            .expect("replay server ip addr")
            .port();
        let url = format!("http://127.0.0.1:{port}");

        let worker = Arc::clone(&server);
        let handle = std::thread::spawn(move || {
            for mut request in worker.incoming_requests() {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);

                match interactions
                    .iter()
                    .find(|i| matches(i, &request, &body, ignore_signature))
                {
                    Some(interaction) => {
                        let response =
                            tiny_http::Response::from_string(interaction.response_body.clone())
                                .with_status_code(interaction.status)
                                .with_header(
                                    tiny_http::Header::from_bytes(
                                        &b"Content-Type"[..],
                                        &b"application/json; charset=utf-8"[..],
                                    )
                                    .expect("content-type header"),
                                );
                        let _ = request.respond(response);
                    }
                    None => {
                        let message = format!(
                            "no cassette interaction matches {} {} (body {:?})",
                            request.method(),
                            request.url(),
                            body
                        );
                        let payload =
                            serde_json::json!({ "error": { "message": message } }).to_string();
                        let _ = request.respond(
                            tiny_http::Response::from_string(payload).with_status_code(501),
                        );
                    }
                }
            }
        });

        ReplayServer {
            url,
            server,
            handle: Some(handle),
        }
    }
}

impl Drop for ReplayServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn matches(
    interaction: &Interaction,
    request: &tiny_http::Request,
    body: &str,
    ignore_signature: bool,
) -> bool {
    if !interaction
        .method
        .eq_ignore_ascii_case(request.method().as_str())
    {
        return false;
    }

    let (path, query) = split_url(request.url());
    if interaction.path != path {
        return false;
    }
    if interaction.query != parse_query(query) {
        return false;
    }

    if ignore_signature {
        return without_signature(&interaction.body) == without_signature(body);
    }
    json_equal(&interaction.body, body)
}

/// Parses a body as JSON and drops the top-level `signature` key. Bodies
/// that are not JSON objects compare as-is.
fn without_signature(body: &str) -> Result<serde_json::Value, String> {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(mut map)) => {
            map.remove("signature");
            Ok(serde_json::Value::Object(map))
        }
        Ok(other) => Ok(other),
        Err(_) => Err(body.to_owned()),
    }
}

/// Compares two bodies semantically when both parse as JSON, and
/// byte-for-byte otherwise (empty matches empty).
fn json_equal(recorded: &str, actual: &str) -> bool {
    if recorded == actual {
        return true;
    }
    match (
        serde_json::from_str::<serde_json::Value>(recorded),
        serde_json::from_str::<serde_json::Value>(actual),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn parse_cassette(raw: &str, name: &str) -> Vec<Interaction> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(raw).unwrap_or_else(|e| panic!("parse cassette {name}: {e}"));

    doc.get("http_interactions")
        .and_then(serde_yaml::Value::as_sequence)
        .unwrap_or_else(|| panic!("cassette {name}: missing http_interactions"))
        .iter()
        .map(|interaction| {
            let request = &interaction["request"];
            let response = &interaction["response"];

            let uri = request["uri"]
                .as_str()
                .unwrap_or_else(|| panic!("cassette {name}: missing request uri"));
            let (path, query) = split_uri_ignoring_host(uri);

            Interaction {
                method: request["method"].as_str().unwrap_or("").to_owned(),
                path,
                query,
                body: body_string(&request["body"], name),
                status: response["status"]["code"].as_u64().unwrap_or(200) as u16,
                response_body: body_string(&response["body"], name),
            }
        })
        .collect()
}

/// Extracts `body.string`, decoding VCR's `!binary` base64 encoding. Ruby's
/// Psych writes non-UTF-8 bodies as base64 with the PRIMARY `!binary` tag
/// (not the canonical `!!binary`) — serde_yaml surfaces it as a tagged
/// value; decode it ourselves.
fn body_string(body: &serde_yaml::Value, name: &str) -> String {
    match body.get("string") {
        None | Some(serde_yaml::Value::Null) => String::new(),
        Some(serde_yaml::Value::String(s)) => s.clone(),
        Some(serde_yaml::Value::Tagged(tagged)) => {
            let tag = tagged.tag.to_string();
            let value = tagged.value.as_str().unwrap_or("");
            if tag == "!binary" || tag == "!!binary" {
                let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(compact)
                    .unwrap_or_else(|e| panic!("cassette {name}: decode !binary body: {e}"));
                String::from_utf8_lossy(&decoded).into_owned()
            } else {
                panic!("cassette {name}: unexpected body tag {tag}")
            }
        }
        Some(other) => panic!("cassette {name}: unexpected body.string shape {other:?}"),
    }
}

/// Splits a recorded URI into decoded path + sorted query pairs, ignoring
/// scheme and host.
fn split_uri_ignoring_host(uri: &str) -> (String, Vec<(String, String)>) {
    let after_scheme = match uri.find("://") {
        Some(idx) => &uri[idx + 3..],
        None => uri,
    };
    let path_and_query = match after_scheme.find('/') {
        Some(idx) => &after_scheme[idx..],
        None => "/",
    };
    let (path, query) = split_url(path_and_query);
    (path.to_owned(), parse_query(query))
}

fn split_url(url: &str) -> (&str, &str) {
    match url.split_once('?') {
        Some((path, query)) => (path, query),
        None => (url, ""),
    }
}

/// Parses a query string into decoded, sorted key/value pairs so ordering
/// differences never break a match.
fn parse_query(query: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect();
    pairs.sort();
    pairs
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 3 <= bytes.len() => match u8::from_str_radix(&input[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
