//! Verifies incoming webhook deliveries. Pure functions, no HTTP.
//!
//! Every delivery carries `X-Manza-Signature` (lowercase hex HMAC-SHA256 of
//! `"<timestamp>.<raw body>"` under the endpoint's signing secret) and
//! `X-Manza-Timestamp` (Unix seconds, fresh on every attempt). The legacy
//! `X-Zazu-*` names carry the same values.
//!
//! Pass the **raw request body**, never re-serialized JSON:
//!
//! ```
//! use manza::webhook::{self, construct_event, DEFAULT_TOLERANCE};
//!
//! # let secret = "whsec_signing_secret";
//! # let body = r#"{"event":"test.ping"}"#;
//! # let timestamp = std::time::SystemTime::now()
//! #     .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs().to_string();
//! # let signature = webhook::sign(body, &timestamp, secret);
//! let headers = [
//!     ("X-Manza-Signature", signature.as_str()),
//!     ("X-Manza-Timestamp", timestamp.as_str()),
//! ];
//! let event = construct_event(body, headers, secret, Some(DEFAULT_TOLERANCE))?;
//! assert_eq!(event["event"], "test.ping");
//! # Ok::<(), manza::Error>(())
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::Error;

/// How far, in seconds, a delivery's timestamp may be from now (5 minutes).
pub const DEFAULT_TOLERANCE: u64 = 300;

/// Lowercase hex HMAC-SHA256 of `"<timestamp>.<payload>"`. For tests and
/// local tools.
pub fn sign(payload: impl AsRef<[u8]>, timestamp: &str, secret: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload.as_ref());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `true` when the signature matches (compared in constant time) and the
/// timestamp is within `tolerance` seconds of now. `None` skips the
/// timestamp check: use it only to replay a captured delivery in a local
/// tool or test, never on a live endpoint. Never fails; bad input is
/// `false`.
pub fn verify(
    payload: impl AsRef<[u8]>,
    signature: &str,
    timestamp: &str,
    secret: &str,
    tolerance: Option<u64>,
) -> bool {
    verify_at(payload, signature, timestamp, secret, tolerance, unix_now())
}

/// [`verify`] against a fixed `now` (Unix seconds), for tests.
pub fn verify_at(
    payload: impl AsRef<[u8]>,
    signature: &str,
    timestamp: &str,
    secret: &str,
    tolerance: Option<u64>,
    now: i64,
) -> bool {
    signature_matches(payload.as_ref(), signature, timestamp, secret)
        && timestamp_fresh(timestamp, tolerance, now)
}

/// Reads `X-Manza-Signature` / `X-Manza-Timestamp` from `headers` (names in
/// any case, the CGI `HTTP_X_MANZA_*` form too, falling back to the legacy
/// `X-Zazu-*` names), verifies them like [`verify`], and returns the parsed
/// JSON object.
///
/// Fails with [`Error::Configuration`] when a header is missing, the
/// signature does not match, the timestamp is outside `tolerance`, or the
/// body is not a JSON object.
pub fn construct_event<I, K, V>(
    payload: impl AsRef<[u8]>,
    headers: I,
    secret: &str,
    tolerance: Option<u64>,
) -> Result<serde_json::Value, Error>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    construct_event_at(payload, headers, secret, tolerance, unix_now())
}

/// [`construct_event`] against a fixed `now` (Unix seconds), for tests.
pub fn construct_event_at<I, K, V>(
    payload: impl AsRef<[u8]>,
    headers: I,
    secret: &str,
    tolerance: Option<u64>,
    now: i64,
) -> Result<serde_json::Value, Error>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let payload = payload.as_ref();
    let headers: Vec<(String, String)> = headers
        .into_iter()
        .map(|(name, value)| (normalize(name.as_ref()), value.as_ref().to_owned()))
        .collect();
    let signature = header(&headers, "signature")?;
    let timestamp = header(&headers, "timestamp")?;

    if !signature_matches(payload, signature, timestamp, secret) {
        return Err(invalid("invalid webhook signature"));
    }
    if !timestamp_fresh(timestamp, tolerance, now) {
        return Err(invalid("webhook timestamp outside tolerance"));
    }

    let event: serde_json::Value =
        serde_json::from_slice(payload).map_err(|_| invalid("webhook payload is not JSON"))?;
    if !event.is_object() {
        return Err(invalid("webhook payload is not a JSON object"));
    }
    Ok(event)
}

fn signature_matches(payload: &[u8], signature: &str, timestamp: &str, secret: &str) -> bool {
    if secret.is_empty() || parse_timestamp(timestamp).is_none() {
        return false;
    }

    let expected = sign(payload, timestamp, secret);
    if expected.len() != signature.len() {
        return false;
    }
    // Constant time: every byte is compared, whatever the first mismatch.
    expected
        .bytes()
        .zip(signature.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn timestamp_fresh(timestamp: &str, tolerance: Option<u64>, now: i64) -> bool {
    let Some(tolerance) = tolerance else {
        return true;
    };
    parse_timestamp(timestamp).is_some_and(|seconds| now.abs_diff(seconds) <= tolerance)
}

fn parse_timestamp(timestamp: &str) -> Option<i64> {
    timestamp.parse().ok()
}

/// `HTTP_X_MANZA_SIGNATURE` and `X-Manza-Signature` both become
/// `x-manza-signature`.
fn normalize(name: &str) -> String {
    let name = match name.get(..5) {
        Some(prefix) if prefix.eq_ignore_ascii_case("http_") => &name[5..],
        _ => name,
    };
    name.replace('_', "-").to_ascii_lowercase()
}

/// The `x-manza-<suffix>` header, else `x-zazu-<suffix>`; the last one wins
/// when a name repeats. Empty counts as missing.
fn header<'a>(headers: &'a [(String, String)], suffix: &str) -> Result<&'a str, Error> {
    let find = |prefix: &str| {
        let name = format!("{prefix}{suffix}");
        headers
            .iter()
            .rev()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    };

    match find("x-manza-").or_else(|| find("x-zazu-")) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(invalid(&format!(
            "missing X-Manza-{}{} header",
            suffix[..1].to_ascii_uppercase(),
            &suffix[1..]
        ))),
    }
}

fn invalid(message: &str) -> Error {
    Error::Configuration(message.to_owned())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}
