# manza-rust

Rust SDK for the [Manza](https://get-manza.com) API.

```toml
# Cargo.toml
[dependencies]
manza = "1"
```

```rust
use serde_json::json;

// Reads MANZA_API_KEY (falling back to the deprecated ZAZU_API_KEY).
let client = manza::Client::new()?;

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
`MANZA_BASE_URL`.

## Environment variables

`MANZA_API_KEY`, `MANZA_BASE_URL` and `MANZA_API_VERSION`. The old `ZAZU_*`
names still work throughout 1.x: the SDK prints a one-time deprecation warning
per variable on stderr when it falls back to one. `MANZA_*` wins when both
are set.

## Migrating from `zazu-sdk`

| Before (`zazu-sdk` 0.x) | After (`manza` 1.x) |
|---|---|
| `zazu-sdk = "0.3"` | `manza = "1"` |
| `use zazu_sdk::...` | `use manza::...` |
| `zazu: ...` error messages | `manza: ...` |
| `ZAZU_API_KEY`, `ZAZU_BASE_URL`, `ZAZU_API_VERSION` | `MANZA_API_KEY`, `MANZA_BASE_URL`, `MANZA_API_VERSION` |
| `Zazu-Version` request header, `zazu-rust/x` User-Agent | `Manza-Version`, `manza-rust/x` |

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
use manza::transfer_authorization::{payee_for, sign, signature_input};

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
every Manza SDK (Ruby, TypeScript, Python, Go, Rust, ...) so the cassette
contract is one-to-one.

## Errors

Non-2xx responses come back as `manza::Error::Api` carrying `status`,
`kind` (`authentication`, `forbidden`, `not_found`, `conflict`, `validation`,
`rate_limit`, `server`, `api`), the API's `error_type`/`message`/`param`,
the `payment_id` (on a 409 `duplicate_client_reference`), the `request_id`, and `retry_after` for 429s. Transport failures are
`Error::Connection`; client-build and invalid-argument failures are
`Error::Configuration`.

## Tests

Tests replay the canonical cassettes recorded by
[manza-ruby](https://github.com/getmanza/manza-ruby). The cassettes are
downloaded from the Ruby SDK's release tarball and served from a local
`tiny_http` replay server. Same interactions, same assertions, every
language.

```bash
scripts/fetch-cassettes.sh
cargo test
```

## The SDK family

- [manza-ruby](https://github.com/getmanza/manza-ruby) — reference implementation (records the cassettes)
- [manza-ts](https://github.com/getmanza/manza-ts)
- [manza-python](https://github.com/getmanza/manza-python)
- [manza-go](https://github.com/getmanza/manza-go)
- [cli](https://github.com/getmanza/cli)

## Releasing

```bash
bin/release list        # last releases + what patch/minor/major would give
bin/release --dry-run   # version + changes since the last tag, publishes nothing
bin/release minor       # or patch (default), major, an explicit 0.3.0; --force re-creates
```
