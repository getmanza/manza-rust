use serde_json::json;

use crate::client::{encode_component, Client, Response};
use crate::error::Error;

/// Requests that a human approver mark beneficiary bank accounts as trusted
/// payees, which is what lets a transfer draft enter the machine-authorization
/// envelope. A request stays `pending` until someone resolves it in the app
/// (`approved`, `declined` or `cancelled`).
pub struct PayeeTrustRequests<'a> {
    pub(crate) client: &'a Client,
}

impl PayeeTrustRequests<'_> {
    /// Calls `POST /api/payee_trust_requests` (needs the
    /// `beneficiaries:request_trust` scope). `external_account_ids` takes at
    /// most 100 ids.
    pub fn create(&self, external_account_ids: &[&str]) -> Result<Response, Error> {
        self.client.post(
            "api/payee_trust_requests",
            Some(&json!({ "external_account_ids": external_account_ids })),
        )
    }

    /// Calls `GET /api/payee_trust_requests/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client.get(
            &format!("api/payee_trust_requests/{}", encode_component(id)),
            &[],
        )
    }
}
