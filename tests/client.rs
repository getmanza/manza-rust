//! Mirror of zazu-go's client_unit_test.go.

use zazu_sdk::{Client, Error, ListParams, MAX_PER_PAGE};

#[test]
fn new_requires_api_key() {
    std::env::remove_var("ZAZU_API_KEY");

    let err = Client::new().expect_err("expected configuration error without an API key");
    assert!(
        matches!(err, Error::Configuration(_)),
        "expected Error::Configuration, got {err:?}"
    );
}

#[test]
fn list_limit_validation() {
    let client = Client::builder()
        .api_key("test")
        .base_url("http://127.0.0.1:1")
        .build()
        .expect("build client");

    let err = client
        .beneficiaries()
        .list(ListParams {
            limit: Some(MAX_PER_PAGE + 1),
            ..Default::default()
        })
        .expect_err("expected limit validation error");
    assert!(
        matches!(err, Error::Configuration(_)),
        "expected Error::Configuration, got {err:?}"
    );
}

#[test]
fn default_base_url_is_production() {
    std::env::remove_var("ZAZU_BASE_URL");

    let client = Client::builder()
        .api_key("test")
        .build()
        .expect("build client");
    assert!(
        format!("{client:?}").contains("https://ma.manza.finance"),
        "got {client:?}"
    );
}
