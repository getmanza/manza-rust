//! Checks every setup step before you create a transfer, and says how to fix
//! the ones that fail:
//!
//!   cargo run --example preflight              # for a 10.00 transfer
//!   cargo run --example preflight -- 250.00
//!
//! ✓ passed, ✗ failed (exit status 1), ! cannot be checked through the API.

#[path = "dotenv.rs"]
pub mod dotenv;

#[cfg(test)]
#[path = "test_api.rs"]
pub mod test_api;
#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;

use std::collections::HashMap;
use std::io::Write;

use manza::{Client, Error, ErrorKind};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://ma.manza.finance";
const SECRET_MIN_DAYS: i64 = 7;
/// Hosts that hand out a new URL on every restart. An enrolled endpoint's
/// URL is locked, so the next restart breaks the authorizer.
const UNSTABLE_TUNNEL_HOSTS: [&str; 2] = ["ngrok-free.app", "trycloudflare.com"];
const SUSPENSION_FIXES: [(&str, &str); 4] = [
    (
        "member",
        "an approver suspended it: resume it in Settings → Developer → Transfer approval",
    ),
    (
        "signature_failures",
        "five invalid signatures in a row: fix the signing, then an approver resumes it in the app",
    ),
    (
        "secret_expired",
        "an approver rotates the secret in the app (and resumes it if it stays suspended); \
         set the new one as MANZA_AUTHORIZER_SECRET",
    ),
    (
        "endpoint_disabled",
        "re-enable the webhook endpoint (webhook_endpoints().enable), or enrol a new one if it was \
         deleted; if it stays suspended, an approver resumes it in the app",
    ),
];

/// Decimal amounts as integers scaled by 10^18: exact for the API's
/// decimal strings, without a decimal crate.
const SCALE: i128 = 1_000_000_000_000_000_000;

pub struct Preflight<'a, W: Write> {
    env: &'a HashMap<String, String>,
    raw_amount: String,
    amount: Option<i128>,
    out: W,
    now: i64,
    ok: bool,
}

impl<'a, W: Write> Preflight<'a, W> {
    /// `now` is Unix seconds.
    pub fn new(env: &'a HashMap<String, String>, amount: &str, out: W, now: i64) -> Self {
        // Plain decimals only: create_transfer sends the string as is, so
        // "1/2" or "1e2" would not mean the same to the API.
        let parsed = parse_decimal(amount).filter(|a| *a > 0);
        Preflight {
            env,
            raw_amount: amount.to_owned(),
            amount: parsed,
            out,
            now,
            ok: true,
        }
    }

    /// Prints one line per step; `true` when no step failed.
    pub fn run(mut self) -> bool {
        if self.amount.is_none() {
            let message = format!("amount {:?} is not a positive decimal", self.raw_amount);
            self.fail_step(&message, "pass an amount like 10.00 or 250.00");
        }
        if !self.keys() {
            return self.ok;
        }

        self.creator_reaches_entity();
        let authorizer = self.enrolled_authorizer();
        if let Some(authorizer) = &authorizer {
            self.currency_matches(authorizer);
            // An expired secret already failed the authorizer line, with its fix.
            if authorizer["suspension_reason"] != "secret_expired" {
                self.secret_fresh(authorizer);
            }
        }
        self.secret_set();
        if let Some(authorizer) = &authorizer {
            self.endpoint_enabled(authorizer);
            if let Some(amount) = self.amount {
                self.amount_within_limits(authorizer, amount);
            }
        }
        self.payee_trusted();
        self.ok
    }

    /// 1. Both keys (and the ids the other steps read) are set, the keys
    ///    different.
    fn keys(&mut self) -> bool {
        let missing: Vec<&str> = [
            "MANZA_CREATOR_API_KEY",
            "MANZA_AUTHORIZER_API_KEY",
            "MANZA_ACCOUNT_ID",
            "MANZA_BENEFICIARY_ID",
        ]
        .into_iter()
        .filter(|name| self.value(name).is_empty())
        .collect();
        if !missing.is_empty() {
            self.fail_step(
                &format!("{} not set", missing.join(", ")),
                "set them in .env",
            );
            false
        } else if self.value("MANZA_CREATOR_API_KEY") == self.value("MANZA_AUTHORIZER_API_KEY") {
            self.fail_step(
                "key A and key B are the same key",
                "use two different keys: the API refuses to let the key that created a transfer authorize it",
            );
            false
        } else {
            self.pass("key A and key B are set and different");
            true
        }
    }

    /// 2. Key A reaches the API.
    fn creator_reaches_entity(&mut self) {
        match self.creator().entity().get() {
            Ok(entity) => self.pass(&format!(
                "key A reaches the API (entity {})",
                entity.body["name"]
            )),
            Err(e) => self.fail_step(
                &format!("key A cannot read the entity: {}", describe(&e)),
                &self.key_fix(&e, "MANZA_CREATOR_API_KEY", "entity:read", None),
            ),
        }
    }

    /// 3. Key B reads the authorizer, which must be active.
    fn enrolled_authorizer(&mut self) -> Option<Value> {
        match self.authorizer_client().transfer_authorizer().get() {
            Ok(response) => {
                let authorizer = response.body;
                if authorizer["status"] == "active" {
                    self.pass("transfer authorizer is active");
                } else {
                    let reason = authorizer["suspension_reason"].as_str().unwrap_or_default();
                    let fix = SUSPENSION_FIXES
                        .iter()
                        .find(|(known, _)| *known == reason)
                        .map_or(
                            "see suspension_reason in the transfer authorization docs",
                            |(_, fix)| fix,
                        );
                    self.fail_step(
                        &format!(
                            "transfer authorizer is {} ({reason})",
                            text(&authorizer["status"])
                        ),
                        fix,
                    );
                }
                Some(authorizer)
            }
            Err(Error::Api(e)) if e.kind == ErrorKind::NotFound => {
                self.fail_step(
                    "no transfer authorizer is enrolled",
                    "enrol your webhook endpoint in Settings → Developer → Transfer approval",
                );
                None
            }
            Err(e) => {
                self.fail_step(
                    &format!(
                        "key B cannot read the transfer authorizer: {}",
                        describe(&e)
                    ),
                    &self.key_fix(&e, "MANZA_AUTHORIZER_API_KEY", "transfers:authorize", None),
                );
                None
            }
        }
    }

    fn currency_matches(&mut self, authorizer: &Value) {
        let account_id = self.value("MANZA_ACCOUNT_ID");
        let enrolled = text(&authorizer["currency_code"]).to_owned();
        match self.creator().accounts().get(&account_id) {
            Ok(account) => {
                let currency = text(&account.body["currency_code"]).to_owned();
                if currency == enrolled {
                    self.pass(&format!(
                        "account currency {currency} is the enrolled currency"
                    ));
                } else {
                    self.fail_step(
                        &format!(
                            "account currency {currency} is not the enrolled currency {enrolled}"
                        ),
                        &format!(
                            "pay from a {enrolled} account (MANZA_ACCOUNT_ID), or enrol {currency}"
                        ),
                    );
                }
            }
            Err(e) => self.fail_step(
                &format!("key A cannot read account {account_id:?}: {}", describe(&e)),
                &self.key_fix(
                    &e,
                    "MANZA_CREATOR_API_KEY",
                    "accounts:read",
                    Some("MANZA_ACCOUNT_ID"),
                ),
            ),
        }
    }

    fn secret_fresh(&mut self, authorizer: &Value) {
        let raw = text(&authorizer["secret_expires_at"]);
        let Some(expires_at) = parse_time(raw) else {
            self.fail_step(
                &format!("secret_expires_at {raw:?} is not a time"),
                "check MANZA_BASE_URL, then rerun",
            );
            return;
        };
        let days = (expires_at - self.now).div_euclid(86_400);
        if days >= SECRET_MIN_DAYS {
            self.pass(&format!("signing secret expires in {days} days"));
            return;
        }
        let when = if days < 0 {
            "expired".to_owned()
        } else {
            format!("expires in {days} days")
        };
        self.fail_step(
            &format!("signing secret {when} ({raw})"),
            "an approver should rotate the secret in Settings → Developer → Transfer approval; set \
             the new one as MANZA_AUTHORIZER_SECRET",
        );
    }

    /// 4. The signing secret is set.
    fn secret_set(&mut self) {
        if self.value("MANZA_AUTHORIZER_SECRET").is_empty() {
            self.fail_step(
                "MANZA_AUTHORIZER_SECRET is not set",
                "set it to the signing secret shown at enrolment or at the last rotation",
            );
        } else {
            self.pass("MANZA_AUTHORIZER_SECRET is set");
        }
    }

    /// 5. The enrolled webhook endpoint is enabled, on a URL that lasts.
    fn endpoint_enabled(&mut self, authorizer: &Value) {
        let endpoint_id = text(&authorizer["webhook_endpoint_id"]);
        let endpoint = match self.creator().webhook_endpoints().get(endpoint_id) {
            Ok(response) => response.body,
            Err(Error::Api(e)) if e.kind == ErrorKind::NotFound => {
                self.fail_step(
                    &format!("the enrolled webhook endpoint {endpoint_id} was deleted"),
                    "create an endpoint at a stable URL and enrol it in Settings → Developer → Transfer approval",
                );
                return;
            }
            Err(e) => {
                self.fail_step(
                    &format!(
                        "key A cannot read the enrolled webhook endpoint: {}",
                        describe(&e)
                    ),
                    &self.key_fix(&e, "MANZA_CREATOR_API_KEY", "webhook_endpoints:read", None),
                );
                return;
            }
        };

        let url = text(&endpoint["url"]);
        if endpoint["status"] == "active" {
            self.pass(&format!("webhook endpoint {url} is enabled"));
        } else {
            self.fail_step(
                &format!(
                    "webhook endpoint {url} is {}",
                    text(&endpoint["status"])
                ),
                &format!(
                    "enable it (webhook_endpoints().enable({})); if the authorizer stays suspended, \
                     an approver resumes it in the app",
                    endpoint["id"]
                ),
            );
        }
        let host = host(url);
        if let Some(unstable) = UNSTABLE_TUNNEL_HOSTS
            .iter()
            .find(|domain| host.ends_with(*domain))
        {
            self.warn_step(
                &format!(
                    "webhook endpoint is on {unstable}, whose URL changes when the tunnel restarts"
                ),
                "use a reserved domain or a named tunnel, then enrol an endpoint at that URL",
            );
        }
    }

    /// 6a. The test amount fits both limits.
    fn amount_within_limits(&mut self, authorizer: &Value, amount: i128) {
        let decimal = |name: &str| parse_decimal(text(&authorizer[name]));
        let (Some(per_transfer), Some(daily), Some(total)) = (
            decimal("per_transfer_limit"),
            decimal("daily_limit"),
            decimal("daily_authorized_total"),
        ) else {
            self.fail_step(
                "the transfer authorizer's limits are not decimals",
                "check MANZA_BASE_URL, then rerun",
            );
            return;
        };
        let room = daily - total;
        if amount > per_transfer {
            self.fail_step(
                &format!(
                    "{} is over the limit per transfer ({})",
                    format_amount(amount),
                    format_amount(per_transfer)
                ),
                &format!(
                    "send at most {}, or have an approver raise the limit",
                    format_amount(per_transfer)
                ),
            );
        } else if amount > room {
            self.fail_step(
                &format!(
                    "{} does not fit the last 24 hours' room ({} left)",
                    format_amount(amount),
                    format_amount(room)
                ),
                "wait for older authorizations to leave the 24-hour window, or have an approver raise the limit",
            );
        } else {
            self.pass(&format!(
                "{} is within the limit per transfer and the 24-hour room",
                format_amount(amount)
            ));
        }
    }

    /// 6b. The payee is trusted, or may be trusted on authorize.
    fn payee_trusted(&mut self) {
        let beneficiary_id = self.value("MANZA_BENEFICIARY_ID");
        let beneficiary = match self.creator().beneficiaries().get(&beneficiary_id) {
            Ok(response) => response.body,
            Err(e) => {
                self.fail_step(
                    &format!(
                        "key A cannot read beneficiary {beneficiary_id:?}: {}",
                        describe(&e)
                    ),
                    &self.key_fix(
                        &e,
                        "MANZA_CREATOR_API_KEY",
                        "beneficiaries:read",
                        Some("MANZA_BENEFICIARY_ID"),
                    ),
                );
                return;
            }
        };
        let account = beneficiary["external_accounts"]
            .as_array()
            .and_then(|accounts| accounts.iter().find(|a| a["default"] == true));
        let Some(account) = account else {
            self.fail_step(
                "beneficiary has no default bank account",
                "add one, or set a beneficiary that has one",
            );
            return;
        };

        let id = text(&account["id"]);
        if account["trusted"] == true {
            self.pass(&format!("payee {id} is trusted"));
        } else {
            self.warn_step(
                &format!("payee {id} is not trusted yet"),
                "it qualifies only with \"Approve transfers to new payees\" on in Settings → Developer → \
                 Transfer approval (not visible through the API), key A bound to an IP allowlist, and the \
                 amount within the limit per transfer to a new payee, and only if its bank details were \
                 never trusted (revoked or edited details always go to your approvers); otherwise have an \
                 approver trust it",
            );
        }
    }

    fn creator(&self) -> Client {
        self.client("MANZA_CREATOR_API_KEY")
    }

    fn authorizer_client(&self) -> Client {
        self.client("MANZA_AUTHORIZER_API_KEY")
    }

    fn client(&self, key_var: &str) -> Client {
        let base_url = match self.value("MANZA_BASE_URL") {
            url if url.is_empty() => DEFAULT_BASE_URL.to_owned(),
            url => url,
        };
        Client::builder()
            .api_key(self.value(key_var))
            .base_url(base_url)
            .build()
            .expect("an API key is set: keys() checked it")
    }

    fn value(&self, name: &str) -> String {
        self.env.get(name).cloned().unwrap_or_default()
    }

    fn key_fix(&self, error: &Error, key_var: &str, scope: &str, id_var: Option<&str>) -> String {
        match error {
            Error::Api(e) if e.kind == ErrorKind::Authentication => {
                format!("check {key_var}: the key is wrong or revoked")
            }
            Error::Api(e) if e.kind == ErrorKind::Forbidden => format!(
                "give the key the {scope} scope, or add this machine's address to its IP allowlist"
            ),
            Error::Api(e) if e.kind == ErrorKind::NotFound => {
                format!("check {}", id_var.unwrap_or("MANZA_BASE_URL"))
            }
            _ => "check MANZA_BASE_URL and the network, then rerun".to_owned(),
        }
    }

    fn pass(&mut self, message: &str) {
        let _ = writeln!(self.out, "✓ {message}");
    }

    fn fail_step(&mut self, message: &str, fix: &str) {
        self.ok = false;
        let _ = writeln!(self.out, "✗ {message}: {fix}");
    }

    fn warn_step(&mut self, message: &str, note: &str) {
        let _ = writeln!(self.out, "! {message}: {note}");
    }
}

/// `forbidden insufficient_scope: message`, the error type or the status.
fn describe(error: &Error) -> String {
    match error {
        Error::Api(e) => format!(
            "{} {}: {}",
            e.kind,
            e.error_type.clone().unwrap_or_else(|| e.status.to_string()),
            e.message
        ),
        Error::Connection(message) => format!("connection: {message}"),
        Error::Configuration(message) => message.clone(),
    }
}

/// A JSON string's contents, or `""`.
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// `"12.50"` → 12.5 × 10^18. Plain decimals only: digits, optionally a dot
/// and more digits.
fn parse_decimal(input: &str) -> Option<i128> {
    let (whole, fraction) = input.split_once('.').unwrap_or((input, ""));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || (input.contains('.') && !digits(fraction)) || fraction.len() > 18 {
        return None;
    }
    let whole: i128 = whole.parse().ok()?;
    let fraction: i128 = format!("{fraction:0<18}").parse().ok()?;
    whole.checked_mul(SCALE)?.checked_add(fraction)
}

/// Two decimals, rounded half away from zero.
fn format_amount(amount: i128) -> String {
    let unit = (SCALE / 100) as u128;
    let magnitude = amount.unsigned_abs();
    let cents = magnitude / unit + u128::from(magnitude % unit >= unit / 2);
    let sign = if amount < 0 && cents > 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", cents / 100, cents % 100)
}

/// An RFC 3339 time (`2027-01-18T00:00:00Z`, fractional seconds and
/// `±HH:MM` offsets too) as Unix seconds.
pub fn parse_time(input: &str) -> Option<i64> {
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = input.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let bytes = input.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut rest = &input[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        rest = &fraction[end..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6
            && (rest.starts_with('+') || rest.starts_with('-'))
            && rest.as_bytes()[3] == b':' =>
        {
            let hours: i64 = rest[1..3].parse().ok()?;
            let minutes: i64 = rest[4..6].parse().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let seconds = hours * 3600 + minutes * 60;
            if rest.starts_with('-') {
                -seconds
            } else {
                seconds
            }
        }
        _ => return None,
    };

    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// The host of `https://host:port/path`, or `""`.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    host.split(':').next().unwrap_or_default()
}

fn main() {
    dotenv::load();
    let env: HashMap<String, String> = std::env::vars().collect();
    let amount = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "10.00".to_owned());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let ok = Preflight::new(&env, &amount, std::io::stdout(), now).run();
    std::process::exit(if ok { 0 } else { 1 });
}
