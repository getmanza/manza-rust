//! The ten API resources. Each hangs off [`Client`](crate::Client) as an
//! accessor method (`client.accounts()`, `client.invoices()`, ...).

mod accounts;
mod beneficiaries;
mod checkout_sessions;
mod customers;
mod entity;
mod invoices;
mod payee_trust_requests;
mod payment_links;
mod transfer_drafts;
mod webhook_endpoints;

pub use accounts::{AccountListParams, Accounts, TransactionListParams};
pub use beneficiaries::Beneficiaries;
pub use checkout_sessions::CheckoutSessions;
pub use customers::{CustomerListParams, Customers};
pub use entity::Entity;
pub use invoices::{InvoiceListParams, Invoices};
pub use payee_trust_requests::PayeeTrustRequests;
pub use payment_links::{PaymentLinkListParams, PaymentLinks};
pub use transfer_drafts::TransferDrafts;
pub use webhook_endpoints::WebhookEndpoints;

/// A request body for create/update calls — `snake_case` keys, exactly what
/// the API accepts (see the per-endpoint docs). Build one with
/// [`serde_json::json!`].
pub type Attributes = serde_json::Value;

pub(crate) fn push_if_present(
    query: &mut Vec<(String, String)>,
    key: &str,
    value: Option<&String>,
) {
    if let Some(value) = value {
        if !value.is_empty() {
            query.push((key.to_owned(), value.clone()));
        }
    }
}
