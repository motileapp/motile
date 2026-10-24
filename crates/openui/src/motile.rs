//! Motile's library, which is open-intelligent-ui's (`src/lib/library.ts`), and the system prompt
//! the server gives the agents: open-intelligent-ui's rules and examples (`prompt-options.ts`),
//! changed where Motile differs. There the answer is the whole page; here it is a fenced block
//! inside a coding agent's Markdown reply, written only when a picture or a tool helps, and a
//! later fence can patch the latest program.

use crate::components;
use crate::functions;
use crate::library::{ActionDef, Library, LibraryDef};
use crate::prompt::{self, PromptOptions, Tool, ToolSpec};
use crate::schema::Schema;

pub fn library() -> Library {
    Library::new(LibraryDef {
        id: None,
        version: None,
        root: Some("Col".into()),
        components: components::components(),
        groups: components::groups(),
        functions: functions::functions(),
        actions: vec![copy_to_clipboard()],
    })
    .expect("open-intelligent-ui's library is valid")
}

/// `@CopyToClipboard(text)`: the client copies the text.
pub fn copy_to_clipboard() -> ActionDef {
    ActionDef::new(
        "CopyToClipboard",
        "Copy text to the clipboard, e.g. a shopping list built with @Join",
        [Schema::String.field("text")],
    )
}

/// The system prompt, with the `Query` and `Mutation` tools the server runs.
pub fn system_prompt(tools: Vec<ToolSpec>) -> String {
    prompt::generate_system_prompt(&library(), &prompt_options(tools))
}

/// Motile's options. With tools, the rule for dashboards over `Query` rows is added; the server
/// may add `tool_examples` that use them.
pub fn prompt_options(tools: Vec<ToolSpec>) -> PromptOptions {
    let mut additional_rules: Vec<String> = RULES
        .iter()
        .enumerate()
        .map(|(index, rule)| match index {
            5 => WORKING_TOOL_RULE.to_owned(),
            11 => MEDIUM_RULE.to_owned(),
            15 => CHANGE_RULE.to_owned(),
            17 => MARKDOWN_RULE.to_owned(),
            _ => rule.replace("the app ", "Motile ").replace("The app ", "Motile "),
        })
        .collect();
    additional_rules.push(PATCH_RULE.to_owned());
    if !tools.is_empty() {
        additional_rules.push(DASHBOARD_RULE.to_owned());
    }
    let fenced = |(title, program): &(&str, &str)| format!("{title}\n\n```openui-lang\n{program}\n```");
    PromptOptions {
        preamble: Some(PREAMBLE.to_owned()),
        bindings: Some(true),
        tools: tools.into_iter().map(Tool::Spec).collect(),
        examples: EXAMPLES.iter().map(fenced).chain([PATCH_EXAMPLE.to_owned()]).collect(),
        additional_rules,
        data_workflow: Some(DATA_WORKFLOW.to_owned()),
        any_root: true,
        ..PromptOptions::default()
    }
}

/// open-intelligent-ui's own chat options (`promptOptions`), unchanged.
pub fn open_intelligent_ui_options() -> PromptOptions {
    PromptOptions {
        bindings: Some(true),
        examples: EXAMPLES.iter().map(|(title, program)| format!("{title}\n\n{program}")).collect(),
        additional_rules: RULES.iter().map(|rule| (*rule).to_owned()).collect(),
        ..PromptOptions::default()
    }
}

/// What every agent is told of programs, before it reads the rest.
pub const PREAMBLE: &str = "You can show an answer with openui-lang, a small declarative UI language that Motile's \
                        clients draw natively inside your reply: photos, maps with pins and routes, cards, charts, \
                        tables, drawings, forms and small tools to play with, and any web page, like a 3D model to \
                        rotate. Write a program in a fenced block that \
                        opens with ```openui-lang and closes with ```. Text outside the fence is ordinary Markdown, \
                        shown around the program.

When the request is not about code, answer with a program whenever the answer has structure: places, trips and \
                        itineraries (a map of the stops, a card for each day, a photo of each sight), \
                        recommendations, choices and comparisons, recipes, schedules, how-tos and steps, numbers to \
                        compare or compute, something to learn or play. If you are about to write headings over \
                        lists of places, options or steps, write a program instead, with a sentence or two of \
                        Markdown around it. Searching or thinking first doesn't change this: what you found goes into \
                        the program. Plain Markdown is for explaining code (code, reviews, implementation plans, \
                        replies while you work) and for answers of a few sentences. Showing what the work made is not \
                        explaining it: a model, a chart, a page or a result shows in a program as well after work on \
                        code as after any other request.

Once this thread has a program, a later fence with no `root` statement is a patch: Motile merges it over the \
                        latest program of this thread by statement name (the same name replaces a statement, a new \
                        name adds one, `name = null` removes one) and draws the merged program in place of that one. \
                        A fence with `root` starts a new program.";

/// The reference's workflow has the model call the tools first; an agent can't call a program's
/// tools, and reads their shapes from the prompt instead.
const DATA_WORKFLOW: &str = "## Data Workflow

The tools below run when the program does, in the thread's folder: only a program's Query and Mutation call them, \
you can't. When a program shows them:
1. Use Query() for READ operations (data that should stay live) — NEVER hardcode what a tool returns as literal arrays or objects
2. Use Mutation() for WRITE operations — triggered by button clicks via @Run(mutationRef)
3. Start each Query from its shape under \"Default values for Query results\" so the UI renders immediately
4. Use @-prefixed builtins (@Count, @Filter, @Sort, @Sum) on Query results for KPIs and aggregations — the runtime evaluates these live on every refresh
5. Hardcoded arrays are ONLY for static display data (labels, options) where no tool exists

WRONG — you read the data yourself, with git or a shell, and inlined it:
```
openCount = 2
item1 = SomeComp(\"first item title\")
item2 = SomeComp(\"second item title\")
list = SomeList([item1, item2])
chart = SomeChart([\"A\", \"B\"], [12, 8])
```
This is static — it shows stale data and won't update. Creating item1, item2, item3... manually is ALWAYS wrong when a tool exists.

RIGHT — use Query() for live data, Mutation() for writes, @builtins to derive values:
```
data = Query(\"tool_name\", {}, {rows: []})
openCount = @Count(@Filter(data.rows, \"field\", \"==\", \"value\"))
list = @Each(data.rows, \"item\", SomeComp(item.title, item.field))
createResult = Mutation(\"create_tool\", {title: $title})
submitBtn = Button(\"Create\", Action([@Run(createResult), @Run(data), @Reset($title)]))
```
Everything derives from the Query — when data refreshes, the entire dashboard updates automatically.";

const WORKING_TOOL_RULE: &str = "When you answer with a program and the request involves numbers, choices, lists, \
                                 steps, a game or something to explore, make it a working tool: keep inputs and lists \
                                 in $variables, compute everything shown from them with expressions and @functions, \
                                 and change them with @Set. The page must work on its own, without another message.";

const MEDIUM_RULE: &str = "Pick the medium that shows the idea: Chart for numbers across time or categories; Svg \
                           drawings for shapes, parts and movement; Map with MapMarker for places and routes; Table \
                           for comparisons; Flashcard and Celebration for learning and games; HtmlView for a game or \
                           animation that needs a loop, and for what no component draws, like a 3D model (STL, OBJ \
                           or glTF, inline, in three.js) to rotate and zoom. An HtmlView page starts on the \
                           thread's own background and text color, light or dark as the user's Motile is: a page \
                           that paints its own reads them with getComputedStyle (the root's background and \
                           color-scheme, the body's color), so that it sits in the thread.";

const CHANGE_RULE: &str = "To change a program from an earlier answer, write a patch with the statements that \
                           change, keeping its statement and $variable names. When you write a new program instead, \
                           start its $variables from the \"context and form values\" of the user's message, so what \
                           the user typed, added or ticked carries over.";

const MARKDOWN_RULE: &str = "Answer a plain knowledge or safety question, an explanation or anything about code in \
                             Markdown, outside any fence; add a short program with a SearchImage, a Quote or a \
                             drawing only where it helps.";

const PATCH_RULE: &str = "A patch holds only the statements that change or are new and never a root statement, \
                          which would start a new program: change the page through the statements root lists. Never \
                          repeat a whole program to change it.";

const DASHBOARD_RULE: &str = "Fetch rows with Query and compute every number, group and series from them with @Sum, \
                              @Count, @GroupBy, @Bucket, @Top, @Stats and @Change. A filter is a $variable (from this \
                              library's Select or SegmentedControl) applied with @Filter; select dates and ranges with \
                              @Between, since @Filter's < and > compare numbers only.";

const PATCH_EXAMPLE: &str = "Example 4: A later reply that adds basil to Example 1's list, as a patch with no root \
                             statement, merged over that program:

```openui-lang
base = [{name: \"spaghetti\", qty: 400, unit: \" g\"}, {name: \"tomatoes\", qty: 6, unit: \"\"}, {name: \"parmesan\", qty: 80, unit: \" g\"}, {name: \"basil\", qty: 1, unit: \" bunch\"}]
```";

/// open-intelligent-ui's 18 rules, as `prompt-options.ts` writes them.
const RULES: [&str; 18] = [
    "Compose the answer like a page, not a form letter: a Title, a short Text, then a mix of blocks that show the content. SearchImage for every place, dish, product or thing that has a look (the app finds the photo, so never say you cannot show photos); Row, Grid and Carousel to set things side by side; Badge rows for quick facts; Card only to group a block; Popover for an aside.",
    "Spacing is a system with three tiers that always nest: inner (gap 1-2, 4-8px) for a title with its subtitle, tags or line; group (gap 3-4, 12-16px) between blocks inside a Card or grid cell; section (24px) between the blocks of the page. Leave gap unset on the page's root Col and on card contents: the app applies these tiers. Set a gap only on a Row or Grid. Never use one gap for everything, and never a gap larger than the section gap.",
    "A photo beside text is a media row: Row([Box([SearchImage(query, 1.33)]), Col([Title(..., \"md\"), Row([Badge(...), Badge(...)], 1, null, null, true), Text(..., \"sm\")], 2)], 4, \"start\"). The app sizes the photo to a third of the row (at most 200px) and top-aligns the text. Facts are Badges, never Buttons or boxed pills.",
    "Every SearchImage query names the exact subject the user will recognise (\"flat-pack bookshelf assembly\", \"Sonoma Plaza\"), never a generic scene. If no photo would show the subject, use no image.",
    "Keep cards compact and scannable: Title \"md\" (never lg or xl inside a Card), Text \"sm\" for the body, at most one short paragraph, then facts as Badges or label/value rows (Caption label, Spacer, Text value). Don't write bold-lead paragraphs like \"**Weekend rhythm:** ...\" or long prose in a card. Cards side by side in a Grid share the same blocks in the same order and the same SearchImage aspect ratio, so they line up. For example: c = Card([SearchImage(\"Sonoma Plaza\", 1.5), Col([Title(\"Sonoma\", \"md\"), Row([Badge(\"1 h drive\", null, \"car\"), Badge(\"Wine country\", \"purple\")], 1, null, null, true)], 2), Text(\"A slow plaza, tasting rooms and quiet back roads.\", \"sm\"), Col([Row([Caption(\"Best for\"), Spacer(), Text(\"Food and wine\", \"sm\")]), Row([Caption(\"Trade-off\"), Spacer(), Text(\"Tastings add up\", \"sm\")])], 1)])",
    "Answer with a working tool whenever the request involves numbers, choices, lists, steps, a game or something to explore: keep inputs and lists in $variables, compute everything shown from them with expressions and @functions, and change them with @Set. The page must work on its own, without another message.",
    "Use @ToAssistant only for what the page cannot compute: a Form's submit Button, or a new request (\"Plan another day\"). Its message is short and first person, like the user typing it (\"Help me choose a bike\"); the form values travel with it, so never repeat them in the message.",
    "$variable defaults are literals (numbers, strings, lists, objects), never expressions or @calls. Edit lists with @Append, @Remove, @RemoveAt and @Update; track selections with @Toggle and @Includes.",
    "cond ? a : b also picks between components; there is no @If. Use only the listed @functions; you cannot define your own.",
    "Derive rows once in a statement with @Each(list, \"x\", {...}), then reuse them in @Each, Table, Chart, @Sum(rows.field) and @Join. @Each(@Range(0, n), \"i\", ...) builds a series or repeats a step.",
    "@Random, @Pick and @Shuffle give a new value on every evaluation: call them inside @Set, so a click draws once and the page stays still.",
    "Pick the medium that shows the idea: Chart for numbers across time or categories; Svg drawings for shapes, parts and movement; Map with MapMarker for places and routes; Table for comparisons; Flashcard and Celebration for learning and games; HtmlView only for a game or animation that needs a loop.",
    "Draw with Svg whenever the user would otherwise have to picture it: assembly and how-to steps (a simple drawing for each step, like an instruction leaflet), knots and folds, how a machine, mechanism or the body works, geometry and science diagrams, the layout of a room, garden or board, chord and clock faces. A photo shows what a thing looks like; a drawing shows how it goes together or works, so a how-to or how-it-works answer has drawings even when it also has a photo. Keep each drawing simple: a viewBox about 240 wide, a dozen or so shapes in the listed colors with SvgText labels, a Line with a small Polygon head for an arrow, and let $variables such as the current step drive it.",
    "Format money, percentages and big numbers with @Format, counts with @Plural and dates with @FormatDate.",
    "Dates relative to now (today, this week, next month) start from @Today(), never a fixed date. <, >, <= and >= compare numbers only, so order two dates with @DaysBetween(a, b) > 0.",
    "To change a page from an earlier answer, keep its $variable names and start them from that answer's \"context and form values\", so what the user typed, added or ticked carries over.",
    "Read a number input for a count or amount through @Default and @Clamp with a sensible minimum, e.g. @Clamp(@Default($people, 1), 1, 50), so 0, empty or negative cannot divide by zero or zero out the page.",
    "Answer a plain knowledge or safety question with Markdown text, adding a SearchImage or a Quote only where it helps.",
];

/// open-intelligent-ui's three examples, their titles and programs.
const EXAMPLES: [(&str, &str); 3] = [
    (
        "Example 1: A guest stepper that rescales a shopping list, with a checklist and a copy button:",
        r#"root = Card([title, stepper, list, copy])
$guests = 4
$got = []
title = Title("Pasta night for " + @Plural($guests, "guest"), "lg")
stepper = Row([IconButton("minus", @Set($guests, @Clamp($guests - 1, 1, 20)), "Fewer"), Text($guests + "", "lg", null, "semibold"), IconButton("plus", @Set($guests, @Clamp($guests + 1, 1, 20)), "More")], 3)
lines = @Each(base, "i", {name: i.name, qty: @Round(i.qty * $guests / 4) + i.unit})
list = Col(@Each(lines, "l", Pressable([Row([Icon(@Includes($got, l.name) ? "check-circle" : "circle", "sm"), Text(l.qty + " " + l.name, "sm", null, null, false, @Includes($got, l.name))], 2)], @Set($got, @Toggle($got, l.name)), @Includes($got, l.name), 2)), 1)
copy = Button("Copy list", @CopyToClipboard(@Join(@Each(lines, "l", l.qty + " " + l.name), "\n")), "outline", null, "sm", "copy")
base = [{name: "spaghetti", qty: 400, unit: " g"}, {name: "tomatoes", qty: 6, unit: ""}, {name: "parmesan", qty: 80, unit: " g"}]"#,
    ),
    (
        "Example 2: A composed trip answer: photos, facts, day rows and a short form:",
        r#"root = Col([title, intro, photos, facts, Title("Day by day", "md"), days, prefs])
$pace = "Relaxed"
title = Title("Three days in Lisbon", "xl")
intro = Text("Hills, tiled facades and the Tagus: start in **Alfama**, give a day to Belém, then take the train to Sintra.")
photos = Carousel([SearchImage("Alfama Lisbon", 1.25), SearchImage("Belém Tower", 1.25), SearchImage("Pena Palace", 1.25)], 260)
facts = Row([Badge("Best in April to June"), Badge("Tram 28", "info"), Badge("About €120 a day", "success")], 2, null, null, true)
days = Col(@Each(plan, "d", Row([Box([SearchImage(d.place, 1, "lg")], null, null, null, null, null, null, 96), Col([Text([Icon("map-pin", "sm"), " Day " + d.day + " · " + d.title], "md", null, "semibold"), Text(d.text, "sm", "muted")], 1)], 4, "start")), 4)
prefs = Card([Title("Make it yours", "sm"), Form([RadioGroup($pace, ["Relaxed", "Packed"], "Pace", "pills"), Button("Plan my trip", @ToAssistant("Tailor my Lisbon trip"))])], "soft")
plan = [{day: 1, title: "Alfama", place: "São Jorge Castle", text: "Climb to the castle, then lunch on grilled sardines."}, {day: 2, title: "Belém", place: "Jerónimos Monastery", text: "The monastery, the tower and pastéis de Belém."}, {day: 3, title: "Sintra", place: "Pena Palace", text: "Train from Rossio; book the palace for 9 AM."}]"#,
    ),
    (
        "Example 3: Step-by-step instructions with a drawing for each step, driven by $step:",
        r##"root = Col([head, viewer, strip])
$step = 0
head = Col([Title("Build a small bookcase", "xl"), Text("Four steps and about 30 minutes. You need the Allen key from the box and a hammer.", "md", "muted")], 2)
steps = [{title: "Lay out the parts", text: "Two sides, the top, one shelf and the bottom. Tap the 12 wooden dowels into the holes on the sides."}, {title: "Fix the boards to one side", text: "Push the top, shelf and bottom onto one side's dowels, then turn each cam lock clockwise until it pulls tight."}, {title: "Add the other side", text: "Fit the second side onto the free dowels and tighten its cam locks the same way."}, {title: "Nail on the back", text: "Lay it face down, check the corners are square, then nail the back panel about every 10 cm."}]
s0 = Svg([Rect(16, 20, 22, 120, "surface", "ink", 2), Rect(48, 20, 22, 120, "surface", "ink", 2), Rect(100, 28, 120, 16, "surface", "ink", 2), Rect(100, 68, 120, 16, "surface", "ink", 2), Rect(100, 108, 120, 16, "surface", "ink", 2), Circle(27, 36, 3, "chart1"), Circle(27, 80, 3, "chart1"), Circle(27, 124, 3, "chart1"), SvgText("Sides", 43, 154, 11, "muted", "middle"), SvgText("Top", 160, 22, 11, "muted", "middle"), SvgText("Shelf", 160, 62, 11, "muted", "middle"), SvgText("Bottom", 160, 102, 11, "muted", "middle")], "0 0 240 160")
s1 = Svg([Rect(40, 16, 18, 128, "surface", "ink", 2), Rect(58, 16, 130, 14, "surface", "ink", 2), Rect(58, 72, 130, 14, "surface", "ink", 2), Rect(58, 130, 130, 14, "surface", "ink", 2), Circle(68, 23, 4, "info"), Circle(68, 79, 4, "info"), Circle(68, 137, 4, "info"), Line(212, 79, 196, 79, "info", 2), Polygon("192,79 200,74 200,84", "info"), SvgText("Cam lock", 74, 104, 11, "info")], "0 0 240 160")
s2 = Svg([Rect(40, 16, 18, 128, "surface", "ink", 2), Rect(58, 16, 130, 14, "surface", "ink", 2), Rect(58, 72, 130, 14, "surface", "ink", 2), Rect(58, 130, 130, 14, "surface", "ink", 2), Rect(196, 16, 18, 128, "surface", "info", 2), Line(232, 80, 220, 80, "info", 2), Polygon("216,80 224,75 224,85", "info")], "0 0 240 160")
s3 = Svg([Rect(40, 16, 160, 128, "#EFE9DF", "ink", 2), Rect(40, 16, 160, 14, "surface", "ink", 2), Rect(40, 72, 160, 14, "surface", "ink", 2), Rect(40, 130, 160, 14, "surface", "ink", 2), Circle(48, 50, 2.5, "ink"), Circle(192, 50, 2.5, "ink"), Circle(48, 108, 2.5, "ink"), Circle(192, 108, 2.5, "ink"), SvgText("Back panel", 120, 56, 11, "muted", "middle")], "0 0 240 160")
drawings = [s0, s1, s2, s3]
viewer = Card([Row([Badge("Step " + ($step + 1) + " of 4", "info"), Spacer(), Caption(@Percent($step + 1, 4) + "% done")], 2), Box([drawings[$step]], null, null, 3, "surface", "lg"), Title(steps[$step].title, "md"), Text(steps[$step].text, "sm"), Row([Button("Back", @Set($step, @Clamp($step - 1, 0, 3)), "outline", null, null, "arrow-left", $step == 0), Spacer(), Button("Next step", @Set($step, @Clamp($step + 1, 0, 3)), "solid", null, null, "arrow-right", $step == 3)], 2)], null, 4, 3)
strip = Grid(@Each(@Range(0, 4), "i", Pressable([drawings[i], Caption((i + 1) + ". " + steps[i].title)], @Set($step, i), i == $step, 2)), 4, 110, 2)"##,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn fixture(path: &str) -> String {
        std::fs::read_to_string(format!("{}/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn text(value: &Value) -> String {
        serde_json::to_string(value).unwrap()
    }

    /// Compares with a golden file, or writes it when `OPENUI_BLESS` is set.
    fn golden(path: &str, actual: &str) {
        let file = format!("{}/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        if std::env::var_os("OPENUI_BLESS").is_some() {
            std::fs::write(&file, actual).unwrap();
        }
        assert!(fixture(path) == actual, "{path} changed; run with OPENUI_BLESS=1 to write it");
    }

    #[test]
    fn is_the_library_open_intelligent_ui_generates() {
        let spec = Value::from_json_str(&fixture("oiu/spec.json")).unwrap();
        let ours = library().to_spec();
        for key in ["root", "components", "componentGroups", "functions", "actions", "schema"] {
            assert_eq!(text(ours.get(key).unwrap()), text(spec.get(key).unwrap()), "{key}");
        }
        assert_eq!(text(&ours), text(&spec));
    }

    #[test]
    fn writes_the_reference_prompt_for_open_intelligent_ui() {
        golden(
            "prompt/open-intelligent-ui.txt",
            &prompt::generate_system_prompt(&library(), &open_intelligent_ui_options()),
        );
    }

    /// Two tools as the server will pass them: a `Query` and a `Mutation`.
    fn placeholder_tools() -> Vec<ToolSpec> {
        let json = |text: &str| Value::from_json_str(text).unwrap();
        vec![
            ToolSpec {
                name: "git_log".into(),
                description: Some("The thread's commits, newest first".into()),
                input_schema: json(r#"{"type": "object", "properties": {"limit": {"type": "number"}}}"#),
                output_schema: Some(json(
                    r#"{"type": "object", "properties": {"commits": {"type": "array", "items": {"type": "object", "properties": {"hash": {"type": "string"}, "author": {"type": "string"}, "date": {"type": "string"}, "subject": {"type": "string"}}, "required": ["hash", "author", "date", "subject"]}}}}"#,
                )),
            },
            ToolSpec {
                name: "run".into(),
                description: Some("Runs a shell command in the thread's folder; only from a click".into()),
                input_schema: json(
                    r#"{"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}"#,
                ),
                output_schema: Some(json(
                    r#"{"type": "object", "properties": {"exit_code": {"type": "number"}, "stdout": {"type": "string"}, "stderr": {"type": "string"}}}"#,
                )),
            },
        ]
    }

    #[test]
    fn writes_motiles_prompt() {
        let prompt = system_prompt(placeholder_tools());
        assert_eq!(prompt, system_prompt(placeholder_tools()));
        golden("prompt/motile.txt", &prompt);
        golden("prompt/motile-without-tools.txt", &system_prompt(Vec::new()));
    }

    #[test]
    fn lists_the_tools_with_their_defaults() {
        let prompt = system_prompt(placeholder_tools());
        assert!(prompt.contains(
            "- git_log(limit?: number) → {commits?: {hash: string, author: string, date: string, subject: string}[]}\n  \
             The thread's commits, newest first"
        ));
        assert!(prompt.contains("- run(command: string) → {exit_code?: number, stdout?: string, stderr?: string}"));
        assert!(
            prompt
                .contains("- git_log: `{\"commits\":[]}`\n- run: `{\"exit_code\":0,\"stdout\":\"\",\"stderr\":\"\"}`")
        );
        assert!(prompt.contains("## Query — Live Data Fetching") && prompt.contains("@Run(ref: Query | Mutation)"));
        assert!(!system_prompt(Vec::new()).contains("## Query"));
    }

    #[test]
    fn names_only_what_the_library_has() {
        let library = library();
        let placeholders = ["TypeName", "SomeComp", "SomeList", "SomeChart", "Comp"];
        for prompt in [system_prompt(placeholder_tools()), system_prompt(Vec::new())] {
            for word in ["FormControl", "SelectItem", "TextContent", "NOT arrays", "\"secondary\""] {
                assert!(!prompt.contains(word), "{word}");
            }
            let bytes = prompt.as_bytes();
            for (index, _) in prompt.match_indices('(') {
                let start = bytes[..index].iter().rposition(|b| !b.is_ascii_alphanumeric()).map_or(0, |at| at + 1);
                let name = &prompt[start..index];
                if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    continue;
                }
                let known = library.component(name).is_some()
                    || library.call(name).is_some()
                    || ["Query", "Mutation", "Action"].contains(&name)
                    || placeholders.contains(&name);
                assert!(known, "{name} is not in the library");
            }
        }
    }

    #[test]
    fn asks_nothing_the_agent_cant_do() {
        let prompt = system_prompt(placeholder_tools());
        assert!(!prompt.contains("Call the most relevant tool"), "an agent can't call a program's tools");
        assert!(prompt.contains("only a program's Query and Mutation call them, you can't."));
        assert!(prompt.contains("must define `root`, usually `root = Col(...)`"));
        assert!(prompt.contains("1. The root statement is the FIRST line"));
        assert!(prompt.contains("root = Card([title, stepper, list, copy])"));
    }

    #[test]
    fn tells_the_agent_to_fence_programs() {
        let prompt = system_prompt(Vec::new());
        assert!(prompt.starts_with("You can show an answer with openui-lang"));
        assert!(prompt.contains("```openui-lang\nroot = Card([title, stepper, list, copy])"));
        assert!(!prompt.contains("Your ENTIRE response must be valid openui-lang code"));
    }
}
