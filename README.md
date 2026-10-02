# zazu-rust

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

## Response shape

Response bodies are returned as-is from the API — `snake_case` keys in an
untyped `serde_json::Value`, no struct mapping. The same shape ships across
every Zazu SDK (Ruby, TypeScript, Python, Go, Rust, ...) so the cassette
contract is one-to-one.

## Errors

Non-2xx responses come back as `zazu_sdk::Error::Api` carrying `status`,
`kind` (`authentication`, `forbidden`, `not_found`, `validation`,
`rate_limit`, `server`, `api`), the API's `error_type`/`message`/`param`,
the `request_id`, and `retry_after` for 429s. Transport failures are
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
