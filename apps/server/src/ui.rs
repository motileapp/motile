//! What the server keeps and tells of the programs agents show in their replies: what the agents
//! are taught of OpenUI Lang, the state the user leaves programs in, what a message sends the agent
//! of them, and the errors an agent is asked to fix.

use std::collections::HashSet;
use std::sync::LazyLock;

use motile_openui::{Library, Program, ToolSpec, motile};
use motile_protocol::wire::{Item, ItemKind, NoticeEvent, ProgramState};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use serde::Serialize;
use serde_json::{Value, json};

use crate::ui_tools::{self, ToolDefinition};

/// How much of a program's state the server keeps, as JSON.
pub const MAX_STATE: usize = 256 * 1024;
/// How many times the programs of a user's message are sent back to the agent to be fixed.
pub const REPAIRS: usize = 2;
const CONTEXT: &str = "context and form values = ";
const LANGUAGES: [&str; 2] = ["openui-lang", "openui"];

static LIBRARY: LazyLock<Library> = LazyLock::new(motile::library);

/// What every agent is told of programs: OpenUI Lang, Motile's library and the tools a program
/// calls.
pub static PROMPT: LazyLock<String> =
    LazyLock::new(|| motile::system_prompt(ui_tools::definitions().into_iter().map(tool_spec).collect()));

fn tool_spec(tool: ToolDefinition) -> ToolSpec {
    let mut description = tool.description.to_string();
    if tool.mutation {
        description += " It runs only from a click, through Mutation and @Run.";
    }
    ToolSpec {
        name: tool.name.to_string(),
        description: Some(description),
        input_schema: (&tool.input_schema).into(),
        output_schema: Some((&tool.output_schema).into()),
    }
}

/// The text of each program of a reply: its top-level `openui-lang` fences, as the clients find
/// them.
pub fn fences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<String> = None;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if depth == 0 => {
                let language = info.split_whitespace().next().unwrap_or_default();
                open = LANGUAGES.contains(&language).then(String::new);
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    found.extend(open.take());
                }
            }
            Event::Text(text) => {
                if let Some(open) = &mut open {
                    open.push_str(&text);
                }
            }
            _ => {}
        }
    }
    found
}

/// A program a reply shows.
pub struct Shown {
    pub item_id: String,
    /// Which of the reply's programs it is.
    pub index: usize,
    /// Its text, with the program it patches merged in.
    pub text: String,
}

/// The programs the thread's own replies show, in order. Once the thread has one, a fence with no
/// `root` statement is a patch, merged over the latest.
pub fn shown(items: &[Item]) -> Vec<Shown> {
    let mut latest: Option<String> = None;
    let mut shown = Vec::new();
    for item in items.iter().filter(|item| item.parent.is_none()) {
        let ItemKind::Assistant { text } = &item.kind else { continue };
        for (index, fence) in fences(text).iter().enumerate() {
            let (text, _) = over(latest.as_deref(), fence);
            latest = Some(text.clone());
            shown.push(Shown { item_id: item.id.clone(), index, text });
        }
    }
    shown
}

/// What a program draws over the thread's latest one, and whether it is a patch.
fn over(latest: Option<&str>, program: &str) -> (String, bool) {
    match latest {
        Some(latest) if !motile_openui::parse(program).statements.contains_key("root") => {
            (motile_openui::merge(latest, program), true)
        }
        _ => (program.to_string(), false),
    }
}

/// What `check_ui` answers of a program: its errors (language.md §8.3) with their hints, the
/// names it references and never defines, and the statements `root` can't reach.
#[derive(Serialize, Debug)]
pub struct Check {
    pub valid: bool,
    /// It has no `root` statement and was checked merged over the thread's latest program.
    pub patch: bool,
    pub errors: Vec<motile_openui::Error>,
    pub unresolved: Vec<String>,
    pub orphans: Vec<String>,
}

/// Checks a program, with or without its fence, as the clients will draw it after `latest`.
pub fn check(program: &str, latest: Option<&str>) -> Check {
    let (text, patch) = over(latest, program);
    let checked = Program::parse(&LIBRARY, &text);
    Check {
        valid: problems(&checked).is_empty(),
        patch,
        errors: checked.errors,
        unresolved: checked.unresolved,
        orphans: checked.orphans,
    }
}

/// The latest program the thread shows.
pub fn latest(items: &[Item]) -> Option<String> {
    shown(items).pop().map(|shown| shown.text)
}

/// Something that keeps a program from being drawn whole.
struct Problem {
    what: String,
    hint: Option<String>,
}

/// A program's errors that aren't warnings, and its references to statements that don't exist.
fn problems(program: &Program) -> Vec<Problem> {
    let errors = program.errors.iter().filter(|error| error.severity.is_none()).map(|error| {
        let mut what = error.code.as_str().to_string();
        if let Some(statement) = &error.statement_id {
            what += &format!(" in `{statement}`");
        }
        if let Some(component) = &error.component {
            what += &format!(" ({component})");
        }
        // The hint repeats what follows the dash.
        let message = match &error.hint {
            Some(_) => error.message.split(" — ").next().unwrap_or_default(),
            None => &error.message,
        };
        Problem { what: format!("{what}: {message}"), hint: error.hint.clone() }
    });
    let unresolved = program
        .unresolved
        .iter()
        .map(|name| Problem { what: format!("`{name}` is referenced but never defined"), hint: None });
    errors.chain(unresolved).collect()
}

/// What the agent is asked when a program of the turn that just ended has errors.
#[derive(Debug, PartialEq)]
pub struct Repair {
    pub text: String,
    /// The first problem, for the transcript.
    pub summary: String,
}

/// The repair the thread's last turn calls for: none when it was stopped or failed, when its
/// programs are whole, or once what started the turns has been sent back `REPAIRS` times.
pub fn repair(items: &[Item]) -> Option<Repair> {
    let ends: Vec<usize> = items.iter().enumerate().filter_map(|(at, item)| ended(item).map(|_| at)).collect();
    let summary = ended(&items[*ends.last()?])?;
    if summary.stopped || summary.is_error {
        return None;
    }
    let started =
        |item: &Item| matches!(item.kind, ItemKind::User { .. } | ItemKind::Notice { .. }) && !is_repair(item);
    let message = items.iter().rposition(started).unwrap_or(0);
    let repairs = items[message..].iter().filter(|item| is_repair(item)).count();
    if repairs >= REPAIRS {
        return None;
    }
    let start = ends.len().checked_sub(2).map_or(0, |previous| ends[previous] + 1);
    let turn: HashSet<&str> = items[start..].iter().map(|item| item.id.as_str()).collect();
    let broken: Vec<(Shown, Vec<Problem>)> = shown(items)
        .into_iter()
        .filter(|shown| turn.contains(shown.item_id.as_str()))
        .map(|shown| {
            let problems = problems(&Program::parse(&LIBRARY, &shown.text));
            (shown, problems)
        })
        .filter(|(_, problems)| !problems.is_empty())
        .collect();
    let summary = broken.first()?.1.first()?.what.clone();
    let mut text = String::from("Motile's clients can't draw all of what you showed:\n");
    for (shown, problems) in &broken {
        text += &format!("\nProgram {} of your reply (item {}):\n", shown.index + 1, shown.item_id);
        for Problem { what, hint } in problems {
            let hint = hint.as_ref().map(|hint| format!(". Hint: {hint}")).unwrap_or_default();
            text += &format!("- {what}{hint}\n");
        }
    }
    text += "\nReply with only an ```openui-lang fence that holds a patch fixing them: the statements that change, \
             with no root statement.";
    Some(Repair { text, summary })
}

fn ended(item: &Item) -> Option<&motile_protocol::wire::TurnSummary> {
    match &item.kind {
        ItemKind::TurnEnd { summary } => Some(summary),
        _ => None,
    }
}

fn is_repair(item: &Item) -> bool {
    matches!(&item.kind, ItemKind::Notice { notice, .. } if notice.event == NoticeEvent::ProgramErrors)
}

/// A message as the agent reads it: its text, then what its program sent with it and the state of
/// each program the user changed, a line each.
pub fn told(text: &str, context: Option<&Value>, programs: &[ProgramState]) -> String {
    let changed = programs
        .iter()
        .map(|program| json!({ "item": program.item_id, "block": program.block, "state": program.state }));
    let mut told = text.to_string();
    for value in context.cloned().into_iter().chain(changed) {
        if !told.is_empty() {
            told.push('\n');
        }
        told.push_str(CONTEXT);
        told.push_str(&value.to_string());
    }
    told
}

/// The state of every program of these items, a block at a time.
pub fn states(items: &[Item]) -> Vec<ProgramState> {
    let states = items.iter().flat_map(|item| {
        item.ui.iter().map(|(block, state)| ProgramState {
            item_id: item.id.clone(),
            block: *block,
            state: state.clone(),
        })
    });
    states.collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{AgentNotice, TurnSummary};

    use super::*;

    const BROKEN: &str = "root = Card([title, chart, note])
title = Title(\"Signups this week\", \"md\")
chart = BarChart([{day: \"Mon\", signups: 12}, {day: \"Tue\", signups: 18}])
note = Badge()";
    const FIX: &str = "chart = Chart(\"bar\", [{day: \"Mon\", signups: 12}, {day: \"Tue\", signups: 18}], \"day\")
note = Badge(\"Tuesday was the best day\", \"success\")";

    fn item(id: &str, kind: ItemKind) -> Item {
        let item = json!({ "id": id, "seq": 0, "rev": 0, "created_at": 0.0, "type": "assistant", "text": "" });
        Item { kind, ..serde_json::from_value(item).unwrap() }
    }

    fn user(id: &str) -> Item {
        let kind = ItemKind::User {
            text: "Show me".into(),
            attachments: Vec::new(),
            skills: Vec::new(),
            context: None,
            programs: Vec::new(),
        };
        item(id, kind)
    }

    fn reply(id: &str, program: &str) -> Item {
        item(id, ItemKind::Assistant { text: format!("Here:\n\n```openui-lang\n{program}\n```\n\nDone.") })
    }

    fn end(id: &str, summary: TurnSummary) -> Item {
        item(id, ItemKind::TurnEnd { summary })
    }

    fn sent_back(id: &str) -> Item {
        let notice = AgentNotice {
            thread_id: "t".into(),
            title: "Program errors".into(),
            event: NoticeEvent::ProgramErrors,
            agent: motile_protocol::wire::Agent::Claude,
            summary: None,
        };
        item(id, ItemKind::Notice { notice, text: String::new() })
    }

    fn codes(check: &Check) -> Vec<&str> {
        check.errors.iter().map(|error| error.code.as_str()).collect()
    }

    #[test]
    fn a_program_is_checked_alone_or_as_a_patch_over_the_latest() {
        let broken = check(&format!("```openui-lang\n{BROKEN}\n```"), None);
        assert!(!broken.valid && !broken.patch);
        assert_eq!(codes(&broken), ["unknown-component", "missing-required"]);
        assert!(broken.errors.iter().all(|error| error.hint.is_some()), "{broken:?}");

        let fixed = check(FIX, Some(BROKEN));
        assert!(fixed.valid && fixed.patch, "{fixed:?}");
        assert_eq!((fixed.errors.len(), fixed.unresolved.len(), fixed.orphans.len()), (0, 0, 0));

        let alone = check(FIX, None);
        assert!(!alone.valid && !alone.patch);
        assert_eq!(codes(&alone), ["no-root"]);
        let dangling = check("root = Col([title, missing])\ntitle = Title(\"Hi\")\nextra = Text(\"x\")", None);
        assert_eq!(
            (dangling.valid, dangling.unresolved, dangling.orphans),
            (false, vec!["missing".into()], vec!["extra".into()])
        );
    }

    #[test]
    fn the_corpus_programs_are_valid() {
        let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/openui");
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            let checked = check(&std::fs::read_to_string(&path).unwrap(), None);
            assert!(checked.valid, "{}: {checked:?}", path.display());
        }
    }

    #[test]
    fn only_top_level_program_fences_are_programs() {
        let text =
            "```openui\na = 1\n```\n\n- ```openui-lang\n  b = 2\n  ```\n\n```rust\nc\n```\n\n```openui-lang\nd = 3";
        assert_eq!(fences(text), ["a = 1\n", "d = 3"]);
    }

    #[test]
    fn a_turn_whose_program_has_errors_is_sent_back_twice_at_most() {
        let mut items = vec![user("u1"), reply("a1", BROKEN), end("e1", TurnSummary::default())];
        let first = repair(&items).unwrap();
        assert!(first.text.contains("Program 1 of your reply (item a1):\n- unknown-component in `chart`"));
        assert!(first.text.contains("- missing-required in `note` (Badge)"), "{}", first.text);
        assert!(first.text.ends_with("with no root statement."));
        assert!(first.summary.starts_with("unknown-component in `chart`"));

        items.extend([sent_back("n1"), reply("a2", "chart = BarChart([])"), end("e2", TurnSummary::default())]);
        assert!(repair(&items).unwrap().text.contains("(item a2)"), "the patch is checked merged");
        items.extend([sent_back("n2"), reply("a3", "chart = BarChart([])"), end("e3", TurnSummary::default())]);
        assert_eq!(repair(&items), None, "twice at most");

        items.extend([user("u2"), reply("a4", "note = Badge()"), end("e4", TurnSummary::default())]);
        assert!(repair(&items).is_some(), "a new message starts again");
    }

    #[test]
    fn a_turn_is_not_sent_back_when_its_programs_are_whole_or_it_was_stopped() {
        let fixed = [user("u1"), reply("a1", BROKEN), end("e1", TurnSummary::default())];
        let fixed: Vec<Item> =
            fixed.into_iter().chain([sent_back("n1"), reply("a2", FIX), end("e2", TurnSummary::default())]).collect();
        assert_eq!(repair(&fixed), None);

        let later = [
            user("u1"),
            reply("a1", BROKEN),
            end("e1", TurnSummary::default()),
            user("u2"),
            end("e2", TurnSummary::default()),
        ];
        assert_eq!(repair(&later), None, "only the turn that ended is checked");

        let stopped = TurnSummary { stopped: true, ..TurnSummary::default() };
        assert_eq!(repair(&[user("u1"), reply("a1", BROKEN), end("e1", stopped)]), None);
        let failed = TurnSummary { is_error: true, ..TurnSummary::default() };
        assert_eq!(repair(&[user("u1"), reply("a1", BROKEN), end("e1", failed)]), None);
        assert_eq!(repair(&[user("u1"), reply("a1", BROKEN)]), None, "a turn that hasn't ended");
    }

    #[test]
    fn a_message_tells_what_its_program_sent_then_each_changed_program() {
        let context = json!(["User clicked: Book it", { "people": 4 }, { "table": 7 }]);
        let changed = ProgramState { item_id: "a1".into(), block: 2, state: json!({ "$days": "30" }) };

        let told = told("Book it", Some(&context), &[changed]);

        assert_eq!(
            told,
            "Book it\n\
             context and form values = [\"User clicked: Book it\",{\"people\":4},{\"table\":7}]\n\
             context and form values = {\"block\":2,\"item\":\"a1\",\"state\":{\"$days\":\"30\"}}"
        );
        assert_eq!(super::told("Hi", None, &[]), "Hi");
        assert_eq!(super::told("", Some(&json!(1)), &[]), "context and form values = 1");
    }

    #[test]
    fn every_block_of_an_item_has_its_own_state() {
        let mut item: Item = serde_json::from_value(json!({
            "id": "a1", "seq": 1, "rev": 2, "created_at": 0.0, "type": "assistant", "text": "",
        }))
        .unwrap();
        item.ui.insert(3, json!({ "x": 1 }));
        item.ui.insert(1, json!(null));

        let blocks: Vec<u32> = states(&[item]).iter().map(|state| state.block).collect();

        assert_eq!(blocks, [1, 3]);
    }
}
