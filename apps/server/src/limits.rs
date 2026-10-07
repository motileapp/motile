//! How much of their plans the agents' accounts have used, as their CLIs say: `codex app-server`
//! answers `account/read` and `account/rateLimits/read`, and Claude Code answers the `initialize`
//! and `get_usage` control requests. Neither spends tokens.

use std::process::Stdio;
use std::time::Duration;

use motile_protocol::wire::{Agent, AgentAccount, AgentLimits, LimitWindow};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::agents::environment::Environment;

const TIMEOUT: Duration = Duration::from_secs(30);
const SESSION_SECS: u64 = 5 * 3600;
const WEEK_SECS: u64 = 7 * 86400;

/// What each account of the installed agents has used, each read in its own environment.
pub async fn read(accounts: Vec<(AgentAccount, Environment)>) -> Vec<AgentLimits> {
    let reads: Vec<_> = accounts
        .into_iter()
        .map(|(account, environment)| tokio::spawn(async move { read_account(&account, &environment).await }))
        .collect();
    let mut read = Vec::new();
    for task in reads {
        read.extend(task.await.ok().flatten());
    }
    read
}

/// The new read, with what an account said last time kept for it where it couldn't be read now.
pub fn kept(read: Vec<AgentLimits>, before: &[AgentLimits]) -> Vec<AgentLimits> {
    read.into_iter()
        .map(|limits| {
            let Some(error) = limits.error.clone() else { return limits };
            let same = |last: &&AgentLimits| last.agent == limits.agent && last.account_name == limits.account_name;
            let last = before.iter().filter(same).find(|last| last.error.is_none());
            let Some(last) = last else { return limits };
            AgentLimits { error: Some(error), ..last.clone() }
        })
        .collect()
}

async fn read_account(account: &AgentAccount, environment: &Environment) -> Option<AgentLimits> {
    let agent = account.agent;
    environment.executable(agent)?;
    let read = match agent {
        Agent::Claude => read_claude(environment).await,
        Agent::Codex => read_codex(environment).await,
    };
    let read = read.map(|limits| AgentLimits { account_name: account.name.clone(), ..limits });
    Some(read.unwrap_or_else(|error| AgentLimits {
        agent,
        account_name: account.name.clone(),
        account: None,
        plan: None,
        windows: vec![],
        reset_credits: 0,
        error: Some(motile_protocol::error_text(&error)),
    }))
}

async fn read_claude(environment: &Environment) -> anyhow::Result<AgentLimits> {
    let arguments = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--strict-mcp-config",
    ];
    let requests = [("initialize", "initialize"), ("usage", "get_usage")]
        .map(|(id, subtype)| json!({"type": "control_request", "request_id": id, "request": {"subtype": subtype}}));
    let answers = converse(environment, Agent::Claude, &arguments, &requests, |message| {
        let response = &message["response"];
        let id = response["request_id"].as_str().filter(|_| message["type"] == "control_response")?;
        let answer = match response["subtype"].as_str() {
            Some("success") => Ok(response["response"].clone()),
            _ => Err(response["error"].as_str().unwrap_or("Claude Code refused").to_string()),
        };
        Some((id.to_string(), answer))
    })
    .await?;
    let account = answers.get("initialize").cloned().unwrap_or(Ok(Value::Null)).unwrap_or_default();
    let usage = answers.get("usage").cloned().ok_or_else(|| anyhow::anyhow!("Claude Code didn't say"))?;
    Ok(claude_limits(&account["account"], &usage.map_err(anyhow::Error::msg)?))
}

fn claude_limits(account: &Value, usage: &Value) -> AgentLimits {
    let plan = account["subscriptionType"].as_str().map(|plan| plan.trim_start_matches("Claude ").to_string());
    let windows = if usage["rate_limits_available"] == false { vec![] } else { claude_windows(&usage["rate_limits"]) };
    AgentLimits {
        agent: Agent::Claude,
        account_name: String::new(),
        account: account["email"].as_str().map(String::from),
        plan: plan.or_else(|| usage["subscription_type"].as_str().map(capitalized)),
        windows,
        reset_credits: 0,
        error: None,
    }
}

fn claude_windows(limits: &Value) -> Vec<LimitWindow> {
    let Some(listed) = limits["limits"].as_array() else {
        let windows = [("five_hour", "Session", SESSION_SECS), ("seven_day", "Weekly", WEEK_SECS)];
        return windows
            .iter()
            .filter_map(|(key, label, secs)| claude_window(&limits[key], label, *secs, "utilization"))
            .collect();
    };
    listed
        .iter()
        .filter_map(|limit| {
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
        })
        .collect()
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

async fn read_codex(environment: &Environment) -> anyhow::Result<AgentLimits> {
    let client = json!({"name": "motile", "title": "Motile", "version": env!("CARGO_PKG_VERSION")});
    let requests = [
        json!({"id": "initialize", "method": "initialize", "params": {"clientInfo": client}}),
        json!({"method": "initialized"}),
        json!({"id": "account", "method": "account/read", "params": {}}),
        json!({"id": "limits", "method": "account/rateLimits/read"}),
    ];
    let answers = converse(environment, Agent::Codex, &["app-server"], &requests, |message| {
        let id = message["id"].as_str()?;
        let answer = match message["error"]["message"].as_str() {
            Some(error) => Err(error.to_string()),
            None => Ok(message["result"].clone()),
        };
        Some((id.to_string(), answer))
    })
    .await?;
    let account = answers.get("account").cloned().ok_or_else(|| anyhow::anyhow!("Codex didn't say"))?;
    let account = account.map_err(anyhow::Error::msg)?;
    let limits = answers.get("limits").cloned().unwrap_or(Ok(Value::Null));
    codex_limits(&account["account"], limits)
}

fn codex_limits(account: &Value, limits: Result<Value, String>) -> anyhow::Result<AgentLimits> {
    let plan = account["planType"].as_str().map(codex_plan);
    let mut read = AgentLimits {
        agent: Agent::Codex,
        account_name: String::new(),
        account: account["email"].as_str().map(String::from),
        plan,
        windows: vec![],
        reset_credits: 0,
        error: None,
    };
    if account["type"] != "chatgpt" {
        return Ok(read);
    }
    let limits = limits.map_err(anyhow::Error::msg)?;
    let snapshot = match &limits["rateLimitsByLimitId"]["codex"] {
        Value::Null => &limits["rateLimits"],
        codex => codex,
    };
    read.windows = ["primary", "secondary"].iter().filter_map(|key| codex_window(&snapshot[key])).collect();
    read.reset_credits = limits["rateLimitResetCredits"]["availableCount"].as_u64().unwrap_or_default() as u32;
    Ok(read)
}

fn codex_window(window: &Value) -> Option<LimitWindow> {
    let minutes = window["windowDurationMins"].as_u64();
    let label = match minutes {
        Some(minutes) if minutes <= 24 * 60 => "Session",
        Some(minutes) if minutes <= 7 * 24 * 60 => "Weekly",
        Some(_) => "Monthly",
        None => "Limit",
    };
    Some(LimitWindow {
        label: label.to_string(),
        used_percent: window["usedPercent"].as_f64()?,
        resets_at: window["resetsAt"].as_f64(),
        window_secs: minutes.map(|minutes| minutes * 60),
        warning: false,
    })
}

fn codex_plan(plan: &str) -> String {
    match plan {
        "prolite" => "Pro Lite".to_string(),
        "promax" => "Pro Max".to_string(),
        plan => capitalized(&plan.replace('_', " ")),
    }
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map(|first| first.to_uppercase().chain(characters).collect()).unwrap_or_default()
}

type Answers = std::collections::HashMap<String, Result<Value, String>>;

/// Writes `requests` to the agent's CLI and gathers what `answer` picks out of its lines, until
/// every request with an id has its answer.
async fn converse(
    environment: &Environment,
    agent: Agent,
    arguments: &[&str],
    requests: &[Value],
    answer: impl Fn(&Value) -> Option<(String, Result<Value, String>)>,
) -> anyhow::Result<Answers> {
    let executable = environment.executable(agent).ok_or_else(|| anyhow::anyhow!("It isn't installed."))?;
    let mut child = Command::new(executable)
        .args(arguments)
        .current_dir(std::env::temp_dir())
        .env_clear()
        .envs(&environment.variables)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    let input: String = requests.iter().map(|request| format!("{request}\n")).collect();
    stdin.write_all(input.as_bytes()).await?;
    let awaited =
        requests.iter().filter(|request| request["id"].is_string() || request["request_id"].is_string()).count();
    let gather = async {
        let mut answers = Answers::new();
        let mut lines = BufReader::new(stdout).lines();
        while answers.len() < awaited {
            let Some(line) = lines.next_line().await? else { break };
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
            answers.extend(answer(&message));
        }
        anyhow::Ok(answers)
    };
    let answers =
        tokio::time::timeout(TIMEOUT, gather).await.map_err(|_| anyhow::anyhow!("It didn't answer in time."))??;
    drop(stdin);
    Ok(answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_limits_are_read_from_its_list_with_a_model_s_own() {
        let account = json!({"email": "a@b.c", "subscriptionType": "Claude Max"});
        let usage = json!({"rate_limits_available": true, "rate_limits": {"limits": [
            {"kind": "session", "group": "session", "percent": 13, "resets_at": "2026-10-06T20:30:00+00:00", "severity": "normal", "scope": null},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 79, "resets_at": "2026-10-12T04:00:00+00:00", "severity": "warning",
             "scope": {"model": {"display_name": "Fable"}}}
        ]}});
        let limits = claude_limits(&account, &usage);
        assert_eq!(limits.account.as_deref(), Some("a@b.c"));
        assert_eq!(limits.plan.as_deref(), Some("Max"));
        let session = &limits.windows[0];
        assert_eq!((session.label.as_str(), session.used_percent, session.warning), ("Session", 13.0, false));
        assert_eq!(session.resets_at, Some(1791318600.0));
        assert_eq!(session.window_secs, Some(SESSION_SECS));
        let fable = &limits.windows[1];
        assert_eq!((fable.label.as_str(), fable.used_percent, fable.warning), ("Weekly · Fable", 79.0, true));
    }

    #[test]
    fn claude_code_without_its_list_has_its_session_and_week() {
        let usage = json!({"subscription_type": "pro", "rate_limits": {
            "five_hour": {"utilization": 8.0, "resets_at": "2026-10-06T20:30:00+00:00"},
            "seven_day": {"utilization": 45.0, "resets_at": null}
        }});
        let limits = claude_limits(&Value::Null, &usage);
        assert_eq!(limits.plan.as_deref(), Some("Pro"));
        let labels: Vec<_> = limits.windows.iter().map(|window| (window.label.as_str(), window.used_percent)).collect();
        assert_eq!(labels, [("Session", 8.0), ("Weekly", 45.0)]);
    }

    #[test]
    fn claude_code_with_an_api_key_has_no_windows() {
        let limits = claude_limits(&json!({"email": "a@b.c"}), &json!({"rate_limits_available": false}));
        assert!(limits.windows.is_empty() && limits.error.is_none());
    }

    #[test]
    fn codex_limits_are_its_windows_named_by_their_length() {
        let account = json!({"type": "chatgpt", "email": "a@b.c", "planType": "prolite"});
        let limits = json!({
            "rateLimits": {"primary": {"usedPercent": 99, "windowDurationMins": 10, "resetsAt": 1}},
            "rateLimitsByLimitId": {"codex": {
                "primary": {"usedPercent": 1, "windowDurationMins": 300, "resetsAt": 1791308081},
                "secondary": {"usedPercent": 4, "windowDurationMins": 10080, "resetsAt": 1791580413}
            }},
            "rateLimitResetCredits": {"availableCount": 2}
        });
        let limits = codex_limits(&account, Ok(limits)).unwrap();
        assert_eq!(limits.plan.as_deref(), Some("Pro Lite"));
        assert_eq!(limits.reset_credits, 2);
        let windows: Vec<_> =
            limits.windows.iter().map(|window| (window.label.as_str(), window.used_percent)).collect();
        assert_eq!(windows, [("Session", 1.0), ("Weekly", 4.0)]);
        assert_eq!(limits.windows[1].window_secs, Some(WEEK_SECS));
    }

    #[test]
    fn an_agent_that_cant_be_read_keeps_what_it_said_last_time() {
        let read = |agent, used_percent, error: Option<&str>| AgentLimits {
            agent,
            account_name: "Default".into(),
            account: Some("a@b.c".into()),
            plan: Some("Max".into()),
            windows: error
                .is_none()
                .then(|| LimitWindow {
                    label: "Session".into(),
                    used_percent,
                    resets_at: None,
                    window_secs: Some(SESSION_SECS),
                    warning: false,
                })
                .into_iter()
                .collect(),
            reset_credits: 0,
            error: error.map(String::from),
        };
        let before = [read(Agent::Claude, 10.0, None), read(Agent::Codex, 20.0, None)];
        let now = [read(Agent::Claude, 0.0, Some("It didn't answer in time.")), read(Agent::Codex, 30.0, None)];
        let merged = kept(now.to_vec(), &before);
        assert_eq!(merged[0].windows, before[0].windows);
        assert_eq!(merged[0].error.as_deref(), Some("It didn't answer in time."));
        assert_eq!(merged[1], now[1]);
        let first = kept(vec![read(Agent::Claude, 0.0, Some("no"))], &[]);
        assert!(first[0].windows.is_empty());
    }

    #[test]
    fn codex_with_an_api_key_has_no_windows_and_a_failed_read_says_why() {
        let api_key = codex_limits(&json!({"type": "apiKey"}), Err("unused".into())).unwrap();
        assert!(api_key.windows.is_empty() && api_key.error.is_none());
        let account = json!({"type": "chatgpt", "email": "a@b.c", "planType": "plus"});
        assert!(codex_limits(&account, Err("backend unavailable".into())).is_err());
    }
}
