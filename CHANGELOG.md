# Changelog

All notable changes to `zazu-sdk` (zazu-rust) are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Syncs the SDK with the API changes since 2026-07-16.

### Added

- `ErrorKind::Conflict` (409) with `ApiError::payment_id`, read from `error.payment_id` (the existing draft on a duplicate `client_reference`)
- `transfer_drafts().authorize(id, authorization_id, signature)` (a blank signature is refused locally) and `decline(id, authorization_id, reason)`
- `zazu_sdk::transfer_authorization`: `signature_input`, `sign` (lowercase hex HMAC-SHA256) and `payee_for`, tested against the shared vectors
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
