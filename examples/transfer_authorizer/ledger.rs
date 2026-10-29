//! Your own record of the transfers you created, keyed by the
//! `client_reference` you generated. A JSON file, so the creator
//! (`create_transfer`) and the authorizer (`transfer_authorizer`) can run as
//! two processes. Swap it for your database in production.
//!
//! ```json
//! { "po_8812": { "payment_id": "...", "amount": "2500.0",
//!                "currency_code": "MAD", "account_id": "...",
//!                "payee": "ext:..." } }
//! ```

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// One ledger entry: `payment_id`, `amount`, `currency_code`, `account_id`
/// and `payee`, all strings.
pub type Entry = Map<String, Value>;

pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    pub fn new(path: impl Into<PathBuf>) -> Ledger {
        Ledger { path: path.into() }
    }

    pub fn record(
        &self,
        client_reference: &str,
        payment_id: &str,
        amount: &str,
        currency_code: &str,
        account_id: &str,
        payee: &str,
    ) -> io::Result<Entry> {
        let entry = json!({
            "payment_id": payment_id,
            "amount": amount,
            "currency_code": currency_code,
            "account_id": account_id,
            "payee": payee,
        });
        let Value::Object(entry) = entry else {
            unreachable!("json! of an object literal")
        };

        // Serialize writers on a separate lock file, write the new ledger to
        // a temporary file and rename it over the old one: a reader sees the
        // old or the new ledger, never a half-written one, and a crash
        // mid-write leaves the old ledger intact.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(with_suffix(&self.path, ".lock"))?;
        lock.lock()?;

        let mut entries = self.read_entries()?;
        entries.insert(client_reference.to_owned(), Value::Object(entry.clone()));
        let tmp = with_suffix(&self.path, &format!(".{}.tmp", std::process::id()));
        fs::write(&tmp, serde_json::to_string_pretty(&entries)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(entry)
    }

    /// The entry for `client_reference`, or `None`.
    pub fn find(&self, client_reference: &str) -> io::Result<Option<Entry>> {
        Ok(match self.read_entries()?.remove(client_reference) {
            Some(Value::Object(entry)) => Some(entry),
            _ => None,
        })
    }

    fn read_entries(&self) -> io::Result<Map<String, Value>> {
        let json = match fs::read_to_string(&self.path) {
            Ok(json) => json,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
            Err(e) => return Err(e),
        };
        if json.trim().is_empty() {
            return Ok(Map::new());
        }
        Ok(serde_json::from_str(&json)?)
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
