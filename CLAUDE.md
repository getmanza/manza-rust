# manza-rust

Rust SDK for the Manza API (crate `manza`, library `manza`). This SDK **replays manza-ruby's cassettes**: manza-ruby is the reference implementation, records them against `https://ma.manza.dev`, and ships them as a release tarball. Every other SDK (manza-ts, the CLI, manza-python, manza-go, ...) replays the same cassettes, so wire behavior is identical across the family.

## Stack

| Concern | Tool | Notes |
|---|---|---|
| Language | Rust 2021 edition, `stable` toolchain | CI uses `dtolnay/rust-toolchain@stable` with rustfmt + clippy. No version matrix, no MSRV pinned |
| HTTP | `ureq` 3 (blocking) | `src/client.rs`. Synchronous API, no async runtime |
| Bodies | `serde_json::Value` | Untyped, snake_case, no struct mapping |
| Signer | `hmac` + `sha2` | `src/transfer_authorization.rs` |
| Test runner | `cargo test` | Integration tests in `tests/` |
| Cassette replay (tests) | `tiny_http` + `serde_yaml` | `ReplayServer` in `tests/common/mod.rs`; reads `testdata/cassettes/` (git-ignored) |
| Format / lint | `cargo fmt`, `cargo clippy --all-targets -- -D warnings` | Any clippy warning fails CI. Type-checking is the compiler |
| Registry | crates.io `manza` | https://crates.io/crates/manza |
| Release | `bin/release` | manza SDK release kit; OIDC trusted publishing via `rust-lang/crates-io-auth-action`, environment `crates-io` |

## Public API surface

```rust
use serde_json::json;
use manza::{ErrorKind, Error};
use manza::transfer_authorization::{payee_for, sign, signature_input};

let client = manza::Client::builder().api_key("sk_live_...").build()?;

client.entity().get()?;
let page = client.accounts().list(Default::default())?;      // Page { data, has_more, next_cursor, .. }
if let Some(next) = page.next()? { /* following page */ }

client.beneficiaries().create(&json!({ /* ... */ }))?;
client.beneficiaries().list_external_accounts(beneficiary_id, Default::default())?;
client.beneficiaries().create_external_account(beneficiary_id, &json!({ /* ... */ }))?;
client.payee_trust_requests().create(&[external_account_id])?;

match client.transfer_drafts().create(&json!({ "client_reference": "po_1", /* ... */ })) {
    Err(Error::Api(e)) if e.kind == ErrorKind::Conflict => {
        let existing = e.payment_id;                          // 409 duplicate_client_reference
    }
    other => { other?; }
}

// Machine authorization (0.3.0): sign from YOUR record of the transfer, never the webhook's signature_input
let payee = payee_for(Some(external_account_id), None)?;
let input = signature_input(draft_id, nonce, "2500.0", "MAD", account_id, &payee, Some("po_1"));
let signature = sign(signing_secret, &input);
authorizer.transfer_drafts().authorize(draft_id, authorization_id, &signature)?;
authorizer.transfer_drafts().decline(draft_id, authorization_id, Some("reason"))?;
```

- Ten resources: `accounts`, `beneficiaries`, `checkout_sessions`, `customers`, `entity`, `invoices`, `payee_trust_requests`, `payment_links`, `transfer_drafts`, `webhook_endpoints`
- `Page` (`src/page.rs`): cursor-based, `Page::next()`, hard cap of 100 per page (`MAX_PER_PAGE`); `ListParams { limit, cursor }`
- Errors (`src/error.rs`): `Error` is `Api(Box<ApiError>)`, `Configuration(String)` or `Connection(String)`. `ApiError` carries `status`, `kind: ErrorKind`, `error_type`, `message`, `param`, `payment_id`, `request_id`, `retry_after`, `body`. Discriminate on `ErrorKind` (`Authentication` 401, `Forbidden` 403, `NotFound` 404, `Conflict` 409, `Validation` 400/422, `RateLimit` 429, `Server` 5xx, `Api` other), never on status codes or `message`. `ErrorKind` is not `#[non_exhaustive]`: adding a variant breaks downstream exhaustive matches, so it is a major-version change
- `authorize` refuses a blank signature locally (`Error::Configuration`), because the API counts it as a failed attempt
- Snake-case wire format: request and response bodies are returned as-is. **No auto-camelCasing, no typed structs.**

## How to work in this codebase

1. **Tests come first.** Every change to `src/` ships with a test. Cassette-replay tests are the contract: they enforce the same wire format across Ruby, TS, Rust and future SDKs.
2. **Use the SDK's primitives.** `Client`'s `get`/`post`/`patch`/`delete` helpers and resource accessors, `Page` / `list_page`, `Error::Api` + `ErrorKind`, `encode_component` for path segments and query values, `ReplayServer` + `fixture_id()` in tests. Don't hand-roll `ureq` calls, string-interpolate ids into URLs, or parse `error.message`.
3. **Snake-case stays.** Response keys are wire-format. We don't camelCase them or map them to structs.
4. **fmt and clippy must be clean.** `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` gate CI. Don't add `#[allow(...)]` to silence a lint: fix the issue.

## Critical rules

- **Never call a live Manza API** from tests, scripts or Claude sessions. Tests replay manza-ruby's cassettes only. Live staging calls create real transfers and approval requests for the team. Only manza-ruby records cassettes.
- **Cassette contract.**
  - Cassettes come from the manza-ruby release pinned in `scripts/fetch-cassettes.sh` (`cassettes-vX.Y.Z.tar.gz`, currently `v1.0.0`), extracted by that script, extracted to `testdata/cassettes/`.
  - They are recorded against `https://ma.manza.dev`. The harness ignores the recorded host and serves from a local `tiny_http` server.
  - Load **one cassette per test**: `transfer_drafts/authorize` vs `authorize_same_key`, and `transfer_drafts/create` vs `create_duplicate`, share method + URI, so loading both makes the first match win.
  - The three authorize cassettes match the request body minus `signature` (`ReplayServer::start_ignoring_signature`), because the recorded HMAC cannot be reproduced.
  - Every other body is matched semantically: method, path, sorted query, and JSON equality (parsed `serde_json::Value`, key order ignored; non-JSON bodies compare byte for byte). `ReplayServer::start`.
  - The harness serves the recorded status and body with a `Content-Type` header only; recorded response headers (cassette responses carry no `Content-Length`) are not replayed.
  - The `FIXTURE_IDS` table in `tests/common/mod.rs` must stay identical to manza-ruby's `spec/support/fixture_ids.rb`. Tests call `fixture_id("MANZA_FIXTURE_X")`, which panics on an unknown name: add it to both repos together.
- **Hosts.** Default `https://ma.manza.finance`; South Africa `https://za.manza.finance`; staging and cassettes `https://ma.manza.dev`. Env vars are `MANZA_API_KEY`, `MANZA_BASE_URL`, `MANZA_API_VERSION`; the deprecated `ZAZU_*` names are read as a fallback (one `manza:` warning on stderr per variable) for all of 1.x. `src/env.rs` owns the lookup.
- **Error model is shared across the SDK family.** Adding an error class or kind means coordinating manza-ruby and manza-ts at minimum. The 10th is the conflict (409), shipped in 0.3.0 as `ErrorKind::Conflict`.
- **Signer.** `transfer_authorization` must keep reproducing the two fixed vectors from manza-ruby's `spec/manza/transfer_authorization_spec.rb` (asserted in `tests/transfer_authorization.rs`). Never sign the server's `signature_input` blindly: build it from your own record of the transfer; the webhook's copy is only for comparison.
- **Release.** `bin/release` is byte-identical across the SDK repos and never edited in place. Repo-specific logic lives in `scripts/version` and `scripts/release-check`. `release.yml` gates on tag == `Cargo.toml` version, then publishes to crates.io. No long-lived `CARGO_REGISTRY_TOKEN`: publishing uses crates.io trusted publishing through the `crates-io` GitHub environment, and the binding (crate `manza` settings on crates.io) must name `getmanza/manza-rust`, workflow `release.yml`, environment `crates-io`.
- **Renamed from zazu.** The crate was `zazu-sdk` (lib `zazu_sdk`) before 1.0.0; the repo was `zazu-rust`. Remotes and URLs must say `getmanza/manza-rust`.
- **`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` before every commit.** CI runs the same.
- **Never escape backticks in PR bodies.** With `<<'EOF'` (single-quoted heredoc) the shell passes everything through verbatim. Typing `` \` `` produces literal `` \` `` in the rendered PR. See "PR descriptions" below.

## PR descriptions

Write PR description bodies in plain Markdown. **Do not escape backticks** with `` \` `` — GitHub renders `` \` `` literally as a backslash followed by a backtick, producing output like `` \`Page\` `` instead of the monospace `Page` the reader expects.

The usual cause is writing the description inside a bash heredoc (`gh pr create --body "$(cat <<'EOF' ... EOF)"`) and then reflexively escaping every backtick because of shell-quoting muscle memory. With `<<'EOF'` (single-quoted delimiter) the shell does NOT interpret anything inside the heredoc — backticks, dollars, and backslashes all pass through verbatim. So write them exactly as you want them rendered:

```bash
# Good — renders as `Page` in monospace
gh pr create --body "$(cat <<'EOF'
Uses the `Page` helper.
EOF
)"

# Bad — renders as \`Page\` literally in the PR body
gh pr create --body "$(cat <<'EOF'
Uses the \`Page\` helper.
EOF
)"
```

Same rule for code blocks — write triple-backticks unescaped. The single-quoted heredoc delimiter is doing all the shell-escaping work. If you find yourself typing `` \` `` inside a PR body, stop and remove the backslash.

## Striving for excellence

These are the Karpathy guidelines we apply on every change. They reduce common LLM coding mistakes.

### 1. Think before coding

Don't assume. Don't hide confusion. Surface tradeoffs.

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity first

Minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Senior engineer test: would they call this overcomplicated?

### 3. Surgical changes

Touch only what you must. Clean up only your own mess.

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.
- Remove imports/variables/methods that *your* changes orphaned. Don't remove pre-existing dead code unless asked.

### 4. Goal-driven execution

Define success criteria. Loop until verified.

- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan with verification at each step.

## Development workflow

```bash
# One-time setup (the exact steps CI runs)
scripts/fetch-cassettes.sh            # the pinned manza-ruby release; or scripts/fetch-cassettes.sh v1.0.1

# Daily loop
cargo test --test resources transfer_drafts_authorize   # while iterating
cargo fmt                                               # auto-format

# Before commit (CI: fmt, clippy, test)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Release (after PR merge, from a clean, up-to-date main)
bin/release list        # last releases + what patch/minor/major would give
bin/release --dry-run   # version + changes since the last tag, publishes nothing
bin/release minor       # or patch (default), major, an explicit 0.3.0; --force re-creates
# -> scripts/version bumps Cargo.toml, the manza entry in Cargo.lock and VERSION in src/client.rs
# -> scripts/release-check runs fetch-cassettes, fmt, clippy and tests, then main is pushed and the GitHub Release is created
# -> the tag fires release.yml: test gate, tag == version check, crates.io publish (OIDC)
```

## Models

Sessions run on `opus` (Opus 5.5) with `fable` (Fable 5.1) as the advisor (`.claude/settings.json`). Fable is spent where judgment matters most: plans are written on Fable; plan mode itself runs on Opus and asks the advisor. The advisor is consulted at decision points (before choosing an approach, a schema or public API, a migration, a dependency, anything irreversible, and when a failure repeats). The `fable-validator` agent checks every finished implementation before its pull request opens (`/lfg`, Phase 6.5). Agents pin their tier by alias, never by full model ID: `fable` for plans and validation; `opus` for orchestration, security, full PR review, payments and production debugging; `sonnet` for the implementation specialists and TDD; `haiku` for mechanical scans. Every spawned agent names its `model:`; a subagent whose definition names no model runs on `sonnet` (`CLAUDE_CODE_SUBAGENT_MODEL`), never on the session's model.

## Slash commands

These live in `.claude/commands/` and are available in any Claude Code session:

| Command | When |
|---|---|
| `/lfg <issue or feature>` | Full autonomous workflow with TDD + verification |
| `/github-review-pr <PR#>` | Full PR review pass — failures first, then comments |
| `/github-review-failures <PR#>` | Just fix CI failures on a PR |
| `/github-review-comments <PR#>` | Just respond to reviewer comments on a PR |
| `/coderabbit-review <PR#>` | Specifically address CodeRabbit findings (verify, fix valid, push back on stale/wrong) |

## Cross-SDK contract

`manza-ruby` is the reference implementation:

- Records cassettes against `https://ma.manza.dev`
- Ships them as a release tarball (`cassettes-vX.Y.Z.tar.gz`) on each version
- All other SDKs (`manza-ts`, future `manza-python`, `manza-go`, `manza-php`, `manza-crystal`, `manza-elixir`, and this one) replay these cassettes in their own test harness

If the contract breaks (e.g., new request shape, new error kind), it's a coordinated change across at least two repos: manza-ruby and manza-ts.

## Repository links

- Ruby SDK (reference): https://github.com/getmanza/manza-ruby
- This repo: https://github.com/getmanza/manza-rust
- crates.io: https://crates.io/crates/manza
- TypeScript SDK: https://github.com/getmanza/manza-ts
