//! Your own record of the transfers you created, keyed by the
//! `client_reference` you generated. A JSON file, so the creator
//! (`create_transfer`) and the authorizer (`transfer_authorizer`) can run as
//! two processes. Swap it for your database in production.
//!
//! ```json
//! { "po_8812": { "payment_id": "...", "amount": "2500.0",
//!                "currency_code": "MAD", "account_id": "...",
//!                "payee": "ext:...", "transfer_method": "standard" } }
//! ```

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// One ledger entry: `payment_id`, `amount`, `currency_code`, `account_id`
/// and `payee`, all strings, and `transfer_method`, a string or `null`.
pub type Entry = Map<String, Value>;

/// What [`Ledger::record`] writes, from the create response.
pub struct Record<'a> {
    pub payment_id: &'a str,
    pub amount: &'a str,
    pub currency_code: &'a str,
    pub account_id: &'a str,
    pub payee: &'a str,
    /// The create response's `transfer_method` (`None` where the market
    /// offers no choice): the v2 signature signs it.
    pub transfer_method: Option<&'a str>,
}

pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    pub fn new(path: impl Into<PathBuf>) -> Ledger {
        Ledger { path: path.into() }
    }

    pub fn record(&self, client_reference: &str, record: &Record<'_>) -> io::Result<Entry> {
        let entry = json!({
            "payment_id": record.payment_id,
            "amount": record.amount,
            "currency_code": record.currency_code,
            "account_id": record.account_id,
            "payee": record.payee,
            "transfer_method": record.transfer_method,
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
