//! Fixed test vectors, shared by every SDK in the family (see
//! manza-ruby's spec/manza/transfer_authorization_spec.rb). Digests were
//! computed independently with:
//!
//!   printf '%s' '<input>' | openssl dgst -sha256 -hmac 'whsec_test_vector_secret'

use manza::transfer_authorization::{
    payee_for, sign, signature_input, SignatureFields, SIGNATURE_VERSION,
};
use manza::Error;

const SECRET: &str = "whsec_test_vector_secret";
const PAYMENT_ID: &str = "0199a1b2-0000-7000-8000-000000000001";
const NONCE: &str = "n0nce-0123456789abcdef";
const ACCOUNT_ID: &str = "0199a1b2-0000-7000-8000-000000000002";

fn input(payee: &str, client_reference: Option<&str>, transfer_method: Option<&str>) -> String {
    signature_input(&SignatureFields {
        payment_id: PAYMENT_ID,
        nonce: NONCE,
        amount: "2500.0",
        currency_code: "MAD",
        account_id: ACCOUNT_ID,
        payee,
        client_reference,
        transfer_method,
    })
}

#[test]
fn signs_version_v2() {
    assert_eq!(SIGNATURE_VERSION, "manza.transfer-authorization.v2");
}

#[test]
fn external_account_payee_with_client_reference_and_transfer_method() {
    let payee = payee_for(Some("0199a1b2-0000-7000-8000-000000000003"), None).unwrap();
    let input = input(&payee, Some("po_1"), Some("eft"));

    assert_eq!(
        input,
        "manza.transfer-authorization.v2|0199a1b2-0000-7000-8000-000000000001|n0nce-0123456789abcdef|\
         2500.0|MAD|0199a1b2-0000-7000-8000-000000000002|ext:0199a1b2-0000-7000-8000-000000000003|po_1|eft"
    );
    assert_eq!(
        sign(SECRET, &input),
        "904da7d0b73b2c919ffa1ae64d262ce5825ee4d2c8420fd70a3d257353e6c43a"
    );
}

#[test]
fn a_null_transfer_method_signs_an_empty_segment() {
    let payee = payee_for(Some("0199a1b2-0000-7000-8000-000000000003"), None).unwrap();
    let input = input(&payee, Some("po_1"), None);

    assert!(input.ends_with("|ext:0199a1b2-0000-7000-8000-000000000003|po_1|"));
    assert_eq!(
        sign(SECRET, &input),
        "029dd12924a846d7af2f4aecb61e4f4f9bd64d37315230864fb03917adc29edf"
    );
}

#[test]
fn own_account_payee_without_client_reference_or_transfer_method() {
    let payee = payee_for(None, Some("0199a1b2-0000-7000-8000-000000000004")).unwrap();
    let input = input(&payee, None, None);

    assert!(input.ends_with("|own:0199a1b2-0000-7000-8000-000000000004||"));
    assert_eq!(
        sign(SECRET, &input),
        "a21ccf43b94f23d7a595a36bd788c27c7adbc5686a5ad4427b22d9957568c9fb"
    );
}

#[test]
fn payee_for_requires_exactly_one_id() {
    for (ext, own) in [(Some("a"), Some("b")), (None, None)] {
        let err = payee_for(ext, own).expect_err("expected an argument error");
        assert!(matches!(err, Error::Configuration(_)), "got {err:?}");
    }
}
