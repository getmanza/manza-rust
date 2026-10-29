# Changelog

All notable changes to `manza` (manza-rust, formerly `zazu-sdk` / zazu-rust) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
