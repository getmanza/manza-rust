//! Mirror of manza-go's client_unit_test.go.

use std::sync::Mutex;

use manza::{Client, Error, ListParams, MAX_PER_PAGE};

#[test]
fn new_requires_api_key() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();

    let err = Client::new().expect_err("expected configuration error without an API key");
    assert!(
        matches!(err, Error::Configuration(_)),
        "expected Error::Configuration, got {err:?}"
    );
}

#[test]
fn list_limit_validation() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();

    let client = Client::builder()
        .api_key("test")
        .build()
        .expect("build client");
    assert!(
        format!("{client:?}").contains("https://ma.manza.finance"),
        "got {client:?}"
    );
}

/// Serializes tests that touch the process environment.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const ENV_VARS: [&str; 6] = [
    "MANZA_API_KEY",
    "ZAZU_API_KEY",
    "MANZA_BASE_URL",
    "ZAZU_BASE_URL",
    "MANZA_API_VERSION",
    "ZAZU_API_VERSION",
];

fn clear_env() {
    for name in ENV_VARS {
        std::env::remove_var(name);
    }
}

/// Serves one request and returns its (user-agent, manza-version) headers.
fn capture_headers(client: impl FnOnce(String)) -> (Option<String>, Option<String>) {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind");
    let url = format!(
        "http://127.0.0.1:{}",
        server.server_addr().to_ip().unwrap().port()
    );
    let handle = std::thread::spawn(move || {
        let request = server.recv().expect("request");
        let header = |name: &'static str| {
            request
                .headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|h| h.value.to_string())
        };
        let seen = (header("User-Agent"), header("Manza-Version"));
        let _ = request.respond(tiny_http::Response::from_string("{}"));
        seen
    });
    client(url);
    handle.join().expect("server thread")
}

#[test]
fn reads_manza_env_vars() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    std::env::set_var("MANZA_API_KEY", "manza-key");
    std::env::set_var("MANZA_BASE_URL", "http://example.test/");
    std::env::set_var("MANZA_API_VERSION", "2026-01-01");

    let client = Client::new().expect("build client");
    let debug = format!("{client:?}");
    assert!(debug.contains("http://example.test"), "got {debug}");
    assert!(debug.contains("2026-01-01"), "got {debug}");
    clear_env();
}

#[test]
fn falls_back_to_deprecated_zazu_env_vars() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    std::env::set_var("ZAZU_API_KEY", "zazu-key");
    std::env::set_var("ZAZU_BASE_URL", "http://legacy.test");
    std::env::set_var("ZAZU_API_VERSION", "2025-01-01");

    let client = Client::new().expect("ZAZU_API_KEY still configures the client");
    let debug = format!("{client:?}");
    assert!(debug.contains("http://legacy.test"), "got {debug}");
    assert!(debug.contains("2025-01-01"), "got {debug}");
    clear_env();
}

#[test]
fn manza_env_vars_win_over_zazu() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    std::env::set_var("MANZA_API_KEY", "new");
    std::env::set_var("ZAZU_API_KEY", "old");
    std::env::set_var("MANZA_BASE_URL", "http://new.test");
    std::env::set_var("ZAZU_BASE_URL", "http://old.test");

    let debug = format!("{:?}", Client::new().expect("build client"));
    assert!(debug.contains("http://new.test"), "got {debug}");
    clear_env();
}

#[test]
fn sends_manza_version_header_and_user_agent() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();

    let (user_agent, version) = capture_headers(|url| {
        let client = Client::builder()
            .api_key("test")
            .base_url(url)
            .api_version("2026-01-01")
            .build()
            .expect("build client");
        client.entity().get().expect("request");
    });
    assert_eq!(user_agent, Some(format!("manza-rust/{}", manza::VERSION)));
    assert_eq!(version.as_deref(), Some("2026-01-01"));
}
