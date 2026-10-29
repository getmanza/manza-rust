//! Mirror of manza-ruby's spec/manza/resources/*_spec.rb — same cassettes,
//! same assertions, per the cross-language SDK contract.

mod common;

use common::{fixture_id, ReplayServer};
use manza::transfer_authorization::{payee_for, sign, signature_input};
use manza::{Client, Error, ErrorKind};
use serde_json::json;

fn replay_client(server: &ReplayServer) -> Client {
    Client::builder()
        .api_key("test-api-key-for-replay")
        .base_url(&server.url)
        .build()
        .expect("build client")
}

#[test]
fn entity_get() {
    let server = ReplayServer::start(&["entity/get"]);
    let client = replay_client(&server);

    let resp = client.entity().get().expect("entity get");
    assert!(
        resp.body["id"].is_string(),
        "expected string id, got {:?}",
        resp.body["id"]
    );
}

#[test]
fn accounts() {
    let server = ReplayServer::start(&[
        "accounts/list",
        "accounts/get",
        "accounts/list_transactions",
        "accounts/get_transaction",
    ]);
    let client = replay_client(&server);

    let page = client
        .accounts()
        .list(Default::default())
        .expect("accounts list");
    assert!(!page.data.is_empty(), "expected data rows");

    let account_id = fixture_id("MANZA_FIXTURE_ACCOUNT_ID");
    client.accounts().get(account_id).expect("accounts get");

    client
        .accounts()
        .list_transactions(account_id, Default::default())
        .expect("list transactions");

    let tx_id = fixture_id("MANZA_FIXTURE_TRANSACTION_ID");
    client
        .accounts()
        .get_transaction(account_id, tx_id)
        .expect("get transaction");
}

#[test]
fn customers() {
    let server = ReplayServer::start(&["customers/list", "customers/get"]);
    let client = replay_client(&server);

    client
        .customers()
        .list(Default::default())
        .expect("customers list");

    let customer_id = fixture_id("MANZA_FIXTURE_CUSTOMER_ID");
    let resp = client.customers().get(customer_id).expect("customers get");
    assert!(
        resp.body["id"].is_string(),
        "expected string id, got {:?}",
        resp.body["id"]
    );
}

#[test]
fn invoices() {
    let server = ReplayServer::start(&["invoices/list", "invoices/get"]);
    let client = replay_client(&server);

    let page = client
        .invoices()
        .list(Default::default())
        .expect("invoices list");
    assert!(!page.data.is_empty(), "expected data rows");

    let invoice_id = fixture_id("MANZA_FIXTURE_INVOICE_ID");
    client.invoices().get(invoice_id).expect("invoices get");
}

#[test]
fn payment_links() {
    let server = ReplayServer::start(&[
        "payment_links/list",
        "payment_links/get",
        "payment_links/create",
        "payment_links/cancel",
    ]);
    let client = replay_client(&server);

    client
        .payment_links()
        .list(Default::default())
        .expect("payment links list");

    let resp = client
        .payment_links()
        .create(&json!({
            "account_id": fixture_id("MANZA_FIXTURE_ACCOUNT_ID"),
            "amount": "100.00",
            "title": "SDK fixture",
            "description": "Created by zazu-ruby fixture spec",
            "link_type": "single",
        }))
        .expect("payment links create");
    assert_eq!(resp.status, 201, "expected 201, got {}", resp.status);

    client
        .payment_links()
        .cancel(fixture_id("MANZA_FIXTURE_CANCELLABLE_PAYMENT_LINK_ID"))
        .expect("payment links cancel");
}

#[test]
fn checkout_sessions() {
    let server = ReplayServer::start(&["checkout_sessions/get"]);
    let client = replay_client(&server);

    let resp = client
        .checkout_sessions()
        .get(fixture_id("MANZA_FIXTURE_CHECKOUT_SESSION_ID"))
        .expect("checkout sessions get");
    assert!(
        resp.body["id"].is_string(),
        "expected string id, got {:?}",
        resp.body["id"]
    );
}

#[test]
fn webhook_endpoints() {
    let server = ReplayServer::start(&["webhook_endpoints/list", "webhook_endpoints/get"]);
    let client = replay_client(&server);

    client
        .webhook_endpoints()
        .list(Default::default())
        .expect("webhook endpoints list");
    client
        .webhook_endpoints()
        .get(fixture_id("MANZA_FIXTURE_WEBHOOK_ID"))
        .expect("webhook endpoints get");
}

#[test]
fn payments_create_and_get() {
    let server = ReplayServer::start(&["payments/create", "payments/get"]);
    let client = replay_client(&server);

    let resp = client
        .payments()
        .create(&json!({
            "account_id": fixture_id("MANZA_FIXTURE_ACCOUNT_ID"),
            "beneficiary_id": fixture_id("MANZA_FIXTURE_BENEFICIARY_ID"),
            "amount": "10.00",
            "payment_reference": "SDK fixture",
            "internal_notes": "[zazu-ruby-fixture] create spec - do not approve",
            "client_reference": fixture_id("MANZA_FIXTURE_CLIENT_REFERENCE"),
        }))
        .expect("payments create");
    assert_eq!(resp.status, 201, "expected 201, got {}", resp.status);
    assert_eq!(resp.body["status"].as_str(), Some("requested"));
    assert_eq!(
        resp.body["client_reference"].as_str(),
        Some(fixture_id("MANZA_FIXTURE_CLIENT_REFERENCE"))
    );
    // Inside the envelope: the challenge went to the authorizer.
    assert_eq!(
        resp.body["authorization"]["status"].as_str(),
        Some("pending")
    );
    assert!(
        resp.body["transfer"].is_null(),
        "expected null transfer before approval, got {:?}",
        resp.body["transfer"]
    );

    let got = client
        .payments()
        .get(fixture_id("MANZA_FIXTURE_TRANSFER_DRAFT_ID"))
        .expect("payments get");
    assert!(got.body["id"].is_string());
    assert!(got.body.get("status").is_some());
    assert!(got.body.get("transfer").is_some());
}

#[test]
fn payments_get_refused() {
    let server = ReplayServer::start(&["payments/get_refused"]);
    let client = replay_client(&server);

    let id = fixture_id("MANZA_FIXTURE_REFUSED_DRAFT_ID");
    let resp = client.payments().get(id).expect("payments get");
    assert_eq!(resp.body["id"].as_str(), Some(id));
    assert_eq!(
        resp.body["authorization"]["status"].as_str(),
        Some("refused")
    );
    assert_eq!(
        resp.body["authorization"]["refusal_reason"].as_str(),
        Some("over_transfer_limit")
    );
}

#[test]
#[expect(deprecated)] // the alias must keep warning: this fails if it stops
fn transfer_drafts_is_a_deprecated_alias_of_payments() {
    let server = ReplayServer::start(&["payments/get"]);
    let client = replay_client(&server);

    // Replays a payments/ cassette, so the alias calls /api/payments.
    let resp = client
        .transfer_drafts()
        .get(fixture_id("MANZA_FIXTURE_TRANSFER_DRAFT_ID"))
        .expect("transfer drafts get");
    assert!(resp.body["id"].is_string());
}

#[test]
fn payments_create_duplicate() {
    let server = ReplayServer::start(&["payments/create_duplicate"]);
    let client = replay_client(&server);

    let err = client
        .payments()
        .create(&json!({
            "account_id": fixture_id("MANZA_FIXTURE_ACCOUNT_ID"),
            "beneficiary_id": fixture_id("MANZA_FIXTURE_BENEFICIARY_ID"),
            "amount": "10.00",
            "client_reference": fixture_id("MANZA_FIXTURE_AUTHORIZABLE_CLIENT_REFERENCE"),
        }))
        .expect_err("expected a conflict");
    let Error::Api(e) = err else {
        panic!("expected Error::Api, got {err:?}")
    };
    assert_eq!(e.kind, ErrorKind::Conflict);
    assert_eq!(e.error_type.as_deref(), Some("duplicate_client_reference"));
    assert_eq!(
        e.payment_id.as_deref(),
        Some(fixture_id("MANZA_FIXTURE_AUTHORIZABLE_DRAFT_ID"))
    );
}

#[test]
fn payments_authorize_blank_signature_fails_locally() {
    // Nothing listens on this port: a request would be a Connection error.
    let client = Client::builder()
        .api_key("test")
        .base_url("http://127.0.0.1:1")
        .build()
        .expect("build client");

    for signature in ["", " ", "\t\n"] {
        let err = client
            .payments()
            .authorize("draft", "auth", signature)
            .expect_err("expected an argument error");
        assert!(
            matches!(&err, Error::Configuration(m) if m.contains("signature")),
            "got {err:?}"
        );
    }
}

// Order matters while recording (five consecutive bad signatures suspend the
// authorizer), but replay is order-free: each test loads exactly one
// cassette, and the authorize cassettes match the body minus `signature`.
#[test]
fn payments_authorize_bad_signature() {
    let server = ReplayServer::start_ignoring_signature(&["payments/authorize_bad_signature"]);
    let client = replay_client(&server);

    let err = client
        .payments()
        .authorize(
            fixture_id("MANZA_FIXTURE_BAD_SIGNATURE_DRAFT_ID"),
            fixture_id("MANZA_FIXTURE_BAD_SIGNATURE_AUTHORIZATION_ID"),
            &"0".repeat(64),
        )
        .expect_err("expected invalid_signature");
    let Error::Api(e) = err else {
        panic!("expected Error::Api, got {err:?}")
    };
    assert_eq!(e.kind, ErrorKind::Validation);
    assert_eq!(e.error_type.as_deref(), Some("invalid_signature"));
}

#[test]
fn payments_authorize_same_key() {
    let server = ReplayServer::start_ignoring_signature(&["payments/authorize_same_key"]);
    let client = replay_client(&server);

    let err = client
        .payments()
        .authorize(
            fixture_id("MANZA_FIXTURE_AUTHORIZABLE_DRAFT_ID"),
            fixture_id("MANZA_FIXTURE_AUTHORIZABLE_AUTHORIZATION_ID"),
            &"0".repeat(64),
        )
        .expect_err("expected same_key_forbidden");
    let Error::Api(e) = err else {
        panic!("expected Error::Api, got {err:?}")
    };
    assert_eq!(e.kind, ErrorKind::Forbidden);
    assert_eq!(e.error_type.as_deref(), Some("same_key_forbidden"));
}

#[test]
fn payments_authorize() {
    let server = ReplayServer::start_ignoring_signature(&["payments/authorize"]);
    let client = replay_client(&server);

    let draft_id = fixture_id("MANZA_FIXTURE_AUTHORIZABLE_DRAFT_ID");
    let payee = payee_for(
        Some(fixture_id("MANZA_FIXTURE_TRUSTED_EXTERNAL_ACCOUNT_ID")),
        None,
    )
    .unwrap();
    let input = signature_input(
        draft_id,
        fixture_id("MANZA_FIXTURE_AUTHORIZABLE_NONCE"),
        "10.0",
        "MAD",
        fixture_id("MANZA_FIXTURE_ACCOUNT_ID"),
        &payee,
        Some(fixture_id("MANZA_FIXTURE_AUTHORIZABLE_CLIENT_REFERENCE")),
    );

    let resp = client
        .payments()
        .authorize(
            draft_id,
            fixture_id("MANZA_FIXTURE_AUTHORIZABLE_AUTHORIZATION_ID"),
            &sign("replay-signing-secret", &input),
        )
        .expect("payments authorize");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["id"].as_str(), Some(draft_id));
    assert_eq!(
        resp.body["authorization"]["status"].as_str(),
        Some("authorized")
    );
}

// The fresh bank account was never trusted; with "Approve transfers to new
// payees" on, the authorizer approves it and the signature trusts it.
#[test]
fn payments_authorize_new_payee() {
    let server = ReplayServer::start_ignoring_signature(&["payments/authorize_new_payee"]);
    let client = replay_client(&server);

    let draft_id = fixture_id("MANZA_FIXTURE_NEW_PAYEE_DRAFT_ID");
    let payee = payee_for(Some(fixture_id("MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID")), None).unwrap();
    let input = signature_input(
        draft_id,
        fixture_id("MANZA_FIXTURE_NEW_PAYEE_NONCE"),
        "10.0",
        "MAD",
        fixture_id("MANZA_FIXTURE_ACCOUNT_ID"),
        &payee,
        Some(fixture_id("MANZA_FIXTURE_NEW_PAYEE_CLIENT_REFERENCE")),
    );

    let resp = client
        .payments()
        .authorize(
            draft_id,
            fixture_id("MANZA_FIXTURE_NEW_PAYEE_AUTHORIZATION_ID"),
            &sign("replay-signing-secret", &input),
        )
        .expect("payments authorize");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["id"].as_str(), Some(draft_id));
    assert_eq!(
        resp.body["authorization"]["status"].as_str(),
        Some("authorized")
    );
}

// Recorded after the new-payee authorize: its signature trusted the payee.
#[test]
fn beneficiaries_get_external_account_trusted() {
    let server = ReplayServer::start(&["beneficiaries/get_external_account_trusted"]);
    let client = replay_client(&server);

    let account_id = fixture_id("MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID");
    let resp = client
        .beneficiaries()
        .get_external_account(
            fixture_id("MANZA_FIXTURE_CREATED_BENEFICIARY_ID"),
            account_id,
        )
        .expect("get external account");
    assert_eq!(resp.body["id"].as_str(), Some(account_id));
    assert_eq!(resp.body["trusted"], json!(true));
}

#[test]
fn transfer_authorizer_get() {
    let server = ReplayServer::start(&["transfer_authorizer/get"]);
    let client = replay_client(&server);

    let resp = client
        .transfer_authorizer()
        .get()
        .expect("transfer authorizer get");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["status"].as_str(), Some("active"));
    assert!(resp.body["suspension_reason"].is_null());
    assert_eq!(resp.body["currency_code"].as_str(), Some("MAD"));
    for key in [
        "per_transfer_limit",
        "daily_limit",
        "daily_authorized_total",
        "secret_expires_at",
        "webhook_endpoint_id",
        "enrolled_at",
    ] {
        assert!(resp.body.get(key).is_some(), "missing {key}");
    }
}

#[test]
fn payments_decline() {
    let server = ReplayServer::start(&["payments/decline"]);
    let client = replay_client(&server);

    let resp = client
        .payments()
        .decline(
            fixture_id("MANZA_FIXTURE_DECLINABLE_DRAFT_ID"),
            fixture_id("MANZA_FIXTURE_DECLINABLE_AUTHORIZATION_ID"),
            Some("SDK fixture"),
        )
        .expect("payments decline");
    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.body["id"].as_str(),
        Some(fixture_id("MANZA_FIXTURE_DECLINABLE_AUTHORIZATION_ID"))
    );
    assert_eq!(resp.body["status"].as_str(), Some("declined"));
    assert!(resp.body["declined_at"].is_string());
}

#[test]
fn payments_decline_omits_absent_reason() {
    // The decline cassette records a body with `reason`; a call without one
    // must not match it, proving the key is left out rather than nulled.
    let server = ReplayServer::start(&["payments/decline"]);
    let client = replay_client(&server);

    let err = client
        .payments()
        .decline(
            fixture_id("MANZA_FIXTURE_DECLINABLE_DRAFT_ID"),
            fixture_id("MANZA_FIXTURE_DECLINABLE_AUTHORIZATION_ID"),
            None,
        )
        .expect_err("body differs from the recorded one");
    let Error::Api(e) = err else {
        panic!("expected Error::Api, got {err:?}")
    };
    assert!(
        e.message.contains("authorization_id") && !e.message.contains("reason"),
        "unexpected request body: {}",
        e.message
    );
}

#[test]
fn beneficiaries() {
    let server = ReplayServer::start(&["beneficiaries/list", "beneficiaries/get"]);
    let client = replay_client(&server);

    let page = client
        .beneficiaries()
        .list(Default::default())
        .expect("beneficiaries list");
    assert!(!page.data.is_empty(), "expected at least one beneficiary");
    assert!(
        page.data[0]["external_accounts"].is_array(),
        "expected embedded external_accounts, got {:?}",
        page.data[0].get("external_accounts")
    );

    let resp = client
        .beneficiaries()
        .get(fixture_id("MANZA_FIXTURE_BENEFICIARY_ID"))
        .expect("beneficiaries get");
    assert!(resp.body["id"].is_string());
    assert!(resp.body["external_accounts"].is_array());
}

#[test]
fn beneficiaries_create() {
    let server = ReplayServer::start(&["beneficiaries/create"]);
    let client = replay_client(&server);

    let resp = client
        .beneficiaries()
        .create(&json!({
            "beneficiary_type": "business",
            "company_name": "Manza Fixture Beneficiary spec",
            "email": "fixture-beneficiary-spec@example.com",
        }))
        .expect("beneficiaries create");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body["beneficiary_type"].as_str(), Some("business"));
    assert_eq!(resp.body["external_accounts"], json!([]));
}

#[test]
fn beneficiaries_external_accounts() {
    let server = ReplayServer::start(&[
        "beneficiaries/list_external_accounts",
        "beneficiaries/get_external_account",
    ]);
    let client = replay_client(&server);
    let beneficiary_id = fixture_id("MANZA_FIXTURE_CREATED_BENEFICIARY_ID");
    let account_id = fixture_id("MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID");

    let page = client
        .beneficiaries()
        .list_external_accounts(beneficiary_id, Default::default())
        .expect("list external accounts");
    assert_eq!(page.data[0]["id"].as_str(), Some(account_id));
    assert!(page.data[0]["account_number"].is_string());
    assert!(!page.has_more);

    let resp = client
        .beneficiaries()
        .get_external_account(beneficiary_id, account_id)
        .expect("get external account");
    assert_eq!(resp.body["id"].as_str(), Some(account_id));
    assert!(resp.body.get("default").is_some());
}

#[test]
fn beneficiaries_create_external_account() {
    let server = ReplayServer::start(&["beneficiaries/create_external_account"]);
    let client = replay_client(&server);

    let resp = client
        .beneficiaries()
        .create_external_account(
            fixture_id("MANZA_FIXTURE_CREATED_BENEFICIARY_ID"),
            &json!({
                "account_number": fixture_id("MANZA_FIXTURE_NEW_ACCOUNT_NUMBER"),
                "name": "Fixture Secondary Account",
            }),
        )
        .expect("create external account");
    assert_eq!(resp.status, 201);
    assert_eq!(
        resp.body["name"].as_str(),
        Some("Fixture Secondary Account")
    );
    assert_eq!(resp.body["default"], json!(false));
}

#[test]
fn payee_trust_requests() {
    let server = ReplayServer::start(&["payee_trust_requests/create"]);
    let client = replay_client(&server);

    let resp = client
        .payee_trust_requests()
        .create(&[fixture_id("MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID")])
        .expect("payee trust requests create");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body["status"].as_str(), Some("pending"));
    assert_eq!(
        resp.body["external_account_ids"],
        json!([fixture_id("MANZA_FIXTURE_EXTERNAL_ACCOUNT_ID")])
    );
}

#[test]
fn payee_trust_requests_get() {
    let server = ReplayServer::start(&["payee_trust_requests/get"]);
    let client = replay_client(&server);

    let id = fixture_id("MANZA_FIXTURE_PAYEE_TRUST_REQUEST_ID");
    let resp = client
        .payee_trust_requests()
        .get(id)
        .expect("payee trust requests get");
    assert_eq!(resp.body["id"].as_str(), Some(id));
    assert!(resp.body["resolved_at"].is_null());
}
