//! Loads `examples/transfer_authorizer/.env` into the process environment,
//! overriding variables already set (a stale shell export never masks the
//! file). `KEY=value` lines; blank lines and `#` comments are skipped.

use std::path::PathBuf;

/// The example's directory, whatever directory `cargo run` starts in.
pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/transfer_authorizer")
}

pub fn env_path() -> PathBuf {
    dir().join(".env")
}

pub fn load() {
    let Ok(contents) = std::fs::read_to_string(env_path()) else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"');
            std::env::set_var(key.trim(), value);
        }
    }
}

/// The variable's value, or `""` when unset.
pub fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}
