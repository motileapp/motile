//! The tokens a response spent, read from it on its way through: Anthropic's `message_start` and
//! `message_delta` events or its JSON, OpenAI's `response.completed` over SSE, JSON or a WebSocket.

use motile_protocol::wire::Agent;
use serde_json::Value;

/// Tokens of a response. `input` leaves out what was read from or written to the cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spent {
    pub model: Option<String>,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Spent {
    /// Takes in what one event of the agent's API says.
    pub fn read_event(&mut self, agent: Agent, event: &Value) {
        match agent {
            Agent::Claude => self.read_anthropic(event),
            Agent::Codex => self.read_openai(event),
        }
    }

    fn read_anthropic(&mut self, event: &Value) {
        let (model, usage) = match event["type"].as_str() {
            Some("message_start") => (&event["message"]["model"], &event["message"]["usage"]),
            Some("message_delta") => (&Value::Null, &event["usage"]),
            Some("message") => (&event["model"], &event["usage"]),
            _ => return,
        };
        if let Some(model) = model.as_str() {
            self.model = Some(model.to_string());
        }
        let count = |key: &str| usage[key].as_u64();
        // Counts in a delta are the totals so far.
        let keep = |kept: &mut u64, count: Option<u64>| *kept = count.unwrap_or(0).max(*kept);
        keep(&mut self.input, count("input_tokens"));
        keep(&mut self.output, count("output_tokens"));
        keep(&mut self.cache_read, count("cache_read_input_tokens"));
        keep(&mut self.cache_write, count("cache_creation_input_tokens"));
    }

    fn read_openai(&mut self, event: &Value) {
        let response = match event["type"].as_str() {
            Some("response.completed" | "response.done" | "response.incomplete" | "response.failed") => {
                &event["response"]
            }
            Some("response.created") => {
                self.read_openai_model(&event["response"]);
                return;
            }
            _ if event["object"] == "response" => event,
            _ => return,
        };
        self.read_openai_model(response);
        let usage = &response["usage"];
        if !usage.is_object() {
            return;
        }
        let cached = usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
        let written = usage["input_tokens_details"]["cache_write_tokens"].as_u64().unwrap_or(0);
        let input = usage["input_tokens"].as_u64().unwrap_or(0);
        self.input = input.saturating_sub(cached + written);
        self.cache_read = cached;
        self.cache_write = written;
        self.output = usage["output_tokens"].as_u64().unwrap_or(0);
    }

    fn read_openai_model(&mut self, response: &Value) {
        if let Some(model) = response["model"].as_str() {
            self.model = Some(model.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.input + self.output + self.cache_read + self.cache_write == 0
    }
}

/// Finds the events of an SSE stream as its bytes arrive, however they are cut.
#[derive(Default)]
pub struct EventScanner {
    pending: Vec<u8>,
}

impl EventScanner {
    /// The JSON of each `data:` line the chunk finished.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.pending.extend_from_slice(chunk);
        let Some(end) = self.pending.iter().rposition(|byte| *byte == b'\n') else { return vec![] };
        let complete: Vec<u8> = self.pending.drain(..=end).collect();
        complete
            .split(|byte| *byte == b'\n')
            .filter_map(|line| line.strip_prefix(b"data:"))
            .filter_map(|data| serde_json::from_slice(data.trim_ascii()).ok())
            .collect()
    }
}

/// What a whole response body spent: an SSE stream or one JSON object.
pub fn read_body(agent: Agent, body: &[u8]) -> Spent {
    let mut spent = Spent::default();
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        spent.read_event(agent, &value);
        return spent;
    }
    let mut scanner = EventScanner::default();
    let mut events = scanner.feed(body);
    events.extend(scanner.feed(b"\n"));
    for event in events {
        spent.read_event(agent, &event);
    }
    spent
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_anthropic_stream_cut_anywhere_says_its_model_and_final_counts() {
        let stream = concat!(
            "event: message_start\n",
            r#"data: {"type":"message_start","message":{"model":"claude-opus-4-5","usage":{"input_tokens":12,"#,
            r#""cache_creation_input_tokens":300,"cache_read_input_tokens":4000,"output_tokens":1}}}"#,
            "\n\nevent: content_block_delta\n",
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}"#,
            "\n\nevent: message_delta\n",
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":57}}"#,
            "\n\n"
        );
        let mut scanner = EventScanner::default();
        let mut spent = Spent::default();
        for chunk in stream.as_bytes().chunks(7) {
            for event in scanner.feed(chunk) {
                spent.read_event(Agent::Claude, &event);
            }
        }
        let expected =
            Spent { model: Some("claude-opus-4-5".into()), input: 12, output: 57, cache_read: 4000, cache_write: 300 };
        assert_eq!(spent, expected);
        assert_eq!(read_body(Agent::Claude, stream.as_bytes()), expected);
    }

    #[test]
    fn an_anthropic_reply_that_doesnt_stream_is_read_whole() {
        let body =
            json!({"type": "message", "model": "claude-haiku-4-5", "usage": {"input_tokens": 3, "output_tokens": 4}});
        let spent = read_body(Agent::Claude, body.to_string().as_bytes());
        assert_eq!((spent.model.as_deref(), spent.input, spent.output), (Some("claude-haiku-4-5"), 3, 4));
    }

    #[test]
    fn openai_counts_its_cached_tokens_inside_the_input() {
        let completed = json!({"type": "response.completed", "response": {"model": "gpt-5.5-codex", "usage": {
            "input_tokens": 10000, "input_tokens_details": {"cached_tokens": 9000},
            "output_tokens": 250, "output_tokens_details": {"reasoning_tokens": 200}}}});
        let stream = format!("event: response.completed\ndata: {completed}\n\n");
        let spent = read_body(Agent::Codex, stream.as_bytes());
        assert_eq!(
            spent,
            Spent { model: Some("gpt-5.5-codex".into()), input: 1000, output: 250, cache_read: 9000, cache_write: 0 }
        );
    }
}
