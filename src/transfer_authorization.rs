//! Signs a machine-authorization challenge for an API-created transfer
//! draft. Pure functions, no HTTP.
//!
//! The `payment.authorization_requested` webhook delivers the authorization
//! id and a one-time nonce. Build the signature input from your *own*
//! record of the transfer (not the webhook's `signature_input_v2`, which is
//! there only to compare against), sign it with the authorizer endpoint's
//! signing secret, and pass the result to
//! [`Payments::authorize`](crate::Payments::authorize):
//!
//! ```
//! use manza::transfer_authorization::{payee_for, sign, signature_input, SignatureFields};
//!
//! let payee = payee_for(Some("ext-account-id"), None)?;
//! let input = signature_input(&SignatureFields {
//!     payment_id: "payment-id",
//!     nonce: "nonce",
//!     amount: "2500.0",
//!     currency_code: "MAD",
//!     account_id: "account-id",
//!     payee: &payee,
//!     client_reference: Some("po_1"),
//!     transfer_method: Some("standard"),
//! });
//! let signature = sign("whsec_signing_secret", &input);
//! assert_eq!(signature.len(), 64);
//! # Ok::<(), manza::Error>(())
//! ```
//!
//! This is the v2 input, which the API verifies for every request that is
//! unpinned or sends `Manza-Version: 2026-11-13` or later. A client pinned to
//! `Manza-Version: 2026-03-27` must sign v1 and stays on manza 1.x.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::Error;

/// The version prefix of every signature input.
pub const SIGNATURE_VERSION: &str = "manza.transfer-authorization.v2";

/// The fields a signature input signs, taken from your own record of the
/// payment. No `Default`: every field is named, so `transfer_method` cannot
/// be left out by accident (a v1-shaped call fails to compile instead of
/// failing at the API, where five bad signatures fail the challenge).
#[derive(Debug, Clone, Copy)]
pub struct SignatureFields<'a> {
    /// The payment (transfer draft) id.
    pub payment_id: &'a str,
    /// The one-time nonce from the webhook's `authorization.nonce`.
    pub nonce: &'a str,
    /// The API's decimal string verbatim (e.g. `"2500.0"`).
    pub amount: &'a str,
    /// The payment's currency code.
    pub currency_code: &'a str,
    /// The funding account id.
    pub account_id: &'a str,
    /// The payee token from [`payee_for`].
    pub payee: &'a str,
    /// The payment's `client_reference`; `None` signs an empty segment.
    pub client_reference: Option<&'a str>,
    /// The payment's rail as the API stores it (the create response's
    /// `transfer_method`); `None` signs an empty segment, for a payment whose
    /// `transfer_method` is `null` (a market with no rail choice).
    pub transfer_method: Option<&'a str>,
}

/// Builds the pipe-joined v2 signature input:
/// `manza.transfer-authorization.v2|payment_id|nonce|amount|currency_code|account_id|payee|client_reference|transfer_method`.
pub fn signature_input(fields: &SignatureFields<'_>) -> String {
    [
        SIGNATURE_VERSION,
        fields.payment_id,
        fields.nonce,
        fields.amount,
        fields.currency_code,
        fields.account_id,
        fields.payee,
        fields.client_reference.unwrap_or(""),
        fields.transfer_method.unwrap_or(""),
    ]
    .join("|")
}

/// Lowercase hex HMAC-SHA256 of the signature input under the authorizer
/// endpoint's signing secret.
pub fn sign(secret: &str, signature_input: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(signature_input.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The payee token: `ext:<id>` for a beneficiary's bank account, `own:<id>`
/// for one of the entity's own accounts. Pass exactly one.
pub fn payee_for(
    external_account_id: Option<&str>,
    destination_account_id: Option<&str>,
) -> Result<String, Error> {
    match (external_account_id, destination_account_id) {
        (Some(id), None) => Ok(format!("ext:{id}")),
        (None, Some(id)) => Ok(format!("own:{id}")),
        _ => Err(Error::Configuration(
            "pass exactly one of external_account_id or destination_account_id".to_owned(),
        )),
    }
}
