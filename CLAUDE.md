# CLAUDE.md

## Models

Sessions run on `opus` (Opus 5.5) with `fable` (Fable 5.1) as the advisor (`.claude/settings.json`). Fable is spent where judgment matters most: ask for a plan on Fable (a `fable` subagent or `/model fable`); plan mode itself runs on Opus and asks the advisor. The advisor is consulted at decision points (before choosing an approach, a schema or public API, a migration, a dependency, anything irreversible, and when a failure repeats). Run the `fable-validator` agent on a finished implementation before its pull request opens (no command in this repo runs it for you). Agents pin their tier by alias, never by full model ID: `fable` for plans and validation; `opus` for orchestration, security, full PR review, payments and production debugging; `sonnet` for the implementation specialists and TDD; `haiku` for mechanical scans. Every spawned agent names its `model:`; a subagent whose definition names no model runs on `sonnet` (`CLAUDE_CODE_SUBAGENT_MODEL`), never on the session's model.

## Release

```bash
bin/release list        # last releases + what patch/minor/major would give
bin/release --dry-run   # version + changes since the last tag, publishes nothing
bin/release minor       # or patch (default), major, an explicit 0.3.0; --force re-creates
```

Run from a clean, up-to-date main. `bin/release` is the zazu SDK release kit (byte-identical across SDK repos, never edited in place); the repo-specific parts are `scripts/version` (Cargo.toml, the `zazu-sdk` entry in Cargo.lock, `VERSION` in `src/client.rs`) and `scripts/release-check` (fmt, clippy, tests, as in CI). It bumps, runs the checks, pushes main and publishes the GitHub Release; the tag fires `release.yml`, which checks the tag against the Cargo.toml version and publishes to crates.io.
