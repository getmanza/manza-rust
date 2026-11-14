//! Status-to-error mapping, against a one-shot local server.

use manza::{Client, Error, ErrorKind};

fn error_for(status: u16, body: &str) -> Error {
    error_from(status, body, |client| client.entity().get())
}

fn error_from(
    status: u16,
    body: &str,
    call: impl FnOnce(&Client) -> Result<manza::Response, Error>,
) -> Error {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind");
    let url = format!(
        "http://127.0.0.1:{}",
        server.server_addr().to_ip().unwrap().port()
    );
    let body = body.to_owned();
    let handle = std::thread::spawn(move || {
        let request = server.recv().expect("request");
        let _ = request.respond(tiny_http::Response::from_string(body).with_status_code(status));
    });

    let client = Client::builder()
        .api_key("test")
        .base_url(url)
        .build()
        .expect("build client");
    let err = call(&client).expect_err("expected an API error");
    handle.join().expect("server thread");
    err
}

fn api(err: Error) -> manza::ApiError {
    match err {
        Error::Api(e) => *e,
        other => panic!("expected Error::Api, got {other:?}"),
    }
}

#[test]
fn maps_400_to_validation() {
    let e = api(error_for(
        400,
        r#"{"error":{"message":"bad limit","type":"invalid_request"}}"#,
    ));
    assert_eq!(e.kind, ErrorKind::Validation);
    assert_eq!(e.status, 400);
}

#[test]
fn maps_409_to_conflict_with_payment_id() {
    let e = api(error_for(
        409,
        r#"{"error":{"message":"dup","type":"duplicate_client_reference","payment_id":"pay-1"}}"#,
    ));
    assert_eq!(e.kind, ErrorKind::Conflict);
    assert_eq!(e.kind.as_str(), "conflict");
    assert_eq!(e.payment_id.as_deref(), Some("pay-1"));
}

#[test]
fn payment_id_is_absent_on_other_errors() {
    let e = api(error_for(422, r#"{"error":{"message":"nope"}}"#));
    assert_eq!(e.kind, ErrorKind::Validation);
    assert_eq!(e.payment_id, None);
}

#[test]
fn transfer_authorizer_not_enrolled_is_not_found() {
    let e = api(error_from(
        404,
        r#"{"error":{"message":"No transfer authorizer is enrolled","type":"not_found"}}"#,
        |client| client.transfer_authorizer().get(),
    ));
    assert_eq!(e.kind, ErrorKind::NotFound);
}

// The API's body verbatim (getmanza/app spec/requests/api/invoices_spec.rb).
#[test]
fn maps_422_plan_limit_reached_to_plan_limit_reached_with_kind_current_and_max() {
    let e = api(error_for(
        422,
        r#"{"error":{"type":"plan_limit_reached","message":"Plan limit reached for invoices: 10 of 10","kind":"invoices","current":10,"max":10}}"#,
    ));
    assert_eq!(e.kind, ErrorKind::PlanLimitReached);
    assert_eq!(e.kind.as_str(), "plan_limit_reached");
    assert_eq!(e.status, 422);
    assert_eq!(e.error_type.as_deref(), Some("plan_limit_reached"));
    assert_eq!(e.message, "Plan limit reached for invoices: 10 of 10");
    assert_eq!(e.limit_kind.as_deref(), Some("invoices"));
    assert_eq!(e.limit_current, Some(10));
    assert_eq!(e.limit_max, Some(10));
}

#[test]
fn keeps_any_other_422_a_plain_validation_error() {
    let e = api(error_for(
        422,
        r#"{"error":{"message":"Customer not found","type":"invalid_request_error","param":"customer_id"}}"#,
    ));
    assert_eq!(e.kind, ErrorKind::Validation);
    assert_eq!(e.param.as_deref(), Some("customer_id"));
    assert_eq!(e.limit_kind, None);
    assert_eq!(e.limit_current, None);
    assert_eq!(e.limit_max, None);
}

// Only a 422 is a plan cap: the same type on another status keeps its kind.
#[test]
fn plan_limit_reached_type_on_another_status_keeps_its_kind() {
    let e = api(error_for(
        500,
        r#"{"error":{"type":"plan_limit_reached","message":"boom","kind":"invoices","current":1,"max":1}}"#,
    ));
    assert_eq!(e.kind, ErrorKind::Server);
    assert_eq!(e.limit_kind, None);
    assert_eq!(e.limit_current, None);
    assert_eq!(e.limit_max, None);
}
