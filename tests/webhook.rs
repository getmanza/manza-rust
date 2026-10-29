//! Fixed test vector, shared by every SDK in the family (see manza-ruby's
//! spec/manza/webhook_spec.rb). The signature was computed independently
//! with:
//!
//!   printf '%s' '1767225600.<payload>' | openssl dgst -sha256 -hmac 'whsec_test_vector_secret'

use manza::webhook::{construct_event, construct_event_at, sign, verify, verify_at};
use manza::Error;
use serde_json::json;

const SECRET: &str = "whsec_test_vector_secret";
const TIMESTAMP: &str = "1767225600";
const PAYLOAD: &str = r#"{"event":"test.ping","event_id":"0199a1b2-0000-7000-8000-000000000010"}"#;
const SIGNATURE: &str = "970fdd13680015f73dbd172884498c6fd1e07d5b5c41aae081272598607fea75";
const NOW: i64 = 1_767_225_600;
const TOLERANCE: Option<u64> = Some(manza::webhook::DEFAULT_TOLERANCE);

fn verify_vector(payload: &str, signature: &str, timestamp: &str, secret: &str) -> bool {
    verify_at(payload, signature, timestamp, secret, TOLERANCE, NOW)
}

fn message(result: Result<serde_json::Value, Error>) -> String {
    match result {
        Err(Error::Configuration(message)) => message,
        other => panic!("expected Error::Configuration, got {other:?}"),
    }
}

#[test]
fn sign_reproduces_the_shared_vector() {
    assert_eq!(sign(PAYLOAD, TIMESTAMP, SECRET), SIGNATURE);
    assert_eq!(sign(PAYLOAD.as_bytes(), TIMESTAMP, SECRET), SIGNATURE);
}

#[test]
fn verify_accepts_the_shared_vector() {
    assert!(verify_vector(PAYLOAD, SIGNATURE, TIMESTAMP, SECRET));
}

#[test]
fn verify_rejects_a_wrong_secret() {
    assert!(!verify_vector(PAYLOAD, SIGNATURE, TIMESTAMP, "whsec_other"));
}

#[test]
fn verify_rejects_a_tampered_payload() {
    let tampered = PAYLOAD.replace("ping", "pong");
    assert!(!verify_vector(&tampered, SIGNATURE, TIMESTAMP, SECRET));
}

#[test]
fn verify_rejects_a_signature_of_the_wrong_length() {
    assert!(!verify_vector(PAYLOAD, "abc", TIMESTAMP, SECRET));
    assert!(!verify_vector(PAYLOAD, "", TIMESTAMP, SECRET));
}

#[test]
fn verify_rejects_an_uppercase_signature() {
    assert!(!verify_vector(
        PAYLOAD,
        &SIGNATURE.to_uppercase(),
        TIMESTAMP,
        SECRET
    ));
}

#[test]
fn verify_checks_the_timestamp_within_the_tolerance() {
    let at = |now| verify_at(PAYLOAD, SIGNATURE, TIMESTAMP, SECRET, TOLERANCE, now);

    assert!(!at(NOW + 301), "301 seconds old");
    assert!(!at(NOW - 301), "301 seconds in the future");
    assert!(at(NOW + 300), "exactly at the tolerance");
    assert!(at(NOW - 300), "exactly at the tolerance, ahead");
}

#[test]
fn verify_skips_the_timestamp_check_without_a_tolerance() {
    assert!(verify_at(
        PAYLOAD,
        SIGNATURE,
        TIMESTAMP,
        SECRET,
        None,
        NOW + 86_400
    ));
}

#[test]
fn verify_rejects_a_non_numeric_timestamp() {
    let forged = sign(PAYLOAD, "yesterday", SECRET);
    assert!(!verify_at(PAYLOAD, &forged, "yesterday", SECRET, None, NOW));
    assert!(!verify_at(PAYLOAD, SIGNATURE, "", SECRET, None, NOW));
}

#[test]
fn verify_rejects_an_empty_secret_even_with_a_signature_made_under_it() {
    let forged = sign(PAYLOAD, TIMESTAMP, "");
    assert!(!verify_vector(PAYLOAD, &forged, TIMESTAMP, ""));
}

#[test]
fn verify_uses_the_clock() {
    // The vector's timestamp is long past: only a disabled check accepts it.
    assert!(!verify(PAYLOAD, SIGNATURE, TIMESTAMP, SECRET, TOLERANCE));
    assert!(verify(PAYLOAD, SIGNATURE, TIMESTAMP, SECRET, None));
}

fn construct(headers: &[(&str, &str)]) -> Result<serde_json::Value, Error> {
    construct_event_at(PAYLOAD, headers.iter().copied(), SECRET, TOLERANCE, NOW)
}

#[test]
fn construct_event_returns_the_parsed_event() {
    let event = construct(&[
        ("X-Manza-Signature", SIGNATURE),
        ("X-Manza-Timestamp", TIMESTAMP),
    ])
    .expect("event");
    assert_eq!(
        event,
        json!({ "event": "test.ping", "event_id": "0199a1b2-0000-7000-8000-000000000010" })
    );
}

#[test]
fn construct_event_reads_mixed_case_header_names() {
    let event = construct(&[
        ("x-MANZA-signature", SIGNATURE),
        ("X-manza-Timestamp", TIMESTAMP),
    ])
    .expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_reads_the_cgi_env_form() {
    let event = construct(&[
        ("HTTP_X_MANZA_SIGNATURE", SIGNATURE),
        ("HTTP_X_MANZA_TIMESTAMP", TIMESTAMP),
    ])
    .expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_falls_back_to_x_zazu_headers() {
    let event = construct(&[
        ("X-Zazu-Signature", SIGNATURE),
        ("X-Zazu-Timestamp", TIMESTAMP),
    ])
    .expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_prefers_x_manza_over_x_zazu() {
    let zeros = "0".repeat(64);
    let event = construct(&[
        ("X-Zazu-Signature", &zeros),
        ("X-Zazu-Timestamp", "1"),
        ("X-Manza-Signature", SIGNATURE),
        ("X-Manza-Timestamp", TIMESTAMP),
    ])
    .expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_accepts_owned_header_maps() {
    let headers: std::collections::HashMap<String, String> = [
        ("X-Manza-Signature".to_owned(), SIGNATURE.to_owned()),
        ("X-Manza-Timestamp".to_owned(), TIMESTAMP.to_owned()),
    ]
    .into();
    let event = construct_event_at(PAYLOAD, &headers, SECRET, TOLERANCE, NOW).expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_fails_on_a_missing_signature_header() {
    assert_eq!(
        message(construct(&[("X-Manza-Timestamp", TIMESTAMP)])),
        "missing X-Manza-Signature header"
    );
}

#[test]
fn construct_event_fails_on_a_missing_timestamp_header() {
    assert_eq!(
        message(construct(&[("X-Manza-Signature", SIGNATURE)])),
        "missing X-Manza-Timestamp header"
    );
}

#[test]
fn construct_event_treats_an_empty_header_as_missing() {
    assert_eq!(
        message(construct(&[
            ("X-Manza-Signature", ""),
            ("X-Manza-Timestamp", TIMESTAMP)
        ])),
        "missing X-Manza-Signature header"
    );
}

#[test]
fn construct_event_fails_on_a_bad_signature() {
    let zeros = "0".repeat(64);
    assert_eq!(
        message(construct(&[
            ("X-Manza-Signature", &zeros),
            ("X-Manza-Timestamp", TIMESTAMP)
        ])),
        "invalid webhook signature"
    );
}

#[test]
fn construct_event_fails_on_a_stale_timestamp() {
    let headers = [
        ("X-Manza-Signature", SIGNATURE),
        ("X-Manza-Timestamp", TIMESTAMP),
    ];
    assert_eq!(
        message(construct_event_at(
            PAYLOAD,
            headers,
            SECRET,
            TOLERANCE,
            NOW + 301
        )),
        "webhook timestamp outside tolerance"
    );
}

#[test]
fn construct_event_accepts_a_stale_timestamp_without_a_tolerance() {
    let headers = [
        ("X-Manza-Signature", SIGNATURE),
        ("X-Manza-Timestamp", TIMESTAMP),
    ];
    let event = construct_event_at(PAYLOAD, headers, SECRET, None, NOW + 301).expect("event");
    assert_eq!(event["event"], "test.ping");
}

#[test]
fn construct_event_uses_the_clock() {
    let headers = [
        ("X-Manza-Signature", SIGNATURE),
        ("X-Manza-Timestamp", TIMESTAMP),
    ];
    assert_eq!(
        message(construct_event(PAYLOAD, headers, SECRET, TOLERANCE)),
        "webhook timestamp outside tolerance"
    );
    assert!(construct_event(PAYLOAD, headers, SECRET, None).is_ok());
}

#[test]
fn construct_event_fails_on_a_body_that_is_not_json() {
    let body = "not json";
    let signature = sign(body, TIMESTAMP, SECRET);
    let headers = [
        ("X-Manza-Signature", signature.as_str()),
        ("X-Manza-Timestamp", TIMESTAMP),
    ];
    assert_eq!(
        message(construct_event_at(body, headers, SECRET, TOLERANCE, NOW)),
        "webhook payload is not JSON"
    );
}

#[test]
fn construct_event_fails_on_an_empty_secret() {
    let signature = sign(PAYLOAD, TIMESTAMP, "");
    let headers = [
        ("X-Manza-Signature", signature.as_str()),
        ("X-Manza-Timestamp", TIMESTAMP),
    ];
    assert_eq!(
        message(construct_event_at(PAYLOAD, headers, "", TOLERANCE, NOW)),
        "invalid webhook signature"
    );
}

#[test]
fn construct_event_fails_on_json_that_is_not_an_object() {
    for body in ["[]", "null", "42"] {
        let signature = sign(body, TIMESTAMP, SECRET);
        let headers = [
            ("X-Manza-Signature", signature.as_str()),
            ("X-Manza-Timestamp", TIMESTAMP),
        ];
        assert_eq!(
            message(construct_event_at(body, headers, SECRET, TOLERANCE, NOW)),
            "webhook payload is not a JSON object",
            "body {body}"
        );
    }
}
