//! Fixed test vectors, shared by every SDK in the family (see
//! manza-ruby's spec/manza/transfer_authorization_spec.rb). Digests were
//! computed independently with:
//!
//!   printf '%s' '<input>' | openssl dgst -sha256 -hmac 'whsec_test_vector_secret'

use manza::transfer_authorization::{payee_for, sign, signature_input};
use manza::Error;

const SECRET: &str = "whsec_test_vector_secret";
const PAYMENT_ID: &str = "0199a1b2-0000-7000-8000-000000000001";
const NONCE: &str = "n0nce-0123456789abcdef";
const ACCOUNT_ID: &str = "0199a1b2-0000-7000-8000-000000000002";

#[test]
fn external_account_payee_with_client_reference() {
    let payee = payee_for(Some("0199a1b2-0000-7000-8000-000000000003"), None).unwrap();
    let input = signature_input(
        PAYMENT_ID,
        NONCE,
        "2500.0",
        "MAD",
        ACCOUNT_ID,
        &payee,
        Some("po_1"),
    );

    assert_eq!(
        input,
        "manza.transfer-authorization.v1|0199a1b2-0000-7000-8000-000000000001|n0nce-0123456789abcdef|\
         2500.0|MAD|0199a1b2-0000-7000-8000-000000000002|ext:0199a1b2-0000-7000-8000-000000000003|po_1"
    );
    assert_eq!(
        sign(SECRET, &input),
        "6e8eaec0f89a4eb3b22df1133b3d6dfebfa8505c34c58ed0ff192516e4223078"
    );
}

#[test]
fn own_account_payee_without_client_reference() {
    let payee = payee_for(None, Some("0199a1b2-0000-7000-8000-000000000004")).unwrap();
    let input = signature_input(PAYMENT_ID, NONCE, "2500.0", "MAD", ACCOUNT_ID, &payee, None);

    assert!(input.ends_with("|own:0199a1b2-0000-7000-8000-000000000004|"));
    assert_eq!(
        sign(SECRET, &input),
        "af9440b1de1bebb51f381ce43e3d0d27b6a4ccb99dcd548c0b5435ff4fdd1895"
    );
}

#[test]
fn payee_for_requires_exactly_one_id() {
    for (ext, own) in [(Some("a"), Some("b")), (None, None)] {
        let err = payee_for(ext, own).expect_err("expected an argument error");
        assert!(matches!(err, Error::Configuration(_)), "got {err:?}");
    }
}
