//! The spec's conformance cases (`fixtures/openui/1.0`, language.md Appendix B and the fixtures'
//! README): every program case, batch and streamed, and every interaction replay.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use motile_openui::eval::{Evaluator, Inputs, idle_mutation};
use motile_openui::node::Dep;
use motile_openui::{
    ActionStep, Effects, Element, FieldSpec, FixedEnv, Forms, Library, Map, Parser, Program, Runtime, ToolError, Value,
    merge, rules, serialize,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/openui/1.0")
}

fn library() -> Library {
    let mut library = Library::from_spec_json(&fs::read_to_string(fixtures().join("library.json")).unwrap()).unwrap();
    library.implement("Upper", |args| Ok(Value::from(args.str("text").unwrap_or_default().to_uppercase()))).unwrap();
    library
}

fn cases() -> Vec<(String, PathBuf)> {
    let mut cases = Vec::new();
    for area in fs::read_dir(fixtures()).unwrap() {
        let area = area.unwrap().path();
        if !area.is_dir() {
            continue;
        }
        for case in fs::read_dir(&area).unwrap() {
            let case = case.unwrap().path();
            if case.is_dir() {
                let id = format!(
                    "{}/{}",
                    area.file_name().unwrap().to_string_lossy(),
                    case.file_name().unwrap().to_string_lossy()
                );
                cases.push((id, case));
            }
        }
    }
    cases.sort();
    cases
}

fn read(case: &Path, file: &str) -> Option<String> {
    fs::read_to_string(case.join(file)).ok()
}

/// The state at stream end: defaults only; queries read their defaults, mutations are idle.
struct EndInputs<'a> {
    program: &'a Program,
    state: Map,
    library: &'a Library,
}

impl Inputs for EndInputs<'_> {
    fn state(&self, name: &str) -> Value {
        self.state.get(name).cloned().unwrap_or_default()
    }

    fn query(&self, statement: &str) -> Value {
        let Some(query) = self.program.queries.iter().find(|query| query.statement == statement) else {
            return Value::Null;
        };
        let env = FixedEnv::default();
        Evaluator::new(self.library, &env, self).eval(&query.defaults, None)
    }

    fn mutation(&self, _statement: &str) -> Value {
        idle_mutation()
    }

    fn version(&self, _dep: &Dep) -> u64 {
        0
    }
}

struct Summary {
    tree: Option<Value>,
    root: Value,
    errors: Vec<Value>,
    error_objects: Vec<Value>,
    unresolved: Vec<String>,
    orphans: Vec<String>,
    incomplete: bool,
    state: Value,
}

fn summarize_program(library: &Library, program: &Program) -> Summary {
    let env = FixedEnv::default();
    let mut state = Map::new();
    for (name, node) in &program.state {
        let inputs = EndInputs { program, state: state.clone(), library };
        let value = Evaluator::new(library, &env, &inputs).eval(node, None);
        state.insert(name.clone(), value);
    }
    let inputs = EndInputs { program, state: state.clone(), library };
    let mut evaluator = Evaluator::new(library, &env, &inputs);
    let tree = program.root.as_ref().map(|root| evaluator.eval(root, None)).filter(|tree| tree.as_element().is_some());
    let mut errors: Vec<motile_openui::Error> = program.errors.clone();
    errors.extend(evaluator.errors.iter().cloned());
    let root = tree.as_ref().map_or(Value::Null, |tree| summarize(tree, &inputs, library));
    let error_objects: Vec<Value> =
        errors.iter().map(|error| Value::from(serde_json::to_value(error).unwrap())).collect();
    let errors = error_objects
        .iter()
        .map(|error| {
            let mut map = Map::new();
            map.insert("code".into(), error.get("code").cloned().unwrap_or_default());
            map.insert("statementId".into(), error.get("statementId").cloned().unwrap_or_default());
            Value::Object(map)
        })
        .collect();
    Summary {
        tree,
        root,
        errors,
        error_objects,
        unresolved: program.unresolved.clone(),
        orphans: program.orphans.clone(),
        incomplete: program.incomplete,
        state: Value::Object(state),
    }
}

fn summarize(value: &Value, inputs: &dyn Inputs, library: &Library) -> Value {
    match value {
        Value::Element(element) => {
            let mut props = Map::new();
            for (name, prop) in &element.props {
                props.insert(name.clone(), summarize(prop, inputs, library));
            }
            for (name, binding) in &element.bindings {
                props.insert(name.clone(), Value::from_iter([("$binding", Value::from(binding.variable.as_str()))]));
            }
            let props: Map = element
                .props
                .keys()
                .chain(element.bindings.keys())
                .filter_map(|name| Some((name.clone(), props.get(name)?.clone())))
                .filter(|(_, value)| !value.is_null())
                .collect();
            Value::from_iter([("type", Value::from(element.component.as_str())), ("props", Value::Object(props))])
        }
        Value::Plan(plan) => {
            let actions = plan.steps.iter().map(|step| summarize_step(step, inputs, library)).collect();
            Value::from_iter([("actions", Value::Array(actions))])
        }
        Value::Array(items) => items.iter().map(|item| summarize(item, inputs, library)).collect(),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(key, item)| (key.clone(), summarize(item, inputs, library))).collect())
        }
        value => value.clone(),
    }
}

fn summarize_step(step: &ActionStep, inputs: &dyn Inputs, library: &Library) -> Value {
    let (action, args): (&str, Vec<Value>) = match step {
        ActionStep::ToAssistant { message, context } => {
            let mut args = vec![Value::from(message.as_str())];
            args.extend(context.clone());
            ("ToAssistant", args)
        }
        ActionStep::OpenUrl { url } => ("OpenUrl", vec![Value::from(url.as_str())]),
        ActionStep::Set { variable, value } => {
            let env = FixedEnv::default();
            let value = Evaluator::new(library, &env, inputs).eval(value, None);
            ("Set", vec![Value::from(variable.as_str()), value])
        }
        ActionStep::Reset { variables } => ("Reset", variables.iter().map(|name| Value::from(name.as_str())).collect()),
        ActionStep::Run { statement, .. } => ("Run", vec![Value::from(statement.as_str())]),
        ActionStep::Custom { name, params } => {
            let keys: Vec<String> = library
                .action(name)
                .map(|action| action.params.iter().map(|param| param.name.clone()).collect())
                .unwrap_or_else(|| params.keys().cloned().collect());
            let args = keys.iter().map(|key| summarize(params.get(key).unwrap_or(&Value::Null), inputs, library));
            (name.as_str(), args.collect())
        }
    };
    Value::from_iter([("action", Value::from(action)), ("args", Value::Array(args))])
}

fn json(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

fn same(a: &Value, b: &Value) -> bool {
    serde_json::Value::from(a) == serde_json::Value::from(b)
}

/// Each expected entry claims an actual entry that agrees on the keys it lists.
fn claims(expected: &[Value], actual: &[Value]) -> bool {
    let mut pool: Vec<&Value> = actual.iter().collect();
    let mut expected: Vec<&Value> = expected.iter().collect();
    expected.sort_by_key(|entry| std::cmp::Reverse(entry.as_object().map_or(0, Map::len)));
    expected.iter().all(|entry| {
        let found = pool.iter().position(|candidate| {
            entry
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| same(value, candidate.get(key).unwrap_or(&Value::Null)))
        });
        found.map(|index| pool.remove(index)).is_some()
    })
}

fn as_set(names: &[String]) -> Vec<String> {
    let set: HashSet<&String> = names.iter().collect();
    let mut names: Vec<String> = set.into_iter().cloned().collect();
    names.sort();
    names
}

fn names(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_str).map(str::to_owned).collect()
}

fn compare(expected: &Value, summary: &Summary, statements: Option<&[String]>) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut differ = |key: &str, want: String, got: String| diffs.push(format!("{key}: expected {want}, got {got}"));
    let root = expected.get("root").unwrap();
    if !same(root, &summary.root) {
        differ("root", json(root), json(&summary.root));
    }
    let errors = expected.get("errors").and_then(Value::as_array).unwrap();
    if errors.len() != summary.errors.len() || !claims(errors, &summary.errors) {
        differ("errors", json(&Value::Array(errors.to_vec())), json(&Value::Array(summary.errors.clone())));
    }
    for (key, got) in [("unresolved", &summary.unresolved), ("orphans", &summary.orphans)] {
        let want = names(expected.get(key));
        if as_set(&want) != as_set(got) {
            differ(key, format!("{want:?}"), format!("{got:?}"));
        }
    }
    let incomplete = expected.get("incomplete").and_then(Value::as_bool).unwrap();
    if incomplete != summary.incomplete {
        differ("incomplete", incomplete.to_string(), summary.incomplete.to_string());
    }
    if let Some(state) = expected.get("state")
        && !same(state, &summary.state)
    {
        differ("state", json(state), json(&summary.state));
    }
    if let (Some(want), Some(got)) = (expected.get("statements"), statements) {
        let want = names(Some(want));
        if want != got {
            differ("statements", format!("{want:?}"), format!("{got:?}"));
        }
    }
    if let Some(objects) = expected.get("errorObjects").and_then(Value::as_array)
        && !claims(objects, &summary.error_objects)
    {
        differ(
            "errorObjects",
            json(&Value::Array(objects.to_vec())),
            json(&Value::Array(summary.error_objects.clone())),
        );
    }
    diffs
}

fn streamed(library: &Library, chunks: &[&str]) -> Program {
    let mut parser = Parser::new();
    let mut lowerer = motile_openui::Lowerer::default();
    let mut program = Program::default();
    for (index, chunk) in chunks.iter().enumerate() {
        let parsed = parser.push(chunk, index + 1 == chunks.len());
        program = Program::build(library, &parsed, &mut lowerer);
    }
    program
}

/// Splits a text into chunks of `size` characters.
fn chunked(text: &str, size: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let indices: Vec<usize> = text.char_indices().map(|(at, _)| at).chain([text.len()]).collect();
    while start + 1 < indices.len() {
        let end = (start + size).min(indices.len() - 1);
        chunks.push(&text[indices[start]..indices[end]]);
        start = end;
    }
    if chunks.is_empty() {
        chunks.push("");
    }
    chunks
}

fn run_program(library: &Library, id: &str, case: &Path) -> Vec<String> {
    let expected = Value::from_json_str(&read(case, "expected.json").unwrap()).unwrap();
    let mut diffs = Vec::new();
    let (text, statements): (String, Option<Vec<String>>) = match (read(case, "chunks.json"), read(case, "patch.oui")) {
        (Some(chunks), _) => {
            let chunks = Value::from_json_str(&chunks).unwrap();
            let chunks: Vec<&str> = chunks.as_array().unwrap().iter().map(|chunk| chunk.as_str().unwrap()).collect();
            let program = streamed(library, &chunks);
            diffs.extend(
                compare(&expected, &summarize_program(library, &program), None)
                    .into_iter()
                    .map(|diff| format!("streamed {diff}")),
            );
            (chunks.concat(), None)
        }
        (None, Some(patch)) => {
            let merged = merge(&read(case, "input.oui").unwrap(), &patch);
            let statements = merged
                .lines()
                .filter_map(|line| {
                    let name: String =
                        line.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').collect();
                    let rest = line[name.len()..].trim_start();
                    (!name.is_empty() && rest.starts_with('=') && !rest.starts_with("==")).then_some(name)
                })
                .collect();
            (merged, Some(statements))
        }
        (None, None) => (read(case, "input.oui").unwrap(), None),
    };
    let batch = Program::parse(library, &text);
    let summary = summarize_program(library, &batch);
    diffs.extend(compare(&expected, &summary, statements.as_deref()).into_iter().map(|diff| format!("batch {diff}")));
    for size in [1, 2, 3, 5, 8, 13, 64] {
        let program = streamed(library, &chunked(&text, size));
        let streamed = summarize_program(library, &program);
        if !same(&streamed.root, &summary.root)
            || json(&Value::Array(streamed.error_objects.clone())) != json(&Value::Array(summary.error_objects.clone()))
            || streamed.unresolved != summary.unresolved
            || streamed.orphans != summary.orphans
            || streamed.incomplete != summary.incomplete
            || !same(&streamed.state, &summary.state)
        {
            diffs.push(format!(
                "{id}: chunks of {size} differ from the batch parse: {} vs {}",
                json(&streamed.root),
                json(&summary.root)
            ));
        }
    }
    if id.contains("-roundtrip-")
        && let Some(Value::Element(tree)) = &summary.tree
    {
        let text = serialize(tree, library, None);
        let again = summarize_program(library, &Program::parse(library, &text));
        if !same(&again.root, expected.get("root").unwrap()) {
            diffs.push(format!("roundtrip: {text:?} parses to {}", json(&again.root)));
        }
    }
    diffs
}

#[test]
fn programs() {
    let library = library();
    let mut failures = Vec::new();
    let mut count = 0;
    for (id, case) in cases() {
        if read(&case, "validation.json").is_some() {
            continue;
        }
        count += 1;
        let diffs = run_program(&library, &id, &case);
        if !diffs.is_empty() {
            failures.push(format!("{id}\n  {}", diffs.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "{} of {count} failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn validation_rules() {
    let mut count = 0;
    for (id, case) in cases() {
        let Some(validation) = read(&case, "validation.json") else { continue };
        let validation = Value::from_json_str(&validation).unwrap();
        for entry in validation.get("cases").and_then(Value::as_array).unwrap() {
            count += 1;
            let error = rules::validate(entry.get("value").unwrap(), entry.get("rules").unwrap());
            let valid = entry.get("valid").and_then(Value::as_bool).unwrap();
            assert_eq!(error.is_none(), valid, "{id}: {} gives {error:?}", json(entry));
        }
    }
    assert_eq!(count, 19);
}

/// The test library's forms: `Form(name, ...)`, and an `Input` keeps a field under its name.
struct TestForms;

impl Forms for TestForms {
    fn form(&self, element: &Element) -> Option<String> {
        if element.component != "Form" {
            return None;
        }
        element.prop("name")?.as_str().map(str::to_owned)
    }

    fn field(&self, element: &Element) -> Option<FieldSpec> {
        if !matches!(element.component.as_str(), "Input" | "Select") {
            return None;
        }
        Some(FieldSpec {
            name: element.prop("name")?.as_str()?.to_owned(),
            prop: "value".into(),
            component_type: Some(element.component.to_lowercase()),
            rules: element.prop("rules").cloned(),
        })
    }
}

struct Host {
    runtime: Runtime,
    tools: HashMap<String, Value>,
    events: Vec<Value>,
    calls: Vec<Value>,
}

impl Host {
    /// Carries out what the runtime asked: every tool call answered at once.
    fn settle(&mut self, effects: Effects) {
        let mut queue = vec![effects];
        while let Some(effects) = queue.pop() {
            self.events.extend(effects.events.iter().map(|event| event.to_value()));
            for call in effects.calls {
                let result = match self.tools.get(&call.tool) {
                    Some(result) => {
                        self.calls.push(Value::from_iter([
                            ("tool", Value::from(call.tool.as_str())),
                            ("args", call.args.clone()),
                        ]));
                        Ok(result.clone())
                    }
                    None => Err(ToolError::NotFound { available: self.tools.keys().cloned().collect() }),
                };
                queue.push(self.runtime.tool_result(call.id, result));
            }
        }
    }
}

fn run_interactions(library: &Arc<Library>, program: &str, replay: &Value) -> Vec<String> {
    let steps = replay.get("interactions").and_then(Value::as_array).unwrap();
    let expected = replay.get("expected").unwrap();
    let runtime = Runtime::new(library.clone(), Arc::new(FixedEnv::default()), Arc::new(TestForms));
    let mut host = Host { runtime, tools: HashMap::new(), events: Vec::new(), calls: Vec::new() };
    let mut steps = steps.iter().peekable();
    while let Some(step) = steps.next_if(|step| step.get("tool").is_some()) {
        host.tools.insert(step.get("tool").unwrap().as_str().unwrap().to_owned(), step.get("result").unwrap().clone());
    }
    let effects = host.runtime.set_text(program, true);
    host.settle(effects);
    let mut diffs = Vec::new();
    for step in steps {
        if let Some(tool) = step.get("tool").and_then(Value::as_str) {
            host.tools.insert(tool.to_owned(), step.get("result").unwrap().clone());
            continue;
        }
        let (statement, text) = match (step.get("click"), step.get("type")) {
            (Some(click), _) => (click.as_str().unwrap(), None),
            (None, Some(typed)) => {
                (typed.as_array().unwrap()[0].as_str().unwrap(), typed.as_array().unwrap()[1].as_str())
            }
            _ => panic!("an unknown step {}", json(step)),
        };
        let Some(path) = host.runtime.find(statement) else {
            diffs.push(format!("nothing rendered for {statement}"));
            break;
        };
        let effects = match text {
            Some(text) => host.runtime.input(&path, "value", Value::from(text)),
            None => {
                let element = host.runtime.root().unwrap().clone();
                let label = locate(&element, &path).and_then(|element| element.prop("label").cloned());
                match locate(&element, &path).and_then(|element| element.prop("action")) {
                    Some(_) => host.runtime.press(&path, "action"),
                    None => host.runtime.ask(&path, label.as_ref().and_then(Value::as_str).unwrap_or_default()),
                }
            }
        };
        host.settle(effects);
    }

    let want = expected.get("events").and_then(Value::as_array).map(<[Value]>::to_vec).unwrap_or_default();
    let events_match = want.len() == host.events.len()
        && want.iter().zip(&host.events).all(|(want, got)| {
            want.as_object().unwrap().iter().all(|(key, value)| same(value, got.get(key).unwrap_or(&Value::Null)))
        });
    if !events_match {
        diffs.push(format!(
            "events: expected {}, got {}",
            json(&Value::Array(want)),
            json(&Value::Array(host.events.clone()))
        ));
    }
    for (name, value) in expected.get("state").and_then(Value::as_object).into_iter().flatten() {
        let got = host.runtime.state(name).cloned().unwrap_or_default();
        if !same(value, &got) {
            diffs.push(format!("state {name}: expected {}, got {}", json(value), json(&got)));
        }
    }
    if let Some(want) = expected.get("toolCalls") {
        let mut calls: Vec<Value> = Vec::new();
        for call in &host.calls {
            if calls.last().is_none_or(|last| !same(last, call)) {
                calls.push(call.clone());
            }
        }
        if !same(want, &Value::Array(calls.clone())) {
            diffs.push(format!("toolCalls: expected {}, got {}", json(want), json(&Value::Array(calls))));
        }
    }
    diffs
}

fn locate<'a>(root: &'a Element, path: &[motile_openui::Step]) -> Option<&'a Element> {
    let mut current: &Value = root.props.get(match path.first()? {
        motile_openui::Step::Key(key) => key,
        motile_openui::Step::Index(_) => return None,
    })?;
    for step in &path[1..] {
        current = match (step, current) {
            (motile_openui::Step::Key(key), Value::Element(element)) => element.props.get(key)?,
            (motile_openui::Step::Key(key), Value::Object(map)) => map.get(key)?,
            (motile_openui::Step::Index(index), Value::Array(items)) => items.get(*index)?,
            _ => return None,
        };
    }
    current.as_element()
}

#[test]
fn interactions() {
    let library = Arc::new(library());
    let mut failures = Vec::new();
    let mut count = 0;
    for (id, case) in cases() {
        let Some(replay) = read(&case, "interactions.json") else { continue };
        count += 1;
        let diffs =
            run_interactions(&library, &read(&case, "input.oui").unwrap(), &Value::from_json_str(&replay).unwrap());
        if !diffs.is_empty() {
            failures.push(format!("{id}\n  {}", diffs.join("\n  ")));
        }
    }
    assert_eq!(count, 12);
    assert!(failures.is_empty(), "{} of {count} failed:\n{}", failures.len(), failures.join("\n"));
}
