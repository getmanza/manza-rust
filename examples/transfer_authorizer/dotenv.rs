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
    for (key, value) in assignments(&contents) {
        std::env::set_var(key, value);
    }
}

/// The `KEY=value` pairs of a `.env` file. Skips what `set_var` would panic
/// on: an empty key, or a NUL in the key or value.
fn assignments(contents: &str) -> Vec<(&str, &str)> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim().trim_matches('"')))
        .filter(|(key, value)| !key.is_empty() && !key.contains('\0') && !value.contains('\0'))
        .collect()
}

/// The variable's value, or `""` when unset.
pub fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn skips_assignments_set_var_would_panic_on() {
        let parsed = super::assignments("=value\nA\0B=1\nC=x\0y\n# note\n\nGOOD=\"yes\"\n");
        assert_eq!(parsed, vec![("GOOD", "yes")]);
    }
}
