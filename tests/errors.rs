//! Status-to-error mapping, against a one-shot local server.

use manza::{Client, Error, ErrorKind};

fn error_for(status: u16, body: &str) -> Error {
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
    let err = client.entity().get().expect_err("expected an API error");
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
