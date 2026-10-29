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

// Initiate a transfer: your enrolled transfer authorizer or your in-app
// approvers authorize it; the API never executes a transfer by itself.
let draft = client.payments().create(&json!({
    "account_id": account_id,
    "beneficiary_id": beneficiary_id,
    "amount": "150.00",
    "payment_reference": "INV-000042",
    "client_reference": "po_1042", // unique per entity; a duplicate is ErrorKind::Conflict
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
methods; an external account's `trusted` says whether it is a trusted payee),
`checkout_sessions`, `customers`, `entity`, `invoices`,
`payee_trust_requests`, `payment_links`, `payments` (`create`, `get`,
`authorize`, `decline`; a draft's `authorization` carries its `status` and
`refusal_reason`), `transfer_authorizer` (`get`: `status`,
`suspension_reason`, limits, `daily_authorized_total`; `ErrorKind::NotFound`
when none is enrolled) and `webhook_endpoints`.

`transfer_drafts()` is a deprecated alias of `payments()`: same methods, and
it calls `/api/payments` too. It stays for all of 1.x.

## Machine-authorized transfers

A draft inside your entity's authorization envelope (trusted payee, or a new
one when you allow it, within limits) is sent to your enrolled authorizer
endpoint as a `payment.authorization_requested` webhook carrying an
`authorization.id` and a one-time `nonce`. Your system checks it against its
own ledger, signs it and authorizes it with a **different API key** from the
one that created it. [`examples/transfer_authorizer/`](examples/transfer_authorizer/)
runs the whole flow. The [transfer authorization docs](https://ma.manza.finance/docs/api/transfer-authorization)
cover every step below.

### Setup, step by step

1. **Feature flags.** Ask support to enable API transfers and transfer authorization (`release_api_webhook_authorization`) on your workspace.
2. **Two API keys.** Key A creates drafts (`transfers:write`; `beneficiaries:request_trust` for step 6; `entity:read`, `accounts:read`, `webhook_endpoints:read` and `beneficiaries:read` for the preflight in step 7). Key B answers the challenges (`transfers:authorize`). The creating key is always refused with 403 `same_key_forbidden`, so keep key B and the signing secret on the system that holds your ledger. Bind key B to an IP allowlist: authorizing from a key without one is refused with 403 `unrestricted_api_key`. Bind key A to one too if you allow new payees (step 6): a draft to a new payee from a key without an allowlist goes to your approvers (`unrestricted_api_key`).
3. **A stable public URL.** Your authorizer needs an HTTPS URL that never changes: an enrolled endpoint's URL is locked. Use a reserved ngrok domain (`ngrok http --url=your-name.ngrok.app 4599`) or a named Cloudflare tunnel. Never a free ngrok URL: it changes on every restart.
4. **Create the webhook endpoint** at that URL:

   ```rust
   client.webhook_endpoints().create(&json!({
       "url": "https://your-name.ngrok.app/",
       "events": ["transfer.executed"],
   }))?;
   ```

5. **Enrol it in the app**: Settings → Developer → Transfer approval. Pick the endpoint, the currency and both limits, confirm with your two-factor code, and copy the signing secret: it is shown once.
6. **Allow new payees, or trust each payee.** Turn on *Approve transfers to new payees* at enrolment (or later, same page, with your two-factor code) and set the *Limit per transfer to a new payee*. Your authorizer may then approve a transfer to bank details your workspace never trusted, up to that limit, and its signature trusts them: the webhook's `payment.payee_trusted` is `false` for such a transfer. Bank details whose trust was revoked, or that were edited after trusting, still go to your approvers. Without the option, an approver marks the beneficiary's bank account as trusted from the Beneficiaries list, or you ask them to:

   ```rust
   client.payee_trust_requests().create(&[external_account_id])?;
   ```

   `trusted` on `client.beneficiaries().get_external_account(...)` says which it is. The API does not show whether the option is on.
7. **Run the preflight, then the example** (after filling in `examples/transfer_authorizer/.env` from its `.env.example`): `cargo run --example preflight` checks what the API shows of steps 1–6 and prints a fix for each that fails. What it cannot see, check by hand: that key B is bound to an IP allowlist, and, for a payee not trusted yet, that *Approve transfers to new payees* is on and key A is bound to an IP allowlist too (it warns, it does not fail). Then `cargo run --example transfer_authorizer` in one terminal, `cargo run --example create_transfer` in another.
8. **Troubleshooting**: a draft that goes to your approvers instead has an `authorization` with status `refused` and a `refusal_reason` (`create_transfer` prints it in words); `client.transfer_authorizer().get()` shows a suspension and its `suspension_reason`. An authorize answered 409 `authorization_busy` (a bank-detail edit, trust or trust revocation on the same payee was in flight) is safe to retry. The reasons, and an API error's `error_type`, are in the [transfer authorization docs](https://ma.manza.finance/docs/api/transfer-authorization).

### Verifying the webhook

Pass the **raw request body**, never re-serialized JSON. `construct_event`
reads `X-Manza-Signature` and `X-Manza-Timestamp` (names in any case, the CGI
`HTTP_X_MANZA_*` form, or the legacy `X-Zazu-*` names) from any iterator of
`(name, value)` string pairs, checks the signature in constant time and the
timestamp within 5 minutes, and returns the parsed event (a
`serde_json::Value` object). It fails with `Error::Configuration` otherwise:

```rust
use manza::webhook::{construct_event, DEFAULT_TOLERANCE};

// `headers` is an http::HeaderMap (axum, hyper, ...). Any iterator of
// (name, value) string pairs works, a &HashMap<String, String> too.
let pairs = headers
    .iter()
    .map(|(name, value)| (name.as_str(), value.to_str().unwrap_or("")));
let event = match construct_event(&raw_body, pairs, signing_secret, Some(DEFAULT_TOLERANCE)) {
    Ok(event) => event,
    Err(manza::Error::Configuration(reason)) => return respond(401, &reason),
    Err(other) => return respond(401, &other.to_string()),
};
```

`manza::webhook::verify(payload, signature, timestamp, secret, Some(DEFAULT_TOLERANCE))`
returns a `bool` instead. Both accept `None` as the tolerance to skip the
timestamp check. Use it only to replay a captured delivery in a local tool or
test, never on a live endpoint: without the check, anyone holding a captured
delivery can replay it. `webhook::sign` signs a payload, for tests.

### Signing the challenge

Sign the draft from **your own record** of it with the endpoint's signing
secret, never from the webhook's `signature_input` (it is there only to compare
against):

```rust
use manza::transfer_authorization::{payee_for, sign, signature_input};

let payee = payee_for(Some(external_account_id), None)?;
let input = signature_input(
    draft_id,
    nonce,         // webhook data.authorization.nonce
    "2500.0",      // the API's decimal string, verbatim
    "MAD",
    account_id,
    &payee,
    Some("po_1"),  // client_reference, None when the draft has none
);
let signature = sign(signing_secret, &input);

let authorizer = manza::Client::builder().api_key(authorizer_api_key).build()?;
authorizer.payments().authorize(draft_id, authorization_id, &signature)?;
// or: authorizer.payments().decline(draft_id, authorization_id, Some("Not ours"))?;
```

A wrong signature is `ErrorKind::Validation` (`error_type`
`invalid_signature`). Five on one challenge send the draft to your in-app
approvers; five in a row suspend the authorizer. `authorize` refuses a blank
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

| SDK | Repository | Install |
|---|---|---|
| Ruby (reference implementation, records the cassettes) | [getmanza/manza-ruby](https://github.com/getmanza/manza-ruby) | `gem "manza"` |
| TypeScript / JavaScript | [getmanza/manza-ts](https://github.com/getmanza/manza-ts) | `npm install @getmanza/sdk` |
| Python | [getmanza/manza-python](https://github.com/getmanza/manza-python) | `pip install manza` |
| Go | [getmanza/manza-go](https://github.com/getmanza/manza-go) | `go get github.com/getmanza/manza-go` |
| PHP | [getmanza/manza-php](https://github.com/getmanza/manza-php) | `composer require manza/manza-php` |
| Rust | [getmanza/manza-rust](https://github.com/getmanza/manza-rust) (this repo) | `cargo add manza` |
| Crystal | [getmanza/manza-crystal](https://github.com/getmanza/manza-crystal) | shard `manza` (`github: getmanza/manza-crystal`) |
| Elixir | [getmanza/manza-elixir](https://github.com/getmanza/manza-elixir) | `{:manza, "~> 1.0"}` |
| CLI | [getmanza/cli](https://github.com/getmanza/cli) | `npm install -g @getzazu/cli` or `brew install getmanza/tap/zazu` |

## Releasing

```bash
bin/release list        # last releases + what patch/minor/major would give
bin/release --dry-run   # version + changes since the last tag, publishes nothing
bin/release minor       # or patch (default), major, an explicit 0.3.0; --force re-creates
```
