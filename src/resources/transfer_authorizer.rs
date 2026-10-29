use crate::client::{Client, Response};
use crate::error::Error;

/// The entity's enrolled transfer authorizer, read-only. Enrolment, limits,
/// the "Approve transfers to new payees" option and resume stay in the app
/// behind a second factor. A singular resource: no list, no id.
pub struct TransferAuthorizer<'a> {
    pub(crate) client: &'a Client,
}

impl TransferAuthorizer<'_> {
    /// Calls `GET /api/transfer_authorizer`.
    ///
    /// Any key with `transfers:read`, `transfers:write` or
    /// `transfers:authorize`. Returns `status` (`active` / `suspended`),
    /// `suspension_reason` (`member`, `signature_failures`, `secret_expired`,
    /// `endpoint_disabled`; `null` when active), `currency_code`,
    /// `per_transfer_limit`, `daily_limit`, `daily_authorized_total` (the
    /// last 24 hours), `secret_expires_at`, `webhook_endpoint_id` and
    /// `enrolled_at`. It does not show whether new payees are approved.
    /// Fails with [`ErrorKind::NotFound`](crate::ErrorKind::NotFound) when no
    /// authorizer is enrolled.
    pub fn get(&self) -> Result<Response, Error> {
        self.client.get("api/transfer_authorizer", &[])
    }
}
