//! The tree a client draws for a program: each of the library's 55 components with its props
//! final for drawing, as `crates/openui/LAYOUT.md` says. Lengths are points, colours tokens or an
//! RGBA, text the runs prose rows use (UTF-16 offsets), and open-intelligent-ui's context rules
//! are applied. `scripts/ui-types.sh` writes the Swift and Kotlin copies of these types, so this
//! file holds only them.

use std::sync::Arc;

use motile_openui::Value;
use serde::{Deserialize, Serialize};

pub use super::canvas::scene::{Color, Layer};
use crate::render::highlight::Spans;
use crate::render::markdown::{Link, Prose};

mod color;
mod controls;
mod icons;
mod lucide;
mod media;
mod resolve;
#[cfg(test)]
mod tests;
mod text;

pub use icons::glyph;
pub use resolve::{NoState, ResolveCtx, Resolver, Route, UiState, Widgets, grid_columns, resolve};

pub type Nodes = Vec<Arc<UiNode>>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UiNode {
    /// The path from the program's root, its props and indexes joined by `/`: `children/2`.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a11y: Option<A11y>,
    #[serde(flatten)]
    pub kind: UiKind,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "component")]
pub enum UiKind {
    /// Its children stacked, each `spacing` below the one before.
    Col {
        children: Nodes,
        align: Align,
        spacing: Vec<f32>,
    },
    Row {
        children: Nodes,
        gap: f32,
        align: Align,
        justify: Justify,
        wrap: bool,
        /// How wide each child starts and whether it grows or shrinks.
        sizing: Vec<Sizing>,
    },
    Box {
        children: Nodes,
        direction: Direction,
        /// Between children in a row; a column has `spacing`.
        gap: f32,
        spacing: Vec<f32>,
        sizing: Vec<Sizing>,
        padding: f32,
        background: Option<Color>,
        /// The content lies on the box's inner surface.
        inner_surface: bool,
        /// Rounds and clips; nothing is clipped without one.
        radius: Option<Radius>,
        border: bool,
        width: Option<Length>,
        height: Option<Length>,
        flex: Option<f32>,
        align: Align,
        justify: Justify,
        wrap: bool,
    },
    Grid {
        children: Nodes,
        /// The most columns; `grid_columns` gives how many fit a width.
        columns: u32,
        min_child_width: Option<f32>,
        gap: f32,
    },
    Card {
        children: Nodes,
        soft: bool,
        /// A card in a card.
        nested: bool,
        /// The program's padding, else the client's for the column's width.
        padding: Option<f32>,
        spacing: Vec<f32>,
    },
    Carousel {
        children: Nodes,
        item_width: f32,
        gap: f32,
        /// The items as tall as the tallest: a Card or a Pressable.
        fill: Vec<bool>,
    },
    Popover {
        trigger: Slot,
        content: Slot,
        open: bool,
    },
    Divider {},
    Spacer {},
    Title {
        text: Rich,
        style: TextStyle,
        color: Color,
    },
    Text {
        /// A U+FFFC stands for each of `inlines`, in order.
        text: Rich,
        inlines: Nodes,
        style: TextStyle,
        color: Color,
        weight: Weight,
        italic: bool,
        strike: bool,
        digits: bool,
        max_lines: Option<u32>,
    },
    Caption {
        text: Rich,
        style: TextStyle,
        color: Color,
    },
    Label {
        text: String,
        color: Color,
    },
    Badge {
        text: String,
        /// The chip's tone, a token; none for neutral and info.
        tone: Option<String>,
        icon: Option<Glyph>,
    },
    Link {
        text: String,
        /// None when the program's URL isn't http(s): the text is plain.
        url: Option<String>,
    },
    List {
        /// ListItems.
        items: Nodes,
        marker: ListMarker,
        timeline: bool,
        /// A marker longer than two characters: the markers have a lane of text.
        wide: bool,
    },
    ListItem {
        title: Rich,
        style: TextStyle,
        /// Medium, as it has a description or children.
        medium: bool,
        description: Option<Rich>,
        marker: ItemMarker,
        children: Nodes,
    },
    Markdown {
        blocks: Vec<MarkdownBlock>,
    },
    Quote {
        text: Rich,
        cite: Option<Rich>,
    },
    Code {
        /// "code" when the program names none.
        language: String,
        code: String,
        /// `None` until the code has been highlighted, like a code row's.
        spans: Option<Spans>,
    },
    Math {
        canvas: Canvas,
    },
    Button {
        label: String,
        variant: ButtonVariant,
        /// The colour an outline, soft or ghost button takes.
        tint: Option<String>,
        size: ControlSize,
        icon: Option<Glyph>,
        block: bool,
        disabled: Option<Disabled>,
    },
    IconButton {
        icon: Option<Glyph>,
        label: String,
        tooltip: Option<String>,
        variant: ButtonVariant,
        disabled: Option<Disabled>,
    },
    Pressable {
        children: Nodes,
        padding: f32,
        background: Option<Color>,
        inner_surface: bool,
        radius: Radius,
        selected: Option<bool>,
        /// Selected, and its only child isn't a Row: the ring and the picked mark.
        ring: bool,
        disabled: Option<Disabled>,
    },
    Input {
        value: String,
        number: bool,
        label: Option<String>,
        placeholder: Option<String>,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    TextArea {
        value: String,
        label: Option<String>,
        placeholder: Option<String>,
        rows: u32,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    Select {
        value: String,
        options: Vec<Choice>,
        selected: Option<u32>,
        label: Option<String>,
        /// Shown when no option is chosen.
        placeholder: String,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    SegmentedControl {
        value: String,
        options: Vec<Choice>,
        selected: Option<u32>,
        block: bool,
        disabled: Option<Disabled>,
    },
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        /// Where the thumb is, from 0 to 1.
        at: f64,
        label: Option<String>,
        readout: String,
        /// The label and the readout show above the track.
        head: bool,
        marks: Vec<SliderMark>,
        disabled: Option<Disabled>,
    },
    Checkbox {
        checked: bool,
        label: String,
        strike: bool,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    RadioGroup {
        value: String,
        options: Vec<Choice>,
        selected: Option<u32>,
        label: Option<String>,
        variant: RadioVariant,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    DatePicker {
        /// `YYYY-MM-DD`.
        value: Option<String>,
        /// The date as the field shows it, or "Date".
        text: String,
        min: Option<String>,
        max: Option<String>,
        label: Option<String>,
        error: Option<String>,
        disabled: Option<Disabled>,
    },
    Timer {
        label: Option<String>,
        time: String,
        seconds: f64,
        running: bool,
        done: bool,
        toggle: TimerButton,
        reset: TimerButton,
    },
    Form {
        children: Nodes,
        spacing: Vec<f32>,
    },
    SearchImage {
        query: String,
        aspect_ratio: f32,
        radius: Radius,
        fit: Fit,
        photos: Photos,
    },
    Image {
        src: Option<String>,
        alt: String,
        aspect_ratio: f32,
        radius: Radius,
        fit: Fit,
        photos: Photos,
    },
    Icon {
        /// None for a name Lucide doesn't have: nothing is drawn.
        icon: Option<Glyph>,
        size: f32,
        color: Color,
    },
    Table {
        columns: Vec<TableColumn>,
        rows: Vec<Vec<TableCell>>,
    },
    Chart {
        canvas: Canvas,
    },
    Map {
        pins: Vec<MapPin>,
        route: RouteKind,
        /// `[lat, lng]` along the streets, or straight between the pins shown.
        line: Vec<[f64; 2]>,
        selected: Option<String>,
        height: f32,
        categories: Vec<String>,
        /// The category shown, none for every place.
        filter: Option<String>,
        note: String,
    },
    MapMarker {
        label: String,
        lat: f64,
        lng: f64,
        category: Option<String>,
        icon: Option<String>,
    },
    YouTube {
        /// The embed's address; none for a video it can't name, which shows nothing.
        url: Option<String>,
        title: String,
    },
    Flashcard {
        front: Slot,
        back: Slot,
        hint: String,
        flipped: bool,
        /// The flip.
        canvas: Canvas,
    },
    Celebration {
        /// Counts the bursts: a new one fires the confetti.
        burst: u32,
        canvas: Canvas,
    },
    Svg {
        canvas: Canvas,
    },
    G {
        canvas: Canvas,
    },
    Circle {
        canvas: Canvas,
    },
    Ellipse {
        canvas: Canvas,
    },
    Rect {
        canvas: Canvas,
    },
    Line {
        canvas: Canvas,
    },
    Path {
        canvas: Canvas,
    },
    Polyline {
        canvas: Canvas,
    },
    Polygon {
        canvas: Canvas,
    },
    SvgText {
        canvas: Canvas,
    },
    HtmlView {
        html: String,
        height: Option<f32>,
        title: String,
        /// The spinner shows in its place.
        streaming: bool,
    },
}

/// Text with its inline formatting, as a prose row's: `[start, length, style bits]` runs.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Rich {
    pub text: String,
    pub runs: Vec<[u32; 3]>,
    pub links: Vec<Link>,
}

/// Text or components, where a slot takes either.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Slot {
    pub text: Option<Rich>,
    pub style: TextStyle,
    pub children: Nodes,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Body,
    BodyCard,
    Large,
    Small,
    Caption,
    Label,
    TitleSm,
    TitleMd,
    TitleLg,
    TitleXl,
    Title2xl,
    Title3xl,
    CardTitle,
    CardSubtitle,
    CardHeading,
    Readout,
    Tick,
    Timer,
    Code,
    /// A flashcard's word: `title-lg` on the front.
    Word,
    /// The back's word, `body` medium.
    WordBack,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Length {
    pub unit: LengthUnit,
    pub value: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Points,
    /// Of the parent's content box.
    Fraction,
    /// Of the transcript column's width.
    Column,
    /// Of the window's height.
    Window,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Radius {
    pub kind: RadiusKind,
    /// 0 for `full`, half the shorter side.
    pub points: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RadiusKind {
    Sm,
    Md,
    Lg,
    Xl,
    Full,
    Points,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Row,
    Col,
}

/// A row's child: its width before the row's space is shared.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Sizing {
    pub basis: Basis,
    pub length: Option<Length>,
    pub grow: f32,
    pub shrink: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Its widest single line.
    Content,
    Zero,
    /// `length`.
    Length,
    /// The row's width.
    Row,
    /// A media row's image: clamp(120, a third of the row, 200).
    Media,
}

/// A Lucide icon: its name, and its character in `lucide.ttf`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Glyph {
    pub name: String,
    pub glyph: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ListMarker {
    Bullet,
    Number,
    Plain,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemMarker {
    Blank,
    Dot,
    Number {
        number: u32,
    },
    /// One or two letters or digits, in a pill.
    Chip {
        text: String,
    },
    /// Any other marker, as text.
    Label {
        text: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkdownBlock {
    Prose { prose: Prose },
    Code { language: String, code: String, spans: Option<Spans> },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Primary,
    Danger,
    Success,
    Outline,
    Secondary,
    Ghost,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ControlSize {
    Small,
    Regular,
    Large,
}

/// Why a control takes no presses.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Disabled {
    /// The program says so.
    Program,
    /// The program is still streaming.
    Streaming,
    /// It runs a tool, which the thread's server can't.
    NoTools,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SliderMark {
    /// From 0 to 1.
    pub at: f64,
    pub label: String,
    pub edge: MarkEdge,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MarkEdge {
    /// Leading at the track's start.
    Start,
    Middle,
    /// Trailing at the track's end.
    End,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RadioVariant {
    List,
    Pills,
    Cards,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TimerButton {
    pub label: String,
    pub icon: Option<Glyph>,
    pub disabled: Option<Disabled>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Cover,
    Contain,
}

/// The photos found for an image's query.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Photos {
    pub state: PhotoState,
    /// Media refs, best first.
    pub media: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PhotoState {
    /// Not looked for: the image has its own address.
    Idle,
    Waiting,
    Found,
    /// Nothing found, or the lookup failed.
    Missing,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TableColumn {
    pub title: String,
    /// Numbers: trailing, digits, one line.
    pub numeric: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TableCell {
    Text { text: Rich },
    Node { node: Arc<UiNode> },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// No route is drawn.
    Off,
    /// Straight while the street route is looked for.
    Loading,
    Street,
    /// Straight, as no street route was found: "Route overview".
    Overview,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct MapPin {
    /// The label, numbered when another pin has it too: "Café (2)".
    pub key: String,
    pub label: String,
    pub lat: f64,
    pub lng: f64,
    pub category: Option<String>,
    /// One emoji, else the pin shows `number`.
    pub icon: Option<String>,
    pub number: u32,
    pub selected: bool,
    /// In the category shown.
    pub visible: bool,
}

/// What `motile_ui_layout` lays out, which the client gives its width and text size: the
/// component and its props by name, in order, an Svg's shapes as `{kind, props}` too.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Canvas {
    pub kind: String,
    pub props: Value,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct A11y {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// A heading's level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Pressed, expanded or checked, by the role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Group,
    Button,
    Dialog,
    Heading,
    Text,
    Link,
    List,
    ListItem,
    Blockquote,
    Code,
    Image,
    TextField,
    PopUpButton,
    RadioGroup,
    Adjustable,
    Checkbox,
    Timer,
    Form,
    Table,
    Region,
    Hidden,
}
