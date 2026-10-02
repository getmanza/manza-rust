//! Signs a machine-authorization challenge for an API-created transfer
//! draft. Pure functions, no HTTP.
//!
//! The `payment.authorization_requested` webhook delivers the authorization
//! id and a one-time nonce. Build the signature input from your *own*
//! record of the transfer (not the webhook's `signature_input`, which is
//! there only to compare against), sign it with the authorizer endpoint's
//! signing secret, and pass the result to
//! [`TransferDrafts::authorize`](crate::TransferDrafts::authorize):
//!
//! ```
//! use zazu_sdk::transfer_authorization::{payee_for, sign, signature_input};
//!
//! let payee = payee_for(Some("ext-account-id"), None)?;
//! let input = signature_input(
//!     "payment-id", "nonce", "2500.0", "MAD", "account-id", &payee, Some("po_1"),
//! );
//! let signature = sign("whsec_signing_secret", &input);
//! assert_eq!(signature.len(), 64);
//! # Ok::<(), zazu_sdk::Error>(())
//! ```

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::Error;

/// The version prefix of every signature input.
pub const SIGNATURE_VERSION: &str = "manza.transfer-authorization.v1";

/// Builds the pipe-joined signature input. `amount` must be the API's
/// decimal string verbatim (e.g. `"2500.0"`); `client_reference` is empty
/// when the transfer has none.
pub fn signature_input(
    payment_id: &str,
    nonce: &str,
    amount: &str,
    currency_code: &str,
    account_id: &str,
    payee: &str,
    client_reference: Option<&str>,
) -> String {
    [
        SIGNATURE_VERSION,
        payment_id,
        nonce,
        amount,
        currency_code,
        account_id,
        payee,
        client_reference.unwrap_or(""),
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
