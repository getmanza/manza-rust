use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::resources::Attributes;
use serde_json::{json, Map, Value};

/// API-initiated transfers, called payments (formerly transfer drafts;
/// `/api/transfer_drafts` stays a permanent server-side alias). Creating a
/// draft never executes a transfer by itself. A draft inside the entity's machine-authorization envelope
/// (trusted payee, within limits) is sent to the enrolled transfer
/// authorizer as a `payment.authorization_requested` webhook; answer it with
/// [`authorize`](Payments::authorize) or
/// [`decline`](Payments::decline), using an API key other than the one
/// that created the draft. Every other draft goes to the in-app approval
/// flow, where a manager or legal representative approves it. Poll
/// [`get`](Payments::get) (status: `requested` → `processing` →
/// `completed` / `failed`) or subscribe to the `transfer.executed` webhook.
///
/// A draft's `authorization` says which way it went: `pending` (a challenge
/// was sent), `authorized`, `declined` (the draft is deleted), or one of two
/// outcomes that send the draft to the in-app approvers, each with a
/// `refusal_reason`:
///
/// - `refused`: it did not qualify, at create time (no webhook is sent) or
///   again when the challenge was answered: `untrusted_payee`,
///   `unrestricted_api_key`, `over_new_payee_limit`, `over_transfer_limit`,
///   `over_daily_limit`, `currency_mismatch`, `suspended` or
///   `secret_expired`.
/// - `expired` / `superseded` / `failed`: a challenge was sent and closed
///   without executing the transfer: `expired` (no answer within the hour),
///   `stale` (the draft, its payee's bank details or the endpoint changed
///   while it was open), `signature_failed` (five invalid signatures) or
///   `rail_limit_exceeded` (status `superseded`: the challenge was
///   authorized, but the transfer was over its rail's limit when it
///   executed, so nothing moved).
///
/// `authorization` is `null` when no authorizer was enrolled.
pub struct Payments<'a> {
    pub(crate) client: &'a Client,
}

impl Payments<'_> {
    /// Calls `POST /api/payments`.
    ///
    /// Required: `account_id`, `amount`, and exactly one of `beneficiary_id`
    /// (external transfer) or `destination_account_id` (own-account move).
    ///
    /// Optional: `external_account_id`, `currency_code`, `payment_reference`,
    /// `internal_notes`, `client_reference` (unique per entity, at most
    /// 128 characters; a duplicate returns a [`ErrorKind::Conflict`](crate::ErrorKind::Conflict) error
    /// whose `payment_id` names the existing draft), and `transfer_method`
    /// (the rail: `eft` or `rtc` in South Africa, `standard` or `instant` in
    /// Morocco; one the market does not offer is a 422
    /// [`ErrorKind::Validation`](crate::ErrorKind::Validation) with `param`
    /// `transfer_method`, and nothing is created).
    ///
    /// The response's `transfer_method` is the rail sent, or the provider's
    /// default when left out; `null` where the market offers no choice. The
    /// machine-authorization signature signs it
    /// ([`SignatureFields::transfer_method`](crate::transfer_authorization::SignatureFields::transfer_method)).
    ///
    /// The created draft awaits authorization — the API never executes a
    /// transfer itself.
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/payments", Some(attributes))
    }

    /// Calls `GET /api/payments/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client
            .get(&format!("api/payments/{}", encode_component(id)), &[])
    }

    /// Calls `POST /api/payments/:id/authorize`.
    ///
    /// Executes the draft. `authorization_id` comes from the
    /// `payment.authorization_requested` webhook; build `signature` with
    /// [`transfer_authorization`](crate::transfer_authorization) (the v2
    /// input, which the API verifies unless the client is pinned to
    /// `Manza-Version: 2026-03-27`). Requires
    /// the `transfers:authorize` scope on a key other than the draft's
    /// creator (otherwise 403 `same_key_forbidden`), bound to an IP
    /// allowlist (otherwise 403 `unrestricted_api_key`). A 409
    /// [`ErrorKind::Conflict`](crate::ErrorKind::Conflict) with `error_type`
    /// `authorization_busy` means a bank-detail edit, trust or trust
    /// revocation on the payee was in flight: retry. A blank signature is
    /// refused locally with [`Error::Configuration`]: the API counts it as a
    /// failed attempt, and five fail the challenge. A valid signature on a
    /// transfer over its rail's limit is a 422
    /// [`ErrorKind::Validation`](crate::ErrorKind::Validation)
    /// (`state_error`): nothing moved, the authorization is `superseded` with
    /// `refusal_reason` `rail_limit_exceeded`, and the draft went to your
    /// approvers.
    pub fn authorize(
        &self,
        id: &str,
        authorization_id: &str,
        signature: &str,
    ) -> Result<Response, Error> {
        if signature.trim().is_empty() {
            return Err(Error::Configuration("signature cannot be blank".to_owned()));
        }

        self.client.post(
            &format!("api/payments/{}/authorize", encode_component(id)),
            Some(&json!({ "authorization_id": authorization_id, "signature": signature })),
        )
    }

    /// Calls `POST /api/payments/:id/decline`.
    ///
    /// Declines the challenge and deletes the draft. Returns the
    /// authorization (`status: "declined"`, `declined_at`, and a `null`
    /// `refusal_reason`). `reason` is left out of the request when `None`.
    pub fn decline(
        &self,
        id: &str,
        authorization_id: &str,
        reason: Option<&str>,
    ) -> Result<Response, Error> {
        let mut body = Map::new();
        body.insert("authorization_id".to_owned(), json!(authorization_id));
        if let Some(reason) = reason {
            body.insert("reason".to_owned(), json!(reason));
        }

        self.client.post(
            &format!("api/payments/{}/decline", encode_component(id)),
            Some(&Value::Object(body)),
        )
    }
}
