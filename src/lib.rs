//! Rust SDK for the Manza API.
//!
//! Response bodies are returned as-is from the API — `snake_case` keys, no
//! struct mapping. The same shape ships across every Manza SDK (Ruby,
//! TypeScript, Python, Go, Rust, ...) so the cassette contract is
//! one-to-one.
//!
//! ```no_run
//! let client = manza::Client::builder().api_key("sk_live_...").build()?;
//!
//! let entity = client.entity().get()?;
//! println!("{}", entity.body["name"]);
//!
//! let page = client.accounts().list(Default::default())?;
//! for account in &page.data {
//!     println!("{} {}", account["id"], account["name"]);
//! }
//! # Ok::<(), manza::Error>(())
//! ```

#![warn(missing_docs)]

mod client;
mod env;
mod error;
mod page;
mod resources;
pub mod transfer_authorization;

pub use client::{Client, ClientBuilder, Response, VERSION};
pub use error::{ApiError, Error, ErrorKind};
pub use page::{ListParams, Page, MAX_PER_PAGE};
pub use resources::{
    AccountListParams, Accounts, Attributes, Beneficiaries, CheckoutSessions, CustomerListParams,
    Customers, Entity, InvoiceListParams, Invoices, PayeeTrustRequests, PaymentLinkListParams,
    PaymentLinks, TransactionListParams, TransferDrafts, WebhookEndpoints,
};
