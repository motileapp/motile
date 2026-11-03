//! How much of their plans the accounts have used, labelled as the server labels its own accounts'
//! windows so that the clients show them alike. Read from every response: Anthropic's
//! `anthropic-ratelimit-unified-*` headers, Codex's `x-codex-*` headers and `codex.rate_limits`
//! events. An account nothing went through for a minute is asked: Claude's `/api/oauth/usage`, the
//! ChatGPT backend's `/wham/usage`.

use motile_protocol::wire::{Agent, LimitWindow};
use serde_json::Value;

const SESSION_SECS: u64 = 5 * 3600;
const WEEK_SECS: u64 = 7 * 86400;
/// How long an account that hit a limit rests when the error doesn't say until when.
const REST_UNSAID: f64 = 60.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub windows: Vec<LimitWindow>,
    pub reset_credits: Option<u32>,
    /// It lists every window of the account, rather than those a response's headers carry.
    pub whole: bool,
}

pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

fn number(headers: &[(String, String)], name: &str) -> Option<f64> {
    header(headers, name)?.trim().parse().ok()
}

pub fn from_headers(agent: Agent, headers: &[(String, String)]) -> Option<Reading> {
    let windows: Vec<LimitWindow> = match agent {
        Agent::Claude => [("5h", "Session", SESSION_SECS), ("7d", "Weekly", WEEK_SECS)]
            .iter()
            .filter_map(|(window, label, secs)| {
                let prefix = format!("anthropic-ratelimit-unified-{window}");
                Some(LimitWindow {
                    label: label.to_string(),
                    used_percent: number(headers, &format!("{prefix}-utilization"))?.clamp(0.0, 1.0) * 100.0,
                    resets_at: number(headers, &format!("{prefix}-reset")),
                    window_secs: Some(*secs),
                    warning: header(headers, &format!("{prefix}-surpassed-threshold")).is_some(),
                })
            })
            .collect(),
        Agent::Codex => ["primary", "secondary"]
            .iter()
            .filter_map(|which| {
                let prefix = format!("x-codex-{which}");
                codex_window(
                    number(headers, &format!("{prefix}-used-percent"))?,
                    number(headers, &format!("{prefix}-window-minutes")).map(|minutes| minutes as u64 * 60),
                    number(headers, &format!("{prefix}-reset-at")),
                )
            })
            .collect(),
    };
    (!windows.is_empty()).then_some(Reading { windows, reset_credits: None, whole: false })
}

/// Codex's `codex.rate_limits` event on a WebSocket.
pub fn from_codex_event(event: &Value) -> Option<Reading> {
    if event["type"] != "codex.rate_limits" {
        return None;
    }
    let limits = &event["rate_limits"];
    let windows: Vec<LimitWindow> = ["primary", "secondary"]
        .iter()
        .filter_map(|which| {
            let window = &limits[which];
            codex_window(
                window["used_percent"].as_f64()?,
                window["window_minutes"].as_u64().map(|minutes| minutes * 60),
                window["reset_at"].as_f64(),
            )
        })
        .collect();
    (!windows.is_empty()).then_some(Reading { windows, reset_credits: None, whole: false })
}

fn codex_window(used_percent: f64, window_secs: Option<u64>, resets_at: Option<f64>) -> Option<LimitWindow> {
    let label = match window_secs {
        Some(secs) if secs <= 86400 => "Session",
        Some(secs) if secs <= WEEK_SECS => "Weekly",
        Some(_) => "Monthly",
        None => "Limit",
    };
    Some(LimitWindow { label: label.to_string(), used_percent, resets_at, window_secs, warning: false })
}

/// Claude's `/api/oauth/usage`.
pub fn from_claude_usage(usage: &Value) -> Reading {
    let windows = match usage["limits"].as_array() {
        Some(listed) => listed.iter().filter_map(claude_listed_window).collect(),
        None => [
            ("five_hour", "Session", SESSION_SECS),
            ("seven_day", "Weekly", WEEK_SECS),
            ("seven_day_opus", "Weekly · Opus", WEEK_SECS),
            ("seven_day_sonnet", "Weekly · Sonnet", WEEK_SECS),
        ]
        .iter()
        .filter_map(|(key, label, secs)| claude_window(&usage[key], label, *secs, "utilization"))
        .chain(usage["model_scoped"].as_array().into_iter().flatten().filter_map(claude_model_window))
        .collect(),
    };
    Reading { windows, reset_credits: None, whole: true }
}

/// A model's own week, which some accounts are told apart from the list they aren't given.
fn claude_model_window(limit: &Value) -> Option<LimitWindow> {
    let label = format!("Weekly · {}", limit["display_name"].as_str()?);
    claude_window(limit, &label, WEEK_SECS, "utilization")
}

fn claude_listed_window(limit: &Value) -> Option<LimitWindow> {
    let (label, secs) = match limit["group"].as_str()? {
        "session" => ("Session", SESSION_SECS),
        "weekly" => ("Weekly", WEEK_SECS),
        _ => return None,
    };
    let label = match limit["scope"]["model"]["display_name"].as_str() {
        Some(model) => format!("{label} · {model}"),
        None => label.to_string(),
    };
    let mut window = claude_window(limit, &label, secs, "percent")?;
    window.warning = limit["severity"].as_str().is_some_and(|severity| severity != "normal");
    Some(window)
}

fn claude_window(limit: &Value, label: &str, secs: u64, used: &str) -> Option<LimitWindow> {
    let resets_at = limit["resets_at"].as_str().and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok());
    Some(LimitWindow {
        label: label.to_string(),
        used_percent: limit[used].as_f64()?,
        resets_at: resets_at.map(|at| at.timestamp() as f64),
        window_secs: Some(secs),
        warning: false,
    })
}

/// The ChatGPT backend's `/wham/usage`, and the plan it names.
pub fn from_codex_usage(usage: &Value) -> (Reading, Option<String>) {
    let limits = &usage["rate_limit"];
    let windows = ["primary_window", "secondary_window"]
        .iter()
        .filter_map(|which| {
            let window = &limits[which];
            codex_window(
                window["used_percent"].as_f64()?,
                window["limit_window_seconds"].as_u64(),
                window["reset_at"].as_f64(),
            )
        })
        .collect();
    let credits = usage["rate_limit_reset_credits"]["available_count"].as_u64().map(|count| count as u32);
    let plan = usage["plan_type"].as_str().map(codex_plan);
    (Reading { windows, reset_credits: Some(credits.unwrap_or(0)), whole: true }, plan)
}

pub fn codex_plan(plan: &str) -> String {
    match plan {
        "prolite" => "Pro Lite".to_string(),
        "promax" => "Pro Max".to_string(),
        plan => capitalized(&plan.replace('_', " ")),
    }
}

pub fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map(|first| first.to_uppercase().chain(characters).collect()).unwrap_or_default()
}

/// Until when an account rests after a response that says it reached a limit, or `None` when the
/// response says no such thing.
pub fn limit_reached(agent: Agent, status: u16, headers: &[(String, String)], body: &[u8], now: f64) -> Option<f64> {
    if status != 429 {
        return None;
    }
    let body: Value = serde_json::from_slice(body).unwrap_or_default();
    let said = match agent {
        Agent::Claude => number(headers, "anthropic-ratelimit-unified-reset"),
        Agent::Codex => body["error"]["resets_at"].as_f64(),
    };
    let retry_after = number(headers, "retry-after").map(|seconds| now + seconds);
    Some(said.or(retry_after).filter(|until| *until > now).unwrap_or(now + REST_UNSAID))
}

/// A limit error a WebSocket sent in place of a response: `{"type":"error","status":429,…}`.
pub fn limit_reached_event(event: &Value, now: f64) -> Option<f64> {
    if event["type"] != "error" || event["status"] != 429 {
        return None;
    }
    let said = event["error"]["resets_at"].as_f64();
    Some(said.filter(|until| *until > now).unwrap_or(now + REST_UNSAID))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    #[test]
    fn anthropics_unified_headers_are_the_session_and_the_week_in_percent() {
        let reading = from_headers(
            Agent::Claude,
            &headers(&[
                ("anthropic-ratelimit-unified-status", "allowed"),
                ("anthropic-ratelimit-unified-5h-utilization", "0.42"),
                ("anthropic-ratelimit-unified-5h-reset", "1791318600"),
                ("Anthropic-Ratelimit-Unified-7d-Utilization", "0.9"),
                ("anthropic-ratelimit-unified-7d-reset", "1791580000"),
                ("anthropic-ratelimit-unified-7d-surpassed-threshold", "0.75"),
            ]),
        )
        .unwrap();
        let windows: Vec<_> =
            reading.windows.iter().map(|window| (window.label.as_str(), window.used_percent, window.warning)).collect();
        assert_eq!(windows, [("Session", 42.0, false), ("Weekly", 90.0, true)]);
        assert_eq!(reading.windows[0].resets_at, Some(1791318600.0));
        assert!(!reading.whole);
        assert_eq!(from_headers(Agent::Claude, &headers(&[("content-type", "text/event-stream")])), None);
    }

    #[test]
    fn codex_headers_name_their_windows_by_length() {
        let reading = from_headers(
            Agent::Codex,
            &headers(&[
                ("x-codex-primary-used-percent", "12.5"),
                ("x-codex-primary-window-minutes", "300"),
                ("x-codex-primary-reset-at", "1791308081"),
                ("x-codex-secondary-used-percent", "40"),
                ("x-codex-secondary-window-minutes", "10080"),
            ]),
        )
        .unwrap();
        let windows: Vec<_> =
            reading.windows.iter().map(|window| (window.label.as_str(), window.used_percent)).collect();
        assert_eq!(windows, [("Session", 12.5), ("Weekly", 40.0)]);
        assert_eq!(reading.windows[1].window_secs, Some(WEEK_SECS));
    }

    #[test]
    fn a_codex_rate_limits_event_reads_like_its_headers() {
        let event = json!({"type": "codex.rate_limits", "rate_limits": {
            "primary": {"used_percent": 3.0, "window_minutes": 300, "reset_at": 100},
            "secondary": {"used_percent": 7.0, "window_minutes": 10080}}});
        let windows = from_codex_event(&event).unwrap().windows;
        assert_eq!((windows[0].label.as_str(), windows[0].resets_at), ("Session", Some(100.0)));
        assert_eq!((windows[1].label.as_str(), windows[1].used_percent), ("Weekly", 7.0));
    }

    #[test]
    fn claudes_usage_lists_a_models_own_window() {
        let usage = json!({"five_hour": {"utilization": 99, "resets_at": null}, "limits": [
            {"kind": "session", "group": "session", "percent": 13, "resets_at": "2026-10-06T20:30:00+00:00", "severity": "normal"},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 79, "resets_at": "2026-10-12T04:00:00+00:00",
             "severity": "warning", "scope": {"model": {"display_name": "Opus"}}}]});
        let reading = from_claude_usage(&usage);
        let windows: Vec<_> =
            reading.windows.iter().map(|window| (window.label.as_str(), window.used_percent, window.warning)).collect();
        assert_eq!(windows, [("Session", 13.0, false), ("Weekly · Opus", 79.0, true)]);
        assert_eq!(reading.windows[0].resets_at, Some(1791318600.0));

        let older = json!({"five_hour": {"utilization": 8.0, "resets_at": "2026-10-06T20:30:00+00:00"},
                           "seven_day": {"utilization": 45.0, "resets_at": null}, "seven_day_opus": null});
        let labels: Vec<_> = from_claude_usage(&older).windows.into_iter().map(|window| window.label).collect();
        assert_eq!(labels, ["Session", "Weekly"]);

        let apart = json!({"five_hour": {"utilization": 90, "resets_at": null}, "seven_day": {"utilization": 0, "resets_at": null},
                           "model_scoped": [{"display_name": "Fable", "utilization": 12, "resets_at": null}]});
        let windows: Vec<_> =
            from_claude_usage(&apart).windows.into_iter().map(|window| (window.label, window.used_percent)).collect();
        assert_eq!(windows, [("Session".into(), 90.0), ("Weekly".into(), 0.0), ("Weekly · Fable".to_string(), 12.0)]);
    }

    #[test]
    fn codex_usage_has_its_windows_reset_credits_and_plan() {
        let usage = json!({"plan_type": "prolite", "rate_limit": {
            "primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_at": 1791308081},
            "secondary_window": {"used_percent": 4, "limit_window_seconds": 604800, "reset_at": 1791580413}},
            "rate_limit_reset_credits": {"available_count": 2}});
        let (reading, plan) = from_codex_usage(&usage);
        assert_eq!(plan.as_deref(), Some("Pro Lite"));
        assert_eq!(reading.reset_credits, Some(2));
        let windows: Vec<_> =
            reading.windows.iter().map(|window| (window.label.as_str(), window.used_percent)).collect();
        assert_eq!(windows, [("Session", 1.0), ("Weekly", 4.0)]);
    }

    #[test]
    fn a_limit_error_rests_the_account_until_the_reset_it_names() {
        let now = 1000.0;
        let claude = headers(&[("anthropic-ratelimit-unified-reset", "5000"), ("retry-after", "30")]);
        assert_eq!(limit_reached(Agent::Claude, 429, &claude, b"{}", now), Some(5000.0));
        assert_eq!(limit_reached(Agent::Claude, 429, &headers(&[("retry-after", "30")]), b"", now), Some(1030.0));
        assert_eq!(limit_reached(Agent::Claude, 529, &claude, b"{}", now), None);

        let codex = json!({"error": {"type": "usage_limit_reached", "resets_at": 9000}}).to_string();
        assert_eq!(limit_reached(Agent::Codex, 429, &[], codex.as_bytes(), now), Some(9000.0));
        assert_eq!(limit_reached(Agent::Codex, 429, &[], b"", now), Some(now + REST_UNSAID));

        let event =
            json!({"type": "error", "status": 429, "error": {"type": "usage_limit_reached", "resets_at": 7000}});
        assert_eq!(limit_reached_event(&event, now), Some(7000.0));
        assert_eq!(limit_reached_event(&json!({"type": "error", "error": {}}), now), None);
    }
}
