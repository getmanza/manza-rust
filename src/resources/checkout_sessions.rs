use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::resources::Attributes;

/// One-off hosted checkout sessions. No list, update, or delete; sessions
/// are created and inspected by id.
pub struct CheckoutSessions<'a> {
    pub(crate) client: &'a Client,
}

impl CheckoutSessions<'_> {
    /// Calls `POST /api/checkout_sessions`.
    /// Required attributes: `account_id`, `amount`, `success_url`. Optional:
    /// `customer_name`, `collect_billing_address` and `billing_address`. The
    /// response gains `settled_at` and `transaction`, and the status can be
    /// `clearing`.
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/checkout_sessions", Some(attributes))
    }

    /// Calls `GET /api/checkout_sessions/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client.get(
            &format!("api/checkout_sessions/{}", encode_component(id)),
            &[],
        )
    }
}
