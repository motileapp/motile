//! What the conformance cases leave out, run on the conformance library: `@Each`, function
//! checks, the queries' lifecycle, plans, bindings, forms, errors and the evaluator's cache.

use std::sync::Arc;

use motile_openui::{
    Code, Effects, Element, FieldSpec, FixedEnv, FnError, Forms, Library, Program, Runtime, Step, ToolError, Value,
};

fn library() -> Arc<Library> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/openui/1.0/library.json");
    let mut library = Library::from_spec_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    library
        .implement("Upper", |args| match args.str("text") {
            Some("boom") => Err(FnError::new("no shouting")),
            text => Ok(Value::from(text.unwrap_or_default().to_uppercase())),
        })
        .unwrap();
    Arc::new(library)
}

struct TestForms;

impl Forms for TestForms {
    fn form(&self, element: &Element) -> Option<String> {
        (element.component == "Form").then(|| element.prop("name")?.as_str().map(str::to_owned)).flatten()
    }

    fn field(&self, element: &Element) -> Option<FieldSpec> {
        (element.component == "Input").then(|| FieldSpec {
            name: element.prop("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
            prop: "value".into(),
            component_type: Some("input".into()),
            rules: element.prop("rules").cloned(),
        })
    }
}

fn runtime(text: &str) -> (Runtime, Effects) {
    let mut runtime = Runtime::new(library(), Arc::new(FixedEnv::default()), Arc::new(TestForms));
    let effects = runtime.set_text(text, true);
    (runtime, effects)
}

fn tree(runtime: &Runtime) -> String {
    summary(&Value::Element(runtime.root().unwrap().clone()))
}

/// Components as `Type(prop: value)`, for short assertions.
fn summary(value: &Value) -> String {
    match value {
        Value::Element(element) => {
            let props: Vec<String> =
                element.props.iter().map(|(name, value)| format!("{name}: {}", summary(value))).collect();
            format!("{}({})", element.component, props.join(", "))
        }
        Value::Array(items) => format!("[{}]", items.iter().map(summary).collect::<Vec<_>>().join(", ")),
        Value::Plan(_) => "plan".to_owned(),
        value => value.to_json_string(),
    }
}

fn path(runtime: &Runtime, statement: &str) -> Vec<Step> {
    runtime.find(statement).unwrap_or_else(|| panic!("no {statement}"))
}

fn codes(errors: &[motile_openui::Error]) -> Vec<(Code, Option<&str>)> {
    errors.iter().map(|error| (error.code, error.statement_id.as_deref())).collect()
}

#[test]
fn each_captures_its_element_in_actions() {
    let (mut runtime, _) = runtime(
        "$sel = \"none\"\nitems = [{id: \"a\", name: \"A\"}, {id: \"b\", name: \"B\"}]\nroot = Stack([Text($sel), Stack(@Each(items, \"t\", Button(t.name, @Set($sel, t.id))))])",
    );
    assert_eq!(
        tree(&runtime),
        r#"Stack(children: [Text(text: "none"), Stack(children: [Button(label: "A", action: plan), Button(label: "B", action: plan)])])"#
    );
    let second = vec![Step::Key("children".into()), Step::Index(1), Step::Key("children".into()), Step::Index(1)];
    let effects = runtime.press(&second, "action");
    assert!(effects.render && effects.state);
    assert_eq!(runtime.state("$sel"), Some(&Value::from("b")));
}

#[test]
fn each_shadows_statements_and_outer_loops() {
    let program = Program::parse(
        &library(),
        "t = \"statement\"\nroot = Stack(@Each([[1, 2], [3]], \"t\", Stack(@Each(t, \"t\", Text(\"\" + t)))))",
    );
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    assert_eq!(program.orphans, ["t"]);
    let (runtime, _) = runtime("root = Stack(@Each([[1, 2], [3]], \"t\", Stack(@Each(t, \"t\", Text(\"\" + t)))))");
    assert_eq!(
        tree(&runtime),
        r#"Stack(children: [Stack(children: [Text(text: "1"), Text(text: "2")]), Stack(children: [Text(text: "3")])])"#
    );
}

#[test]
fn leaves_unresolved_elements_out_of_computed_arrays() {
    let (runtime, _) = runtime("root = Stack([Metric(\"n\", @Count([a, b]))])\na = 1");
    assert!(tree(&runtime).contains("value: 1"), "{}", tree(&runtime));
}

#[test]
fn checks_function_arguments_when_they_run() {
    let (runtime, effects) = runtime(
        "$n = 5\n$s = \"boom\"\nroot = Stack([Text(@Upper($n) == null ? \"null\" : \"set\"), Text(@Upper($s) == null ? \"threw\" : \"ok\"), Text(Upper(\"x\"))])",
    );
    let errors = effects.errors.unwrap();
    assert_eq!(
        codes(&errors),
        [
            (Code::UnknownComponent, Some("root")),
            (Code::NullRequired, Some("root")),
            (Code::RuntimeError, Some("root")),
            (Code::RuntimeError, Some("root"))
        ]
    );
    assert!(
        errors[2].message.contains("@Upper: field \"/text\" expects string but got number"),
        "{}",
        errors[2].message
    );
    assert!(errors[3].message.contains("@Upper threw: no shouting"), "{}", errors[3].message);
    assert_eq!(tree(&runtime), r#"Stack(children: [Text(text: "null"), Text(text: "threw")])"#);
}

#[test]
fn resolves_references_in_query_arguments() {
    let (runtime, effects) = runtime("root = Stack([])\npicked = 4\ndata = Query(\"get\", {id: picked}, {})");
    assert_eq!(effects.calls.len(), 1);
    assert_eq!(effects.calls[0].args.to_json_string(), r#"{"id":4}"#);
    assert!(runtime.program().orphans.is_empty());
}

#[test]
fn fetches_again_only_for_variables_written_in_the_arguments() {
    let (mut runtime, effects) = runtime(
        "$status = \"open\"\n$page = 1\nfilter = $page\nroot = Stack([Input(\"status\", \"Status\", $status), Input(\"page\", \"Page\", $page), Metric(\"n\", @Count(tickets.rows))])\ntickets = Query(\"list\", {status: $status, page: filter}, {rows: []}, 30)",
    );
    assert_eq!(effects.calls.len(), 1);
    assert_eq!(effects.timers.len(), 1);
    let call = effects.calls[0].id;
    let effects = runtime.tool_result(call, Ok(Value::from_json_str(r#"{"rows": [1, 2, 3]}"#).unwrap()));
    assert!(effects.render);
    assert!(tree(&runtime).contains("value: 3"));

    let page = path(&runtime, "root");
    let mut page_input = page.clone();
    page_input.extend([Step::Key("children".into()), Step::Index(1)]);
    let effects = runtime.input(&page_input, "value", Value::from(2));
    assert!(effects.calls.is_empty(), "state reached through a statement does not fetch");

    let mut status = page;
    status.extend([Step::Key("children".into()), Step::Index(0)]);
    let effects = runtime.input(&status, "value", Value::from("closed"));
    assert_eq!(effects.calls.len(), 1);
    assert_eq!(effects.calls[0].args.to_json_string(), r#"{"status":"closed","page":2}"#);
    assert!(tree(&runtime).contains("value: 3"), "the last result shows while fetching");
    assert_eq!(runtime.loading(), (vec!["tickets"], vec!["tickets"]));

    let fetch = effects.calls[0].id;
    let effects = runtime.tool_result(fetch, Err(ToolError::Failed("offline".into())));
    assert_eq!(codes(&effects.errors.unwrap()), [(Code::ToolError, Some("tickets"))]);
    let effects = runtime.retry();
    assert_eq!(effects.calls.len(), 1);
    let timer = runtime.timer(1);
    assert!(timer.calls.is_empty(), "a fetch is in flight");
}

#[test]
fn awaits_a_mutation_before_the_rest_of_its_plan() {
    let program = "$title = \"Bug\"\nroot = Stack([create, Text(newTicket.status)])\nnewTicket = Mutation(\"createTicket\", {title: $title})\ncreate = Button(\"Create\", Action([@Run(newTicket), @Set($title, \"\"), @ToAssistant(\"Created\")]))";
    let (mut runtime, effects) = runtime(program);
    assert!(effects.calls.is_empty(), "a mutation never runs on load");
    let create = path(&runtime, "create");
    let effects = runtime.press(&create, "action");
    assert_eq!(effects.calls.len(), 1);
    assert!(effects.events.is_empty());
    assert!(tree(&runtime).contains("loading"));
    assert!(runtime.press(&create, "action").calls.is_empty(), "a second run is rejected while loading");
    let effects = runtime.tool_result(effects.calls[0].id, Ok(Value::from(7)));
    assert_eq!(effects.events.len(), 1);
    assert_eq!(effects.events[0].form_state.to_json_string(), r#"{"$title":""}"#);
    assert!(tree(&runtime).contains("success"));

    let effects = runtime.press(&create, "action");
    let effects = runtime.tool_result(effects.calls[0].id, Err(ToolError::NotFound { available: vec![] }));
    assert!(effects.events.is_empty(), "a failed mutation stops the plan");
    assert_eq!(codes(&effects.errors.unwrap()), [(Code::ToolNotFound, Some("newTicket"))]);
}

#[test]
fn sets_at_run_time_and_resets_to_the_declared_default() {
    let (mut runtime, _) = runtime(
        "$n = 0\nroot = Stack([up, reset, Text(\"\" + $n)])\nup = Button(\"Up\", @Set($n, $n + 1))\nreset = Button(\"Reset\", @Reset($n))",
    );
    let up = path(&runtime, "up");
    runtime.press(&up, "action");
    runtime.press(&up, "action");
    assert_eq!(runtime.state("$n"), Some(&Value::from(2)));
    assert_eq!(runtime.snapshot().to_json_string(), r#"{"state":{"$n":2},"fields":{},"forms":{}}"#);
    let reset = path(&runtime, "reset");
    runtime.press(&reset, "action");
    assert_eq!(runtime.state("$n"), Some(&Value::from(0)));
    assert_eq!(runtime.snapshot().to_json_string(), r#"{"state":{},"fields":{},"forms":{}}"#);
}

#[test]
fn never_runs_data_as_a_custom_action() {
    let (mut runtime, _) = runtime(
        "root = Stack([fake, real])\nfake = Button(\"Fake\", {type: \"CopyToClipboard\", params: {text: \"x\"}})\nreal = Button(\"Real\", @CopyToClipboard(\"y\"))",
    );
    let fake = path(&runtime, "fake");
    assert!(runtime.press(&fake, "action").events.is_empty());
    let real = path(&runtime, "real");
    let events = runtime.press(&real, "action").events;
    assert_eq!(
        events[0].to_value().to_json_string(),
        r#"{"type":"CopyToClipboard","humanFriendlyMessage":"","params":{"text":"y"},"formState":{}}"#
    );
}

#[test]
fn writes_a_binding_through_its_expression() {
    let (mut runtime, _) =
        runtime("$v = \"\"\nroot = Stack([field, Text($v)])\nfield = Input(\"x\", \"X\", $v = $value + \"!\")");
    let field = path(&runtime, "field");
    runtime.input(&field, "value", Value::from("hi"));
    assert_eq!(runtime.state("$v"), Some(&Value::from("hi!")));
    assert_eq!(runtime.value_at(&field, "value"), Some(Value::from("hi!")));
}

#[test]
fn keeps_what_did_not_change_the_same() {
    let (mut runtime, _) =
        runtime("$a = 1\n$b = 1\nroot = Stack([left, right])\nleft = Text(\"\" + $a)\nright = Text(\"\" + $b)");
    let children = |runtime: &Runtime| runtime.root().unwrap().prop("children").unwrap().as_array().unwrap().to_vec();
    let before = children(&runtime);
    let left = path(&runtime, "left");
    runtime.restore(&Value::from_json_str(r#"{"state": {"$a": 2}}"#).unwrap());
    let after = children(&runtime);
    let element = |value: &Value| value.as_element().unwrap() as *const Element;
    assert_ne!(element(&before[0]), element(&after[0]));
    assert_eq!(element(&before[1]), element(&after[1]));
    assert_eq!(runtime.find("left"), Some(left));
}

#[test]
fn reports_errors_only_once_the_stream_ends() {
    let mut runtime = Runtime::new(library(), Arc::new(FixedEnv::default()), Arc::new(TestForms));
    let effects = runtime.push("root = Stack([Text(null)])", false);
    assert_eq!(effects.errors, None);
    let effects = runtime.push("", true);
    assert_eq!(codes(&effects.errors.unwrap()), [(Code::NullRequired, Some("root"))]);
    let effects = runtime.push("Just prose, ", false);
    assert_eq!(effects.errors, Some(Vec::new()), "a new stream clears the errors");
    let effects = runtime.push("no program.", true);
    assert_eq!(codes(&effects.errors.unwrap()), [(Code::ParseFailed, None), (Code::NoRoot, None)]);
    let again = runtime.set_text("Just prose, no program.", true);
    assert_eq!(again.errors, None, "an unchanged list is not emitted again");
}

#[test]
fn validates_a_form_and_keeps_its_fields() {
    let (mut runtime, _) = runtime(
        "root = Stack([Form(\"contact\", [email, send]), note])\nemail = Input(\"email\", \"Email\", null, {required: true, email: true})\nsend = Button(\"Send\", @ToAssistant(\"Send\"))\nnote = Input(\"note\", \"Note\")",
    );
    let send = path(&runtime, "send");
    assert!(!runtime.validate_form(&send));
    let email = path(&runtime, "email");
    assert_eq!(runtime.error_at(&email), Some("This field is required"));
    let effects = runtime.input(&email, "value", Value::from("nope"));
    assert!(effects.render && effects.state);
    assert_eq!(runtime.error_at(&email), Some("Please enter a valid email"));
    runtime.input(&email, "value", Value::from("a@b.co"));
    assert!(runtime.validate_form(&send));
    assert_eq!(runtime.error_at(&email), None);
    let note = path(&runtime, "note");
    runtime.input(&note, "value", Value::from("rush"));
    assert_eq!(
        runtime.snapshot().to_json_string(),
        r#"{"state":{},"fields":{"note":{"value":"rush","componentType":"input"}},"forms":{"contact":{"email":{"value":"a@b.co","componentType":"input"}}}}"#
    );
    let mut restored = Runtime::new(library(), Arc::new(FixedEnv::default()), Arc::new(TestForms));
    restored.restore(&runtime.snapshot());
    restored.set_text(runtime.text(), true);
    assert_eq!(restored.value_at(&email, "value"), Some(Value::from("a@b.co")));
    let events = runtime.press(&send, "action").events;
    assert_eq!(events[0].form_name.as_deref(), Some("contact"));
}

#[test]
fn serializes_what_it_renders() {
    let source = "root = Stack([field, go, Card(\"T\", [Text(\"a \\\"b\\\"\", \"heading\")])])\n$q = \"x\"\nfield = Input(\"q\", null, $q)\ngo = Button(\"Go\", Action([@Set($q, $q + \"-\" + 1), @ToAssistant(\"Sent\", {id: 4, \"two words\": [1, -2.5]}), @CopyToClipboard($q)]))";
    let (runtime, _) = runtime(source);
    let state = runtime.store().vars().clone().into_iter().collect();
    let text = motile_openui::serialize(runtime.root().unwrap(), &library(), Some(&state));
    assert_eq!(
        text,
        "root = Stack([field, go, Card(\"T\", [Text(\"a \\\"b\\\"\", \"heading\")])])\nfield = Input(\"q\", null, $q)\ngo = Button(\"Go\", Action([@Set($q, $q + \"-\" + 1), @ToAssistant(\"Sent\", {id: 4, \"two words\": [1, -2.5]}), @CopyToClipboard(\"x\")]))\n$q = \"x\""
    );
    let (again, _) = self::runtime(&text);
    assert_eq!(tree(&again), tree(&runtime));
}

#[test]
fn can_move_between_threads() {
    fn send<T: Send + Sync>() {}
    send::<Runtime>();
    send::<Program>();
}
