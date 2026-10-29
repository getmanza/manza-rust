//! Says where the API routed a new draft, from its `authorization`:
//! `pending` means the challenge went to the authorizer; `refused`, with a
//! `refusal_reason`, means it went to your in-app approvers.

use serde_json::Value;

/// The reasons a draft can be refused when it is created. The transfer
/// authorization docs' troubleshooting table has the fix for each.
pub const REFUSAL_REASONS: [(&str, &str); 8] = [
    (
        "untrusted_payee",
        "the payee's bank details are not trusted, and \"Approve transfers to new payees\" is off \
         or their trust was revoked or they were edited",
    ),
    (
        "unrestricted_api_key",
        "the payee is new and key A has no IP allowlist",
    ),
    (
        "over_new_payee_limit",
        "the payee is new and the amount is over the limit per transfer to a new payee",
    ),
    (
        "over_transfer_limit",
        "the amount is over the limit per transfer",
    ),
    (
        "over_daily_limit",
        "this amount plus the last 24 hours' authorized total is over the limit per 24 hours",
    ),
    (
        "currency_mismatch",
        "the account's currency is not the enrolled currency",
    ),
    (
        "suspended",
        "transfer authorization is suspended, or its webhook endpoint is disabled",
    ),
    ("secret_expired", "the signing secret has expired"),
];

/// One line for the draft's `authorization` (`None` or JSON `null` when no
/// authorizer was enrolled).
pub fn describe(authorization: Option<&Value>) -> String {
    let Some(authorization) = authorization.filter(|a| !a.is_null()) else {
        return "no transfer authorizer was enrolled (or the feature is off): sent to your approvers"
            .to_owned();
    };
    let status = authorization["status"].as_str().unwrap_or_default();
    if status == "pending" {
        return "challenge sent, waiting for the authorizer: watch the transfer_authorizer terminal"
            .to_owned();
    }

    let reason = authorization["refusal_reason"].as_str().unwrap_or_default();
    let words = REFUSAL_REASONS
        .iter()
        .find(|(known, _)| *known == reason)
        .map_or(
            "see the transfer authorization docs' troubleshooting table",
            |(_, words)| words,
        );
    format!("sent to your approvers, not the authorizer: {status} ({reason}), {words}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn says_the_challenge_went_to_the_authorizer_when_it_is_pending() {
        assert!(describe(Some(&json!({ "status": "pending" }))).contains("challenge sent"));
    }

    #[test]
    fn explains_every_create_time_refusal_reason_in_words() {
        let expected = [
            ("untrusted_payee", "bank details are not trusted"),
            ("unrestricted_api_key", "key A has no IP allowlist"),
            ("over_new_payee_limit", "limit per transfer to a new payee"),
            ("over_transfer_limit", "over the limit per transfer"),
            ("over_daily_limit", "limit per 24 hours"),
            ("currency_mismatch", "not the enrolled currency"),
            ("suspended", "is suspended"),
            ("secret_expired", "secret has expired"),
        ];
        let mut known: Vec<&str> = REFUSAL_REASONS.iter().map(|(r, _)| *r).collect();
        let mut wanted: Vec<&str> = expected.iter().map(|(r, _)| *r).collect();
        known.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(known, wanted);

        for (reason, words) in expected {
            let line = describe(Some(
                &json!({ "status": "refused", "refusal_reason": reason }),
            ));
            for needle in ["approvers", "refused", reason, words] {
                assert!(line.contains(needle), "{needle:?} not in {line:?}");
            }
        }
    }

    #[test]
    fn points_at_the_docs_for_a_reason_it_does_not_know() {
        let line = describe(Some(
            &json!({ "status": "refused", "refusal_reason": "something_new" }),
        ));
        assert!(line.contains("something_new"), "{line}");
        assert!(line.contains("transfer authorization docs"), "{line}");
    }

    #[test]
    fn says_no_authorizer_was_enrolled_when_there_is_no_authorization() {
        assert!(describe(None).contains("no transfer authorizer"));
        assert!(describe(Some(&Value::Null)).contains("no transfer authorizer"));
    }
}
