//! The creator process (key A). Creates a transfer draft, writes it to the
//! ledger from the API's response, and says where the API routed it.
//!
//!   cargo run --example create_transfer            # 10.00
//!   cargo run --example create_transfer -- 250.00

#[path = "dotenv.rs"]
pub mod dotenv;
#[path = "ledger.rs"]
pub mod ledger;
#[path = "routing.rs"]
pub mod routing;

use std::hash::{BuildHasher, Hasher};
use std::process::exit;

use manza::transfer_authorization::payee_for;
use manza::{Client, Error};
use serde_json::json;

fn main() {
    dotenv::load();
    let amount = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "10.00".to_owned());
    let client_reference = format!("example-{}", random_hex());
    let ledger = ledger::Ledger::new(dotenv::dir().join("ledger.json"));
    let creator = Client::builder()
        .api_key(required("MANZA_CREATOR_API_KEY"))
        .build()
        .unwrap_or_else(|e| abort(&e.to_string()));

    let draft = match creator.payments().create(&json!({
        "account_id": required("MANZA_ACCOUNT_ID"),
        "beneficiary_id": required("MANZA_BENEFICIARY_ID"),
        "amount": amount,
        "payment_reference": "Transfer authorizer example",
        "client_reference": client_reference,
    })) {
        Ok(response) => response.body,
        // A timeout may still have created the draft: look for this
        // client_reference in the app before rerunning, or the rerun creates
        // a second transfer.
        Err(e) => abort(&format!(
            "create failed for client_reference {client_reference}: {}",
            describe(&e)
        )),
    };

    let text = |name: &str| draft[name].as_str().unwrap_or_default().to_owned();
    let id = text("id");
    let destination = draft["destination_account_id"].as_str();
    let external = if destination.is_some() {
        None
    } else {
        draft["external_account_id"].as_str()
    };
    let payee = payee_for(external, destination).unwrap_or_else(|_| {
        abort(&format!(
            "payment {id} names neither an external_account_id nor a destination_account_id, so it was \
             not written to the ledger: if it reaches the authorizer, the authorizer declines it"
        ))
    });
    if let Err(e) = ledger.record(
        &client_reference,
        &id,
        &text("amount"),
        &text("currency_code"),
        &text("account_id"),
        &payee,
    ) {
        abort(&format!(
            "payment {id} (client_reference {client_reference}) was created but not written to the \
             ledger ({e}): if it reaches the authorizer, the authorizer declines it"
        ));
    }
    println!(
        "created payment {id}: {} {} to {payee} (client_reference {client_reference})",
        text("amount"),
        text("currency_code")
    );

    println!("{}", routing::describe(draft.get("authorization")));
}

fn required(name: &str) -> String {
    let value = dotenv::var(name);
    if value.is_empty() {
        abort(&format!("Set {name} in {}", dotenv::env_path().display()));
    }
    value
}

/// `validation (422): message (type "x", request_id "y")`.
fn describe(error: &Error) -> String {
    let inspect = |v: Option<&str>| v.map_or_else(|| "none".to_owned(), |v| format!("{v:?}"));
    match error {
        Error::Api(e) => format!(
            "{} ({}): {} (type {}, request_id {})",
            e.kind,
            e.status,
            e.message,
            inspect(e.error_type.as_deref()),
            inspect(e.request_id.as_deref())
        ),
        other => other.to_string(),
    }
}

/// Twelve hex characters, unique enough for an example's client_reference.
fn random_hex() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff)
}

fn abort(message: &str) -> ! {
    eprintln!("{message}");
    exit(1)
}
