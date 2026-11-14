use crate::client::{encode_component, Client, Response};
use crate::error::Error;
use crate::page::{list_page, ListParams, Page};
use crate::resources::{push_if_present, Attributes};

/// Invoices and their lifecycle actions.
pub struct Invoices<'a> {
    pub(crate) client: &'a Client,
}

/// Filters for `GET /api/invoices`.
#[derive(Debug, Clone, Default)]
pub struct InvoiceListParams {
    /// Shared pagination inputs.
    pub list: ListParams,
    /// Filter by invoice status: `draft`, `pending_approval`, `declined`,
    /// `approved`, `sent`, `paid`, `overdue` or `cancelled`.
    pub status: Option<String>,
    /// Filter by customer id.
    pub customer_id: Option<String>,
}

impl Invoices<'_> {
    /// Calls `GET /api/invoices`.
    pub fn list(&self, params: InvoiceListParams) -> Result<Page, Error> {
        let mut query = Vec::new();
        push_if_present(&mut query, "status", params.status.as_ref());
        push_if_present(&mut query, "customer_id", params.customer_id.as_ref());
        list_page(self.client, "api/invoices", query, &params.list)
    }

    /// Calls `GET /api/invoices/:id`.
    pub fn get(&self, id: &str) -> Result<Response, Error> {
        self.client
            .get(&format!("api/invoices/{}", encode_component(id)), &[])
    }

    /// Calls `POST /api/invoices`.
    ///
    /// Required: `customer_id`, `account_id` (the entity's account the
    /// invoice is paid into, from [`Accounts::list`](crate::Accounts::list);
    /// without one the API answers 422 `state_error` with no `param`, and an
    /// account that is not the entity's 422 with `param` `account_id`),
    /// `issue_date`, `due_date` and `items`. Optional: `currency_code`,
    /// `reference`, `notes`, `payment_terms`, `send_to_email`, `discounts`,
    /// `delivery_date` (MA only) and `generate_payment_link`.
    ///
    /// `"generate_payment_link": true` asks for a card payment link, made
    /// from the invoice's account when the invoice is approved; it needs the
    /// `payment_links:write` scope and is taken only while payment links are
    /// enabled for the entity. A boolean, `null` (no link) or the strings
    /// `"true"`, `"false"`, `"1"` and `"0"` are taken; any other value is a
    /// 400 [`ErrorKind::Validation`](crate::ErrorKind::Validation) and
    /// nothing is created. It replaces the removed `create_payment_link`.
    ///
    /// The response carries `generate_payment_link`, the embedded `account`
    /// (`id`, `name`, `currency_code`, and `rib` in Morocco or
    /// `account_number` in South Africa) and `payment_link` once one exists.
    /// A plan that caps invoices answers
    /// [`ErrorKind::PlanLimitReached`](crate::ErrorKind::PlanLimitReached).
    pub fn create(&self, attributes: &Attributes) -> Result<Response, Error> {
        self.client.post("api/invoices", Some(attributes))
    }

    /// Calls `PATCH /api/invoices/:id`.
    ///
    /// Takes the create fields, `account_id` and `generate_payment_link`
    /// included (the same scope and entity rules apply), on an invoice that
    /// is still `draft`, `pending_approval` or `declined`; any other is a 422
    /// [`ErrorKind::Validation`](crate::ErrorKind::Validation)
    /// (`state_error`).
    pub fn update(&self, id: &str, attributes: &Attributes) -> Result<Response, Error> {
        self.client.patch(
            &format!("api/invoices/{}", encode_component(id)),
            Some(attributes),
        )
    }

    /// Calls `POST /api/invoices/:id/send`.
    pub fn send(&self, id: &str) -> Result<Response, Error> {
        self.client
            .post(&format!("api/invoices/{}/send", encode_component(id)), None)
    }

    /// Calls `POST /api/invoices/:id/mark_as_paid`.
    pub fn mark_as_paid(&self, id: &str) -> Result<Response, Error> {
        self.client.post(
            &format!("api/invoices/{}/mark_as_paid", encode_component(id)),
            None,
        )
    }

    /// Calls `POST /api/invoices/:id/cancel`.
    pub fn cancel(&self, id: &str) -> Result<Response, Error> {
        self.client.post(
            &format!("api/invoices/{}/cancel", encode_component(id)),
            None,
        )
    }

    /// Calls `POST /api/invoices/:id/credit_note`.
    pub fn credit_note(&self, id: &str) -> Result<Response, Error> {
        self.client.post(
            &format!("api/invoices/{}/credit_note", encode_component(id)),
            None,
        )
    }

    /// Calls `DELETE /api/invoices/:id`.
    ///
    /// Only a `draft`, `pending_approval` or `declined` invoice that has no
    /// number yet; any other is a 422
    /// [`ErrorKind::Validation`](crate::ErrorKind::Validation)
    /// (`state_error`).
    pub fn delete(&self, id: &str) -> Result<Response, Error> {
        self.client
            .delete(&format!("api/invoices/{}", encode_component(id)))
    }
}
