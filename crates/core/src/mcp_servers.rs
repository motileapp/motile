//! The McpServers page: the MCPs the account's gateway holds the logins of, as their cards show them,
//! the known ones the add sheet offers, the form a tool is tried with, and the calls agents made.

use motile_protocol::gateway_wire::{McpAuth, McpCall, McpCallDetail, McpKey, McpServer, McpStatus, McpTool};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::gateway::{Naming, latency, log_time, short_key};

/// How many calls a page of the activity has.
pub const CALLS_PAGE: u32 = 50;
/// The colours a mcp_server's tile takes, by its name.
const TINTS: [&str; 6] = ["chart-1", "chart-2", "chart-3", "chart-4", "chart-5", "chart-6"];

/// An MCP the add sheet offers by name.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Known {
    pub name: String,
    pub url: String,
    /// "mcp.linear.app".
    pub host: String,
    pub monogram: String,
    pub tint: String,
    /// What finds it in the sheet's search beyond its name.
    pub keywords: String,
}

/// The MCPs that run remotely and sign in themselves, or need no login.
const KNOWN: [(&str, &str, &str); 12] = [
    ("Linear", "https://mcp.linear.app/mcp", "issues tracker projects"),
    ("GitHub", "https://api.githubcopilot.com/mcp/", "code repositories pull requests issues"),
    ("Notion", "https://mcp.notion.com/mcp", "docs wiki pages notes"),
    ("Sentry", "https://mcp.sentry.dev/mcp", "errors crashes monitoring"),
    ("Atlassian", "https://mcp.atlassian.com/v1/mcp", "jira confluence issues wiki"),
    ("Supabase", "https://mcp.supabase.com/mcp", "database postgres"),
    ("Stripe", "https://mcp.stripe.com", "payments billing"),
    ("Vercel", "https://mcp.vercel.com", "deployments hosting"),
    ("Figma", "https://mcp.figma.com/mcp", "design"),
    ("Hugging Face", "https://huggingface.co/mcp", "models datasets ml"),
    ("Context7", "https://mcp.context7.com/mcp", "docs libraries documentation"),
    ("Cloudflare Docs", "https://docs.mcp.cloudflare.com/mcp", "documentation workers"),
];

pub fn known() -> Vec<Known> {
    KNOWN
        .iter()
        .map(|(name, url, keywords)| Known {
            name: name.to_string(),
            url: url.to_string(),
            host: host(url),
            monogram: monogram(name),
            tint: tint(name),
            keywords: keywords.to_string(),
        })
        .collect()
}

/// A mcp_server as its card shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct McpServerRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub url: String,
    pub host: String,
    pub monogram: String,
    pub tint: String,
    pub enabled: bool,
    pub auth: McpAuth,
    pub state: McpServerState,
    /// "Signed in · mcp.linear.app", "Off · agents don't get it", what went wrong.
    pub status_text: String,
    /// "30 of 32 tools on", "No tools".
    pub tools_text: String,
    pub tools: Vec<ToolRow>,
    /// Some of its tools write and some only read: "Read Only" changes something.
    pub has_read_only: bool,
    /// How agents name one of its tools: `mcp__linear__create_issue`.
    pub example: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerState {
    Ready,
    Off,
    SignInAgain,
    Failing,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ToolRow {
    pub name: String,
    /// Its title, or its name made readable: "Create issue".
    pub title: String,
    /// The first sentence of its description.
    pub summary: Option<String>,
    pub writes: bool,
    pub enabled: bool,
}

pub fn rows(mcp_servers: &[McpServer]) -> Vec<McpServerRow> {
    mcp_servers.iter().map(row).collect()
}

fn row(mcp_server: &McpServer) -> McpServerRow {
    let on = mcp_server.tools.iter().filter(|tool| tool.enabled).count();
    let total = mcp_server.tools.len();
    let tools_text = match total {
        0 => "No tools".to_string(),
        _ if on == total => format!("{total} {}", plural(total, "tool")),
        _ => format!("{on} of {total} tools on"),
    };
    let (state, status_text) = match &mcp_server.status {
        _ if !mcp_server.enabled => (McpServerState::Off, "Off · agents don't get it".to_string()),
        McpStatus::SignInAgain => (McpServerState::SignInAgain, "Its login stopped working".to_string()),
        McpStatus::Failing { error } => (McpServerState::Failing, error.clone()),
        McpStatus::Ready => {
            let login = match mcp_server.auth {
                McpAuth::SignIn => "Signed in",
                McpAuth::Key => "With a key",
                McpAuth::None => "Needs no login",
            };
            (McpServerState::Ready, format!("{login} · {}", host(&mcp_server.url)))
        }
    };
    let writes = mcp_server.tools.iter().filter(|tool| tool.writes).count();
    let example_tool = mcp_server.tools.iter().find(|tool| tool.writes).or(mcp_server.tools.first());
    McpServerRow {
        id: mcp_server.id.clone(),
        slug: mcp_server.slug.clone(),
        name: mcp_server.name.clone(),
        url: mcp_server.url.clone(),
        host: host(&mcp_server.url),
        monogram: monogram(&mcp_server.name),
        tint: tint(&mcp_server.name),
        enabled: mcp_server.enabled,
        auth: mcp_server.auth,
        state,
        status_text,
        tools_text,
        tools: mcp_server.tools.iter().map(tool_row).collect(),
        has_read_only: writes > 0 && writes < total,
        example: agent_name(&mcp_server.slug, example_tool.map_or("tool", |tool| tool.name.as_str())),
    }
}

/// How agents name a mcp_server's tool.
pub fn agent_name(slug: &str, tool: &str) -> String {
    format!("mcp__{slug}__{tool}")
}

fn tool_row(tool: &McpTool) -> ToolRow {
    ToolRow {
        name: tool.name.clone(),
        title: tool.title.clone().unwrap_or_else(|| readable(&tool.name)),
        summary: tool.description.as_deref().and_then(summary),
        writes: tool.writes,
        enabled: tool.enabled,
    }
}

/// `create_issue` and `createIssue` are "Create issue".
fn readable(name: &str) -> String {
    let mut words = String::new();
    let mut last_lower = false;
    for character in name.chars() {
        if character == '_' || character == '-' || character == '.' {
            words.push(' ');
            last_lower = false;
            continue;
        }
        if character.is_uppercase() && last_lower {
            words.push(' ');
        }
        last_lower = character.is_lowercase() || character.is_ascii_digit();
        words.extend(character.to_lowercase());
    }
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = words.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => name.to_string(),
    }
}

/// The description's first sentence, on one line.
fn summary(description: &str) -> Option<String> {
    let line = description.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let end = line.find(". ").map_or(line.len(), |end| end + 1);
    Some(line[..end].trim().to_string())
}

/// The tile's letter: the name's first letter or digit.
pub fn monogram(name: &str) -> String {
    name.chars()
        .find(|character| character.is_alphanumeric())
        .map_or("?".to_string(), |first| first.to_uppercase().collect())
}

/// The tile's colour, the same for a name everywhere.
pub fn tint(name: &str) -> String {
    let sum = name.to_lowercase().bytes().fold(0usize, |sum, byte| sum.wrapping_mul(31).wrapping_add(byte as usize));
    TINTS[sum % TINTS.len()].to_string()
}

fn host(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|url| url.host_str().map(String::from)).unwrap_or_else(|| url.to_string())
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 { word.to_string() } else { format!("{word}s") }
}

/// The key as the mcp_server takes it: in `Authorization` unless another header is named, after
/// `Bearer` unless it says its scheme.
pub fn key(header: &str, value: &str) -> Option<McpKey> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let header = match header.trim() {
        "" => "Authorization",
        header => header,
    };
    let bare = header.eq_ignore_ascii_case("authorization") && !value.contains(' ');
    let value = if bare { format!("Bearer {value}") } else { value.to_string() };
    Some(McpKey { header: header.to_string(), value })
}

/// The state and code of the address a sign-in came back to, `motile://mcp?code=…&state=…`.
pub fn sent_back(address: &str) -> Result<(String, String), String> {
    let address = url::Url::parse(address).map_err(|_| "The sign-in came back without a code.".to_string())?;
    let value = |name: &str| address.query_pairs().find(|(key, _)| key == name).map(|(_, value)| value.into_owned());
    if let Some(error) = value("error") {
        let said = value("error_description").unwrap_or(error);
        return Err(format!("The mcp_server didn't sign in: {said}"));
    }
    match (value("state"), value("code")) {
        (Some(state), Some(code)) => Ok((state, code)),
        _ => Err("The sign-in came back without a code.".to_string()),
    }
}

/// Whether what the add sheet's field holds is an address rather than a search.
pub fn is_address(text: &str) -> bool {
    let text = text.trim();
    (text.starts_with("https://") || text.starts_with("http://")) && text.len() > "https://".len()
}

/// A field of the form a tool is tried with.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub description: Option<String>,
    pub kind: FieldKind,
    pub required: bool,
    /// The choices of a `Choice`.
    pub choices: Vec<String>,
    /// What the field starts with: the schema's default, or `{}`/`[]` for JSON.
    pub value: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Switch,
    Choice,
    /// An object or a list, written as JSON.
    Json,
}

/// The fields of a tool's input schema, its required ones first.
pub fn fields(schema: &Value) -> Vec<Field> {
    let required: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let Some(properties) = schema["properties"].as_object() else { return vec![] };
    let mut fields: Vec<Field> = properties
        .iter()
        .map(|(name, property)| {
            let kind_name = match &property["type"] {
                Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).find(|kind| *kind != "null"),
                kind => kind.as_str(),
            };
            let choices: Vec<String> = property["enum"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|choice| choice.as_str().map_or_else(|| choice.to_string(), String::from))
                .collect();
            let kind = match kind_name {
                _ if !choices.is_empty() => FieldKind::Choice,
                Some("number" | "integer") => FieldKind::Number,
                Some("boolean") => FieldKind::Switch,
                Some("object" | "array") => FieldKind::Json,
                _ => FieldKind::Text,
            };
            let value = match &property["default"] {
                Value::Null => match kind {
                    FieldKind::Switch => "false".to_string(),
                    FieldKind::Json if kind_name == Some("array") => "[]".to_string(),
                    FieldKind::Json => "{}".to_string(),
                    _ => String::new(),
                },
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            Field {
                name: name.clone(),
                description: property["description"].as_str().and_then(summary),
                kind,
                required: required.contains(&name.as_str()),
                choices,
                value,
            }
        })
        .collect();
    fields.sort_by_key(|field| !field.required);
    fields
}

/// The tool's arguments from what its fields hold; a field left empty that isn't required is left
/// out.
pub fn arguments(schema: &Value, values: &Map<String, Value>) -> Result<Value, String> {
    let mut arguments = Map::new();
    for field in fields(schema) {
        let text = values.get(&field.name).and_then(Value::as_str).unwrap_or_default().trim();
        if text.is_empty() {
            if field.required && field.kind != FieldKind::Switch {
                return Err(format!("Fill in {}.", field.name));
            }
            continue;
        }
        let integer = schema["properties"][&field.name]["type"] == "integer";
        let value = match field.kind {
            FieldKind::Text | FieldKind::Choice => json!(text),
            FieldKind::Switch => json!(text == "true"),
            FieldKind::Number if integer => {
                json!(text.parse::<i64>().map_err(|_| format!("{} is a whole number.", field.name))?)
            }
            FieldKind::Number => json!(text.parse::<f64>().map_err(|_| format!("{} is a number.", field.name))?),
            FieldKind::Json => serde_json::from_str(text).map_err(|_| format!("{} isn't valid JSON.", field.name))?,
        };
        arguments.insert(field.name, value);
    }
    Ok(Value::Object(arguments))
}

/// What a tool answered: its text, and the whole answer as JSON.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Answered {
    pub text: String,
    pub json: String,
    pub error: Option<String>,
}

pub fn answered(result: &Value, error: Option<String>) -> Answered {
    let texts: Vec<&str> =
        result["content"].as_array().into_iter().flatten().filter_map(|part| part["text"].as_str()).collect();
    let text = match texts.is_empty() {
        true if result["structuredContent"].is_null() => pretty(result),
        true => pretty(&result["structuredContent"]),
        false => texts.join("\n\n"),
    };
    Answered { text, json: pretty(result), error }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// JSON text laid out to read, or as it is when it isn't JSON.
fn readable_json(text: &str) -> String {
    serde_json::from_str::<Value>(text).map(|value| pretty(&value)).unwrap_or_else(|_| text.to_string())
}

/// A call in the activity, as its row shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CallRow {
    pub id: i64,
    pub at: f64,
    pub time: String,
    pub mcp_server_id: String,
    /// The mcp_server's name, or "Removed mcp_server".
    pub mcp_server: String,
    pub monogram: String,
    pub tint: String,
    pub tool: String,
    pub server_id: Option<String>,
    pub thread_id: Option<String>,
    /// "Fix the sync race on studio", or "Tried in Settings".
    pub source: String,
    pub latency: String,
    pub ok: bool,
    pub error: Option<String>,
}

pub fn call_rows(
    calls: &[McpCall],
    mcp_servers: &[McpServer],
    names: &dyn Naming,
    now: f64,
    utc_offset_secs: i32,
) -> Vec<CallRow> {
    calls
        .iter()
        .map(|call| {
            let name = mcp_servers
                .iter()
                .find(|mcp_server| mcp_server.id == call.mcp_server)
                .map_or_else(|| "Removed mcp_server".to_string(), |mcp_server| mcp_server.name.clone());
            let source = match (&call.server, &call.thread) {
                (Some(server), Some(thread)) => {
                    let title = names.thread(server, thread).unwrap_or_else(|| "A thread".to_string());
                    let server = names.server(server).unwrap_or_else(|| short_key(server));
                    format!("{title} on {server}")
                }
                _ => "Tried in Settings".to_string(),
            };
            CallRow {
                id: call.id,
                at: call.at,
                time: log_time(call.at, now, utc_offset_secs),
                mcp_server_id: call.mcp_server.clone(),
                monogram: monogram(&name),
                tint: tint(&name),
                mcp_server: name,
                tool: call.tool.clone(),
                server_id: call.server.clone(),
                thread_id: call.thread.clone(),
                source,
                latency: latency(call.latency_ms),
                ok: call.error.is_none(),
                error: call.error.as_deref().map(|error| error.lines().next().unwrap_or_default().to_string()),
            }
        })
        .collect()
}

/// A call in full.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CallView {
    pub row: CallRow,
    pub arguments: String,
    pub result: String,
    pub error: Option<String>,
}

pub fn call_view(detail: McpCallDetail, row: CallRow) -> CallView {
    CallView {
        arguments: readable_json(&detail.arguments),
        result: readable_json(&detail.result),
        error: detail.call.error,
        row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mcp_server(tools: Vec<McpTool>, status: McpStatus, enabled: bool) -> McpServer {
        McpServer {
            id: "c1".into(),
            slug: "linear".into(),
            name: "Linear".into(),
            url: "https://mcp.linear.app/mcp".into(),
            enabled,
            auth: McpAuth::SignIn,
            status,
            tools,
            icon: None,
        }
    }

    fn tool(name: &str, writes: bool, enabled: bool) -> McpTool {
        McpTool {
            name: name.into(),
            title: None,
            description: Some("Lists the issues. Filters by team.\nMore.".into()),
            input_schema: json!({}),
            writes,
            enabled,
        }
    }

    #[test]
    fn a_card_says_how_its_mcp_server_stands_and_how_agents_name_its_tools() {
        let tools = vec![tool("list_issues", false, true), tool("createIssue", true, false)];
        let ready = row(&mcp_server(tools.clone(), McpStatus::Ready, true));
        assert_eq!(ready.status_text, "Signed in · mcp.linear.app");
        assert_eq!(ready.tools_text, "1 of 2 tools on");
        assert_eq!((ready.host.as_str(), ready.monogram.as_str()), ("mcp.linear.app", "L"));
        assert_eq!(ready.example, "mcp__linear__createIssue");
        assert!(ready.has_read_only);
        assert_eq!(ready.tools[0].title, "List issues");
        assert_eq!(ready.tools[1].title, "Create issue");
        assert_eq!(ready.tools[0].summary.as_deref(), Some("Lists the issues."));

        let off = row(&mcp_server(tools.clone(), McpStatus::Ready, false));
        assert_eq!((off.state, off.status_text.as_str()), (McpServerState::Off, "Off · agents don't get it"));
        let again = row(&mcp_server(tools, McpStatus::SignInAgain, true));
        assert_eq!(again.state, McpServerState::SignInAgain);
        let all_read = row(&mcp_server(vec![tool("a", false, true)], McpStatus::Ready, true));
        assert_eq!((all_read.tools_text.as_str(), all_read.has_read_only), ("1 tool", false));
        assert_eq!(tint("Linear"), tint("linear"));
    }

    #[test]
    fn a_tools_form_follows_its_schema_and_makes_its_arguments() {
        let schema = json!({
            "type": "object",
            "required": ["title"],
            "properties": {
                "priority": {"type": "integer", "default": 2},
                "title": {"type": "string", "description": "What it says."},
                "state": {"type": "string", "enum": ["open", "closed"]},
                "urgent": {"type": "boolean"},
                "labels": {"type": ["array", "null"]},
            },
        });
        let made = fields(&schema);
        let kinds: Vec<(&str, FieldKind, bool)> =
            made.iter().map(|field| (field.name.as_str(), field.kind, field.required)).collect();
        assert_eq!(kinds[0], ("title", FieldKind::Text, true));
        assert!(kinds.contains(&("priority", FieldKind::Number, false)));
        assert!(kinds.contains(&("state", FieldKind::Choice, false)));
        assert!(kinds.contains(&("urgent", FieldKind::Switch, false)));
        assert!(kinds.contains(&("labels", FieldKind::Json, false)));
        assert_eq!(made.iter().find(|field| field.name == "priority").unwrap().value, "2");

        let values = |pairs: &[(&str, &str)]| {
            pairs.iter().map(|(name, value)| (name.to_string(), json!(value))).collect::<Map<String, Value>>()
        };
        assert_eq!(arguments(&schema, &values(&[])), Err("Fill in title.".to_string()));
        let made = arguments(
            &schema,
            &values(&[("title", "Hi"), ("priority", "3"), ("urgent", "true"), ("labels", "[\"a\"]")]),
        );
        assert_eq!(made, Ok(json!({"title": "Hi", "priority": 3, "urgent": true, "labels": ["a"]})));
        assert_eq!(
            arguments(&schema, &values(&[("title", "Hi"), ("priority", "x")])),
            Err("priority is a whole number.".into())
        );
    }

    #[test]
    fn an_answer_shows_its_text_and_its_json() {
        let result = json!({"content": [{"type": "text", "text": "MOT-1"}, {"type": "text", "text": "MOT-2"}]});
        assert_eq!(answered(&result, None).text, "MOT-1\n\nMOT-2");
        assert!(answered(&json!({"structuredContent": {"a": 1}, "content": []}), None).text.contains("\"a\": 1"));
    }

    #[test]
    fn a_bare_key_goes_after_bearer_in_authorization() {
        let key = |header: &str, value: &str| super::key(header, value).map(|key| (key.header, key.value));
        assert_eq!(key("", " sk-1 "), Some(("Authorization".into(), "Bearer sk-1".into())));
        assert_eq!(key("Authorization", "Token sk-1"), Some(("Authorization".into(), "Token sk-1".into())));
        assert_eq!(key("x-api-key", "sk-1"), Some(("x-api-key".into(), "sk-1".into())));
        assert_eq!(key("", "  "), None);
    }

    #[test]
    fn a_sign_in_comes_back_with_its_state_and_code_or_what_went_wrong() {
        assert_eq!(sent_back("motile://mcp?code=c1&state=s1"), Ok(("s1".into(), "c1".into())));
        assert_eq!(
            sent_back("motile://mcp?error=access_denied&error_description=No"),
            Err("The mcp_server didn't sign in: No".into())
        );
        assert!(sent_back("motile://mcp").is_err());
    }

    #[test]
    fn an_address_is_told_from_a_search() {
        assert!(is_address(" https://mcp.linear.app/mcp"));
        assert!(!is_address("https://"));
        assert!(!is_address("linear"));
    }
}
