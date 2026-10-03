# zazu-rust

> **Deprecated.** This crate is now [`manza`](https://crates.io/crates/manza) (`cargo add manza`, `use manza::...`). `zazu-sdk` gets no further updates.

Rust SDK for the [Zazu](https://zazu.ma) API.

```toml
# Cargo.toml — install as a git dependency for now (not on crates.io)
[dependencies]
zazu-sdk = { git = "https://github.com/getzazu/zazu-rust" }
```

```rust
use serde_json::json;

let client = zazu_sdk::Client::builder()
    .api_key(std::env::var("ZAZU_API_KEY")?)
    .build()?;

let entity = client.entity().get()?;

let page = client.accounts().list(Default::default())?;
for account in &page.data {
    println!("{} {}", account["id"], account["name"]);
}

// Initiate a transfer — it lands in your workspace's in-app approval
// queue; the API never executes a transfer itself.
let draft = client.transfer_drafts().create(&json!({
    "account_id": account_id,
    "beneficiary_id": beneficiary_id,
    "amount": "150.00",
    "payment_reference": "INV-000042",
}))?;
```

## Hosts

The default base URL is production, `https://ma.manza.finance`. For South
Africa use `https://za.manza.finance`; the test cassettes are recorded against
`https://ma.manza.dev`. Override with `ClientBuilder::base_url` or
`ZAZU_BASE_URL`.

## Resources

`accounts`, `beneficiaries` (including `create` and the external-account
methods), `checkout_sessions`, `customers`, `entity`, `invoices`,
`payee_trust_requests`, `payment_links`, `transfer_drafts` (`create`, `get`,
`authorize`, `decline`) and `webhook_endpoints`.

## Machine authorization

A transfer draft inside your machine-authorization envelope is sent to your
enrolled authorizer as a `payment.authorization_requested` webhook. Sign it
from your *own* record of the transfer and answer with an API key other than
the one that created the draft:

```rust
use zazu_sdk::transfer_authorization::{payee_for, sign, signature_input};

let payee = payee_for(Some(external_account_id), None)?;
let input = signature_input(
    draft_id, nonce, "2500.0", "MAD", account_id, &payee, Some("po_1"),
);
let signature = sign(signing_secret, &input);
authorizer.transfer_drafts().authorize(draft_id, authorization_id, &signature)?;
// or: authorizer.transfer_drafts().decline(draft_id, authorization_id, Some("reason"))?;
```

`amount` is the API's decimal string verbatim. `authorize` refuses a blank
signature locally (`Error::Configuration`), because the API counts it as a
failed attempt.

## Response shape

Response bodies are returned as-is from the API — `snake_case` keys in an
untyped `serde_json::Value`, no struct mapping. The same shape ships across
every Zazu SDK (Ruby, TypeScript, Python, Go, Rust, ...) so the cassette
contract is one-to-one.

## Errors

Non-2xx responses come back as `zazu_sdk::Error::Api` carrying `status`,
`kind` (`authentication`, `forbidden`, `not_found`, `conflict`, `validation`,
`rate_limit`, `server`, `api`), the API's `error_type`/`message`/`param`,
the `payment_id` (on a 409 `duplicate_client_reference`), the `request_id`, and `retry_after` for 429s. Transport failures are
`Error::Connection`; client-build and invalid-argument failures are
`Error::Configuration`.

## Tests

Tests replay the canonical cassettes recorded by
[zazu-ruby](https://github.com/getzazu/zazu-ruby). The cassettes are
downloaded from the Ruby SDK's release tarball and served from a local
`tiny_http` replay server. Same interactions, same assertions, every
language.

```bash
scripts/fetch-cassettes.sh
cargo test
```

## The SDK family

- [zazu-ruby](https://github.com/getzazu/zazu-ruby) — reference implementation (records the cassettes)
- [zazu-ts](https://github.com/getzazu/zazu-ts)
- [zazu-python](https://github.com/getzazu/zazu-python)
- [zazu-go](https://github.com/getzazu/zazu-go)
- [cli](https://github.com/getzazu/cli)

## Releasing

```bash
bin/release list        # last releases + what patch/minor/major would give
bin/release --dry-run   # version + changes since the last tag, publishes nothing
bin/release minor       # or patch (default), major, an explicit 0.3.0; --force re-creates
```
