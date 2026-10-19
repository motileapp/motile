//! The resolved trees of the corpus (`fixtures/openui`, `OPENUI_BLESS=1` rewrites them), and the
//! rules each component follows.

use std::path::Path;
use std::sync::Arc;

use motile_openui::{FixedEnv, Runtime, Step, Value};
use serde_json::json;

use super::*;
use crate::render::markdown::{BOLD, CODE, LINK};

fn runtime(program: &str) -> Runtime {
    let library = crate::render::openui::host::library();
    let forms = Arc::new(crate::render::openui::LibraryForms::new(library.clone()));
    let mut runtime = Runtime::new(library, Arc::new(FixedEnv::default()), forms);
    runtime.set_text(program, true);
    runtime
}

fn ctx(state: &dyn UiState) -> ResolveCtx<'_> {
    ResolveCtx { streaming: false, tools: true, state }
}

fn tree(program: &str) -> UiNode {
    let runtime = runtime(program);
    resolve(runtime.root().expect("a root"), &ctx(&NoState))
}

/// The node at an id.
fn at<'a>(node: &'a UiNode, id: &str) -> &'a UiNode {
    fn find<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        children(node).into_iter().find_map(|child| find(child, id))
    }
    find(node, id).unwrap_or_else(|| panic!("no node {id}"))
}

fn slot(slot: &Slot) -> Vec<&UiNode> {
    slot.children.iter().map(Arc::as_ref).collect()
}

fn children(node: &UiNode) -> Vec<&UiNode> {
    match &node.kind {
        UiKind::Col { children, .. }
        | UiKind::Row { children, .. }
        | UiKind::Box { children, .. }
        | UiKind::Grid { children, .. }
        | UiKind::Card { children, .. }
        | UiKind::Carousel { children, .. }
        | UiKind::Pressable { children, .. }
        | UiKind::Form { children, .. }
        | UiKind::ListItem { children, .. } => children.iter().map(Arc::as_ref).collect(),
        UiKind::Text { inlines, .. } => inlines.iter().map(Arc::as_ref).collect(),
        UiKind::List { items, .. } => items.iter().map(Arc::as_ref).collect(),
        UiKind::Popover { trigger, content, .. } => [slot(trigger), slot(content)].concat(),
        UiKind::Flashcard { front, back, .. } => [slot(front), slot(back)].concat(),
        UiKind::Table { rows, .. } => rows
            .iter()
            .flatten()
            .filter_map(|cell| match cell {
                TableCell::Node { node } => Some(node.as_ref()),
                TableCell::Text { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn kind(program: &str, id: &str) -> UiKind {
    at(&tree(program), id).kind.clone()
}

fn token(name: &'static str) -> Color {
    Color::token(name)
}

#[test]
fn the_corpus_resolves_as_its_goldens_say() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/openui");
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/openui");
    let bless = std::env::var_os("OPENUI_BLESS").is_some();
    let mut programs: Vec<_> = std::fs::read_dir(&corpus)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "oui"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty());
    let mut stale = Vec::new();
    for program in programs {
        let name = program.file_stem().unwrap().to_string_lossy().into_owned();
        let tree = tree(&std::fs::read_to_string(&program).unwrap());
        let resolved = serde_json::to_string_pretty(&tree).unwrap() + "\n";
        let golden = goldens.join(format!("{name}.json"));
        if bless {
            std::fs::create_dir_all(&goldens).unwrap();
            std::fs::write(&golden, &resolved).unwrap();
            continue;
        }
        if std::fs::read_to_string(&golden).ok().as_deref() != Some(resolved.as_str()) {
            stale.push(name);
        }
    }
    assert!(stale.is_empty(), "these trees changed (OPENUI_BLESS=1 rewrites them): {stale:?}");
}

#[test]
fn a_tree_reads_back_as_it_was_written() {
    fn same(a: &serde_json::Value, b: &serde_json::Value) -> bool {
        use serde_json::Value::{Array, Number, Object};
        match (a, b) {
            (Number(a), Number(b)) => {
                let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                (a - b).abs() <= 1e-6 * a.abs().max(1.0)
            }
            (Array(a), Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
            (Object(a), Object(b)) => {
                a.len() == b.len() && a.iter().all(|(key, value)| b.get(key).is_some_and(|other| same(value, other)))
            }
            (a, b) => a == b,
        }
    }
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/openui");
    for entry in std::fs::read_dir(goldens).unwrap() {
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap();
        let tree: UiNode = serde_json::from_value(json.clone()).unwrap();
        assert!(same(&serde_json::to_value(&tree).unwrap(), &json));
    }
}

#[test]
fn colours_become_tokens_washes_or_safe_rgba() {
    let program = r##"root = Col([Box([Text("a", "md", "muted"), Text("b", "md", "chart3"), Text("c", "md", "url(x)"), Text("d", "md", "red;x")], "col", 1, 2, "success"), Box([], "col", null, null, "rebeccapurple"), Box([], "col", null, null, "var(--x)"), Box([], "col", null, null, "surface")])"##;
    let tree = tree(program);
    let UiKind::Box { background, inner_surface, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!(background, &Some(Color::Wash("success".into())));
    assert!(inner_surface);
    let colour = |id: &str| match &at(&tree, id).kind {
        UiKind::Text { color, .. } => color.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(colour("children/0/children/0"), token("muted-foreground"));
    assert_eq!(colour("children/0/children/1"), token("chart-3"));
    assert_eq!(colour("children/0/children/2"), token("foreground"));
    assert_eq!(colour("children/0/children/3"), token("foreground"));
    let background = |id: &str| match &at(&tree, id).kind {
        UiKind::Box { background, .. } => background.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(background("children/1"), Some(Color::Rgba([102, 51, 153, 255])));
    assert_eq!(background("children/2"), None);
    assert_eq!(background("children/3"), Some(Color::Surface(Layer::Box)));
}

#[test]
fn text_on_a_raw_colour_takes_the_ink_that_reads_on_it() {
    let program = r##"root = Col([Box([Title("t"), Caption("c"), Text("f", "md", "faint"), Icon("star")], "col", null, 3, "#fafafa"), Box([Text("x")], "col", null, null, "rgb(20, 30, 90)"), Box([Text("y")], "col", null, null, "orange")])"##;
    let tree = tree(program);
    let dark = 13;
    let ink = |level: u8, a: u8| Color::Rgba([level, level, level, a]);
    let color = |id: &str| match &at(&tree, id).kind {
        UiKind::Title { color, .. }
        | UiKind::Caption { color, .. }
        | UiKind::Text { color, .. }
        | UiKind::Icon { color, .. } => color.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(color("children/0/children/0"), ink(dark, 255));
    assert_eq!(color("children/0/children/1"), ink(dark, 184));
    assert_eq!(color("children/0/children/2"), ink(dark, 140));
    assert_eq!(color("children/0/children/3"), ink(dark, 255));
    assert_eq!(color("children/1/children/0"), ink(255, 255));
    assert_eq!(color("children/2/children/0"), token("foreground"), "only hex and rgb() choose an ink");
    let UiKind::Box { inner_surface, .. } = kind(program, "children/0") else { panic!() };
    assert!(!inner_surface);
}

#[test]
fn lengths_and_steps_are_points() {
    let program = r#"root = Row([Box([], "col", 3, 2, null, 6, true, "50%"), Box([], "col", null, null, null, "lg", null, 120), Box([], "col", null, null, null, null, null, "2em", "10vh"), Box([], "col", null, null, null, null, null, "auto"), Box([], "col", null, null, null, null, null, null, null, 2)], 5)"#;
    let tree = tree(program);
    let UiKind::Row { gap, sizing, .. } = &tree.kind else { panic!() };
    assert_eq!(*gap, 20.0);
    let UiKind::Box { gap, padding, radius, width, border, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!((*gap, *padding, *border), (12.0, 8.0, true));
    assert_eq!(*radius, Some(Radius { kind: RadiusKind::Points, points: 6.0 }));
    assert_eq!(*width, Some(Length { unit: LengthUnit::Fraction, value: 0.5 }));
    let UiKind::Box { radius, width, .. } = &at(&tree, "children/1").kind else { panic!() };
    assert_eq!(*radius, Some(Radius { kind: RadiusKind::Lg, points: 12.0 }));
    assert_eq!(*width, Some(Length { unit: LengthUnit::Points, value: 120.0 }));
    let UiKind::Box { width, height, .. } = &at(&tree, "children/2").kind else { panic!() };
    assert_eq!(*width, Some(Length { unit: LengthUnit::Points, value: 28.0 }));
    assert_eq!(*height, Some(Length { unit: LengthUnit::Window, value: 0.1 }));
    let UiKind::Box { width, .. } = &at(&tree, "children/3").kind else { panic!() };
    assert_eq!(*width, None);
    assert_eq!(
        sizing[0],
        Sizing {
            basis: Basis::Length,
            length: Some(Length { unit: LengthUnit::Fraction, value: 0.5 }),
            grow: 0.0,
            shrink: true
        }
    );
    assert_eq!(
        sizing[1],
        Sizing {
            basis: Basis::Length,
            length: Some(Length { unit: LengthUnit::Points, value: 120.0 }),
            grow: 0.0,
            shrink: false
        }
    );
    assert_eq!(sizing[4], Sizing { basis: Basis::Zero, length: None, grow: 2.0, shrink: false });
}

#[test]
fn text_lists_lay_their_components_in_the_run() {
    let program =
        r#"root = Col([Text([Icon("clock", "sm"), "  25   min · ", Badge("Easy", "success"), " **fast**"], "sm")])"#;
    let UiKind::Text { text, inlines, style, .. } = kind(program, "children/0") else { panic!() };
    assert_eq!(text.text, "\u{fffc} 25 min · \u{fffc} fast");
    assert_eq!(text.runs, [[13, 4, BOLD]]);
    assert_eq!(style, TextStyle::Small);
    let ids: Vec<&str> = inlines.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids, ["children/0/text/0", "children/0/text/2"]);
    let UiKind::Badge { tone, .. } = &inlines[1].kind else { panic!() };
    assert_eq!(tone.as_deref(), Some("success"));
}

#[test]
fn text_keeps_its_line_breaks_and_formatting() {
    let program =
        r#"root = Col([Text("One  line\n  then `code` and [a link](https://motile.app)"), Title("A   **b**\nc")])"#;
    let UiKind::Text { text, .. } = kind(program, "children/0") else { panic!() };
    assert_eq!(text.text, "One line\u{2028}then code and a link");
    assert_eq!(text.runs, [[14, 4, CODE], [23, 6, LINK]]);
    let UiKind::Title { text, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!(text.text, "A b c");
}

#[test]
fn numeric_columns_align_to_the_end() {
    let program = r#"root = Col([Table(["Plan", "Seats", "Price", "Note", "Mixed"], [["Basic", 1, "$9", "n/a", 1], ["Team", 10, "$1.2k", "—", "lots"], ["Big", "", "45%", null, 3]])])"#;
    let UiKind::Table { columns, rows } = kind(program, "children/0") else { panic!() };
    let numeric: Vec<bool> = columns.iter().map(|column| column.numeric).collect();
    assert_eq!(numeric, [false, true, true, false, false]);
    let TableCell::Text { text } = &rows[1][1] else { panic!() };
    assert_eq!(text.text, "10");
}

#[test]
fn table_cells_format_numbers_and_hold_components() {
    let program = r#"root = Col([Table(["A", "B"], [[1234.5678, Badge("ok")], "not a row", ["**x**"]])])"#;
    let tree = tree(program);
    let UiKind::Table { rows, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!(rows.len(), 2);
    let TableCell::Text { text } = &rows[0][0] else { panic!() };
    assert_eq!(text.text, "1,234.57");
    let TableCell::Node { node } = &rows[0][1] else { panic!() };
    assert_eq!(node.id, "children/0/rows/0/1");
    let TableCell::Text { text } = &rows[1][0] else { panic!() };
    assert_eq!((text.text.as_str(), text.runs.as_slice()), ("x", [[0, 1, BOLD]].as_slice()));
    let TableCell::Text { text } = &rows[1][1] else { panic!() };
    assert_eq!(text.text, "");
}

#[test]
fn grids_drop_columns_that_get_too_narrow() {
    assert_eq!(grid_columns(3, None, 12.0, 300.0), 3);
    assert_eq!(grid_columns(3, Some(150.0), 16.0, 720.0), 3);
    assert_eq!(grid_columns(3, Some(150.0), 16.0, 390.0), 2);
    assert_eq!(grid_columns(3, Some(150.0), 16.0, 140.0), 1);
    assert_eq!(grid_columns(3, Some(100.0), 8.0, 390.0), 3, "three columns that just fit");
    assert_eq!(grid_columns(2, Some(200.0), 16.0, 390.0), 1);
    assert_eq!(grid_columns(40, None, 12.0, 300.0), 12);
    let UiKind::Grid { columns, min_child_width, gap, .. } =
        kind(r#"root = Col([Grid([], 2.6, 150, 4)])"#, "children/0")
    else {
        panic!()
    };
    assert_eq!((columns, min_child_width, gap), (3, Some(150.0), 16.0));
    let UiKind::Grid { columns, gap, .. } = kind(r#"root = Col([Grid([], 0)])"#, "children/0") else { panic!() };
    assert_eq!((columns, gap), (1, 12.0));
}

#[test]
fn icons_are_lucide_characters_by_name_or_alias() {
    let program =
        r#"root = Col([Icon("Home", "lg", "success"), Icon("no-such-thing"), Badge("x", "neutral", "map_pin")])"#;
    let UiKind::Icon { icon, size, color } = kind(program, "children/0") else { panic!() };
    assert_eq!(icon, Some(Glyph { name: "house".into(), glyph: "\u{e0f5}".into() }));
    assert_eq!((size, color), (24.0, token("success")));
    let UiKind::Icon { icon, size, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!((icon, size), (None, 18.0));
    let UiKind::Badge { icon, .. } = kind(program, "children/2") else { panic!() };
    assert_eq!(icon.map(|icon| icon.name), Some("map-pin".into()));
}

#[test]
fn sliders_keep_their_value_in_bounds_and_read_it_out() {
    let program = r#"$v = 250
root = Col([Slider($v, 0, 200, 0, "Budget", "$", [{value: 0, label: "0"}, {value: 100, label: "half"}, {value: 199, label: "max"}, {value: 300, label: "out"}]), Slider(3.14159, 10, 5, null, null, "%"), Slider(5, 0, 10)])"#;
    let UiKind::Slider { value, min, max, step, at, readout, head, marks, .. } = kind(program, "children/0") else {
        panic!()
    };
    assert_eq!((value, min, max, step, at), (200.0, 0.0, 200.0, 1.0, 1.0));
    assert_eq!((readout.as_str(), head), ("$200", true));
    let edges: Vec<(f64, MarkEdge)> = marks.iter().map(|mark| (mark.at, mark.edge)).collect();
    assert_eq!(edges, [(0.0, MarkEdge::Start), (0.5, MarkEdge::Middle), (0.995, MarkEdge::End)]);
    let UiKind::Slider { value, min, max, readout, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!((value, min, max, readout.as_str()), (10.0, 10.0, 110.0, "10%"));
    let UiKind::Slider { head, readout, .. } = kind(program, "children/2") else { panic!() };
    assert_eq!((head, readout.as_str()), (false, "5"));
}

fn spacing_of(program: &str, id: &str) -> Vec<f32> {
    match kind(program, id) {
        UiKind::Col { spacing, .. }
        | UiKind::Card { spacing, .. }
        | UiKind::Box { spacing, .. }
        | UiKind::Form { spacing, .. } => spacing,
        other => panic!("{other:?}"),
    }
}

#[test]
fn flows_space_blocks_like_an_article() {
    let program = r#"root = Col([Title("T"), Text("a"), Text("b"), Card([]), Caption("c"), Title("U"), Row([Badge("x")]), Divider(), Card([]), Col([Text("n"), Card([])])])"#;
    assert_eq!(spacing_of(program, ""), [0.0, 8.0, 12.0, 24.0, 24.0, 4.0, 8.0, 24.0, 28.0, 24.0]);
    assert_eq!(spacing_of(program, "children/9"), [0.0, 12.0], "a flow inside a flow");
    let gapped = r#"root = Col([Divider(), Text("a"), Divider(), Text("b")], 2)"#;
    assert_eq!(spacing_of(gapped, ""), [4.0, 12.0, 12.0, 12.0]);
}

#[test]
fn a_cards_first_small_title_and_its_subtitle_are_the_cards() {
    let program = r#"root = Col([Card([Title("Plan"), Text("Monthly", "lg"), Title("Big", "2xl"), Text("Body")]), Card([Title("Plan", "sm"), Caption("Sub")], null, null, 2), Card([Row([Title("In row", "md"), Caption("next")]), Text("x")]), Card([Text("x"), Title("Late", "sm")])])"#;
    let tree = tree(program);
    let style = |id: &str| match &at(&tree, id).kind {
        UiKind::Title { style, .. } | UiKind::Text { style, .. } | UiKind::Caption { style, .. } => *style,
        other => panic!("{other:?}"),
    };
    assert_eq!(style("children/0/children/0"), TextStyle::CardTitle);
    assert_eq!(style("children/0/children/1"), TextStyle::CardSubtitle);
    assert_eq!(style("children/0/children/2"), TextStyle::CardHeading);
    assert_eq!(style("children/0/children/3"), TextStyle::BodyCard);
    let UiKind::Text { color, .. } = &at(&tree, "children/0/children/1").kind else { panic!() };
    assert_eq!(color, &token("muted-foreground"));
    assert_eq!(spacing_of(program, "children/0"), [0.0, 8.0, 12.0, 8.0]);
    assert_eq!(spacing_of(program, "children/1"), [0.0, 10.0]);
    assert_eq!(style("children/2/children/0/children/0"), TextStyle::CardTitle);
    assert_eq!(style("children/2/children/0/children/1"), TextStyle::CardSubtitle);
    assert_eq!(style("children/3/children/1"), TextStyle::TitleSm);
}

#[test]
fn grid_cells_and_carousel_items_that_flow_take_card_typography() {
    let program = r#"root = Col([Grid([Col([Title("Big", "xl"), Text("t")]), Col([Title("Gap", "xl")], 1)]), Carousel([Box([Text("t")]), Card([]), Text("loose")], 900, 2), Title("Out", "xl"), Text("out")])"#;
    let tree = tree(program);
    let style = |id: &str| match &at(&tree, id).kind {
        UiKind::Title { style, .. } | UiKind::Text { style, .. } => *style,
        other => panic!("{other:?}"),
    };
    assert_eq!(style("children/0/children/0/children/0"), TextStyle::CardHeading);
    assert_eq!(style("children/0/children/0/children/1"), TextStyle::BodyCard);
    assert_eq!(style("children/0/children/1/children/0"), TextStyle::TitleXl);
    assert_eq!(style("children/1/children/0/children/0"), TextStyle::BodyCard);
    assert_eq!(style("children/2"), TextStyle::TitleXl);
    assert_eq!(style("children/3"), TextStyle::Body);
    let UiKind::Carousel { item_width, gap, fill, .. } = &at(&tree, "children/1").kind else { panic!() };
    assert_eq!((*item_width, *gap, fill.as_slice()), (640.0, 8.0, [false, true, false].as_slice()));
}

#[test]
fn rows_around_an_image_are_media_rows() {
    let program = r#"root = Col([Row([SearchImage("Belém Tower", 2), Col([Title("Belém")])], 2, "center"), Row([Box([Image("https://x.dev/a.png", "A")], "col", 0, 0, null, null, false, 64, 64), Text("t")]), Row([Box([Image("https://x.dev/a.png", "A"), Text("t")])])])"#;
    let tree = tree(program);
    let UiKind::Row { gap, align, sizing, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!((*gap, *align, sizing[0].basis, sizing[1].basis), (16.0, Align::Start, Basis::Media, Basis::Content));
    let UiKind::SearchImage { aspect_ratio, .. } = &at(&tree, "children/0/children/0").kind else { panic!() };
    assert_eq!(*aspect_ratio, 4.0 / 3.0);
    let UiKind::Row { sizing, .. } = &at(&tree, "children/1").kind else { panic!() };
    assert_eq!(sizing[0].basis, Basis::Media);
    let UiKind::Image { aspect_ratio, .. } = &at(&tree, "children/1/children/0/children/0").kind else { panic!() };
    assert_eq!(*aspect_ratio, 4.0 / 3.0, "a box's only image in a media row is 4:3 too");
    let UiKind::Box { width, height, .. } = &at(&tree, "children/1/children/0").kind else { panic!() };
    assert_eq!((width, height), (&None, &None), "the media row sizes the box around the image");
    let UiKind::Row { gap, sizing, .. } = &at(&tree, "children/2").kind else { panic!() };
    assert_eq!((*gap, sizing[0].basis), (8.0, Basis::Content));
}

#[test]
fn cards_in_cards_are_nested() {
    let UiKind::Card { nested, padding, .. } = kind(r#"root = Card([Card([])])"#, "children/0") else { panic!() };
    assert_eq!((nested, padding), (true, Some(16.0)));
    let UiKind::Card { nested, padding, soft, .. } = kind(r#"root = Col([Card([], "soft", 3)])"#, "children/0") else {
        panic!()
    };
    assert_eq!((nested, padding, soft), (false, Some(12.0), true));
}

#[test]
fn a_forms_closing_button_stands_apart() {
    let program = r#"root = Form([Input("", "Name"), Divider(), Row([Button("Send")])])"#;
    assert_eq!(spacing_of(program, ""), [0.0, 28.0, 44.0]);
}

#[test]
fn lists_number_their_items_and_mark_them() {
    let program = r#"root = Col([List(["one", ListItem("two", "desc"), null, ListItem("three", null, "B2"), ListItem("four", null, "9:30")], "number")])"#;
    let tree = tree(program);
    let UiKind::List { items, wide, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert!(wide);
    let markers: Vec<(String, ItemMarker, bool)> = items
        .iter()
        .map(|item| match &item.kind {
            UiKind::ListItem { marker, medium, .. } => (item.id.clone(), marker.clone(), *medium),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        markers,
        [
            ("children/0/items/0".into(), ItemMarker::Number { number: 1 }, false),
            ("children/0/items/1".into(), ItemMarker::Number { number: 2 }, true),
            ("children/0/items/3".into(), ItemMarker::Chip { text: "B2".into() }, false),
            ("children/0/items/4".into(), ItemMarker::Label { text: "9:30".into() }, false),
        ]
    );
}

#[test]
fn links_check_their_address() {
    let program = r#"root = Col([Link("Docs", " https://motile.app/docs "), Link("", "https://www.bea.gov/x"), Link("Bad", "javascript:alert(1)")])"#;
    assert_eq!(
        kind(program, "children/0"),
        UiKind::Link { text: "Docs".into(), url: Some("https://motile.app/docs".into()) }
    );
    assert_eq!(
        kind(program, "children/1"),
        UiKind::Link { text: "www.bea.gov".into(), url: Some("https://www.bea.gov/x".into()) }
    );
    assert_eq!(kind(program, "children/2"), UiKind::Link { text: "Bad".into(), url: None });
}

#[test]
fn buttons_take_motiles_variants() {
    let program = r#"root = Row([Button("A", null, "solid", "danger", "sm"), Button("B", null, "outline", "success", "lg", "check"), Button("C", null, "soft"), Button("D", null, null, null, null, null, true), IconButton("x", null, null, "ghost")])"#;
    let tree = tree(program);
    let button = |id: &str| match &at(&tree, id).kind {
        UiKind::Button { variant, tint, size, disabled, .. } => (*variant, tint.clone(), *size, *disabled),
        other => panic!("{other:?}"),
    };
    assert_eq!(button("children/0"), (ButtonVariant::Danger, None, ControlSize::Small, None));
    assert_eq!(button("children/1"), (ButtonVariant::Outline, Some("success".into()), ControlSize::Large, None));
    assert_eq!(button("children/2"), (ButtonVariant::Secondary, None, ControlSize::Regular, None));
    assert_eq!(button("children/3").3, Some(Disabled::Program));
    let node = at(&tree, "children/4");
    let UiKind::IconButton { label, tooltip, variant, .. } = &node.kind else { panic!() };
    assert_eq!((label.as_str(), tooltip, *variant), ("x", &None, ButtonVariant::Ghost));
    assert_eq!(node.a11y.as_ref().and_then(|a11y| a11y.label.clone()), Some("x".into()));
}

#[test]
fn controls_take_no_presses_while_streaming_or_when_their_tools_cant_run() {
    let program = r#"done = Mutation("run", {command: "ls"})
root = Col([Button("Run", Action([@Run(done)])), Button("Ask"), Input("")])"#;
    let runtime = runtime(program);
    let root = runtime.root().unwrap();
    let disabled = |tree: &UiNode, id: &str| match &at(tree, id).kind {
        UiKind::Button { disabled, .. } | UiKind::Input { disabled, .. } => *disabled,
        other => panic!("{other:?}"),
    };
    let without_tools = resolve(root, &ResolveCtx { streaming: false, tools: false, state: &NoState });
    assert_eq!(disabled(&without_tools, "children/0"), Some(Disabled::NoTools));
    assert_eq!(disabled(&without_tools, "children/1"), None);
    let streaming = resolve(root, &ResolveCtx { streaming: true, tools: true, state: &NoState });
    assert_eq!(disabled(&streaming, "children/0"), Some(Disabled::Streaming));
    assert_eq!(disabled(&streaming, "children/2"), Some(Disabled::Streaming));
}

#[test]
fn choices_are_normalised_and_the_chosen_one_found() {
    let program = r#"$pace = "b"
root = Col([Select($pace, ["a", {value: "b", label: "Bee"}, null]), RadioGroup("x", [{value: "x", label: "Ex", description: "first"}], "Pick", "cards"), Select("", ["a"])])"#;
    let UiKind::Select { options, selected, placeholder, .. } = kind(program, "children/0") else { panic!() };
    assert_eq!(options.len(), 2);
    assert_eq!((selected, placeholder.as_str()), (Some(1), "Choose…"));
    let UiKind::RadioGroup { options, selected, variant, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!((options[0].description.as_deref(), selected, variant), (Some("first"), Some(0), RadioVariant::Cards));
    let UiKind::Select { selected, .. } = kind(program, "children/2") else { panic!() };
    assert_eq!(selected, None);
}

#[test]
fn dates_show_formatted_within_their_bounds() {
    let program = r#"root = Col([DatePicker("2026-10-24", "Start", "2026-01-01", "soon"), DatePicker("tomorrow")])"#;
    let UiKind::DatePicker { value, text, min, max, .. } = kind(program, "children/0") else { panic!() };
    assert_eq!((value.as_deref(), text.as_str()), (Some("2026-10-24"), "Oct 24, 2026"));
    assert_eq!((min.as_deref(), max), (Some("2026-01-01"), None));
    let UiKind::DatePicker { value, text, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!((value, text.as_str()), (None, "Date"));
}

struct Running;

impl UiState for Running {
    fn value(&self, _path: &[Step], _prop: &str) -> Option<Value> {
        None
    }

    fn error(&self, _path: &[Step]) -> Option<String> {
        Some("Required".into())
    }

    fn photos(&self, query: &str) -> Photos {
        Photos { state: PhotoState::Found, media: vec![format!("media:{query}")] }
    }

    fn route(&self, stops: &[[f64; 2]]) -> Route {
        if stops.len() > 2 { Route::Failed } else { Route::Street(vec![[1.0, 2.0], [1.5, 2.5], [3.0, 4.0]]) }
    }

    fn widgets(&self, _id: &str) -> Widgets {
        Widgets {
            open: true,
            flipped: true,
            running: true,
            started: Some(90.0),
            filter: Some("food".into()),
            burst: 2,
            failed: false,
        }
    }

    fn spans(&self, _language: &str, _code: &str) -> Option<crate::render::highlight::Spans> {
        None
    }
}

#[test]
fn what_core_keeps_shows_in_the_tree() {
    let program = r#"root = Col([Timer(75, "Tea"), Input("", "Name"), SearchImage("pastel de nata"), Map([MapMarker("A", 1, 2, "food", "🍕 pizza"), MapMarker("B", 3, 4, "food"), MapMarker("C", 5, 6, "sights"), MapMarker("A", 7, 8)], true), Flashcard("front", "back"), Celebration(true)])"#;
    let runtime = runtime(program);
    let tree = resolve(runtime.root().unwrap(), &ctx(&Running));
    let UiKind::Timer { time, running, toggle, reset, .. } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!((time.as_str(), *running, toggle.label.as_str(), reset.disabled), ("1:15", true, "Pause", None));
    let UiKind::Input { error, .. } = &at(&tree, "children/1").kind else { panic!() };
    assert_eq!(error.as_deref(), Some("Required"));
    let UiKind::SearchImage { photos, .. } = &at(&tree, "children/2").kind else { panic!() };
    assert_eq!(photos.media, ["media:pastel de nata"]);
    let UiKind::Map { pins, route, line, categories, filter, .. } = &at(&tree, "children/3").kind else { panic!() };
    let keys: Vec<(&str, Option<&str>, u32, bool)> =
        pins.iter().map(|pin| (pin.key.as_str(), pin.icon.as_deref(), pin.number, pin.visible)).collect();
    assert_eq!(
        keys,
        [("A", Some("🍕"), 1, true), ("B", None, 2, true), ("C", None, 3, false), ("A (2)", None, 4, false)]
    );
    assert_eq!(
        (categories.as_slice(), filter.as_deref()),
        (["food".to_owned(), "sights".to_owned()].as_slice(), Some("food"))
    );
    assert_eq!((*route, line.len()), (RouteKind::Street, 3));
    let UiKind::Flashcard { flipped, front, canvas, .. } = &at(&tree, "children/4").kind else { panic!() };
    assert_eq!((*flipped, front.style, canvas.kind.as_str()), (true, TextStyle::Word, "Flashcard"));
    let UiKind::Celebration { burst, canvas } = &at(&tree, "children/5").kind else { panic!() };
    assert_eq!((*burst, canvas.props.to_json()), (2, json!({"when": true})));
}

#[test]
fn canvas_components_carry_their_props_for_the_layout() {
    let program = r#"root = Col([Svg([Circle(5, 5, 4, null, "danger"), G([Rect(0, 0, 5, 5)], "rotate(30 50 50)")], "0 0 200 100"), Math("E = mc^2", true), Chart("bar", [{m: "Jan", v: 2}], "m", ["v"])])"#;
    let tree = tree(program);
    let UiKind::Svg { canvas } = &at(&tree, "children/0").kind else { panic!() };
    assert_eq!(canvas.kind, "Svg");
    assert_eq!(
        serde_json::to_value(canvas).unwrap(),
        json!({"kind": "Svg", "props": {"children": [
            {"kind": "Circle", "props": {"cx": 5, "cy": 5, "r": 4, "stroke": "danger"}},
            {"kind": "G", "props": {"children": [{"kind": "Rect", "props": {"x": 0, "y": 0, "width": 5, "height": 5}}], "transform": "rotate(30 50 50)"}},
        ], "viewBox": "0 0 200 100"}})
    );
    let math = at(&tree, "children/1");
    assert_eq!(math.a11y.as_ref().and_then(|a11y| a11y.label.as_deref()), Some("E = mc^2"));
    let chart = at(&tree, "children/2");
    let UiKind::Chart { canvas } = &chart.kind else { panic!() };
    let keys: Vec<&String> = canvas.props.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["type", "data", "x", "series"]);
    assert_eq!(chart.a11y.as_ref().and_then(|a11y| a11y.label.as_deref()), Some("bar chart of v by m"));
}

#[test]
fn media_props_are_checked() {
    let program = r#"root = Col([YouTube("https://youtu.be/dQw4w9WgXcQ?t=1"), YouTube("not a video"), Image("javascript:x", "ab", 9, "nope", "contain"), HtmlView("<p>", 5000)])"#;
    assert_eq!(
        kind(program, "children/0"),
        UiKind::YouTube {
            url: Some("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ".into()),
            title: "YouTube video".into()
        }
    );
    let UiKind::YouTube { url, .. } = kind(program, "children/1") else { panic!() };
    assert_eq!(url, None);
    let UiKind::Image { src, aspect_ratio, radius, fit, photos, .. } = kind(program, "children/2") else { panic!() };
    assert_eq!(
        (src, aspect_ratio, radius.points, fit, photos.state),
        (None, 1.5, 0.0, Fit::Contain, PhotoState::Missing)
    );
    let UiKind::HtmlView { height, title, .. } = kind(program, "children/3") else { panic!() };
    assert_eq!((height, title.as_str()), (Some(2400.0), "Interactive view"));
}

#[test]
fn unchanged_components_are_taken_from_the_last_pass() {
    let mut runtime = runtime(
        r#"$n = 1
root = Col([Card([Title("Fixed")]), Text("" + $n), Input($n)])"#,
    );
    let mut resolver = Resolver::default();
    let first = resolver.resolve(runtime.root().unwrap(), &ctx(&NoState));
    runtime.input(&[Step::Key("children".into()), Step::Index(2)], "value", Value::Number(2.0));
    let second = resolver.resolve(runtime.root().unwrap(), &ctx(&NoState));
    let child = |tree: &UiNode, index: usize| match &tree.kind {
        UiKind::Col { children, .. } => children[index].clone(),
        other => panic!("{other:?}"),
    };
    assert!(Arc::ptr_eq(&child(&first, 0), &child(&second, 0)), "the card didn't change");
    assert!(!Arc::ptr_eq(&child(&first, 1), &child(&second, 1)));
    let UiKind::Text { text, .. } = &child(&second, 1).kind else { panic!() };
    assert_eq!(text.text, "2");
    let flagged =
        resolver.resolve(runtime.root().unwrap(), &ResolveCtx { streaming: true, tools: true, state: &NoState });
    assert!(!Arc::ptr_eq(&child(&second, 0), &child(&flagged, 0)), "streaming changes every control");
}

#[test]
fn every_component_resolves_to_its_own_kind() {
    let library = motile_openui::motile::library();
    let program = r#"root = Col([Col([]), Row([]), Box(), Grid([]), Card([]), Carousel([]), Popover("a", "b"), Divider(), Spacer(), Title("t"), Text("t"), Caption("c"), Label("l"), Badge("b"), Link("l", "https://a.b"), List([]), ListItem("i"), Markdown("m"), Quote("q"), Code("c"), Math("x"), Button("b"), IconButton("x"), Pressable([]), Input(""), TextArea(""), Select("", []), SegmentedControl("", []), Slider(1, 0, 2), Checkbox(false, "c"), RadioGroup("", []), DatePicker(""), Timer(1), Form([]), SearchImage("q"), Image("https://a.b/c.png", "a"), Icon("x"), Table([], []), Chart("bar", []), Map([]), MapMarker("m", 1, 2), YouTube("v"), Flashcard("a", "b"), Celebration(false), Svg([], "0 0 1 1"), G([]), Circle(1, 1, 1), Ellipse(1, 1, 1, 1), Rect(1, 1, 1, 1), Line(1, 1, 1, 1), Path("M0 0"), Polyline("0,0"), Polygon("0,0"), SvgText("t", 1, 1), HtmlView("<p>")])"#;
    let tree = tree(program);
    let names: Vec<String> = children(&tree)
        .iter()
        .map(|node| serde_json::to_value(node).unwrap()["component"].as_str().unwrap().to_owned())
        .collect();
    let expected: Vec<String> = library.components().map(|component| component.name.clone()).collect();
    assert_eq!(names.len(), 55);
    let mut sorted_names = names.clone();
    let mut sorted_expected = expected;
    sorted_names.sort();
    sorted_expected.sort();
    assert_eq!(sorted_names, sorted_expected);
}
