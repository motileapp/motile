//! The limit resets an account holds, as Claude's and Codex's backends list them, and what they
//! answer when one is used. The server and the gateway both read them.
//!
//! Claude's are the grants of the `cedar_ember` block that `/api/oauth/usage?cedar_ember=1` adds,
//! used with `POST /api/organizations/<organisation>/reset_rate_limits`. Codex's are the credits
//! of `rate_limit_reset_credits`, used with `/wham/rate-limit-reset-credits/consume` or the
//! app-server's `account/rateLimitResetCredit/consume`.

use serde_json::{Value, json};

use crate::wire::{LimitReset, ResetKind};

/// The program Claude's resets are claimed under.
pub const CLAUDE_PROGRAM: &str = "cedar_ember";
/// The query that has Claude's usage page list the resets.
pub const CLAUDE_USAGE_QUERY: &str = "cedar_ember=1&skip_spend=1";

/// The grants of Claude's `cedar_ember` block that have uses left and haven't ended.
pub fn claude(block: &Value, now: f64) -> Vec<LimitReset> {
    if block["eligible"] != true {
        return Vec::new();
    }
    let grants = block["grants"].as_array().into_iter().flatten();
    grants
        .filter_map(|grant| {
            let left = grant["resets_left"].as_u64().filter(|left| *left > 0)? as u32;
            let expires_at = grant["ends_at"].as_str().and_then(timestamp);
            if grant["paused"] == true || expires_at.is_some_and(|at| at <= now) {
                return None;
            }
            Some(LimitReset {
                id: grant["id"].as_str()?.to_string(),
                kind: claude_kind(grant),
                left,
                expires_at,
                usable: grant["usable_now"] == true,
                needs_limit: grant["use_requires_limit"] != false,
            })
        })
        .collect()
}

/// As Claude Code names them: one that gives back what Fable used of the week is Fable's, else
/// its type, else what it clears.
fn claude_kind(grant: &Value) -> ResetKind {
    if grant["gives_back_weekly"] == true {
        return ResetKind::Model { name: "Fable".to_string() };
    }
    match grant["type"].as_str() {
        Some("session") => return ResetKind::Session,
        Some("weekly") => return ResetKind::Weekly,
        Some("full") => return ResetKind::Full,
        _ => {}
    }
    let clears: Vec<&str> = grant["clears"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let session = clears.contains(&"five_hour");
    let other = clears.iter().any(|limit| *limit != "five_hour");
    match (session, other) {
        (true, false) => ResetKind::Session,
        (false, true) => ResetKind::Weekly,
        _ => ResetKind::Full,
    }
}

/// What claims a Claude reset.
pub fn claude_claim(grant: &str, request: &str) -> Value {
    json!({"program": CLAUDE_PROGRAM, "grant_id": grant, "request_id": request})
}

/// What Claude answered a claim, as an error the user can read unless it reset.
pub fn claude_claimed(answer: &Value) -> Result<(), String> {
    match answer["result"].as_str() {
        Some("reset") => Ok(()),
        Some("already_used") => Err(ALREADY_USED.to_string()),
        Some("not_limited") => Err(NOT_LIMITED.to_string()),
        Some("ineligible") => Err(NONE_LEFT.to_string()),
        Some("cooldown") => Err("Claude is cooling resets down. Try again later.".to_string()),
        _ => Err(UNCONFIRMED.to_string()),
    }
}

/// Codex's credits that are available, from its usage page's `rate_limit_reset_credits`, its
/// list of them, or the app-server's `rateLimitResetCredits`. One for each, or one with all of
/// them when it only said how many there are.
pub fn codex(summary: &Value) -> Vec<LimitReset> {
    let count = either(summary, "available_count", "availableCount").as_u64().unwrap_or_default() as u32;
    let credits: Vec<LimitReset> = summary["credits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|credit| credit["status"] == "available")
        .filter_map(|credit| {
            let expires = either(credit, "expires_at", "expiresAt");
            Some(LimitReset {
                id: credit["id"].as_str()?.to_string(),
                kind: ResetKind::Full,
                left: 1,
                expires_at: expires.as_f64().or_else(|| expires.as_str().and_then(timestamp)),
                usable: true,
                needs_limit: false,
            })
        })
        .collect();
    if !credits.is_empty() || count == 0 {
        return credits;
    }
    vec![LimitReset {
        id: String::new(),
        kind: ResetKind::Full,
        left: count,
        expires_at: None,
        usable: true,
        needs_limit: false,
    }]
}

/// What uses a Codex credit, the next one when `credit` is empty.
pub fn codex_consume(credit: &str, request: &str) -> Value {
    match credit.is_empty() {
        true => json!({"redeem_request_id": request}),
        false => json!({"redeem_request_id": request, "credit_id": credit}),
    }
}

/// What Codex answered, its usage page's `code` or the app-server's `outcome`, as an error the user
/// can read unless it reset.
pub fn codex_consumed(outcome: &str) -> Result<(), String> {
    match outcome {
        "reset" => Ok(()),
        "nothing_to_reset" | "nothingToReset" => Err(NOT_LIMITED.to_string()),
        "no_credit" | "noCredit" => Err(NONE_LEFT.to_string()),
        "already_redeemed" | "alreadyRedeemed" => Err(ALREADY_USED.to_string()),
        _ => Err(UNCONFIRMED.to_string()),
    }
}

const ALREADY_USED: &str = "That reset was already used.";
const NOT_LIMITED: &str = "None of the limits this reset refills is used up yet, so nothing was spent.";
const NONE_LEFT: &str = "This account has no reset left to use.";
const UNCONFIRMED: &str =
    "Couldn't confirm that the reset went through. If the limits are still used up in a minute, try again.";

fn either<'a>(value: &'a Value, snake: &str, camel: &str) -> &'a Value {
    match &value[snake] {
        Value::Null => &value[camel],
        found => found,
    }
}

fn timestamp(text: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|at| at.timestamp() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn claude_lists_the_grants_with_uses_left_by_what_they_refill() {
        let block = json!({"eligible": true, "next_grant_id": "week", "grants": [
            {"id": "week", "resets_left": 2, "ends_at": "2027-12-10T22:00:00Z", "usable_now": true,
             "use_requires_limit": false, "clears": ["seven_day"]},
            {"id": "five", "resets_left": 1, "usable_now": false, "clears": ["five_hour"]},
            {"id": "both", "resets_left": 1, "type": "full", "usable_now": true},
            {"id": "fable", "resets_left": 1, "gives_back_weekly": true, "usable_now": true},
            {"id": "spent", "resets_left": 0, "usable_now": true},
            {"id": "ended", "resets_left": 1, "ends_at": "2020-01-01T00:00:00Z", "usable_now": true},
            {"id": "paused", "resets_left": 1, "paused": true, "usable_now": true}
        ]});
        let resets = claude(&block, NOW);
        let kinds: Vec<_> = resets.iter().map(|reset| (reset.id.as_str(), reset.kind.clone())).collect();
        assert_eq!(
            kinds,
            [
                ("week", ResetKind::Weekly),
                ("five", ResetKind::Session),
                ("both", ResetKind::Full),
                ("fable", ResetKind::Model { name: "Fable".into() })
            ]
        );
        assert_eq!((resets[0].left, resets[0].usable, resets[0].needs_limit), (2, true, false));
        assert_eq!(resets[0].expires_at, Some(1_828_476_000.0));
        assert_eq!((resets[1].usable, resets[1].needs_limit), (false, true));
        assert!(claude(&json!({"eligible": false, "grants": block["grants"]}), NOW).is_empty());
    }

    #[test]
    fn codex_lists_each_available_credit_or_only_how_many() {
        let listed = json!({"available_count": 2, "credits": [
            {"id": "a", "reset_type": "codex_rate_limits", "status": "available", "expires_at": "2027-12-10T22:00:00Z"},
            {"id": "b", "status": "redeemed"},
            {"id": "c", "resetType": "codexRateLimits", "status": "available", "expiresAt": 1_900_000_000}
        ]});
        let resets = codex(&listed);
        let ids: Vec<_> = resets.iter().map(|reset| (reset.id.as_str(), reset.expires_at)).collect();
        assert_eq!(ids, [("a", Some(1_828_476_000.0)), ("c", Some(1_900_000_000.0))]);
        let counted = codex(&json!({"availableCount": 3}));
        assert_eq!((counted.len(), counted[0].id.as_str(), counted[0].left), (1, "", 3));
        assert!(codex(&json!({"available_count": 0})).is_empty());
    }

    #[test]
    fn only_a_reset_is_success_and_the_rest_say_why() {
        assert_eq!(claude_claimed(&json!({"result": "reset"})), Ok(()));
        assert!(claude_claimed(&json!({"result": "not_limited"})).unwrap_err().contains("nothing was spent"));
        assert!(claude_claimed(&json!({"result": "unavailable"})).unwrap_err().contains("Couldn't confirm"));
        assert_eq!(codex_consumed("reset"), Ok(()));
        assert_eq!(codex_consumed("noCredit"), codex_consumed("no_credit"));
        assert_eq!(codex_consume("", "r"), json!({"redeem_request_id": "r"}));
    }
}
