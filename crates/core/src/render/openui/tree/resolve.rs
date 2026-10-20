//! An evaluated program to the tree the clients draw. A component whose element the runtime kept
//! (the same `Arc`) and that read nothing but its props is taken from the last pass as it was.

use std::collections::HashMap;
use std::sync::Arc;

use motile_openui::{Element, Step, Value, js};

use super::color::{self, Ink};
use super::media::{canvas, map_marker, youtube};
use super::text::{self, Builder, collapse_lines, trim_block};
use super::*;
use crate::render::highlight::Spans;
use crate::render::markdown::{self, Block, ParaKind};
use crate::render::openui::node_id;

/// What resolving reads besides the program's tree.
pub struct ResolveCtx<'a> {
    /// The program is still streaming: its controls take no input yet.
    pub streaming: bool,
    /// The thread's server runs the program's tools.
    pub tools: bool,
    pub state: &'a dyn UiState,
}

/// What the host keeps beside the tree.
pub trait UiState {
    /// What a control shows when it isn't its prop, as `Runtime::value_at` says.
    fn value(&self, path: &[Step], prop: &str) -> Option<Value>;
    /// A field's validation error, as `Runtime::error_at` says.
    fn error(&self, path: &[Step]) -> Option<String>;
    /// The photos found for an image's query.
    fn photos(&self, query: &str) -> Photos;
    /// The street route through stops, `[lat, lng]`.
    fn route(&self, stops: &[[f64; 2]]) -> Route;
    /// The state core keeps for a node: a timer, a popover, a flip, the map's filter.
    fn widgets(&self, id: &str) -> Widgets;
    /// The highlighting of code, once it has been worked out.
    fn spans(&self, language: &str, code: &str) -> Option<Spans>;
}

pub enum Route {
    Loading,
    Failed,
    Street(Vec<[f64; 2]>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widgets {
    pub open: bool,
    pub flipped: bool,
    pub running: bool,
    /// The seconds a timer started from, until it is reset.
    pub started: Option<f64>,
    pub filter: Option<String>,
    pub burst: u32,
    /// An image whose address couldn't be loaded: a photo is found for its `alt`.
    pub failed: bool,
}

/// Nothing beside the tree: controls show their props and every lookup waits.
pub struct NoState;

impl UiState for NoState {
    fn value(&self, _path: &[Step], _prop: &str) -> Option<Value> {
        None
    }

    fn error(&self, _path: &[Step]) -> Option<String> {
        None
    }

    fn photos(&self, _query: &str) -> Photos {
        Photos { state: PhotoState::Waiting, media: Vec::new() }
    }

    fn route(&self, _stops: &[[f64; 2]]) -> Route {
        Route::Loading
    }

    fn widgets(&self, _id: &str) -> Widgets {
        Widgets::default()
    }

    fn spans(&self, _language: &str, _code: &str) -> Option<Spans> {
        None
    }
}

pub fn resolve(root: &Element, ctx: &ResolveCtx) -> UiNode {
    Resolver::default().resolve(root, ctx)
}

/// Resolves a program again and again, reusing what didn't change.
#[derive(Default)]
pub struct Resolver {
    cache: HashMap<Key, Entry>,
    flags: Option<(bool, bool)>,
}

type Key = (usize, String, Cx);

struct Entry {
    /// Holds the element, so that no other takes its address.
    _element: Arc<Element>,
    node: Arc<UiNode>,
}

impl Resolver {
    pub fn resolve(&mut self, root: &Element, ctx: &ResolveCtx) -> UiNode {
        if self.flags != Some((ctx.streaming, ctx.tools)) {
            self.cache.clear();
            self.flags = Some((ctx.streaming, ctx.tools));
        }
        let mut pass = Pass {
            ctx,
            previous: std::mem::take(&mut self.cache),
            current: HashMap::new(),
            path: Vec::new(),
            impure: false,
        };
        let node = pass.build(root, Cx::default());
        self.cache = pass.current;
        node
    }
}

/// What a node's place decides about it, besides its props.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(super) struct Cx {
    /// A column that spaces its children as a flow is around it.
    pub flow: bool,
    /// A Card is around it.
    pub card: bool,
    /// It takes a card's typography: a direct child of a Card, or of a flow that is a grid cell or
    /// a carousel item.
    pub typography: bool,
    /// It is a grid cell or a carousel item.
    pub cell: bool,
    /// The first Row of a Card is around it, whose small titles are the card's.
    pub title_row: bool,
    /// The image of a media row, or the Box holding only one.
    pub media: bool,
    /// What a raw background around it makes its text.
    pub ink: Option<Ink>,
    pub part: Part,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(super) enum Part {
    #[default]
    Plain,
    CardTitle,
    CardSubtitle,
}

pub(super) struct Pass<'a, 'c> {
    pub ctx: &'a ResolveCtx<'c>,
    previous: HashMap<Key, Entry>,
    current: HashMap<Key, Entry>,
    pub path: Vec<Step>,
    /// The node being built read something beside its props.
    impure: bool,
}

/// A slot's values after `@Each`'s lists are flattened, each with its steps from the prop.
pub(super) type Items<'v> = Vec<(Vec<Step>, &'v Value)>;

impl Pass<'_, '_> {
    pub fn id(&self) -> String {
        node_id(&self.path)
    }

    /// The value a control shows, from the host when it keeps one.
    pub fn value<'e>(&mut self, element: &'e Element, prop: &str) -> Option<std::borrow::Cow<'e, Value>> {
        self.impure = true;
        if let Some(value) = self.ctx.state.value(&self.path, prop) {
            return Some(std::borrow::Cow::Owned(value));
        }
        element.prop(prop).map(std::borrow::Cow::Borrowed)
    }

    pub fn error(&mut self) -> Option<String> {
        self.impure = true;
        self.ctx.state.error(&self.path)
    }

    pub fn photos(&mut self, query: &str) -> Photos {
        self.impure = true;
        self.ctx.state.photos(query)
    }

    pub fn route(&mut self, stops: &[[f64; 2]]) -> Route {
        self.impure = true;
        self.ctx.state.route(stops)
    }

    pub fn widgets(&mut self) -> Widgets {
        self.impure = true;
        self.ctx.state.widgets(&self.id())
    }

    /// The highlighting of code; a node whose code waits for it is built again.
    pub fn spans(&mut self, language: &str, code: &str) -> Option<Spans> {
        let spans = self.ctx.state.spans(language, code);
        self.impure |= spans.is_none();
        spans
    }

    fn markdown(&mut self, element: &Element) -> UiKind {
        let mut blocks = Vec::new();
        for block in markdown::parse(&string(element, "text")) {
            match block {
                Block::Prose(mut prose) => {
                    for para in &mut prose.paras {
                        let ParaKind::Pre { language, code, spans, .. } = &mut para.kind else { continue };
                        *spans = self.spans(language, code);
                    }
                    blocks.push(MarkdownBlock::Prose { prose });
                }
                Block::Code { language, code } => {
                    let spans = self.spans(&language, &code);
                    blocks.push(MarkdownBlock::Code { language, code, spans });
                }
                Block::Ui { code, .. } => {
                    let language = "openui-lang".to_owned();
                    let spans = self.spans(&language, &code);
                    blocks.push(MarkdownBlock::Code { language, code, spans });
                }
                Block::Image { .. } => {}
            }
        }
        UiKind::Markdown { blocks }
    }

    fn code(&mut self, element: &Element) -> UiKind {
        let language = string(element, "language");
        let language = if language.is_empty() { "code".to_owned() } else { language };
        let code = string(element, "code");
        UiKind::Code { spans: self.spans(&language, &code), language, code }
    }

    /// A component, from the last pass when nothing it reads changed.
    pub fn node(&mut self, element: &Arc<Element>, cx: Cx) -> Arc<UiNode> {
        let key = (Arc::as_ptr(element) as usize, self.id(), cx);
        if let Some(entry) = self.previous.remove(&key) {
            let node = entry.node.clone();
            self.current.insert(key, entry);
            return node;
        }
        let outer = std::mem::replace(&mut self.impure, false);
        let node = Arc::new(self.build(element, cx));
        if !self.impure {
            self.current.insert(key, Entry { _element: element.clone(), node: node.clone() });
        }
        self.impure |= outer;
        node
    }

    /// The value at a slot's item: a component, or text where one stands in a slot.
    pub fn item(&mut self, prop: &str, steps: &[Step], value: &Value, cx: Cx) -> Option<Arc<UiNode>> {
        self.path.push(Step::Key(prop.to_owned()));
        self.path.extend(steps.iter().cloned());
        let node = match value {
            Value::Element(element) => Some(self.node(element, cx)),
            Value::String(_) | Value::Number(_) | Value::Bool(_) => Some(Arc::new(self.leaf(UiKind::Text {
                text: text::inline(&js::to_string(value)),
                inlines: Vec::new(),
                style: body_style(cx),
                color: color::ink(cx.ink),
                weight: Weight::Regular,
                italic: false,
                strike: false,
                digits: false,
                max_lines: None,
            }))),
            _ => None,
        };
        self.path.truncate(self.path.len() - steps.len() - 1);
        node
    }

    pub fn leaf(&self, kind: UiKind) -> UiNode {
        let a11y = a11y_of(&kind);
        UiNode { id: self.id(), a11y, kind }
    }

    pub fn build(&mut self, element: &Element, cx: Cx) -> UiNode {
        let kind = match element.component.as_str() {
            "Col" => self.col(element, cx),
            "Row" => self.row(element, cx),
            "Box" => self.box_(element, cx),
            "Grid" => self.grid(element, cx),
            "Card" => self.card(element, cx),
            "Carousel" => self.carousel(element, cx),
            "Popover" => self.popover(element, cx),
            "Divider" => UiKind::Divider {},
            "Spacer" => UiKind::Spacer {},
            "Title" => title(element, cx),
            "Text" => self.text(element, cx),
            "Caption" => caption(element, cx),
            "Label" => UiKind::Label { text: string(element, "text"), color: color::ink(cx.ink) },
            "Badge" => badge(element),
            "Link" => link(element),
            "List" => self.list(element, cx),
            "ListItem" => self.list_item(element, cx, None),
            "Markdown" => self.markdown(element),
            "Quote" => quote(element),
            "Code" => self.code(element),
            "Math" => UiKind::Math { canvas: canvas(element) },
            "Button" => self.button(element),
            "IconButton" => self.icon_button(element),
            "Pressable" => self.pressable(element, cx),
            "Input" => self.input(element),
            "TextArea" => self.text_area(element),
            "Select" => self.select(element),
            "SegmentedControl" => self.segmented(element),
            "Slider" => self.slider(element),
            "Checkbox" => self.checkbox(element),
            "RadioGroup" => self.radio_group(element),
            "DatePicker" => self.date_picker(element),
            "Timer" => self.timer(element),
            "Form" => self.form(element, cx),
            "SearchImage" => self.search_image(element, cx),
            "Image" => self.image(element, cx),
            "Icon" => icon(element, cx),
            "Table" => self.table(element, cx),
            "Chart" => UiKind::Chart { canvas: canvas(element) },
            "Map" => self.map(element),
            "MapMarker" => map_marker(element),
            "YouTube" => youtube(element),
            "Flashcard" => self.flashcard(element, cx),
            "Celebration" => self.celebration(element),
            "Svg" => UiKind::Svg { canvas: canvas(element) },
            "G" => UiKind::G { canvas: canvas(element) },
            "Circle" => UiKind::Circle { canvas: canvas(element) },
            "Ellipse" => UiKind::Ellipse { canvas: canvas(element) },
            "Rect" => UiKind::Rect { canvas: canvas(element) },
            "Line" => UiKind::Line { canvas: canvas(element) },
            "Path" => UiKind::Path { canvas: canvas(element) },
            "Polyline" => UiKind::Polyline { canvas: canvas(element) },
            "Polygon" => UiKind::Polygon { canvas: canvas(element) },
            "SvgText" => UiKind::SvgText { canvas: canvas(element) },
            "HtmlView" => self.html_view(element),
            _ => UiKind::Spacer {},
        };
        let mut node = self.leaf(kind);
        if let Some(a11y) = &mut node.a11y
            && a11y.role == Role::Button
            && a11y.label.is_none()
        {
            a11y.label = Some(spoken(&node.kind)).filter(|label| !label.is_empty());
        }
        node
    }

    /// A column's children with what their places decide, and the spacing between them.
    fn column(&mut self, element: &Element, prop: &str, gap: Option<f32>, kind: Column, cx: Cx) -> (Nodes, Vec<f32>) {
        let items = items(element.prop(prop));
        let flow = gap.is_none() && kind != Column::Form;
        let inner = Cx {
            flow: cx.flow || flow,
            card: cx.card || kind == Column::Card,
            typography: kind == Column::Card || (flow && cx.cell),
            cell: false,
            title_row: cx.title_row,
            media: cx.media && kind == Column::Plain,
            ink: cx.ink,
            part: Part::Plain,
        };
        let parts = parts(&items, kind == Column::Card, cx.title_row);
        let nodes = self.items(prop, &items, |index| Cx {
            part: parts[index].0,
            title_row: inner.title_row || parts[index].1,
            ..inner
        });
        let spacing = match kind {
            Column::Form => form_spacing(&nodes),
            _ => spacing(&nodes, gap, flow && (cx.flow || kind == Column::Card), kind == Column::Card),
        };
        (nodes, spacing)
    }

    /// Resolves a slot's items, each in the place `cx` gives it.
    pub fn items(&mut self, prop: &str, items: &Items, cx: impl Fn(usize) -> Cx) -> Nodes {
        items
            .iter()
            .enumerate()
            .filter_map(|(index, (steps, value))| self.item(prop, steps, value, cx(index)))
            .collect()
    }

    fn col(&mut self, element: &Element, cx: Cx) -> UiKind {
        let gap = steps(element.prop("gap"));
        let (children, spacing) = self.column(element, "children", gap, Column::Plain, cx);
        UiKind::Col { children, align: align(element, Align::Stretch), spacing }
    }

    fn row(&mut self, element: &Element, cx: Cx) -> UiKind {
        let (children, sizing, media) = self.row_children(element, cx);
        let gap = if media { 16.0 } else { steps(element.prop("gap")).unwrap_or(8.0) };
        let align = if media { Align::Start } else { align(element, Align::Center) };
        UiKind::Row { children, gap, align, justify: justify(element), wrap: flag(element, "wrap"), sizing }
    }

    /// A row's children, how each is sized, and whether it is a media row.
    fn row_children(&mut self, element: &Element, cx: Cx) -> (Nodes, Vec<Sizing>, bool) {
        let items = items(element.prop("children"));
        let media: Vec<bool> = items.iter().map(|(_, value)| is_media(value)).collect();
        let is_media_row = media.contains(&true);
        let parts = parts(&items, false, cx.title_row);
        let inner = Cx { cell: false, typography: false, media: false, part: Part::Plain, ..cx };
        let mut sizing = Vec::new();
        let mut children = Vec::new();
        for (index, (steps, value)) in items.iter().enumerate() {
            let cx = Cx { media: media[index], part: parts[index].0, ..inner };
            let Some(node) = self.item("children", steps, value, cx) else { continue };
            sizing.push(if media[index] { MEDIA_SIZING } else { sizing_of(value) });
            children.push(node);
        }
        (children, sizing, is_media_row)
    }

    fn box_(&mut self, element: &Element, cx: Cx) -> UiKind {
        let row = string(element, "direction") == "row";
        let background_value = str_of(element, "background");
        let ink = background_value.and_then(color::ink_on).or(cx.ink);
        let background = background_value.and_then(|value| color::background(value, cx.ink));
        let inner_surface = background.as_ref().is_some_and(color::is_theme);
        let inner = Cx { ink, ..cx };
        let gap = steps(element.prop("gap"));
        let (children, spacing, sizing, media_row) = if row {
            let (children, sizing, media) = self.row_children(element, inner);
            (children, Vec::new(), sizing, media)
        } else {
            let (children, spacing) = self.column(element, "children", gap, Column::Plain, inner);
            (children, spacing, Vec::new(), false)
        };
        let default_align = if row { Align::Center } else { Align::Stretch };
        UiKind::Box {
            children,
            direction: if row { Direction::Row } else { Direction::Col },
            gap: if media_row { 16.0 } else { gap.unwrap_or(0.0) },
            spacing,
            sizing,
            padding: steps(element.prop("padding")).unwrap_or(0.0),
            background,
            inner_surface,
            radius: element
                .prop("radius")
                .map(|value| radius(value).unwrap_or(Radius { kind: RadiusKind::Points, points: 0.0 })),
            border: flag(element, "border"),
            width: element.prop("width").and_then(length).filter(|_| !cx.media),
            height: element.prop("height").and_then(length).filter(|_| !cx.media),
            flex: element
                .prop("flex")
                .and_then(Value::as_number)
                .filter(|flex| flex.is_finite())
                .map(|flex| flex.max(0.0) as f32),
            align: if media_row { Align::Start } else { align(element, default_align) },
            justify: justify(element),
            wrap: flag(element, "wrap"),
        }
    }

    fn grid(&mut self, element: &Element, cx: Cx) -> UiKind {
        let items = items(element.prop("children"));
        let inner = Cx { cell: true, typography: false, media: false, part: Part::Plain, ..cx };
        let children = self.items("children", &items, |_| inner);
        let columns = number(element, "columns").map_or(2.0, js::round).clamp(1.0, 12.0) as u32;
        let min_child_width =
            number(element, "minChildWidth").filter(|width| *width != 0.0).map(|width| width.max(0.0) as f32);
        UiKind::Grid { children, columns, min_child_width, gap: steps(element.prop("gap")).unwrap_or(12.0) }
    }

    fn card(&mut self, element: &Element, cx: Cx) -> UiKind {
        let gap = steps(element.prop("gap"));
        let (children, spacing) = self.column(element, "children", gap, Column::Card, cx);
        let padding = steps(element.prop("padding")).or(cx.card.then_some(16.0));
        UiKind::Card { children, soft: string(element, "variant") == "soft", nested: cx.card, padding, spacing }
    }

    fn carousel(&mut self, element: &Element, cx: Cx) -> UiKind {
        let items = items(element.prop("children"));
        let fill = items.iter().map(|(_, value)| matches!(component(value), Some("Card" | "Pressable"))).collect();
        let inner = Cx { cell: true, typography: false, media: false, part: Part::Plain, ..cx };
        let children = self.items("children", &items, |_| inner);
        let item_width = number(element, "itemWidth").unwrap_or(240.0).clamp(80.0, 640.0) as f32;
        let gap = number(element, "gap").map_or(12.0, |gap| gap.max(0.0) as f32 * 4.0);
        UiKind::Carousel { children, item_width, gap, fill }
    }

    pub fn slot(&mut self, element: &Element, prop: &str, style: TextStyle, cx: Cx) -> Slot {
        match element.prop(prop) {
            Some(value @ (Value::String(_) | Value::Number(_))) => {
                Slot { text: Some(text::inline(&js::to_string(value))), style, children: Vec::new() }
            }
            value => {
                let items = items(value);
                let inner = Cx { cell: false, typography: false, media: false, part: Part::Plain, ..cx };
                Slot { text: None, style, children: self.items(prop, &items, |_| inner) }
            }
        }
    }

    fn popover(&mut self, element: &Element, cx: Cx) -> UiKind {
        let trigger = self.slot(element, "trigger", body_style(cx), cx);
        let content = self.slot(element, "content", TextStyle::Body, Cx { ink: None, ..cx });
        UiKind::Popover { trigger, content, open: self.widgets().open }
    }

    fn text(&mut self, element: &Element, cx: Cx) -> UiKind {
        let style = match string(element, "size").as_str() {
            _ if cx.part == Part::CardSubtitle => TextStyle::CardSubtitle,
            "xs" => TextStyle::Caption,
            "sm" => TextStyle::Small,
            "lg" => TextStyle::Large,
            _ => body_style(cx),
        };
        let fallback = if cx.part == Part::CardSubtitle { color::muted(cx.ink) } else { color::ink(cx.ink) };
        let color = str_of(element, "color").and_then(|value| color::foreground(value, cx.ink)).unwrap_or(fallback);
        let mut builder = Builder::default();
        let mut inlines = Vec::new();
        match element.prop("text") {
            Some(Value::Array(_)) => {
                let items = items(element.prop("text"));
                let last = items.len().saturating_sub(1);
                let inner = Cx { cell: false, typography: false, media: false, part: Part::Plain, ..cx };
                for (index, (steps, value)) in items.iter().enumerate() {
                    if let Value::Element(_) = value {
                        if let Some(node) = self.item("text", steps, value, inner) {
                            builder.object();
                            inlines.push(node);
                        }
                        continue;
                    }
                    let mut part = collapse_lines(&js::to_string(value));
                    if index == 0 {
                        part = part.trim_start_matches(' ').to_owned();
                    }
                    if index == last {
                        part = part.trim_end_matches([' ', '\u{2028}']).to_owned();
                    }
                    builder.inline(&part, 0);
                }
            }
            Some(value) => builder.inline(trim_block(&collapse_lines(&js::to_string(value))), 0),
            None => {}
        }
        let weight = match string(element, "weight").as_str() {
            "medium" => Weight::Medium,
            "semibold" => Weight::Semibold,
            _ => Weight::Regular,
        };
        let max_lines = number(element, "maxLines").filter(|lines| *lines > 0.0).map(|lines| js::round(lines) as u32);
        UiKind::Text {
            text: builder.finish(),
            inlines,
            style,
            color,
            weight,
            italic: flag(element, "italic"),
            strike: flag(element, "lineThrough"),
            digits: flag(element, "tabularNums"),
            max_lines,
        }
    }

    fn list(&mut self, element: &Element, cx: Cx) -> UiKind {
        let items = items(element.prop("items"));
        let marker = match string(element, "marker").as_str() {
            "number" => ListMarker::Number,
            "none" => ListMarker::Plain,
            _ => ListMarker::Bullet,
        };
        let wide = items.iter().any(|(_, value)| {
            value
                .as_element()
                .filter(|item| item.component == "ListItem")
                .and_then(|item| str_of(item, "marker"))
                .is_some_and(|marker| js::utf16_len(marker) > 2)
        });
        let inner = Cx { cell: false, typography: false, media: false, part: Part::Plain, ..cx };
        let mut nodes = Vec::new();
        for (index, (steps, value)) in items.iter().enumerate() {
            let number = steps.first().map_or(index, |step| match step {
                Step::Index(index) => *index,
                Step::Key(_) => index,
            }) as u32
                + 1;
            let default = match marker {
                ListMarker::Number => ItemMarker::Number { number },
                ListMarker::Bullet => ItemMarker::Dot,
                ListMarker::Plain => ItemMarker::Blank,
            };
            self.path.push(Step::Key("items".to_owned()));
            self.path.extend(steps.iter().cloned());
            let node = match value {
                Value::Element(item) if item.component == "ListItem" => {
                    let kind = self.list_item(item, inner, Some(default));
                    Some(Arc::new(self.leaf(kind)))
                }
                Value::Element(item) => Some(self.node(item, inner)),
                value => {
                    let title = text::inline(&js::to_string(value));
                    let kind = UiKind::ListItem {
                        title,
                        style: body_style(inner),
                        medium: false,
                        description: None,
                        marker: default,
                        children: Vec::new(),
                    };
                    Some(Arc::new(self.leaf(kind)))
                }
            };
            self.path.truncate(self.path.len() - steps.len() - 1);
            nodes.extend(node);
        }
        UiKind::List { items: nodes, marker, timeline: flag(element, "timeline"), wide }
    }

    /// A list's item; on its own, without a marker.
    fn list_item(&mut self, element: &Element, cx: Cx, default: Option<ItemMarker>) -> UiKind {
        let marker = match str_of(element, "marker").filter(|marker| !marker.is_empty()) {
            Some(marker) if marker.len() <= 2 && marker.chars().all(|c| c.is_ascii_alphanumeric()) => {
                ItemMarker::Chip { text: marker.to_owned() }
            }
            Some(marker) => ItemMarker::Label { text: marker.to_owned() },
            None => default.unwrap_or(ItemMarker::Blank),
        };
        let description = str_of(element, "description").filter(|text| !text.is_empty()).map(text::inline);
        let items = items(element.prop("children"));
        let inner = Cx { cell: false, typography: false, media: false, part: Part::Plain, ..cx };
        let children = self.items("children", &items, |_| inner);
        UiKind::ListItem {
            title: text::inline(&string(element, "title")),
            style: body_style(cx),
            medium: description.is_some() || element.prop("children").is_some_and(js::truthy),
            description,
            marker,
            children,
        }
    }

    fn form(&mut self, element: &Element, cx: Cx) -> UiKind {
        let (children, spacing) = self.column(element, "children", Some(24.0), Column::Form, cx);
        UiKind::Form { children, spacing }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Plain,
    Card,
    Form,
}

/// The values of a slot, its lists flattened.
pub(super) fn items(value: Option<&Value>) -> Items<'_> {
    fn walk<'v>(value: &'v Value, steps: &mut Vec<Step>, out: &mut Items<'v>) {
        match value {
            Value::Null => {}
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    steps.push(Step::Index(index));
                    walk(value, steps, out);
                    steps.pop();
                }
            }
            value => out.push((steps.clone(), value)),
        }
    }
    let mut out = Vec::new();
    if let Some(value) = value {
        walk(value, &mut Vec::new(), &mut out);
    }
    out
}

pub(super) fn component(value: &Value) -> Option<&str> {
    value.as_element().map(|element| element.component.as_str())
}

/// The card's title and subtitle among a column's children, and the Row whose titles are the
/// card's.
fn parts(items: &Items, card: bool, title_row: bool) -> Vec<(Part, bool)> {
    let small_title = |value: &Value| {
        value.as_element().is_some_and(|element| {
            element.component == "Title" && matches!(str_of(element, "size").unwrap_or("md"), "sm" | "md")
        })
    };
    let mut parts = vec![(Part::Plain, false); items.len()];
    for (index, (_, value)) in items.iter().enumerate() {
        let first = index == 0;
        if small_title(value) && (title_row || (card && first)) {
            parts[index].0 = Part::CardTitle;
        } else if matches!(component(value), Some("Text" | "Caption"))
            && index > 0
            && parts[index - 1].0 == Part::CardTitle
        {
            parts[index].0 = Part::CardSubtitle;
        } else if card && first && component(value) == Some("Row") {
            parts[index].1 = true;
        }
    }
    parts
}

/// The space above each child of a column (`components.css`'s flow rules).
fn spacing(nodes: &[Arc<UiNode>], gap: Option<f32>, nested: bool, card: bool) -> Vec<f32> {
    let mut spacing = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        let divider = matches!(node.kind, UiKind::Divider {});
        let after_divider = index > 0 && matches!(nodes[index - 1].kind, UiKind::Divider {});
        let mut space = match (index, gap) {
            (0, _) => 0.0,
            (_, Some(gap)) => gap,
            (_, None) => flow_space(&nodes[index - 1].kind, &node.kind, nested),
        };
        if divider && (index == 0 || gap.is_some()) {
            space += 4.0;
        }
        if after_divider {
            space += 4.0;
        }
        if gap.is_some() && card && is_subtitle(&node.kind) && index == 1 {
            space += 2.0;
        }
        spacing.push(space);
    }
    spacing
}

fn is_subtitle(kind: &UiKind) -> bool {
    matches!(
        kind,
        UiKind::Text { style: TextStyle::CardSubtitle, .. } | UiKind::Caption { style: TextStyle::CardSubtitle, .. }
    )
}

fn flow_space(before: &UiKind, kind: &UiKind, nested: bool) -> f32 {
    let title = |kind: &UiKind| matches!(kind, UiKind::Title { .. });
    let text = |kind: &UiKind| matches!(kind, UiKind::Text { .. });
    let badge_row = matches!(kind, UiKind::Row { children, .. } if children.iter().any(|child| matches!(child.kind, UiKind::Badge { .. })));
    if title(before) && (text(kind) || matches!(kind, UiKind::Caption { .. }) || badge_row) {
        8.0
    } else if title(kind) && matches!(before, UiKind::Caption { .. }) {
        4.0
    } else if text(before) && text(kind) || nested || title(before) {
        12.0
    } else {
        24.0
    }
}

/// A Form's fields 24 apart, its closing button 40 below them.
fn form_spacing(nodes: &[Arc<UiNode>]) -> Vec<f32> {
    let mut spacing = spacing(nodes, Some(24.0), false, false);
    let closing = nodes.last().is_some_and(|node| match &node.kind {
        UiKind::Button { .. } => true,
        UiKind::Row { children, .. } => children.iter().any(|child| matches!(child.kind, UiKind::Button { .. })),
        _ => false,
    });
    if closing && nodes.len() > 1 {
        let last = spacing.len() - 1;
        spacing[last] += 16.0;
    }
    spacing
}

/// An image, or a Box holding only one: what a media row is made around.
fn is_media(value: &Value) -> bool {
    let Some(element) = value.as_element() else { return false };
    match element.component.as_str() {
        "Image" | "SearchImage" => true,
        "Box" => {
            let children = items(element.prop("children"));
            children.len() == 1 && matches!(component(children[0].1), Some("Image" | "SearchImage"))
        }
        _ => false,
    }
}

const MEDIA_SIZING: Sizing = Sizing { basis: Basis::Media, length: None, grow: 0.0, shrink: false };

/// How a child takes its row's width (`LAYOUT.md`'s Row table).
fn sizing_of(value: &Value) -> Sizing {
    let content = Sizing { basis: Basis::Content, length: None, grow: 0.0, shrink: true };
    let Some(element) = value.as_element() else { return content };
    match element.component.as_str() {
        "Spacer" => Sizing { basis: Basis::Zero, length: None, grow: 1.0, shrink: false },
        "Button" | "IconButton" | "Badge" | "Icon" => Sizing { shrink: false, ..content },
        "Card" | "Input" | "TextArea" | "Select" | "DatePicker" => Sizing { basis: Basis::Row, ..content },
        "Box" => {
            if let Some(flex) = number(element, "flex") {
                return Sizing { basis: Basis::Zero, length: None, grow: flex.max(0.0) as f32, shrink: false };
            }
            let width = element.prop("width");
            match width.and_then(length) {
                Some(length) => {
                    let fixed = matches!(width, Some(Value::Number(_)));
                    Sizing { basis: Basis::Length, length: Some(length), grow: 0.0, shrink: !fixed }
                }
                None => content,
            }
        }
        _ => content,
    }
}

pub(super) fn body_style(cx: Cx) -> TextStyle {
    if cx.card || cx.typography { TextStyle::BodyCard } else { TextStyle::Body }
}

fn title(element: &Element, cx: Cx) -> UiKind {
    let size = str_of(element, "size").unwrap_or("md");
    let style = match size {
        "sm" | "md" if cx.part == Part::CardTitle => TextStyle::CardTitle,
        "lg" | "xl" | "2xl" | "3xl" if cx.typography => TextStyle::CardHeading,
        "sm" => TextStyle::TitleSm,
        "lg" => TextStyle::TitleLg,
        "xl" => TextStyle::TitleXl,
        "2xl" => TextStyle::Title2xl,
        "3xl" => TextStyle::Title3xl,
        _ => TextStyle::TitleMd,
    };
    let color =
        str_of(element, "color").and_then(|value| color::foreground(value, cx.ink)).unwrap_or(color::ink(cx.ink));
    UiKind::Title { text: text::inline(&string(element, "text")), style, color }
}

fn caption(element: &Element, cx: Cx) -> UiKind {
    let style = if cx.part == Part::CardSubtitle { TextStyle::CardSubtitle } else { TextStyle::Caption };
    let color =
        str_of(element, "color").and_then(|value| color::foreground(value, cx.ink)).unwrap_or(color::muted(cx.ink));
    UiKind::Caption { text: text::inline(&string(element, "text")), style, color }
}

fn badge(element: &Element) -> UiKind {
    let tone = match string(element, "color").as_str() {
        "success" => Some("success"),
        "warning" => Some("warning"),
        "danger" => Some("destructive"),
        "purple" => Some("merged"),
        "pink" => Some("pink"),
        _ => None,
    };
    UiKind::Badge {
        text: string(element, "text"),
        tone: tone.map(str::to_owned),
        icon: str_of(element, "icon").filter(|name| !name.is_empty()).and_then(glyph),
    }
}

fn link(element: &Element) -> UiKind {
    let url = str_of(element, "url").and_then(text::web_url);
    let text = string(element, "text");
    let text = match &url {
        Some(url) if text.is_empty() => {
            url::Url::parse(url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default()
        }
        _ => text,
    };
    UiKind::Link { text, url }
}

fn quote(element: &Element) -> UiKind {
    let cite = str_of(element, "cite").filter(|cite| !cite.is_empty()).map(text::inline);
    UiKind::Quote { text: text::inline(&string(element, "text")), cite }
}

fn icon(element: &Element, cx: Cx) -> UiKind {
    let size = match string(element, "size").as_str() {
        "sm" => 14.0,
        "lg" => 24.0,
        "xl" => 32.0,
        _ => 18.0,
    };
    let color =
        str_of(element, "color").and_then(|value| color::foreground(value, cx.ink)).unwrap_or(color::ink(cx.ink));
    UiKind::Icon { icon: glyph(&string(element, "name")), size, color }
}

/// What a node is called aloud, from the text in it (`textOf`), cut to 120 characters.
pub(super) fn spoken(kind: &UiKind) -> String {
    fn walk(kind: &UiKind, out: &mut String) {
        let mut push = |text: &str| {
            if !text.is_empty() {
                out.push(' ');
                out.push_str(text);
            }
        };
        match kind {
            UiKind::Title { text, .. } | UiKind::Caption { text, .. } | UiKind::Quote { text, .. } => push(&text.text),
            UiKind::Text { text, inlines, .. } => {
                let mut parts = text.text.split(text::OBJECT);
                push(parts.next().unwrap_or_default());
                for (inline, part) in inlines.iter().zip(parts) {
                    walk(&inline.kind, out);
                    out.push(' ');
                    out.push_str(part);
                }
            }
            UiKind::Label { text, .. } | UiKind::Badge { text, .. } | UiKind::Link { text, .. } => push(text),
            UiKind::Button { label, .. } | UiKind::Checkbox { label, .. } => push(label),
            UiKind::ListItem { title, description, children, .. } => {
                push(&title.text);
                push(description.as_ref().map_or("", |description| description.text.as_str()));
                children.iter().for_each(|child| walk(&child.kind, out));
            }
            kind => {
                for child in children_of(kind) {
                    walk(&child.kind, out);
                }
            }
        }
    }
    let mut out = String::new();
    walk(kind, &mut out);
    let collapsed = text::collapse(&out.replace('\u{2028}', " ").replace(text::OBJECT, ""));
    collapsed.chars().take(120).collect()
}

fn children_of(kind: &UiKind) -> &[Arc<UiNode>] {
    match kind {
        UiKind::Col { children, .. }
        | UiKind::Row { children, .. }
        | UiKind::Box { children, .. }
        | UiKind::Grid { children, .. }
        | UiKind::Card { children, .. }
        | UiKind::Carousel { children, .. }
        | UiKind::Pressable { children, .. }
        | UiKind::Form { children, .. } => children,
        UiKind::List { items, .. } => items,
        _ => &[],
    }
}

/// How assistive technology reads a node (`LAYOUT.md`'s Accessibility column).
fn a11y_of(kind: &UiKind) -> Option<A11y> {
    let a11y = |role: Role| A11y { role, label: None, value: None, level: None, on: None };
    Some(match kind {
        UiKind::Card { .. } | UiKind::Carousel { .. } => a11y(Role::Group),
        UiKind::Popover { open, .. } => A11y { on: Some(*open), ..a11y(Role::Button) },
        UiKind::Divider {} | UiKind::Spacer {} | UiKind::Icon { .. } | UiKind::Celebration { .. } => a11y(Role::Hidden),
        UiKind::Title { style, .. } => {
            let level = match style {
                TextStyle::Title3xl => 1,
                TextStyle::Title2xl | TextStyle::TitleXl | TextStyle::TitleLg | TextStyle::CardHeading => 2,
                TextStyle::TitleSm => 4,
                _ => 3,
            };
            A11y { level: Some(level), ..a11y(Role::Heading) }
        }
        UiKind::Text { .. } | UiKind::Caption { .. } | UiKind::Label { .. } | UiKind::Badge { .. } => a11y(Role::Text),
        UiKind::Link { url: Some(_), .. } => a11y(Role::Link),
        UiKind::Link { .. } => a11y(Role::Text),
        UiKind::List { .. } => a11y(Role::List),
        UiKind::ListItem { .. } => a11y(Role::ListItem),
        UiKind::Quote { .. } => a11y(Role::Blockquote),
        UiKind::Code { .. } => a11y(Role::Code),
        UiKind::Math { canvas } => A11y { label: canvas.props.get("tex").map(js::to_string), ..a11y(Role::Image) },
        UiKind::Button { label, .. } => A11y { label: Some(label.clone()), ..a11y(Role::Button) },
        UiKind::IconButton { label, .. } => A11y { label: Some(label.clone()), ..a11y(Role::Button) },
        UiKind::Pressable { selected, .. } => A11y { on: *selected, ..a11y(Role::Button) },
        UiKind::Input { label, placeholder, .. } | UiKind::TextArea { label, placeholder, .. } => {
            A11y { label: label.clone().or_else(|| placeholder.clone()), ..a11y(Role::TextField) }
        }
        UiKind::Select { label, options, selected, placeholder, .. } => A11y {
            label: label.clone().or_else(|| Some(placeholder.clone())),
            value: selected.and_then(|index| options.get(index as usize)).map(|option| option.label.clone()),
            ..a11y(Role::PopUpButton)
        },
        UiKind::SegmentedControl { .. } => a11y(Role::RadioGroup),
        UiKind::RadioGroup { label, .. } => A11y { label: label.clone(), ..a11y(Role::RadioGroup) },
        UiKind::Slider { label, readout, .. } => A11y {
            label: Some(label.clone().unwrap_or_else(|| "Value".to_owned())),
            value: Some(readout.clone()),
            ..a11y(Role::Adjustable)
        },
        UiKind::Checkbox { checked, label, .. } => {
            A11y { label: Some(label.clone()), on: Some(*checked), ..a11y(Role::Checkbox) }
        }
        UiKind::DatePicker { label, text, .. } => A11y {
            label: Some(label.clone().unwrap_or_else(|| "Date".to_owned())),
            value: Some(text.clone()),
            ..a11y(Role::Button)
        },
        UiKind::Timer { label, time, .. } => A11y {
            label: Some(label.clone().unwrap_or_else(|| "Timer".to_owned())),
            value: Some(time.clone()),
            ..a11y(Role::Timer)
        },
        UiKind::Form { .. } => a11y(Role::Form),
        UiKind::SearchImage { query: label, .. } | UiKind::Image { alt: label, .. } => {
            A11y { label: Some(label.trim().to_owned()), ..a11y(Role::Image) }
        }
        UiKind::Table { .. } => a11y(Role::Table),
        UiKind::Chart { canvas } => A11y { label: Some(chart_label(&canvas.props)), ..a11y(Role::Image) },
        UiKind::Map { .. } => A11y { label: Some("Map".to_owned()), ..a11y(Role::Region) },
        UiKind::YouTube { title, .. } => A11y { label: Some(title.clone()), ..a11y(Role::Region) },
        UiKind::Flashcard { flipped, .. } => A11y { on: Some(*flipped), ..a11y(Role::Button) },
        UiKind::Svg { .. } => a11y(Role::Image),
        UiKind::HtmlView { title, .. } => A11y { label: Some(title.clone()), ..a11y(Role::Region) },
        _ => return None,
    })
}

/// "bar chart of revenue, cost by month", from the props the program gave.
fn chart_label(props: &Value) -> String {
    let kind = props.get("type").and_then(Value::as_str).unwrap_or("bar");
    let series: Vec<&str> =
        props.get("series").and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_str).collect();
    match (series.is_empty(), props.get("x").and_then(Value::as_str)) {
        (false, Some(x)) => format!("{kind} chart of {} by {x}", series.join(", ")),
        (false, None) => format!("{kind} chart of {}", series.join(", ")),
        (true, _) => format!("{kind} chart"),
    }
}

pub(super) fn str_of<'e>(element: &'e Element, prop: &str) -> Option<&'e str> {
    element.prop(prop).and_then(Value::as_str)
}

/// A prop as the component's `String(value ?? "")` reads it.
pub(super) fn string(element: &Element, prop: &str) -> String {
    element.prop(prop).map(js::to_string).unwrap_or_default()
}

pub(super) fn number(element: &Element, prop: &str) -> Option<f64> {
    element.prop(prop).and_then(Value::as_number).filter(|number| number.is_finite())
}

pub(super) fn flag(element: &Element, prop: &str) -> bool {
    element.prop(prop).is_some_and(js::truthy)
}

/// A count of 4-point steps, in points.
pub(super) fn steps(value: Option<&Value>) -> Option<f32> {
    value.and_then(Value::as_number).filter(|number| number.is_finite()).map(|number| number.max(0.0) as f32 * 4.0)
}

fn align(element: &Element, default: Align) -> Align {
    match string(element, "align").as_str() {
        "start" => Align::Start,
        "center" => Align::Center,
        "end" => Align::End,
        "stretch" => Align::Stretch,
        _ => default,
    }
}

fn justify(element: &Element) -> Justify {
    match string(element, "justify").as_str() {
        "center" => Justify::Center,
        "end" => Justify::End,
        "between" => Justify::Between,
        _ => Justify::Start,
    }
}

/// A Length prop (`style.ts`'s `size`): points, or a share of something; `auto` and `fr` are as
/// if it weren't given.
pub(super) fn length(value: &Value) -> Option<Length> {
    let points = |value: f64| Length { unit: LengthUnit::Points, value: value as f32 };
    match value {
        Value::Number(number) if number.is_finite() => Some(points(number.max(0.0))),
        Value::String(text) => {
            let text = text.trim();
            let split = text.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(text.len());
            let (digits, unit) = text.split_at(split);
            let well_formed = !digits.is_empty()
                && digits.chars().next().is_some_and(|c| c.is_ascii_digit())
                && !digits.ends_with('.')
                && digits.matches('.').count() <= 1;
            let number: f64 = digits.parse().ok().filter(|_| well_formed)?;
            let (unit, value) = match unit {
                "" | "px" => (LengthUnit::Points, number),
                "%" => (LengthUnit::Fraction, number / 100.0),
                "em" | "rem" => (LengthUnit::Points, number * 14.0),
                "vw" => (LengthUnit::Column, number / 100.0),
                "vh" => (LengthUnit::Window, number / 100.0),
                _ => return None,
            };
            Some(Length { unit, value: value as f32 })
        }
        _ => None,
    }
}

/// A Radius prop: a token, `full`, or points.
pub(super) fn radius(value: &Value) -> Option<Radius> {
    let token = |kind: RadiusKind, points: f32| Some(Radius { kind, points });
    match value {
        Value::String(name) => match name.as_str() {
            "sm" => token(RadiusKind::Sm, 6.0),
            "md" => token(RadiusKind::Md, 8.0),
            "lg" => token(RadiusKind::Lg, 12.0),
            "xl" => token(RadiusKind::Xl, 16.0),
            "full" => token(RadiusKind::Full, 0.0),
            _ => None,
        },
        Value::Number(number) if number.is_finite() => token(RadiusKind::Points, number.max(0.0) as f32),
        _ => None,
    }
}

/// How many columns a Grid has at a width: at most `columns`, each at least `min_child_width`.
pub fn grid_columns(columns: u32, min_child_width: Option<f32>, gap: f32, width: f32) -> u32 {
    let columns = columns.clamp(1, 12);
    let Some(min) = min_child_width else { return columns };
    let share = (width - (columns - 1) as f32 * gap) / columns as f32;
    if share >= min {
        return columns;
    }
    let least = width.min(min);
    if least + gap <= 0.0 {
        return 1;
    }
    (((width + gap) / (least + gap)).floor() as u32).clamp(1, columns.max(1))
}
