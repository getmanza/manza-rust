//! Environment lookups with a deprecated `ZAZU_*` fallback.

use std::collections::HashSet;
use std::sync::Mutex;

static WARNED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Reads `new`, falling back to the deprecated `old` name. The fallback
/// prints one deprecation warning per variable for the life of the process.
pub(crate) fn env_setting(new: &str, old: &str) -> Option<String> {
    lookup(new, old, &WARNED, &|message| eprintln!("{message}"))
}

fn lookup(
    new: &str,
    old: &str,
    warned: &Mutex<Option<HashSet<String>>>,
    warn: &dyn Fn(String),
) -> Option<String> {
    if let Some(value) = env_non_empty(new) {
        return Some(value);
    }
    let value = env_non_empty(old)?;
    let first = warned
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(old.to_owned());
    if first {
        warn(format!(
            "manza: the {old} environment variable is deprecated; use {new} instead"
        ));
    }
    Some(value)
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn run(
        new: &str,
        old: &str,
        warned: &Mutex<Option<HashSet<String>>>,
    ) -> (Option<String>, Vec<String>) {
        let messages = RefCell::new(Vec::new());
        let value = lookup(new, old, warned, &|m| messages.borrow_mut().push(m));
        (value, messages.into_inner())
    }

    #[test]
    fn prefers_the_manza_name_without_warning() {
        std::env::set_var("MANZA_T_PREFER", "new");
        std::env::set_var("ZAZU_T_PREFER", "old");
        let (value, warnings) = run("MANZA_T_PREFER", "ZAZU_T_PREFER", &Mutex::new(None));
        assert_eq!(value.as_deref(), Some("new"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn falls_back_to_the_zazu_name_with_a_warning() {
        std::env::remove_var("MANZA_T_FALLBACK");
        std::env::set_var("ZAZU_T_FALLBACK", "old");
        let (value, warnings) = run("MANZA_T_FALLBACK", "ZAZU_T_FALLBACK", &Mutex::new(None));
        assert_eq!(value.as_deref(), Some("old"));
        assert_eq!(
            warnings,
            ["manza: the ZAZU_T_FALLBACK environment variable is deprecated; use MANZA_T_FALLBACK instead"]
        );
    }

    #[test]
    fn warns_only_once_per_variable() {
        std::env::remove_var("MANZA_T_ONCE");
        std::env::set_var("ZAZU_T_ONCE", "old");
        let warned = Mutex::new(None);
        let (_, first) = run("MANZA_T_ONCE", "ZAZU_T_ONCE", &warned);
        let (value, second) = run("MANZA_T_ONCE", "ZAZU_T_ONCE", &warned);
        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
        assert_eq!(value.as_deref(), Some("old"));
    }

    #[test]
    fn empty_values_count_as_unset() {
        std::env::set_var("MANZA_T_EMPTY", "");
        std::env::remove_var("ZAZU_T_EMPTY");
        let (value, warnings) = run("MANZA_T_EMPTY", "ZAZU_T_EMPTY", &Mutex::new(None));
        assert_eq!(value, None);
        assert!(warnings.is_empty());
    }
}
