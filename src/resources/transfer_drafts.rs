use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::resources::Attributes;
use serde_json::{json, Map, Value};

/// API-initiated transfers. Creating a draft never executes a transfer by
/// itself. A draft inside the entity's machine-authorization envelope
/// (trusted payee, within limits) is sent to the enrolled transfer
/// authorizer as a `payment.authorization_requested` webhook; answer it with
/// [`authorize`](TransferDrafts::authorize) or
/// [`decline`](TransferDrafts::decline), using an API key other than the one
/// that created the draft. Every other draft goes to the in-app approval
/// flow, where a manager or legal representative approves it. Poll
/// [`get`](TransferDrafts::get) (status: `requested` → `processing` →
/// `completed` / `failed`) or subscribe to the `transfer.executed` webhook.
pub struct TransferDrafts<'a> {
    pub(crate) client: &'a Client,
}

impl TransferDrafts<'_> {
    /// Calls `POST /api/transfer_drafts`.
    ///
    /// Required: `account_id`, `amount`, and exactly one of `beneficiary_id`
    /// (external transfer) or `destination_account_id` (own-account move).
    ///
    /// Optional: `external_account_id`, `currency_code`, `payment_reference`,
    /// `internal_notes`, and `client_reference` (unique per entity, at most
    /// 128 characters; a duplicate returns a [`ErrorKind::Conflict`](crate::ErrorKind::Conflict) error
    /// whose `payment_id` names the existing draft).
    ///
    /// The created draft awaits authorization — the API never executes a
    /// transfer itself.
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/transfer_drafts", Some(attributes))
    }

    /// Calls `GET /api/transfer_drafts/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client.get(
            &format!("api/transfer_drafts/{}", encode_component(id)),
            &[],
        )
    }

    /// Calls `POST /api/transfer_drafts/:id/authorize`.
    ///
    /// Executes the draft. `authorization_id` comes from the
    /// `payment.authorization_requested` webhook; build `signature` with
    /// [`transfer_authorization`](crate::transfer_authorization). Requires
    /// the `transfers:authorize` scope on a key other than the draft's
    /// creator (otherwise 403 `same_key_forbidden`). A blank signature is
    /// refused locally with [`Error::Configuration`]: the API counts it as a
    /// failed attempt, and five fail the challenge.
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
            &format!("api/transfer_drafts/{}/authorize", encode_component(id)),
            Some(&json!({ "authorization_id": authorization_id, "signature": signature })),
        )
    }

    /// Calls `POST /api/transfer_drafts/:id/decline`.
    ///
    /// Declines the challenge and deletes the draft. Returns the
    /// authorization (`status: "declined"`). `reason` is left out of the
    /// request when `None`.
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
            &format!("api/transfer_drafts/{}/decline", encode_component(id)),
            Some(&Value::Object(body)),
        )
    }
}
