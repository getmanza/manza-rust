//! Drives the authorizer with a signed copy of the documented
//! `payment.authorization_requested` payload (the same one manza-ruby's
//! spec/examples/transfer_authorizer_spec.rb uses), against a local stub
//! of the API.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::ledger::Ledger;
use super::test_api::{reply, route, temp_dir, Reply, StubApi, TempDir};
use super::App;

const SECRET: &str = "whsec_example_authorizer_secret";
const DRAFT_ID: &str = "01958e51-7c5a-7000-8000-000000000012";
const AUTHORIZATION_ID: &str = "01958e51-7c5a-7000-8000-000000000011";
const NONCE: &str = "9f86d081884c7d659a2feaa0c55ad015";
const SIGNATURE_INPUT: &str = "manza.transfer-authorization.v1|01958e51-7c5a-7000-8000-000000000012|\
                               9f86d081884c7d659a2feaa0c55ad015|2500.0|ZAR|01958e51-7c5a-7000-8000-000000000013|\
                               ext:01958e51-7c5a-7000-8000-000000000014|po_8812";

fn example_payload() -> Value {
    json!({
        "event": "payment.authorization_requested",
        "event_id": "01958e51-7c5a-7000-8000-000000000010",
        "created_at": "2026-10-01T12:00:00Z",
        "data": {
            "authorization": {
                "id": AUTHORIZATION_ID,
                "nonce": NONCE,
                "expires_at": "2026-10-01T13:00:00Z",
                "signature_version": "manza.transfer-authorization.v1"
            },
            "payment": {
                "id": DRAFT_ID,
                "amount": "2500.0",
                "currency_code": "ZAR",
                "account_id": "01958e51-7c5a-7000-8000-000000000013",
                "payee": "ext:01958e51-7c5a-7000-8000-000000000014",
                "beneficiary_id": "01958e51-7c5a-7000-8000-000000000015",
                "external_account_id": "01958e51-7c5a-7000-8000-000000000014",
                "destination_account_id": null,
                "payment_reference": "INV-2026-001",
                "client_reference": "po_8812",
                "payee_trusted": false
            },
            "signature_input": SIGNATURE_INPUT
        }
    })
}

struct Harness {
    app: Arc<App>,
    api: StubApi,
    log: Arc<Mutex<String>>,
    _dir: TempDir,
}

impl Harness {
    fn new(routes: Vec<super::test_api::Route>, ledger_amount: Option<&str>) -> Harness {
        Self::with(routes, ledger_amount, true)
    }

    fn with(
        routes: Vec<super::test_api::Route>,
        ledger_amount: Option<&str>,
        allow_new_payees: bool,
    ) -> Harness {
        let api = StubApi::start(routes);
        let dir = temp_dir();
        let ledger = Ledger::new(dir.0.join("ledger.json"));
        if let Some(amount) = ledger_amount {
            ledger
                .record(
                    "po_8812",
                    DRAFT_ID,
                    amount,
                    "ZAR",
                    "01958e51-7c5a-7000-8000-000000000013",
                    "ext:01958e51-7c5a-7000-8000-000000000014",
                )
                .expect("record");
        }
        let log = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&log);
        let client = manza::Client::builder()
            .api_key("key-b")
            .base_url(&api.url)
            .build()
            .expect("client");

        let app = App {
            client,
            secret: SECRET.to_owned(),
            ledger,
            log: Arc::new(move |level, message| {
                sink.lock()
                    .unwrap()
                    .push_str(&format!("{level} {message}\n"));
            }),
            background: false,
            ledger_wait: Duration::ZERO,
            retry_delay: 0,
            allow_new_payees,
        };
        Harness {
            app: Arc::new(app),
            api,
            log,
            _dir: dir,
        }
    }

    fn deliver(&self, body: &str) -> u16 {
        let timestamp = now().to_string();
        let signature = manza::webhook::sign(body, &timestamp, SECRET);
        self.deliver_signed(body, &signature, &timestamp)
    }

    fn deliver_signed(&self, body: &str, signature: &str, timestamp: &str) -> u16 {
        let headers = vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("X-Manza-Signature".to_owned(), signature.to_owned()),
            ("X-Manza-Timestamp".to_owned(), timestamp.to_owned()),
        ];
        self.app.call(body.as_bytes(), &headers)
    }

    fn log(&self) -> String {
        self.log.lock().unwrap().clone()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn authorize_path() -> String {
    format!("/api/payments/{DRAFT_ID}/authorize")
}

fn decline_path() -> String {
    format!("/api/payments/{DRAFT_ID}/decline")
}

fn processing() -> Reply {
    reply(200, json!({ "id": DRAFT_ID, "status": "processing" }))
}

fn error(status: u16, error_type: &str) -> Reply {
    reply(
        status,
        json!({ "error": { "message": "refused", "type": error_type } }),
    )
}

fn expected_signature() -> String {
    let input = manza::transfer_authorization::signature_input(
        DRAFT_ID,
        NONCE,
        "2500.0",
        "ZAR",
        "01958e51-7c5a-7000-8000-000000000013",
        "ext:01958e51-7c5a-7000-8000-000000000014",
        Some("po_8812"),
    );
    manza::transfer_authorization::sign(SECRET, &input)
}

fn with_payment(change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> String {
    let mut payload = example_payload();
    change(payload["data"]["payment"].as_object_mut().unwrap());
    payload.to_string()
}

fn assert_includes(haystack: &str, needles: &[&str]) {
    for needle in needles {
        assert!(haystack.contains(needle), "{needle:?} not in:\n{haystack}");
    }
}

#[test]
fn signs_from_the_ledger_and_authorizes_with_key_b() {
    let h = Harness::new(
        vec![route("POST", authorize_path(), vec![processing()])],
        Some("2500.0"),
    );

    assert_eq!(h.deliver(&example_payload().to_string()), 200);

    let calls = h.api.requests_to("/authorize");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].authorization.as_deref(), Some("Bearer key-b"));
    assert_eq!(
        calls[0].body,
        json!({ "authorization_id": AUTHORIZATION_ID, "signature": expected_signature() })
    );
    assert_includes(
        &h.log(),
        &["verified", "ledger match", "signed", "authorized"],
    );
}

#[test]
fn logs_the_api_error_type_and_request_id_when_authorize_fails() {
    let mut refused = error(422, "invalid_signature");
    refused.request_id = Some("req_123");
    let h = Harness::new(
        vec![route("POST", authorize_path(), vec![refused])],
        Some("2500.0"),
    );

    assert_eq!(h.deliver(&example_payload().to_string()), 200);
    assert_includes(&h.log(), &["invalid_signature", "req_123"]);
}

#[test]
fn does_not_authorize_when_the_webhooks_signature_input_differs() {
    let h = Harness::new(vec![], Some("2500.0"));
    let mut payload = example_payload();
    payload["data"]["signature_input"] = json!("something-else");

    h.deliver(&payload.to_string());

    assert!(h.api.requests().is_empty());
    assert_includes(&h.log(), &["something-else", SIGNATURE_INPUT]);
}

#[test]
fn retries_authorize_after_a_transient_failure() {
    let h = Harness::new(
        vec![route(
            "POST",
            authorize_path(),
            vec![reply(503, json!({})), processing()],
        )],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_eq!(h.api.requests_to("/authorize").len(), 2);
    assert_includes(&h.log(), &["retrying", "authorized"]);
}

#[test]
fn gives_up_after_three_attempts() {
    let h = Harness::new(
        vec![route("POST", authorize_path(), vec![reply(503, json!({}))])],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_eq!(h.api.requests_to("/authorize").len(), 3);
    assert_includes(&h.log(), &["API error server"]);
}

#[test]
fn retries_authorize_while_the_api_answers_authorization_busy() {
    let h = Harness::new(
        vec![route(
            "POST",
            authorize_path(),
            vec![error(409, "authorization_busy"), processing()],
        )],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_eq!(h.api.requests_to("/authorize").len(), 2);
    assert_includes(&h.log(), &["retrying", "authorized"]);
}

#[test]
fn does_not_retry_another_conflict() {
    let h = Harness::new(
        vec![route(
            "POST",
            authorize_path(),
            vec![error(409, "already_answered")],
        )],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_eq!(h.api.requests_to("/authorize").len(), 1);
    assert_includes(&h.log(), &["API error conflict"]);
}

#[test]
fn does_not_retry_a_validation_error() {
    let h = Harness::new(
        vec![route(
            "POST",
            authorize_path(),
            vec![error(422, "invalid_signature")],
        )],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_eq!(h.api.requests_to("/authorize").len(), 1);
}

#[test]
fn logs_that_a_new_payee_will_be_trusted_on_authorize() {
    let h = Harness::new(
        vec![route("POST", authorize_path(), vec![processing()])],
        Some("2500.0"),
    );

    h.deliver(&example_payload().to_string());

    assert_includes(
        &h.log(),
        &["new payee ext:01958e51-7c5a-7000-8000-000000000014, will be trusted on authorize"],
    );
}

#[test]
fn does_not_log_a_new_payee_when_the_payee_is_already_trusted() {
    let h = Harness::new(
        vec![route("POST", authorize_path(), vec![processing()])],
        Some("2500.0"),
    );

    h.deliver(&with_payment(|p| {
        p.insert("payee_trusted".to_owned(), json!(true));
    }));

    assert!(!h.log().contains("new payee"), "{}", h.log());
    assert_eq!(h.api.requests_to("/authorize").len(), 1);
}

#[test]
fn without_new_payees_declines_a_webhook_that_does_not_say_whether_the_payee_is_trusted() {
    let h = Harness::with(
        vec![route("POST", decline_path(), vec![reply(200, json!({}))])],
        Some("2500.0"),
        false,
    );

    h.deliver(&with_payment(|p| {
        p.remove("payee_trusted");
    }));

    let calls = h.api.requests_to("/decline");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].body["authorization_id"], AUTHORIZATION_ID);
    assert_includes(calls[0].body["reason"].as_str().unwrap(), &["new payee"]);
}

#[test]
fn without_new_payees_declines_a_payee_that_is_not_trusted_yet() {
    let h = Harness::with(
        vec![route(
            "POST",
            decline_path(),
            vec![reply(
                200,
                json!({ "id": AUTHORIZATION_ID, "status": "declined" }),
            )],
        )],
        Some("2500.0"),
        false,
    );

    h.deliver(&example_payload().to_string());

    let calls = h.api.requests_to("/decline");
    assert_eq!(calls.len(), 1);
    assert_includes(calls[0].body["reason"].as_str().unwrap(), &["new payee"]);
    assert!(h.api.requests_to("/authorize").is_empty());
    assert_includes(&h.log(), &["declined", "new payee"]);
}

#[test]
fn without_new_payees_authorizes_a_trusted_payee() {
    let h = Harness::with(
        vec![route("POST", authorize_path(), vec![processing()])],
        Some("2500.0"),
        false,
    );

    h.deliver(&with_payment(|p| {
        p.insert("payee_trusted".to_owned(), json!(true));
    }));

    assert_eq!(h.api.requests_to("/authorize").len(), 1);
}

#[test]
fn declines_a_ledger_mismatch_with_a_reason_naming_the_field() {
    let h = Harness::new(
        vec![route(
            "POST",
            decline_path(),
            vec![reply(
                200,
                json!({ "id": AUTHORIZATION_ID, "status": "declined" }),
            )],
        )],
        Some("25000.0"),
    );

    assert_eq!(h.deliver(&example_payload().to_string()), 200);

    let calls = h.api.requests_to("/decline");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].body["authorization_id"], AUTHORIZATION_ID);
    assert_includes(calls[0].body["reason"].as_str().unwrap(), &["amount"]);
    assert!(h.api.requests_to("/authorize").is_empty());
    assert_includes(&h.log(), &["declined"]);
}

#[test]
fn declines_a_client_reference_this_authorizer_never_issued() {
    let h = Harness::new(
        vec![route("POST", decline_path(), vec![reply(200, json!({}))])],
        None,
    );

    h.deliver(&example_payload().to_string());

    let calls = h.api.requests_to("/decline");
    assert_eq!(calls.len(), 1);
    assert_includes(calls[0].body["reason"].as_str().unwrap(), &["po_8812"]);
}

#[test]
fn answers_401_and_calls_nothing_on_a_bad_signature() {
    let h = Harness::new(vec![], Some("2500.0"));

    let status = h.deliver_signed(
        &example_payload().to_string(),
        &"0".repeat(64),
        &now().to_string(),
    );

    assert_eq!(status, 401);
    assert!(h.api.requests().is_empty());
    assert_includes(
        &h.log(),
        &["rejected a delivery: invalid webhook signature"],
    );
}

#[test]
fn answers_401_and_calls_nothing_on_a_signed_body_that_is_not_a_json_object() {
    let h = Harness::new(vec![], Some("2500.0"));

    assert_eq!(h.deliver("[]"), 401);
    assert!(h.api.requests().is_empty());
}

#[test]
fn answers_200_and_calls_nothing_on_another_event() {
    let h = Harness::new(vec![], Some("2500.0"));

    let status = h.deliver(
        &json!({ "event": "test.ping", "event_id": "0199a1b2-0000-7000-8000-000000000010" })
            .to_string(),
    );

    assert_eq!(status, 200);
    assert!(h.api.requests().is_empty());
    assert_includes(&h.log(), &["ignoring test.ping"]);
}

#[test]
fn reads_a_body_up_to_the_cap() {
    let body = vec![b'x'; super::MAX_BODY_BYTES];
    assert_eq!(super::read_body(body.as_slice()), Some(body));
}

#[test]
fn refuses_a_body_over_the_cap() {
    let body = vec![b'x'; super::MAX_BODY_BYTES + 1];
    assert_eq!(super::read_body(body.as_slice()), None);
}
