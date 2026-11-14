# Changelog

All notable changes to `manza` (manza-rust, formerly `zazu-sdk` / zazu-rust) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0] - 2026-11-14

Ports manza-ruby 2.0.0: synced to the Manza API at getmanza/app `906ec486e5` (API version `2026-11-13`). Cassettes are pinned to `cassettes-v2.0.0`. A major release: see Migrating from 1.x below.

### Removed

- **Breaking:** `Invoices::create_payment_link` (`POST /api/invoices/:id/payment_link`). The API retired the endpoint: ask for the link with `"generate_payment_link": true` on `invoices().create` or `invoices().update` instead
- **Breaking:** the v1 transfer-authorization signer. `transfer_authorization::signature_input` builds only the v2 input (below)

### Changed

- **Breaking:** `transfer_authorization::SIGNATURE_VERSION` is `manza.transfer-authorization.v2`, and `signature_input` takes a `&SignatureFields` (`payment_id`, `nonce`, `amount`, `currency_code`, `account_id`, `payee`, `client_reference`, `transfer_method`) instead of seven positional arguments. `transfer_method` is appended after `client_reference` (`""` when `None`): `manza.transfer-authorization.v2|payment_id|nonce|amount|currency_code|account_id|payee|client_reference|transfer_method`. `SignatureFields` has no `Default`, so a call that leaves `transfer_method` out does not compile. The API verifies v2 for every request that is unpinned or sends `Manza-Version: 2026-11-13`; only `2026-03-27` still verifies v1. Pass the payment's `transfer_method` as the API returned it
- **Breaking:** a 422 whose `error_type` is `plan_limit_reached` is the new `ErrorKind::PlanLimitReached`, not `ErrorKind::Validation` (the API used to answer a plan cap with a 500, `ErrorKind::Server`). `ErrorKind` is not `#[non_exhaustive]`, so an exhaustive `match` on it needs the new arm. `ApiError` gains three public fields, so code that builds an `ApiError` with a struct literal must set them
- The example authorizer (`examples/transfer_authorizer`) records the create response's `transfer_method` in its ledger, checks it against the webhook's `payment.transfer_method` (a `null` matches an entry without one), signs v2, and compares its input with the webhook's `signature_input_v2`. `Ledger::record` takes a `&Record`. The preflight points at `trust_new_payees`
- Invoice docs: `account_id` is required on `invoices().create` (without one the API answers 422 `state_error` with no `param`; an account that is not the entity's, 422 with `param` `account_id`), and `invoices().update` takes it too. An invoice's `status` may be `declined`, and `invoices().delete` also takes a `declined` invoice that has no number

### Added

- `ErrorKind::PlanLimitReached` (`as_str()` `plan_limit_reached`; the other SDKs' `PlanLimitReachedError`, a validation subclass): a 422 whose `error_type` is `plan_limit_reached`. `ApiError::limit_kind` names what is capped (e.g. `invoices`), `limit_current` the count the entity holds and `limit_max` the cap; all three are `None` on every other error. Retrying does not help until the plan changes
- `generate_payment_link` on `invoices().create` and `invoices().update` (needs the `payment_links:write` scope; taken only while payment links are enabled for the entity). Invoice responses carry `generate_payment_link` and the embedded `account`
- `transfer_method` on `payments().create`: `eft` or `rtc` in South Africa, `standard` or `instant` in Morocco; one the market does not offer is an `ErrorKind::Validation` error with `param` `transfer_method`. Payment responses carry `transfer_method` (the rail sent, the provider's default when left out, `null` where the market offers no choice)
- Documented: the authorization `refusal_reason` `rail_limit_exceeded` (status `superseded`): an authorized transfer over its rail's limit at execution, sent to the approvers; the authorize call returns a 422 `ErrorKind::Validation` error (`state_error`). A non-boolean `generate_payment_link` is a 400 `ErrorKind::Validation` error
- Documented: the `payment.authorization_requested` webhook's `signature_input_v2` and `payment.transfer_method` (events stay `serde_json::Value`: read them from `data`), and `transfer_authorizer().get()`'s `trust_new_payees` and `new_payee_limit`

### Fixed (API side, no wire change)

- `accounts().list_transactions` and `accounts().get_transaction` return posted movements only (getmanza/app 818432457a); the SDK is unchanged

### Changed (cassettes)

Cassettes come from `cassettes-v2.0.0` (was `v1.3.0`): every cassette was re-recorded against staging. New replay tests mirror manza-ruby 2.0.0's specs:

| Cassette | Request |
|---|---|
| `invoices/create` | `POST /api/invoices` with `account_id`, `generate_payment_link: true`, `issue_date` `2026-11-14`, `due_date` `2026-12-14` |
| `invoices/create_invalid_generate_payment_link` (new) | `generate_payment_link: "yes please"`: 400 |
| `invoices/create_without_account` (new) | no `account_id`: 422 `state_error` |
| `invoices/update` | `PATCH /api/invoices/fixture-invoice-id` with `account_id`, `notes`, `generate_payment_link: true` |
| `payments/create` | `POST /api/payments` with `transfer_method: "standard"` |
| `payments/create_invalid_transfer_method` (new) | an unoffered `transfer_method`: 422, `param` `transfer_method` |

The authorize tests sign v2 over `transfer_method` `standard`; replay still matches their body without the scrubbed `signature`. `transfer_authorizer/get` asserts `trust_new_payees` and `new_payee_limit`.

### Migrating from 1.x

- **`create_payment_link` is gone.** Replace `client.invoices().create_payment_link(id, account_id)` with `client.invoices().update(id, &json!({ "account_id": account_id, "generate_payment_link": true }))`, or pass `"generate_payment_link": true` on create. `update` returns the invoice, not a link: the link is made from the invoice's account when the invoice is approved and appears in the invoice's `payment_link`. `update` takes only an invoice that is still `draft`, `pending_approval` or `declined` (any other is an `ErrorKind::Validation` error, `state_error`); for an invoice already approved or sent there is no API path to attach a link: `client.payment_links().create` makes a standalone one
- **`account_id` is required on invoice create.** Pass the account the invoice is paid into (`client.accounts().list`); 1.x calls without it now return an `ErrorKind::Validation` error
- **v2 signatures only.** Replace `signature_input(id, nonce, amount, currency, account, &payee, client_reference)` with `signature_input(&SignatureFields { payment_id: id, nonce, amount, currency_code: currency, account_id: account, payee: &payee, client_reference, transfer_method })`, where `transfer_method` is the payment's `transfer_method` from your record (the create response; `None` only when it is `null`), and compare with the webhook's `signature_input_v2` instead of `signature_input`. A client pinned to `Manza-Version: 2026-03-27` (`ClientBuilder::api_version` or `MANZA_API_VERSION`) is verified v1 and must stay on manza 1.x; unpin it, or pin `2026-11-13`, to move to 2.0
- **Plan caps.** A check for `ErrorKind::Server` around creates that may hit a plan cap no longer matches: check `ErrorKind::PlanLimitReached`, and do not retry it. It is not `ErrorKind::Validation`, so a `kind == ErrorKind::Validation` check does not catch it either

## [1.1.0]

Syncs the SDK with manza-ruby 1.3.0 and replays `cassettes-v1.3.0`.

### Added

- `manza::webhook` verifies incoming webhook deliveries: `verify` (a `bool`), `construct_event` (the parsed event, or `Error::Configuration` naming what failed) and `sign`. Reads `X-Manza-Signature` / `X-Manza-Timestamp` from any `(name, value)` pairs, names in any case, the CGI `HTTP_X_MANZA_*` form and the legacy `X-Zazu-*` names; compares in constant time and checks the timestamp within 5 minutes (`DEFAULT_TOLERANCE`; `None` skips it, for local replay only). `verify_at` / `construct_event_at` take a fixed `now`. Tested against the fixed vector every SDK shares
- `payments()` (`Payments`): `create`, `get`, `authorize` and `decline` on the API's canonical paths, `POST /api/payments`, `GET /api/payments/:id`, `POST /api/payments/:id/authorize` and `POST /api/payments/:id/decline`. The API renamed transfer drafts to payments; request and response bodies are unchanged, and so is the `transfer_authorization` signature input
- `transfer_authorizer().get()` (`GET /api/transfer_authorizer`): the enrolled authorizer's `status`, `suspension_reason`, limits, `daily_authorized_total`, `secret_expires_at` and `webhook_endpoint_id`; `ErrorKind::NotFound` when none is enrolled
- Documented the pass-through fields: a draft's `authorization` carries `refusal_reason` (status `refused` when it went to the approvers at create time), bank accounts carry `trusted`, and the challenge webhook's `payment.payee_trusted` says whether the payee was trusted already
- `examples/transfer_authorizer/`: three cargo examples, `transfer_authorizer` (a `tiny_http` server that verifies the webhook, checks a JSON ledger by `client_reference`, signs and authorizes or declines; `ALLOW_NEW_PAYEES=false` declines new payees; retries 5xx, 429 and 409 `authorization_busy`), `create_transfer` (creates a draft, records it, prints where it was routed and why) and `preflight` (checks every setup step and prints the fix). Their tests run under `cargo test`. Not shipped in the crate: `examples/` is excluded from the package and `tiny_http` stays a dev-dependency
- README: "Setup, step by step" for machine-authorized transfers, verifying the webhook, signing the challenge

### Changed

- Replay tests fetch `cassettes-v1.3.0` (pinned in `scripts/fetch-cassettes.sh`). The payment tests replay `payments/*` (was `transfer_drafts/*`); `payments/create` sends `amount` `"10.00"` and an `internal_notes`, and `beneficiaries/create` the `company_name` `"Manza Fixture Beneficiary spec"`, as recorded. New fixture ids: `MANZA_FIXTURE_REFUSED_DRAFT_ID` and the four `MANZA_FIXTURE_NEW_PAYEE_*`

### Deprecated

- `transfer_drafts()` and `TransferDrafts`. Both keep working for all of 1.x: the accessor returns `payments()` and the type is an alias of `Payments`, so the old name now calls `/api/payments` too (the server keeps `/api/transfer_drafts` as a permanent alias). Both are `#[deprecated]`, so using them warns at compile time

## [1.0.0]

Renamed from `zazu-sdk` to `manza` (1.0.0).

### Changed

- **Breaking:** the crate is now `manza` (was `zazu-sdk`) and the library is `manza` (was `zazu_sdk`); the repository is `getmanza/manza-rust`
- Error messages are prefixed `manza: ` (was `zazu: `)
- The version header is `Manza-Version` (was `Zazu-Version`) and the User-Agent is `manza-rust/<version>`
- Replay tests fetch `cassettes-v1.0.0` from `getmanza/manza-ruby`, pinned in `scripts/fetch-cassettes.sh`; fixture env vars are `MANZA_FIXTURE_*`

### Deprecated

- `ZAZU_API_KEY`, `ZAZU_BASE_URL` and `ZAZU_API_VERSION` are read as a fallback when the `MANZA_*` name is unset, with a one-time warning per variable on stderr. The fallback stays for all of 1.x

### Migrating from `zazu-sdk` 0.x

| Before | After |
|---|---|
| `zazu-sdk = "0.3"` | `manza = "1"` |
| `use zazu_sdk::...` | `use manza::...` |
| `ZAZU_API_KEY` / `ZAZU_BASE_URL` / `ZAZU_API_VERSION` | `MANZA_API_KEY` / `MANZA_BASE_URL` / `MANZA_API_VERSION` |

## [0.3.0]

Syncs the SDK with the API changes since 2026-07-16.

### Added

- `ErrorKind::Conflict` (409) with `ApiError::payment_id`, read from `error.payment_id` (the existing draft on a duplicate `client_reference`)
- `transfer_drafts().authorize(id, authorization_id, signature)` (a blank signature is refused locally) and `decline(id, authorization_id, reason)`
- `manza::transfer_authorization`: `signature_input`, `sign` (lowercase hex HMAC-SHA256) and `payee_for`, tested against the shared vectors
- `beneficiaries().create`, `list_external_accounts`, `get_external_account` and `create_external_account`
- `payee_trust_requests()` resource (`create`, `get`)
- `client_reference` documented on `transfer_drafts().create`; new checkout session and payment link fields documented

### Changed

- 400 responses now map to `ErrorKind::Validation` (previously `Api`)
- The default base URL is `https://ma.manza.finance` (use `https://za.manza.finance` for South Africa); the replay cassettes are recorded against `https://ma.manza.dev`
- Adds the `hmac` and `sha2` dependencies for the signer

## [0.2.1]

Version alignment: the whole SDK family now releases in lockstep with zazu-ruby. No functional changes since [0.1.0].

## [0.1.0]

Initial release.

### Added

- `zazu_sdk::Client` built on `ureq` (sync, dependency-light — no async runtime)
- Resources: `accounts`, `beneficiaries`, `checkout_sessions`, `customers`, `entity`, `invoices`, `payment_links`, `transfer_drafts`, `webhook_endpoints`
- Cursor-based `Page` with `next()` (max 100 records per page)
- `zazu_sdk::Error` mirroring the shared SDK error taxonomy
- Cassette-replay test harness driven by the Ruby SDK's release tarball
