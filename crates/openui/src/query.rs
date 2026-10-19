//! Queries and mutations (language.md §6.1, §6.2, §6.4), without waiting on anything: the
//! manager says which tool calls to make and which timers to set, and the host hands the results
//! back.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::error::{Code, Error, Source};
use crate::js;
use crate::value::Value;

/// A tool call for the host to make, answered with [`Queries::complete`].
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: u64,
    pub tool: String,
    pub args: Value,
}

/// An interval for the host to keep, every `seconds`, until it is cancelled.
#[derive(Debug, Clone, PartialEq)]
pub struct Timer {
    pub id: u64,
    pub seconds: f64,
}

/// Why a tool call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// No tool has that name; `available` lists the ones there are.
    NotFound { available: Vec<String> },
    /// The tool failed.
    Failed(String),
    /// An MCP tool returned an error.
    Mcp(String),
}

/// What the manager asks of the host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Requests {
    pub calls: Vec<ToolCall>,
    pub timers: Vec<Timer>,
    pub cancel: Vec<u64>,
}

/// A query evaluated with the current state.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryNode {
    pub statement: String,
    pub tool: String,
    pub args: Value,
    pub defaults: Value,
    /// Seconds between fetches; 0 for none.
    pub refresh: f64,
    /// The values of the `$variables` written in its arguments.
    pub deps: Value,
}

/// What a finished tool call was for.
#[derive(Debug, Clone, PartialEq)]
pub enum Completion {
    Query,
    Mutation {
        statement: String,
        ok: bool,
    },
    /// A call the manager no longer waits for.
    Unknown,
}

#[derive(Debug)]
struct Query {
    tool: String,
    args: Value,
    defaults: Value,
    key: String,
    /// The key whose data shows while a fetch for changed arguments is in flight.
    previous: Option<String>,
    loading: bool,
    fetched: bool,
    refresh: f64,
    timer: Option<u64>,
    refetch: bool,
    error: Option<Error>,
}

#[derive(Debug, Default)]
struct Entry {
    data: Option<Value>,
    in_flight: bool,
}

#[derive(Debug)]
struct Mutation {
    tool: String,
    result: Value,
    error: Option<Error>,
}

#[derive(Debug)]
enum Pending {
    Query { statement: String, key: String },
    Mutation { statement: String },
}

#[derive(Debug, Default)]
pub struct Queries {
    queries: IndexMap<String, Query>,
    mutations: IndexMap<String, Mutation>,
    cache: HashMap<String, Entry>,
    pending: HashMap<u64, Pending>,
    timers: HashMap<u64, String>,
    versions: HashMap<String, u64>,
    clock: u64,
    next_id: u64,
}

impl Queries {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn touch(&mut self, statement: &str) {
        self.clock += 1;
        self.versions.insert(statement.to_owned(), self.clock);
    }

    /// A number that changes whenever what the statement reads as changes.
    pub fn version(&self, statement: &str) -> u64 {
        self.versions.get(statement).copied().unwrap_or_default()
    }

    pub fn has_query(&self, statement: &str) -> bool {
        self.queries.contains_key(statement)
    }

    /// The latest result, the previous one while arguments changed, or the defaults.
    pub fn result(&self, statement: &str) -> Option<Value> {
        let query = self.queries.get(statement)?;
        let data = |key: &str| self.cache.get(key).and_then(|entry| entry.data.clone());
        Some(
            data(&query.key)
                .or_else(|| query.previous.as_deref().and_then(data))
                .unwrap_or_else(|| query.defaults.clone()),
        )
    }

    /// `{status, data, error}`.
    pub fn mutation(&self, statement: &str) -> Option<Value> {
        self.mutations.get(statement).map(|mutation| mutation.result.clone())
    }

    /// The queries fetching now.
    pub fn loading(&self) -> Vec<&str> {
        self.queries.iter().filter(|(_, query)| query.loading).map(|(name, _)| name.as_str()).collect()
    }

    /// The queries fetching again, with an earlier result showing.
    pub fn refetching(&self) -> Vec<&str> {
        self.queries.iter().filter(|(_, query)| query.loading && query.fetched).map(|(name, _)| name.as_str()).collect()
    }

    /// The failed queries' and mutations' errors.
    pub fn errors(&self) -> Vec<Error> {
        let queries = self.queries.values().filter_map(|query| query.error.clone());
        queries.chain(self.mutations.values().filter_map(|mutation| mutation.error.clone())).collect()
    }

    /// Keeps only these queries, cancelling the others' timers.
    pub fn retain(&mut self, statements: &[&str], requests: &mut Requests) {
        let removed: Vec<String> =
            self.queries.keys().filter(|name| !statements.contains(&name.as_str())).cloned().collect();
        for name in removed {
            let Some(query) = self.queries.shift_remove(&name) else { continue };
            if let Some(timer) = query.timer {
                self.timers.remove(&timer);
                requests.cancel.push(timer);
            }
            self.cleanup(&query.key);
            if let Some(previous) = &query.previous {
                self.cleanup(previous);
            }
            self.touch(&name);
        }
    }

    /// Registers a query or updates it, fetching when its key has no data yet.
    pub fn update(&mut self, node: QueryNode, requests: &mut Requests) {
        let key = cache_key(&node.tool, &node.args, &node.deps);
        let before = self.result(&node.statement);
        match self.queries.get_mut(&node.statement) {
            Some(query) => {
                if query.key != key {
                    let old = std::mem::replace(&mut query.key, key.clone());
                    let shown = self.cache.get(&old).is_some_and(|entry| entry.data.is_some());
                    let query = self.queries.get_mut(&node.statement).expect("the query");
                    let dropped = if shown { query.previous.replace(old) } else { Some(old) };
                    if let Some(dropped) = dropped {
                        self.cleanup(&dropped);
                    }
                }
                let query = self.queries.get_mut(&node.statement).expect("the query");
                query.tool = node.tool;
                query.args = node.args;
                query.defaults = node.defaults;
            }
            None => {
                let query = Query {
                    tool: node.tool,
                    args: node.args,
                    defaults: node.defaults,
                    key: key.clone(),
                    previous: None,
                    loading: false,
                    fetched: false,
                    refresh: 0.0,
                    timer: None,
                    refetch: false,
                    error: None,
                };
                self.queries.insert(node.statement.clone(), query);
            }
        }
        let entry = self.cache.get(&key);
        let settled = entry.is_some_and(|entry| entry.data.is_some() && !entry.in_flight);
        if !settled && !entry.is_some_and(|entry| entry.in_flight) {
            self.fetch(&node.statement, requests);
        }
        self.set_refresh(&node.statement, node.refresh, requests);
        if self.result(&node.statement) != before {
            self.touch(&node.statement);
        }
    }

    fn set_refresh(&mut self, statement: &str, refresh: f64, requests: &mut Requests) {
        let refresh = if refresh.is_finite() && refresh > 0.0 { refresh } else { 0.0 };
        let Some(query) = self.queries.get(statement) else { return };
        if query.refresh == refresh {
            return;
        }
        let previous = query.timer;
        let timer = (refresh > 0.0).then(|| self.id());
        if let Some(previous) = previous {
            self.timers.remove(&previous);
            requests.cancel.push(previous);
        }
        if let Some(timer) = timer {
            self.timers.insert(timer, statement.to_owned());
            requests.timers.push(Timer { id: timer, seconds: refresh });
        }
        let query = self.queries.get_mut(statement).expect("the query");
        query.refresh = refresh;
        query.timer = timer;
    }

    fn fetch(&mut self, statement: &str, requests: &mut Requests) {
        let id = self.id();
        let Some(query) = self.queries.get_mut(statement) else { return };
        query.loading = true;
        let key = query.key.clone();
        requests.calls.push(ToolCall { id, tool: query.tool.clone(), args: call_args(&query.args) });
        self.cache.entry(key.clone()).or_default().in_flight = true;
        self.pending.insert(id, Pending::Query { statement: statement.to_owned(), key });
    }

    /// Fetches a query again (`@Run`, a retry); after the fetch in flight when there is one.
    pub fn invalidate(&mut self, statement: &str, requests: &mut Requests) {
        let Some(query) = self.queries.get_mut(statement) else { return };
        if self.cache.get(&query.key).is_some_and(|entry| entry.in_flight) {
            query.refetch = true;
            return;
        }
        self.fetch(statement, requests);
    }

    /// The statements of the queries that failed.
    pub fn failed(&self) -> Vec<String> {
        self.queries.iter().filter(|(_, query)| query.error.is_some()).map(|(name, _)| name.clone()).collect()
    }

    /// A refresh interval elapsed.
    pub fn timer(&mut self, timer: u64, requests: &mut Requests) {
        let Some(statement) = self.timers.get(&timer).cloned() else { return };
        let Some(query) = self.queries.get(&statement) else { return };
        if self.cache.get(&query.key).is_some_and(|entry| entry.in_flight) {
            return;
        }
        self.fetch(&statement, requests);
    }

    /// Registers the mutations, each with its tool. One whose tool changed is idle again.
    pub fn register_mutations(&mut self, mutations: &[(String, String)]) {
        let removed: Vec<String> = self
            .mutations
            .keys()
            .filter(|name| !mutations.iter().any(|(statement, _)| statement == *name))
            .cloned()
            .collect();
        for name in removed {
            self.mutations.shift_remove(&name);
            self.touch(&name);
        }
        for (statement, tool) in mutations {
            if self.mutations.get(statement).is_some_and(|mutation| mutation.tool == *tool) {
                continue;
            }
            let mutation =
                Mutation { tool: tool.clone(), result: status("idle", Value::Null, Value::Null), error: None };
            self.mutations.insert(statement.clone(), mutation);
            self.touch(statement);
        }
    }

    /// Runs a mutation with its arguments; None when there is no such mutation or it is
    /// running already.
    pub fn fire(&mut self, statement: &str, args: Value, requests: &mut Requests) -> Option<u64> {
        let mutation = self.mutations.get(statement)?;
        if mutation.result.get("status").and_then(Value::as_str) == Some("loading") {
            return None;
        }
        let tool = mutation.tool.clone();
        let id = self.id();
        let mutation = self.mutations.get_mut(statement).expect("the mutation");
        mutation.result = status("loading", Value::Null, Value::Null);
        requests.calls.push(ToolCall { id, tool, args: call_args(&args) });
        self.pending.insert(id, Pending::Mutation { statement: statement.to_owned() });
        self.touch(statement);
        Some(id)
    }

    /// A tool call's result.
    pub fn complete(&mut self, id: u64, result: Result<Value, ToolError>, requests: &mut Requests) -> Completion {
        match self.pending.remove(&id) {
            Some(Pending::Query { statement, key }) => {
                self.complete_query(&statement, &key, result, requests);
                Completion::Query
            }
            Some(Pending::Mutation { statement }) => {
                let Some(mutation) = self.mutations.get_mut(&statement) else { return Completion::Unknown };
                let ok = result.is_ok();
                match result {
                    Ok(data) => {
                        mutation.result = status("success", data, Value::Null);
                        mutation.error = None;
                    }
                    Err(error) => {
                        let message = error_text(&error);
                        mutation.error = Some(tool_error(Source::Mutation, &statement, &mutation.tool, &error));
                        mutation.result = status("error", Value::Null, Value::from(message));
                    }
                }
                self.touch(&statement);
                Completion::Mutation { statement, ok }
            }
            None => Completion::Unknown,
        }
    }

    fn complete_query(
        &mut self,
        statement: &str,
        key: &str,
        result: Result<Value, ToolError>,
        requests: &mut Requests,
    ) {
        if let Some(entry) = self.cache.get_mut(key) {
            entry.in_flight = false;
        }
        let current = self.queries.get(statement).is_some_and(|query| query.key == key);
        if !current {
            self.cleanup(key);
            return;
        }
        let before = self.result(statement);
        let query = self.queries.get_mut(statement).expect("the query");
        query.loading = false;
        match result {
            Ok(data) => {
                query.fetched = true;
                query.error = None;
                let previous = query.previous.take();
                self.cache.entry(key.to_owned()).or_default().data = Some(data);
                if let Some(previous) = previous.filter(|previous| previous != key) {
                    self.cleanup(&previous);
                }
            }
            Err(error) => query.error = Some(tool_error(Source::Query, statement, &query.tool, &error)),
        }
        let query = self.queries.get_mut(statement).expect("the query");
        if std::mem::take(&mut query.refetch) {
            self.fetch(statement, requests);
        }
        if self.result(statement) != before {
            self.touch(statement);
        }
    }

    /// Drops a cache entry no query refers to.
    fn cleanup(&mut self, key: &str) {
        let used = self.queries.values().any(|query| query.key == key || query.previous.as_deref() == Some(key));
        if !used {
            self.cache.remove(key);
        }
    }
}

fn status(status: &str, data: Value, error: Value) -> Value {
    Value::from_iter([("status", Value::from(status)), ("data", data), ("error", error)])
}

/// A call's arguments: an object, `{}` for none.
fn call_args(args: &Value) -> Value {
    match args {
        Value::Null => Value::Object(Default::default()),
        args => args.clone(),
    }
}

fn error_text(error: &ToolError) -> String {
    match error {
        ToolError::NotFound { available } => format!(
            "No handler for tool. Available: {}",
            if available.is_empty() { "(none)".to_owned() } else { available.join(", ") }
        ),
        ToolError::Failed(message) | ToolError::Mcp(message) => message.clone(),
    }
}

fn tool_error(source: Source, statement: &str, tool: &str, error: &ToolError) -> Error {
    let kind = if source == Source::Query { "Query" } else { "Mutation" };
    let error = match error {
        ToolError::NotFound { available } => {
            let error = Error::new(source, Code::ToolNotFound, format!("{kind} tool \"{tool}\" not found"));
            if available.is_empty() { error } else { error.hint(format!("Available tools: {}", available.join(", "))) }
        }
        ToolError::Mcp(message) => {
            Error::new(source, Code::McpError, format!("{kind} \"{tool}\" returned an error: {message}"))
        }
        ToolError::Failed(message) => {
            Error::new(source, Code::ToolError, format!("{kind} \"{tool}\" failed: {message}"))
        }
    };
    error.statement(Some(statement)).component(kind).tool(tool)
}

/// The tool, its arguments and the values of its dependencies, with object keys sorted.
fn cache_key(tool: &str, args: &Value, deps: &Value) -> String {
    let mut key = format!("{tool}::");
    stable(args, &mut key);
    if !deps.is_null() {
        key.push_str("::");
        stable(deps, &mut key);
    }
    key
}

fn stable(value: &Value, out: &mut String) {
    match value {
        Value::Number(number) if !number.is_finite() => {
            out.push_str(&format!("\"__{}__\"", js::number_to_string(*number)))
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                stable(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::from(key.as_str()).to_json_string());
                out.push(':');
                stable(&map[key], out);
            }
            out.push('}');
        }
        value => out.push_str(&value.to_json_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(statement: &str, status: &str, refresh: f64) -> QueryNode {
        QueryNode {
            statement: statement.into(),
            tool: "list".into(),
            args: Value::from_iter([("status", Value::from(status))]),
            defaults: Value::from_iter([("rows", Value::Array(Vec::new()))]),
            refresh,
            deps: Value::Null,
        }
    }

    #[test]
    fn keeps_the_last_result_while_arguments_change() {
        let mut queries = Queries::default();
        let mut requests = Requests::default();
        queries.update(node("q", "open", 0.0), &mut requests);
        assert_eq!(requests.calls.len(), 1);
        assert_eq!(queries.result("q").unwrap().to_json_string(), r#"{"rows":[]}"#);
        let first = requests.calls[0].id;
        queries.complete(first, Ok(Value::from("open rows")), &mut requests);
        assert_eq!(queries.result("q"), Some(Value::from("open rows")));

        queries.update(node("q", "closed", 0.0), &mut requests);
        let second = requests.calls[1].id;
        assert_eq!(queries.result("q"), Some(Value::from("open rows")));
        assert_eq!(queries.loading(), ["q"]);
        assert_eq!(queries.refetching(), ["q"]);
        queries.update(node("q", "all", 0.0), &mut requests);
        let third = requests.calls[2].id;
        queries.complete(second, Ok(Value::from("stale")), &mut requests);
        assert_eq!(queries.result("q"), Some(Value::from("open rows")));
        queries.complete(third, Ok(Value::from("all rows")), &mut requests);
        assert_eq!(queries.result("q"), Some(Value::from("all rows")));
        assert!(queries.loading().is_empty());

        queries.update(node("q", "open", 0.0), &mut requests);
        assert_eq!(requests.calls.len(), 4, "the open rows were dropped from the cache");
    }

    #[test]
    fn refreshes_on_a_timer_and_reports_errors() {
        let mut queries = Queries::default();
        let mut requests = Requests::default();
        queries.update(node("q", "open", 30.0), &mut requests);
        assert_eq!(requests.timers.len(), 1);
        let timer = requests.timers[0].id;
        queries.timer(timer, &mut requests);
        assert_eq!(requests.calls.len(), 1, "a fetch is in flight already");
        let call = requests.calls[0].id;
        queries.complete(call, Err(ToolError::NotFound { available: vec!["other".into()] }), &mut requests);
        let errors = queries.errors();
        assert_eq!(errors[0].code, Code::ToolNotFound);
        assert_eq!(errors[0].hint.as_deref(), Some("Available tools: other"));
        assert_eq!(queries.result("q").unwrap().to_json_string(), r#"{"rows":[]}"#);
        queries.timer(timer, &mut requests);
        assert_eq!(requests.calls.len(), 2);
        queries.retain(&[], &mut requests);
        assert_eq!(requests.cancel, [timer]);
    }

    #[test]
    fn runs_one_mutation_at_a_time() {
        let mut queries = Queries::default();
        let mut requests = Requests::default();
        queries.register_mutations(&[("m".into(), "create".into())]);
        assert_eq!(queries.mutation("m").unwrap().to_json_string(), r#"{"status":"idle","data":null,"error":null}"#);
        let id = queries.fire("m", Value::Null, &mut requests).unwrap();
        assert_eq!(requests.calls[0].args.to_json_string(), "{}");
        assert_eq!(queries.fire("m", Value::Null, &mut requests), None);
        let done = queries.complete(id, Ok(Value::from(7)), &mut requests);
        assert_eq!(done, Completion::Mutation { statement: "m".into(), ok: true });
        assert_eq!(queries.mutation("m").unwrap().to_json_string(), r#"{"status":"success","data":7,"error":null}"#);
    }
}
