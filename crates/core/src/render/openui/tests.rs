//! The programs as a client drives them, without a client: rows in, events in, what the core
//! carries out out.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use motile_openui::{Step, ToolError};
use motile_protocol::wire::{Item, ItemKind, Media};
use serde_json::{Value, json};

use super::host::{Out, Programs, Ticker, UiEvent};
use super::{UiOp, node_id, node_path};
use crate::render::rows::{Row, RowKind, Transcript};

fn assistant(id: &str, seq: u64, text: &str) -> Item {
    Item {
        id: id.into(),
        seq,
        rev: seq,
        created_at: seq as f64,
        media: Vec::new(),
        parent: None,
        ui: BTreeMap::new(),
        kind: ItemKind::Assistant { text: text.into() },
    }
}

fn fenced(program: &str) -> String {
    format!("Here it is.\n\n```openui-lang\n{program}\n```\n\nTry it.")
}

fn fixture(name: &str) -> String {
    let path = format!("{}/../../fixtures/openui/{name}.oui", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// A thread whose programs run: its transcript, and the rows it sent with their trees.
struct Thread {
    transcript: Transcript,
    programs: Programs,
    saved: BTreeMap<(String, u32), Value>,
    outs: Vec<Out>,
}

impl Thread {
    fn new(items: Vec<Item>) -> Self {
        let mut transcript = Transcript::new("/srv");
        transcript.load(items);
        let mut thread = Self { transcript, programs: Programs::default(), saved: BTreeMap::new(), outs: Vec::new() };
        thread.programs.full_access = true;
        thread
    }

    fn of(program: &str) -> Self {
        Self::new(vec![assistant("a", 1, &fenced(program))])
    }

    /// The rows as a client gets them.
    fn send(&mut self) -> Vec<Row> {
        let mut rows = self.transcript.rows().to_vec();
        let saved = |item: &str, block: u32| self.saved.get(&(item.to_string(), block)).cloned();
        self.outs = self.programs.dress(&mut rows, &saved);
        rows
    }

    fn ui(&mut self, row_id: &str) -> super::UiRow {
        let rows = self.send();
        let row = rows.into_iter().find(|row| row.id == row_id).unwrap_or_else(|| panic!("no row {row_id}"));
        let RowKind::Ui { ui } = row.kind else { panic!("{row_id} is no program") };
        ui
    }

    fn event(&mut self, node: &str, event: UiEvent) -> Vec<Out> {
        self.programs.event(PROGRAM, node, event)
    }

    fn tree(&self) -> Value {
        self.programs.shown(PROGRAM).cloned().unwrap_or_default()
    }
}

/// The row of the first program of item `a`: the prose before it is block 0.
const PROGRAM: &str = "a/1";

/// The id of the first node of `component` whose props have `text` somewhere.
fn find(tree: &Value, component: &str, text: &str) -> Option<String> {
    if tree["component"] == component && tree["props"].to_string().contains(text) {
        return tree["id"].as_str().map(str::to_string);
    }
    tree["children"].as_array()?.iter().find_map(|child| find(child, component, text))
}

fn texts(tree: &Value) -> Vec<String> {
    let mut found = Vec::new();
    if tree["component"] == "Text" {
        found.push(tree["props"]["text"].as_str().unwrap_or_default().to_string());
    }
    for child in tree["children"].as_array().into_iter().flatten() {
        found.extend(texts(child));
    }
    found
}

fn patches(outs: &[Out]) -> Vec<UiOp> {
    outs.iter()
        .filter_map(|out| match out {
            Out::Patch { row_id, ops } if row_id == PROGRAM => Some(ops.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn calls(outs: &[Out]) -> Vec<(u64, u64, String, Value, bool)> {
    outs.iter()
        .filter_map(|out| match out {
            Out::Call { serial, call, tool, args, confirmed, .. } => {
                Some((*serial, *call, tool.clone(), args.clone(), *confirmed))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn names_nodes_by_their_path() {
    let path = vec![Step::Key("children".into()), Step::Index(2), Step::Key("action".into())];
    assert_eq!(node_id(&path), "children/2/action");
    let odd = vec![Step::Key("a/b".into()), Step::Key("7".into()), Step::Key(String::new()), Step::Key("~".into())];
    assert_eq!(node_id(&odd), "a~1b/~k7/~k/~0");
    for path in [path, odd, Vec::new()] {
        assert_eq!(node_path(&node_id(&path)), Some(path));
    }
    assert_eq!(node_path("children//1"), None);
}

#[test]
fn a_program_fence_is_a_row_of_its_own() {
    let mut thread = Thread::of("root = Col([Text(\"Hi\")])");
    let rows = thread.send();
    let kinds: Vec<&str> = rows
        .iter()
        .map(|row| match row.kind {
            RowKind::Prose { .. } => "prose",
            RowKind::Ui { .. } => "ui",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["prose", "ui", "prose"]);
    let RowKind::Ui { ui } = &rows[1].kind else { unreachable!() };
    assert!(ui.complete);
    assert_eq!(texts(&json!(ui.tree)), ["Hi"]);
    let row = serde_json::to_value(&rows[1]).unwrap();
    assert_eq!(row["kind"], "ui");
    assert_eq!(row["source"], "root = Col([Text(\"Hi\")])");
    assert_eq!(row["tree"]["component"], "Col");
    assert!(row.get("fence").is_none() && row.get("base").is_none());
}

#[test]
fn streams_a_program_and_completes_it_when_its_fence_closes() {
    let program = fixture("chart");
    let text = fenced(&program);
    let mut transcript = Transcript::new("/srv");
    transcript.upsert(assistant("a", 1, ""), true);
    let mut programs = Programs::default();
    let mut grown = Vec::new();
    for chunk in text.as_bytes().chunks(40) {
        transcript.append_text("a", std::str::from_utf8(chunk).unwrap(), 2);
        let Some(mut splice) = transcript.refresh("a") else { continue };
        programs.dress(&mut splice.rows, &|_, _| None);
        for row in splice.rows {
            let RowKind::Ui { ui } = row.kind else { continue };
            grown.push((ui.complete, ui.tree.map(|tree| json!(tree).to_string().len()).unwrap_or(0)));
        }
    }
    assert!(grown.len() > 3, "{grown:?}");
    assert!(grown.windows(2).all(|pair| pair[0].1 <= pair[1].1), "the tree only grows: {grown:?}");
    let complete: Vec<bool> = grown.iter().map(|(complete, _)| *complete).collect();
    assert_eq!(complete.iter().filter(|complete| **complete).count(), 1, "{complete:?}");
    assert!(complete.last().unwrap());

    let mut unclosed = Transcript::new("/srv");
    unclosed.upsert(assistant("b", 1, "```openui-lang\nroot = Col([Text(\"x\")])"), true);
    let incomplete = |transcript: &Transcript| {
        transcript.rows().iter().any(|row| matches!(&row.kind, RowKind::Ui { ui } if !ui.complete))
    };
    assert!(incomplete(&unclosed));
    assert!(unclosed.end_streaming().is_some());
    assert!(!incomplete(&unclosed), "a reply that ended completes its program");
}

#[test]
fn a_press_sets_state_and_patches_only_what_changed() {
    let mut thread = Thread::of(
        "$n = 0\nroot = Col([Title(\"Counter\"), count, up])\ncount = Text(\"Count \" + $n)\nup = Button(\"Up\", @Set($n, $n + 1))",
    );
    thread.ui(PROGRAM);
    let button = find(&thread.tree(), "Button", "Up").unwrap();
    let outs = thread.event(&button, UiEvent::Press);
    let text = find(&thread.tree(), "Text", "Count").unwrap();
    let ops = patches(&outs);
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert!(
        matches!(&ops[0], UiOp::Props { id, set, unset } if *id == text && set["props"]["text"] == "Count 1" && unset.is_empty())
    );
    assert!(outs.iter().any(|out| matches!(out, Out::Wake { .. })));
    assert_eq!(texts(&thread.tree()), ["Count 1"]);
}

#[test]
fn types_into_a_bound_field() {
    let mut thread =
        Thread::of("$name = \"\"\nroot = Col([field, Text(\"Hello \" + $name)])\nfield = Input($name, \"Name\")");
    thread.ui(PROGRAM);
    let field = find(&thread.tree(), "Input", "Name").unwrap();
    thread.event(&field, UiEvent::Value { value: json!("Ada") });
    assert_eq!(texts(&thread.tree()), ["Hello Ada"]);
    let tree = thread.tree();
    assert_eq!(tree["children"][0]["props"]["value"], "Ada");
}

#[test]
fn keeps_an_unbound_field_under_its_label() {
    let mut thread = Thread::of("root = Col([Input(\"\", \"Name\"), Checkbox(false, \"Done\")])");
    thread.ui(PROGRAM);
    let field = find(&thread.tree(), "Input", "Name").unwrap();
    thread.event(&field, UiEvent::Value { value: json!("Ada") });
    let checkbox = find(&thread.tree(), "Checkbox", "Done").unwrap();
    thread.event(&checkbox, UiEvent::Value { value: json!(true) });
    assert_eq!(thread.tree()["children"][0]["props"]["value"], "Ada");
    assert_eq!(thread.tree()["children"][1]["props"]["checked"], true);
    let state = thread.programs.snapshot(PROGRAM).unwrap();
    assert_eq!(state["fields"]["Name"]["value"], "Ada");
    assert_eq!(state["fields"]["Done"]["value"], true);
}

#[test]
fn moving_a_slider_patches_the_slider_and_what_reads_it() {
    let mut thread = Thread::of(
        "$v = 20\nroot = Col([Title(\"Mix\"), Text(\"Fixed\"), slider, Text(\"Value \" + $v), Card([Text(\"Other\")])])\nslider = Slider($v, 0, 100, 1, \"Amount\")",
    );
    thread.ui(PROGRAM);
    let slider = find(&thread.tree(), "Slider", "Amount").unwrap();
    let ops = patches(&thread.event(&slider, UiEvent::Value { value: json!(42) }));
    let ids: Vec<&str> = ops
        .iter()
        .map(|op| match op {
            UiOp::Props { id, .. } => id.as_str(),
            other => panic!("only props change: {other:?}"),
        })
        .collect();
    assert_eq!(ids, [slider.as_str(), "children/3"]);
    let UiOp::Props { set, .. } = &ops[0] else { unreachable!() };
    assert_eq!(set["props"]["value"], 42);
    assert!(patches(&thread.event(&slider, UiEvent::Value { value: json!(42) })).is_empty(), "nothing changed");
}

#[test]
fn runs_a_query_against_a_tool_and_draws_its_rows() {
    let mut thread = Thread::of(
        "root = Col([Text(\"Commits: \" + @Count(log)), refresh])\nlog = Query(\"git_log\", {limit: 2}, [], 30)\nrefresh = Button(\"Refresh\", @Run(log))",
    );
    thread.ui(PROGRAM);
    let asked = calls(&thread.outs);
    assert_eq!(asked.len(), 1);
    let (serial, call, tool, args, confirmed) = asked[0].clone();
    assert_eq!((tool.as_str(), args, confirmed), ("git_log", json!({"limit": 2}), false));
    let timer = thread.outs.iter().find_map(|out| match out {
        Out::Interval { key, seconds } => Some((key.clone(), *seconds)),
        _ => None,
    });
    let (timer, seconds) = timer.expect("a refresh timer");
    assert_eq!(seconds, 30.0);
    assert!(matches!(timer.ticker, Ticker::Query(_)));

    let outs = thread.programs.tool_result(PROGRAM, serial, call, Ok(json!([{"hash": "a"}, {"hash": "b"}])));
    assert_eq!(texts(&thread.tree()), ["Commits: 2"]);
    assert_eq!(patches(&outs).len(), 1);

    let refetch = calls(&thread.programs.tick(&timer));
    assert_eq!(refetch.len(), 1, "the timer fetches again");
    let failed = thread.programs.tool_result(PROGRAM, serial, refetch[0].1, Err(ToolError::Failed("offline".into())));
    assert!(patches(&failed).iter().any(|op| matches!(op, UiOp::Errors { errors } if errors.len() == 1)));
    assert!(thread.programs.tool_result(PROGRAM, serial + 1, refetch[0].1, Ok(json!([]))).is_empty(), "stale");
}

#[test]
fn counts_a_timer_down_and_resets_it() {
    let mut thread = Thread::of(&fixture("timer"));
    thread.ui(PROGRAM);
    let timer = find(&thread.tree(), "Timer", "Time left").unwrap();
    let outs = thread.event(&timer, UiEvent::Start);
    let key = outs
        .iter()
        .find_map(|out| match out {
            Out::Interval { key, seconds: 1.0 } => Some(key.clone()),
            _ => None,
        })
        .expect("a countdown");
    thread.programs.tick(&key);
    thread.programs.tick(&key);
    assert_eq!(thread.programs.snapshot(PROGRAM).unwrap()["state"]["$left"], 178);
    let outs = thread.event(&timer, UiEvent::Reset);
    assert!(outs.contains(&Out::Stop { key: key.clone() }));
    assert_eq!(thread.programs.snapshot(PROGRAM).unwrap()["state"]["$left"], 180);
    assert_eq!(thread.programs.tick(&key), [Out::Stop { key }], "a stopped countdown doesn't tick");
}

#[test]
fn to_assistant_sends_a_message_with_the_form_and_its_context() {
    let mut thread = Thread::of(
        "root = Col([Form([Input(\"\", \"Name\"), send])])\nsend = Button(\"Book\", @ToAssistant(\"Book it\", {table: 7}))",
    );
    thread.ui(PROGRAM);
    let field = find(&thread.tree(), "Input", "Name").unwrap();
    thread.event(&field, UiEvent::Value { value: json!("Ada") });
    let button = find(&thread.tree(), "Button", "Book").unwrap();
    let outs = thread.event(&button, UiEvent::Press);
    let sent: Vec<&Out> = outs.iter().filter(|out| matches!(out, Out::Send { .. })).collect();
    assert_eq!(
        sent,
        [&Out::Send {
            text: "Book it".into(),
            context: Some(json!(["User clicked: Book it", {"Name": {"value": "Ada"}}, {"table": 7}])),
        }]
    );

    let mut plain = Thread::of("root = Col([Button(\"Tell me more\")])");
    plain.ui(PROGRAM);
    let button = find(&plain.tree(), "Button", "Tell me more").unwrap();
    let outs = plain.event(&button, UiEvent::Press);
    assert!(matches!(&outs[0], Out::Send { text, .. } if text == "Tell me more"), "{outs:?}");

    plain.programs.limited = true;
    let outs = plain.event(&button, UiEvent::Press);
    let Out::Send { text, context: None } = &outs[0] else { panic!("{outs:?}") };
    assert_eq!(text, "Tell me more\ncontext and form values = [\"User clicked: Tell me more\",{}]");
}

#[test]
fn opens_web_pages_and_copies() {
    let mut thread = Thread::of(
        "root = Col([Button(\"Site\", @OpenUrl(\"https://motile.app\")), Button(\"Local\", @OpenUrl(\"file:///etc/passwd\")), Button(\"Copy\", @CopyToClipboard(\"milk\"))])",
    );
    thread.ui(PROGRAM);
    let press = |thread: &mut Thread, label: &str| {
        let button = find(&thread.tree(), "Button", label).unwrap();
        thread.event(&button, UiEvent::Press)
    };
    assert!(press(&mut thread, "Site").contains(&Out::OpenUrl { url: "https://motile.app".into() }));
    assert!(!press(&mut thread, "Local").iter().any(|out| matches!(out, Out::OpenUrl { .. })));
    assert!(press(&mut thread, "Copy").contains(&Out::Copy { text: "milk".into() }));
}

#[test]
fn asks_before_running_a_command_without_full_access() {
    let program = "root = Col([go, Text(result.status)])\nresult = Mutation(\"run\", {command: \"npm test\"})\ngo = Button(\"Test\", @Run(result))";
    let mut thread = Thread::of(program);
    thread.programs.full_access = false;
    thread.ui(PROGRAM);
    let button = find(&thread.tree(), "Button", "Test").unwrap();
    let outs = thread.event(&button, UiEvent::Press);
    let call_id = outs
        .iter()
        .find_map(|out| match out {
            Out::Confirm { call_id, command, .. } if command == "npm test" => Some(*call_id),
            _ => None,
        })
        .expect("asks first");
    assert!(calls(&outs).is_empty());
    let allowed = calls(&thread.programs.confirm(PROGRAM, call_id, true));
    assert_eq!(allowed.len(), 1);
    assert!(allowed[0].4, "confirmed");
    assert!(thread.programs.confirm(PROGRAM, call_id, true).is_empty(), "asked once");
    let (serial, call, ..) = allowed[0].clone();
    thread.programs.tool_result(PROGRAM, serial, call, Ok(json!({"exit_code": 0})));
    assert_eq!(texts(&thread.tree()), ["success"]);

    let outs = thread.event(&button, UiEvent::Press);
    let Some(Out::Confirm { call_id, .. }) = outs.iter().find(|out| matches!(out, Out::Confirm { .. })) else {
        panic!("{outs:?}")
    };
    thread.programs.confirm(PROGRAM, *call_id, false);
    assert_eq!(texts(&thread.tree()), ["error"]);

    let mut full = Thread::of(program);
    full.ui(PROGRAM);
    let outs = full.event(&button, UiEvent::Press);
    assert!(!calls(&outs)[0].4, "full access runs it without asking");
}

#[test]
fn an_old_server_runs_no_tools() {
    let mut thread = Thread::of("root = Col([Text(\"n\" + @Count(log))])\nlog = Query(\"git_log\", {}, [])");
    thread.programs.limited = true;
    let ui = thread.ui(PROGRAM);
    assert!(ui.limited);
    assert!(calls(&thread.outs).is_empty());
    assert_eq!(ui.errors, ["Query \"git_log\" failed: Your server needs an update to run this."]);
}

#[test]
fn draws_anew_only_on_a_press() {
    let mut thread = Thread::of(
        "$door = -1\n$other = 0\nroot = Col([pick, Text(\"Door \" + $door), field])\npick = Button(\"Pick\", @Set($door, @Random(0, 1000000, true)))\nfield = Input($other, \"Other\", \"number\")",
    );
    thread.ui(PROGRAM);
    let pick = find(&thread.tree(), "Button", "Pick").unwrap();
    thread.event(&pick, UiEvent::Press);
    let first = texts(&thread.tree());
    let field = find(&thread.tree(), "Input", "Other").unwrap();
    thread.event(&field, UiEvent::Value { value: json!(3) });
    assert_eq!(texts(&thread.tree()), first, "a render draws nothing");
    thread.event(&pick, UiEvent::Press);
    assert_ne!(texts(&thread.tree()), first, "a press draws anew");
}

#[test]
fn keeps_state_on_the_server_and_restores_it() {
    let program = "$v = 20\nroot = Col([Slider($v, 0, 100, 1, \"Amount\"), Text(\"Value \" + $v)])";
    let mut thread = Thread::of(program);
    thread.ui(PROGRAM);
    let slider = find(&thread.tree(), "Slider", "Amount").unwrap();
    thread.event(&slider, UiEvent::Value { value: json!(30) });
    thread.event(&slider, UiEvent::Value { value: json!(64) });
    assert!(thread.programs.wake(Instant::now()).iter().all(|out| !matches!(out, Out::Save { .. })), "too soon");
    let outs = thread.programs.wake(Instant::now() + Duration::from_secs(1));
    let saved: Vec<&Out> = outs.iter().filter(|out| matches!(out, Out::Save { .. })).collect();
    let state = json!({"state": {"$v": 64}, "fields": {}, "forms": {}});
    assert_eq!(saved, [&Out::Save { item_id: "a".into(), block: 1, state: state.clone() }]);

    let mut item = assistant("a", 1, &fenced(program));
    item.ui.insert(1, state.clone());
    assert!(thread.programs.saved(&item).is_empty(), "our own state coming back changes nothing");

    let mut reopened = Thread::of(program);
    reopened.saved.insert(("a".into(), 1), state);
    reopened.ui(PROGRAM);
    assert_eq!(texts(&reopened.tree()), ["Value 64"]);

    item.ui.insert(1, json!({"state": {"$v": 5}}));
    let outs = reopened.programs.saved(&item);
    assert_eq!(texts(&reopened.tree()), ["Value 5"], "another device's state");
    assert_eq!(patches(&outs).len(), 2);
}

#[test]
fn a_patch_fence_shows_the_program_it_patches_merged() {
    let first = assistant("a", 1, &fenced("$n = 2\nroot = Col([title, Text(\"n\" + $n)])\ntitle = Title(\"Before\")"));
    let second = assistant("b", 2, &fenced("title = Title(\"After\")"));
    let third = assistant("c", 3, &fenced("root = Col([Text(\"New\")])"));
    let mut thread = Thread::new(vec![first, second, third]);
    thread.saved.insert(("a".into(), 1), json!({"state": {"$n": 5}}));
    let before = thread.ui("a/1");
    assert!(before.source.contains("Before") && before.base.is_none());
    let patched = thread.ui("b/1");
    assert_eq!(patched.base.as_deref(), Some("a/1"));
    assert_eq!(patched.source, "$n = 2\nroot = Col([title, Text(\"n\" + $n)])\ntitle = Title(\"After\")");
    let tree = json!(patched.tree);
    assert_eq!(tree["children"][0]["props"]["text"], "After");
    assert_eq!(texts(&tree), ["n5"], "it starts from the state of the program it patches");
    let fresh = thread.ui("c/1");
    assert!(fresh.base.is_none() && fresh.source == "root = Col([Text(\"New\")])");
}

#[test]
fn looks_up_photos_once_the_query_stays_and_routes_at_once() {
    let mut thread = Thread::of(
        "root = Col([SearchImage(\"Belém Tower\"), Map([MapMarker(\"A\", 38.69, -9.21), MapMarker(\"B\", 38.71, -9.13)], true)])",
    );
    thread.ui(PROGRAM);
    let stops = vec![[38.69, -9.21], [38.71, -9.13]];
    assert!(thread.outs.contains(&Out::Route { stops: stops.clone() }));
    assert!(thread.outs.iter().any(|out| matches!(out, Out::Wake { .. })));
    assert!(!thread.outs.iter().any(|out| matches!(out, Out::Images { .. })));
    let outs = thread.programs.wake(Instant::now() + Duration::from_secs(1));
    assert_eq!(outs, [Out::Images { query: "Belém Tower".into() }]);
    assert!(thread.programs.wake(Instant::now() + Duration::from_secs(2)).is_empty(), "asked once");

    let photo = Media {
        id: "p.jpg".into(),
        src: "p.jpg".into(),
        video: false,
        file: false,
        size: 1,
        width: Some(4),
        height: Some(3),
        poster: None,
    };
    let outs = thread.programs.images("Belém Tower", vec![photo]);
    assert!(
        patches(&outs)
            .iter()
            .any(|op| matches!(op, UiOp::Props { set, .. } if set["props"]["media"] == json!(["p.jpg"])))
    );
    thread.programs.route(&stops, None);
    let route = &thread.tree()["children"][1]["props"]["route"];
    assert_eq!(route["label"], "Route overview");
    assert_eq!(route["points"], json!(stops));
}

#[test]
fn runs_every_corpus_program_without_errors() {
    let folder = format!("{}/../../fixtures/openui", env!("CARGO_MANIFEST_DIR"));
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.strip_suffix(".oui").map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(names.len(), 42);
    for name in names {
        let mut thread = Thread::of(&fixture(&name));
        let ui = thread.ui(PROGRAM);
        assert!(ui.complete, "{name}");
        assert!(ui.errors.is_empty(), "{name}: {:?}", ui.errors);
        assert!(ui.tree.is_some(), "{name} draws a tree");
    }
}
