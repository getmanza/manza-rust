//! Drives the preflight against a local stub of the documented responses
//! (the ones manza-ruby's spec/examples/preflight_spec.rb stubs). No
//! cassette, no network: each failure is a stubbed response.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::test_api::{reply, route, Reply, Route, StubApi};
use super::{format_amount, parse_time, Preflight, SCALE};

const ACCOUNT_ID: &str = "01958e51-7c5a-7000-8000-000000000013";
const BENEFICIARY_ID: &str = "01958e51-7c5a-7000-8000-000000000015";
const ENDPOINT_ID: &str = "01958e51-7c5a-7000-8000-000000000020";

struct Setup {
    env: HashMap<String, String>,
    authorizer: Value,
    endpoint: Value,
    payee: Value,
    /// Routes that replace the default for the same method and path.
    overrides: Vec<Route>,
}

impl Setup {
    fn new() -> Setup {
        let env = [
            ("MANZA_CREATOR_API_KEY", "key-a"),
            ("MANZA_AUTHORIZER_API_KEY", "key-b"),
            ("MANZA_AUTHORIZER_SECRET", "whsec_example"),
            ("MANZA_ACCOUNT_ID", ACCOUNT_ID),
            ("MANZA_BENEFICIARY_ID", BENEFICIARY_ID),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        Setup {
            env,
            authorizer: json!({
                "status": "active", "suspension_reason": null, "currency_code": "MAD",
                "per_transfer_limit": "1000.0", "daily_limit": "5000.0",
                "daily_authorized_total": "100.0", "secret_expires_at": "2027-01-18T00:00:00Z",
                "webhook_endpoint_id": ENDPOINT_ID, "enrolled_at": "2026-10-01T08:30:00Z"
            }),
            endpoint: json!({
                "id": ENDPOINT_ID, "url": "https://authorizer.example.com/", "status": "active"
            }),
            payee: json!({
                "id": "01958e51-7c5a-7000-8000-000000000014", "default": true, "trusted": true
            }),
            overrides: Vec::new(),
        }
    }

    fn set(&mut self, name: &str, value: &str) {
        self.env.insert(name.to_owned(), value.to_owned());
    }

    fn stub(&mut self, path: impl Into<String>, reply: Reply) {
        self.overrides.push(route("GET", path, vec![reply]));
    }

    fn run(self) -> Outcome {
        self.run_with("10.00")
    }

    fn run_with(self, amount: &str) -> Outcome {
        let keyed = |path: String, key, body: Value| Route {
            api_key: Some(key),
            ..route("GET", path, vec![reply(200, body)])
        };
        let defaults = vec![
            keyed(
                "/api/entity".to_owned(),
                "key-a",
                json!({ "id": "e1", "name": "Acme" }),
            ),
            keyed(
                "/api/transfer_authorizer".to_owned(),
                "key-b",
                self.authorizer,
            ),
            keyed(
                format!("/api/accounts/{ACCOUNT_ID}"),
                "key-a",
                json!({ "id": ACCOUNT_ID, "currency_code": "MAD" }),
            ),
            keyed(
                format!("/api/webhook_endpoints/{ENDPOINT_ID}"),
                "key-a",
                self.endpoint,
            ),
            keyed(
                format!("/api/beneficiaries/{BENEFICIARY_ID}"),
                "key-a",
                json!({ "id": BENEFICIARY_ID, "external_accounts": [self.payee] }),
            ),
        ];
        // Overrides first: the stub answers the first matching route.
        let mut routes = self.overrides;
        routes.extend(defaults);
        let api = StubApi::start(routes);

        let mut env = self.env;
        env.insert("MANZA_BASE_URL".to_owned(), api.url.clone());
        let mut out = Vec::new();
        let now = parse_time("2026-10-20T12:00:00Z").unwrap();
        let ok = Preflight::new(&env, amount, &mut out, now).run();

        Outcome {
            ok,
            output: String::from_utf8(out).unwrap(),
            requests: api.requests().len(),
        }
    }
}

struct Outcome {
    ok: bool,
    output: String,
    requests: usize,
}

impl Outcome {
    fn lines(&self, mark: &str) -> Vec<&str> {
        self.output
            .lines()
            .filter(|l| l.starts_with(mark))
            .collect()
    }

    fn failures(&self) -> Vec<&str> {
        self.lines("✗")
    }

    fn first_failure_includes(&self, needles: &[&str]) {
        let first = self
            .failures()
            .first()
            .copied()
            .unwrap_or_else(|| panic!("no failure in:\n{}", self.output));
        for needle in needles {
            assert!(first.contains(needle), "{needle:?} not in {first:?}");
        }
    }
}

fn error(status: u16, error_type: &str) -> Reply {
    reply(
        status,
        json!({ "error": { "type": error_type, "message": "refused" } }),
    )
}

#[test]
fn passes_every_step_when_the_setup_is_complete() {
    let outcome = Setup::new().run();

    assert!(outcome.ok, "{}", outcome.output);
    assert!(outcome.failures().is_empty(), "{}", outcome.output);
    assert_eq!(outcome.lines("✓").len(), 9, "{}", outcome.output);
}

#[test]
fn fails_when_a_key_is_missing() {
    let mut setup = Setup::new();
    setup.set("MANZA_AUTHORIZER_API_KEY", "");
    let outcome = setup.run();

    assert!(!outcome.ok);
    outcome.first_failure_includes(&["MANZA_AUTHORIZER_API_KEY"]);
    assert_eq!(outcome.requests, 0);
}

#[test]
fn fails_when_the_account_or_beneficiary_id_is_missing() {
    let mut setup = Setup::new();
    setup.set("MANZA_BENEFICIARY_ID", "");
    let outcome = setup.run();

    assert!(!outcome.ok);
    outcome.first_failure_includes(&["MANZA_BENEFICIARY_ID"]);
    assert_eq!(outcome.requests, 0);
}

#[test]
fn fails_when_the_amount_is_not_a_decimal_and_runs_the_other_checks() {
    let outcome = Setup::new().run_with("ten");

    assert!(!outcome.ok);
    assert_eq!(outcome.failures().len(), 1, "{}", outcome.output);
    outcome.first_failure_includes(&["\"ten\"", "10.00"]);
    assert!(outcome.output.contains("✓ transfer authorizer is active"));
    assert!(!outcome.output.contains("limit per transfer and"));
}

#[test]
fn fails_when_the_amount_is_not_positive() {
    for amount in ["-5", "0", "1/2", "1e2"] {
        let outcome = Setup::new().run_with(amount);
        outcome.first_failure_includes(&[&format!("{amount:?}"), "positive"]);
    }
}

#[test]
fn stops_before_any_request_on_a_missing_key_even_with_a_bad_amount() {
    let mut setup = Setup::new();
    setup.set("MANZA_CREATOR_API_KEY", "");
    let outcome = setup.run_with("0");

    assert_eq!(outcome.failures().len(), 2, "{}", outcome.output);
    assert_eq!(outcome.requests, 0);
}

#[test]
fn fails_when_both_keys_are_the_same() {
    let mut setup = Setup::new();
    setup.set("MANZA_AUTHORIZER_API_KEY", "key-a");

    setup.run().first_failure_includes(&["different"]);
}

#[test]
fn fails_when_key_a_cannot_reach_the_entity() {
    let mut setup = Setup::new();
    setup.stub("/api/entity", error(401, "authentication_error"));

    setup
        .run()
        .first_failure_includes(&["key A", "MANZA_CREATOR_API_KEY"]);
}

#[test]
fn fails_when_no_authorizer_is_enrolled() {
    let mut setup = Setup::new();
    setup.stub("/api/transfer_authorizer", error(404, "not_found_error"));

    setup
        .run()
        .first_failure_includes(&["no transfer authorizer is enrolled", "Transfer approval"]);
}

#[test]
fn fails_when_key_b_cannot_read_the_authorizer() {
    let mut setup = Setup::new();
    setup.stub("/api/transfer_authorizer", error(403, "insufficient_scope"));

    setup
        .run()
        .first_failure_includes(&["key B", "insufficient_scope", "transfers:authorize"]);
}

#[test]
fn fails_when_key_a_cannot_read_the_account() {
    let mut setup = Setup::new();
    setup.stub(
        format!("/api/accounts/{ACCOUNT_ID}"),
        error(403, "insufficient_scope"),
    );

    setup
        .run()
        .first_failure_includes(&["key A", "accounts:read"]);
}

#[test]
fn fails_when_key_a_cannot_read_the_enrolled_webhook_endpoint() {
    let mut setup = Setup::new();
    setup.stub(
        format!("/api/webhook_endpoints/{ENDPOINT_ID}"),
        error(403, "insufficient_scope"),
    );

    setup
        .run()
        .first_failure_includes(&["key A", "webhook_endpoints:read"]);
}

#[test]
fn fails_when_key_a_cannot_read_the_beneficiary() {
    let mut setup = Setup::new();
    setup.stub(
        format!("/api/beneficiaries/{BENEFICIARY_ID}"),
        error(404, "not_found_error"),
    );

    setup
        .run()
        .first_failure_includes(&["key A", "MANZA_BENEFICIARY_ID"]);
}

#[test]
fn fails_once_on_the_authorizer_line_when_the_secret_has_expired() {
    let mut setup = Setup::new();
    setup.authorizer["status"] = json!("suspended");
    setup.authorizer["suspension_reason"] = json!("secret_expired");
    setup.authorizer["secret_expires_at"] = json!("2026-10-19T00:00:00Z");
    let outcome = setup.run();

    assert_eq!(outcome.failures().len(), 1, "{}", outcome.output);
    outcome.first_failure_includes(&["suspended (secret_expired)", "rotates the secret"]);
}

#[test]
fn fails_when_the_enrolled_webhook_endpoint_was_deleted() {
    let mut setup = Setup::new();
    setup.stub(
        format!("/api/webhook_endpoints/{ENDPOINT_ID}"),
        error(404, "not_found_error"),
    );

    setup
        .run()
        .first_failure_includes(&["was deleted", "enrol"]);
}

#[test]
fn fails_with_the_suspension_reason_and_its_fix() {
    let mut setup = Setup::new();
    setup.authorizer["status"] = json!("suspended");
    setup.authorizer["suspension_reason"] = json!("endpoint_disabled");

    setup
        .run()
        .first_failure_includes(&["suspended (endpoint_disabled)", "re-enable"]);
}

#[test]
fn fails_when_the_accounts_currency_is_not_the_enrolled_one() {
    let mut setup = Setup::new();
    setup.stub(
        format!("/api/accounts/{ACCOUNT_ID}"),
        reply(200, json!({ "id": ACCOUNT_ID, "currency_code": "ZAR" })),
    );

    setup.run().first_failure_includes(&["ZAR", "MAD"]);
}

#[test]
fn fails_when_the_secret_expires_within_7_days() {
    let mut setup = Setup::new();
    setup.authorizer["secret_expires_at"] = json!("2026-10-23T00:00:00Z");

    setup
        .run()
        .first_failure_includes(&["secret expires", "rotate"]);
}

#[test]
fn fails_when_the_signing_secret_is_not_set() {
    let mut setup = Setup::new();
    setup.set("MANZA_AUTHORIZER_SECRET", "");

    setup
        .run()
        .first_failure_includes(&["MANZA_AUTHORIZER_SECRET"]);
}

#[test]
fn fails_when_the_enrolled_webhook_endpoint_is_disabled() {
    let mut setup = Setup::new();
    setup.endpoint["status"] = json!("disabled");

    setup.run().first_failure_includes(&[
        "disabled",
        "enable",
        "if the authorizer stays suspended",
    ]);
}

#[test]
fn warns_when_the_endpoint_is_on_a_url_that_changes_when_the_tunnel_restarts() {
    let mut setup = Setup::new();
    setup.endpoint["url"] = json!("https://abc123.ngrok-free.app/");
    let outcome = setup.run();

    assert!(outcome.ok, "{}", outcome.output);
    let warning = outcome.lines("!")[0];
    assert!(warning.contains("ngrok-free.app"), "{warning}");
    assert!(warning.contains("reserved domain"), "{warning}");
}

#[test]
fn fails_when_the_amount_is_over_the_limit_per_transfer() {
    Setup::new()
        .run_with("1500.00")
        .first_failure_includes(&["limit per transfer"]);
}

#[test]
fn fails_when_the_amount_does_not_fit_in_the_last_24_hours_room() {
    let mut setup = Setup::new();
    setup.authorizer["daily_authorized_total"] = json!("4995.0");

    setup.run().first_failure_includes(&["24 hours"]);
}

#[test]
fn warns_that_a_new_payee_needs_the_toggle_which_the_api_does_not_show() {
    let mut setup = Setup::new();
    setup.payee["trusted"] = json!(false);
    let outcome = setup.run();

    assert!(outcome.ok, "{}", outcome.output);
    let warning = outcome.lines("!")[0];
    for needle in [
        "not trusted yet",
        "Approve transfers to new payees",
        "IP allowlist",
    ] {
        assert!(warning.contains(needle), "{needle:?} not in {warning}");
    }
}

#[test]
fn fails_when_the_beneficiary_has_no_default_bank_account() {
    let mut setup = Setup::new();
    setup.payee["default"] = json!(false);

    setup
        .run()
        .first_failure_includes(&["default bank account"]);
}

#[test]
fn parses_api_times() {
    assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_time("2026-01-01T00:00:00Z"), Some(1_767_225_600));
    assert_eq!(parse_time("2026-01-01T00:00:00.123Z"), Some(1_767_225_600));
    assert_eq!(parse_time("2026-01-01T02:00:00+02:00"), Some(1_767_225_600));
    assert_eq!(parse_time("yesterday"), None);
}

#[test]
fn rejects_malformed_api_times() {
    assert_eq!(parse_time("2026-01-01T00x00x00Z"), None);
    assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
    assert_eq!(parse_time("2026-01-32T00:00:00Z"), None);
    assert_eq!(parse_time("2026-01-01T24:00:00Z"), None);
    assert_eq!(parse_time("2026-01-01T00:60:00Z"), None);
    assert_eq!(parse_time("2026-01-01T00:00:61Z"), None);
    assert_eq!(parse_time("2026-01-01T00:00:00+02x00"), None);
    assert_eq!(parse_time("2026-01-01T00:00:00+24:00"), None);
}

#[test]
fn formats_amounts_without_overflowing() {
    assert_eq!(format_amount(1_005 * SCALE / 1_000), "1.01");
    assert_eq!(format_amount(-1_005 * SCALE / 1_000), "-1.01");
    assert_eq!(format_amount(SCALE / 1_000), "0.00");
    assert_eq!(format_amount(i128::MAX), "170141183460469231731.69");
    assert!(format_amount(i128::MIN).starts_with('-'));
}
