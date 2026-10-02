use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::page::{list_page, ListParams, Page};
use crate::resources::{push_if_present, Attributes};

/// Standalone payment links (not attached to an invoice).
pub struct PaymentLinks<'a> {
    pub(crate) client: &'a Client,
}

/// Filters for `GET /api/payment_links`.
#[derive(Debug, Clone, Default)]
pub struct PaymentLinkListParams {
    /// Shared pagination inputs.
    pub list: ListParams,
    /// Filter by link status.
    pub status: Option<String>,
    /// Filter by link type (`single` / `reusable`).
    pub link_type: Option<String>,
}

impl PaymentLinks<'_> {
    /// Calls `GET /api/payment_links`.
    pub fn list(&self, params: PaymentLinkListParams) -> Result<Page, Error> {
        let mut query = Vec::new();
        push_if_present(&mut query, "status", params.status.as_ref());
        push_if_present(&mut query, "link_type", params.link_type.as_ref());
        list_page(self.client, "api/payment_links", query, &params.list)
    }

    /// Calls `GET /api/payment_links/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client
            .get(&format!("api/payment_links/{}", encode_component(id)), &[])
    }

    /// Calls `POST /api/payment_links`. Optional: `collect_billing_address`
    /// and `billing_address`. The response gains `settled_at`, and the
    /// status can be `clearing`.
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/payment_links", Some(attributes))
    }

    /// Calls `POST /api/payment_links/:id/cancel`.
    pub fn cancel(&self, id: &str) -> Result<Response, Error> {
        self.client.post(
            &format!("api/payment_links/{}/cancel", encode_component(id)),
            None,
        )
    }
}
