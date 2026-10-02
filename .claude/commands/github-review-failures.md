---
description: "Use when CI checks are failing on a PR — fetches failure logs, diagnoses root causes, implements fixes, pushes until CI is green."
model: opus
argument-hint: "PR number (e.g., 12 or #12)"
allowed-tools: Bash(gh pr view:*), Bash(gh pr checks:*), Bash(gh pr diff:*), Bash(gh api:*), Bash(gh run view:*), Bash(git log:*), Bash(git diff:*), Bash(git push:*), Bash(git commit:*), Bash(git add:*), Bash(cargo:*), Bash(scripts/fetch-cassettes.sh:*), Bash(bin/release:*), Read, Write, Edit, Glob, Grep, Agent
---

# Fix GitHub CI Failures: $ARGUMENTS

Diagnose and fix CI failures. Work systematically: identify failures → read logs → diagnose root cause → fix locally → verify → push.

## Phase 0: Determine the PR

Number → PR. `#N` → strip `#`. Empty → current branch (`gh pr view --json number`).

## Phase 1: Inventory failures

```bash
gh pr checks <PR>
```

For each failing check, get the run id and load the failed logs:

```bash
gh run view <run-id> --log-failed
```

Categorize:
- **Test failures** — assertion failed, timeout
- **Format failures** — `cargo fmt --check` found a diff
- **Lint / compile failures** — `cargo clippy --all-targets -- -D warnings` (any warning fails CI)
- **Cassette fetch failures** — `scripts/fetch-cassettes.sh` could not download the tarball
- **Cassette replay failures** — `ReplayServer` answered 501 "no cassette interaction matches"
- **Release / publish failures** — tag/version mismatch, crates.io trusted publishing (`crates-io` environment)

## Phase 2: Diagnose

Read the actual error message, not the surrounding noise. The first stacktrace line that points at our code is usually the culprit.

For each failure:

### Reproduce locally

```bash
# Cassettes
scripts/fetch-cassettes.sh

# Test
cargo test --test resources <name>

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --check

# Full pipeline
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

If you can't reproduce locally, the failure is environmental (CI-only):
- Different Rust toolchain → CI uses `dtolnay/rust-toolchain@stable`, so a new stable can add clippy lints; run `rustup update stable` locally
- Stale cassettes → locally re-run `scripts/fetch-cassettes.sh`; CI fetches the manza-ruby release pinned in the script
- Race condition → re-running the job fixes it
- Network → external service (GitHub release download, crates.io) hiccup
- Secret missing → e.g. the `crates-io` environment or trusted-publishing binding not configured

### Find the root cause

Apply the five-whys ladder until you reach a fix point that prevents the same class of failure recurring. Don't:

- Disable the failing test
- Add `#[allow(...)]` to silence clippy
- Reach for `unwrap()` or `as` casts to bypass the compiler
- Add a dependency to paper over a missing import

These hide the failure; the underlying bug returns elsewhere.

## Phase 3: Fix and verify

### 3.1 Implement the fix

Touch only what the failure cites, plus what the fix requires.

### 3.2 Run the equivalent local check

The CI step that failed has a local equivalent — run it, get green:

| CI step | Local equivalent |
|---|---|
| Fetch cassettes | `scripts/fetch-cassettes.sh` |
| `cargo fmt --check` | `cargo fmt --check` (fix with `cargo fmt`) |
| `cargo clippy --all-targets -- -D warnings` | the same command |
| `cargo test` | `cargo test` |
| Publish to crates.io | requires OIDC — cannot run locally; verify via the release workflow |

Never reproduce a failure by calling a live Manza API. Replay failures are fixed in the SDK or by a new manza-ruby cassette release.

### 3.3 Run the full pipeline

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### 3.4 Commit + push

```bash
git add <files>
git commit -m "fix(ci): <what was failing>

<root cause and how this addresses it>"
git push origin <branch>
```

Use `fix:` for prod fixes, `chore(ci):` for workflow / config changes.

## Phase 4: Watch the next run

```bash
gh pr checks <PR> --watch
# or
gh run watch <run-id> --exit-status
```

Track until green. If the same step fails again with a different error, repeat. If it fails the same way, your fix is wrong — revert and rethink.

## Phase 5: Verify and document

```bash
gh pr checks <PR>            # all green
gh pr view <PR> --json mergeable,reviewDecision
```

If the failure was CI-config drift (workflow YAML out of sync with reality), also update relevant docs:
- `.github/workflows/ci.yml` and `release.yml` (keep them in step with `scripts/release-check`)
- `CLAUDE.md` if a convention changed

## Common patterns and fixes

### Replay says "no cassette interaction matches"

`ReplayServer` (`tests/common/mod.rs`) matches method + path + sorted query + semantic JSON body, and ignores the recorded host. The 501 message prints the method, URL and body the SDK sent. Either:
- The SDK now sends a different shape than the cassette recorded: fix the SDK, or have manza-ruby re-record and release.
- Two cassettes were loaded for one test: `transfer_drafts/authorize` vs `authorize_same_key` and `create` vs `create_duplicate` share method + URI. Load one cassette per test.
- An id placeholder drifted: the `FIXTURE_IDS` table must stay identical to manza-ruby's `spec/support/fixture_ids.rb`.
- An authorize test used `ReplayServer::start` instead of `start_ignoring_signature` (the recorded HMAC cannot be reproduced).

### `read cassette ...: No such file`

Run `scripts/fetch-cassettes.sh`. `testdata/cassettes/` is git-ignored and extracted from the manza-ruby release pinned in the script (`cassettes-vX.Y.Z.tar.gz`). A new cassette name that is missing means manza-ruby has not released it yet.

### `cargo publish` / trusted publishing failed

The trusted-publisher binding on crates.io (crate `manza`, Settings, Trusted Publishing) must name `getmanza/manza-rust`, workflow `release.yml`, environment `crates-io`. A stale `getmanza/manza-rust` binding fails the OIDC exchange. Also check that the tag equals the `Cargo.toml` version (the `Verify tag matches crate version` step).

## Karpathy guidelines

- **Think before coding** — read the actual error, don't pattern-match on the first guess.
- **Goal-driven execution** — the green CI check is the verification.
- **Surgical changes** — fix the failing class of error, not adjacent things.
