//! Asking an agent's CLI what it knows without running a turn: Claude Code answers control
//! requests and `codex app-server` JSON-RPC requests, one JSON object a line. Neither spends tokens.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use motile_protocol::wire::Agent;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use super::environment::Environment;

const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// What each request was answered, under its id, or what the CLI refused it with.
pub type Answers = HashMap<String, Result<Value, String>>;

/// Claude Code's answers to the control requests of `subtypes`, each under its subtype, with
/// `arguments` added to the ones that have it read them.
pub async fn claude(environment: &Environment, arguments: &[&str], subtypes: &[&str]) -> anyhow::Result<Answers> {
    let mut command = vec!["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"];
    command.extend(arguments);
    let requests: Vec<Value> = subtypes
        .iter()
        .map(|subtype| json!({"type": "control_request", "request_id": subtype, "request": {"subtype": subtype}}))
        .collect();
    converse(environment, Agent::Claude, &command, &requests, |message| {
        let response = &message["response"];
        let id = response["request_id"].as_str().filter(|_| message["type"] == "control_response")?;
        let answer = match response["subtype"].as_str() {
            Some("success") => Ok(response["response"].clone()),
            _ => Err(response["error"].as_str().unwrap_or("Claude Code refused").to_string()),
        };
        Some((id.to_string(), answer))
    })
    .await
}

/// `codex app-server`'s answers to `requests`, each a method and its parameters, under its method.
pub async fn codex(environment: &Environment, requests: &[(&str, Value)]) -> anyhow::Result<Answers> {
    let client = json!({"name": "motile", "title": "Motile", "version": env!("CARGO_PKG_VERSION")});
    let mut sent = vec![
        json!({"id": "initialize", "method": "initialize", "params": {"clientInfo": client}}),
        json!({"method": "initialized"}),
    ];
    sent.extend(requests.iter().map(|(method, params)| {
        let mut request = json!({"id": method, "method": method});
        if !params.is_null() {
            request["params"] = params.clone();
        }
        request
    }));
    converse(environment, Agent::Codex, &["app-server"], &sent, |message| {
        let id = message["id"].as_str()?;
        let answer = match message["error"]["message"].as_str() {
            Some(error) => Err(error.to_string()),
            None => Ok(message["result"].clone()),
        };
        Some((id.to_string(), answer))
    })
    .await
}

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
    let answers = tokio::time::timeout(ANSWER_WITHIN, gather)
        .await
        .map_err(|_| anyhow::anyhow!("It didn't answer in time."))??;
    drop(stdin);
    Ok(answers)
}
