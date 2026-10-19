//! A program's row: the tree core resolved, drawn as `crates/openui/LAYOUT.md` says. Each node is
//! a box of its own, laid out with the rest of the transcript's rows, which the list measures as
//! they come into view. What the user does goes back to core as `Command::Ui` events.

mod frames;
mod inputs;
mod layout;
pub mod patch;
mod style;
mod text;

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::{A11y, Role as UiRole, UiKind, UiNode};
use motile_core::render::rows::RowKind;

pub use frames::Framed;
#[cfg(test)]
#[allow(unused_imports, reason = "for the cross-client drift test")]
pub use frames::frame_dump;

use super::model::RowModel;
use super::rows::{RowContext, error_box, row_key};
use crate::store::Store;
use crate::theme::Surface;

/// Under this column width the narrow rules apply.
const NARROW: f32 = 640.;
/// The row keeps this much from the row above it, as a table in prose does from its paragraph.
const ABOVE: f32 = 10.;

/// Sends an event of the node to core.
pub type Sink = Rc<dyn Fn(&str, UiEvent, &mut App)>;

/// What drawing a node needs from around it.
#[derive(Clone)]
pub struct UiCx {
    pub row: SharedString,
    /// The row's selection key.
    pub key: u64,
    /// The transcript column's width, which picks the narrow rules.
    pub column: f32,
    /// The width the node has to lie in.
    pub width: f32,
    pub window_height: f32,
    /// The size of the text around the node, which a formula is set to.
    pub text_size: f32,
    pub surface: Surface,
    pub sink: Sink,
    pub store: Option<Entity<Store>>,
}

impl UiCx {
    pub fn narrow(&self) -> bool {
        self.column < NARROW
    }

    /// The element id of a node's part: the node's id with the part's name.
    pub fn id(&self, node: &str, part: &str) -> ElementId {
        ElementId::Name(SharedString::from(format!("ui:{}:{node}:{part}", self.row)))
    }

    pub fn send(&self, node: &str, event: UiEvent, cx: &mut App) {
        (self.sink)(node, event, cx);
    }

    pub fn within(&self, width: f32) -> Self {
        Self { width: width.max(0.), ..self.clone() }
    }

    pub fn on(&self, surface: Surface) -> Self {
        Self { surface, ..self.clone() }
    }
}

/// A program's row: its tree, and what keeps it from running under it.
pub fn ui_row(model: &RowModel, ctx: &RowContext, window: &mut Window, cx: &mut App) -> AnyElement {
    let RowKind::Ui { ui } = &model.row.kind else { return div().into_any_element() };
    let key = row_key(&model.row.id);
    let sink: Sink = match ctx.thread_id.clone() {
        Some(thread_id) => {
            let (store, row_id) = (ctx.store.clone(), model.row.id.clone());
            Rc::new(move |node: &str, event: UiEvent, cx: &mut App| {
                let command =
                    Command::Ui { thread_id: thread_id.clone(), row_id: row_id.clone(), node: node.into(), event };
                store.update(cx, |store, _| store.send(command));
            })
        }
        None => Rc::new(|_: &str, _: UiEvent, _: &mut App| {}),
    };
    let ui_cx = UiCx {
        row: model.row.id.clone().into(),
        key,
        column: ctx.width,
        width: ctx.width,
        window_height: f32::from(window.viewport_size().height),
        text_size: crate::theme::PROSE_SIZE,
        surface: Surface::Background,
        sink,
        store: Some(ctx.store.clone()),
    };
    let errors = (!ui.errors.is_empty()).then(|| {
        let id = format!("{}-errors", model.row.id);
        error_box(&id, &ui.errors.join("\n"), key, cx)
    });
    div()
        .pt(px(ABOVE))
        .flex()
        .flex_col()
        .gap(px(12.))
        .when_some(ui.tree.as_deref(), |row, tree| row.child(root(tree, &ui_cx, window, cx)))
        .children(errors)
        .into_any_element()
}

/// The program's root, as wide as the column.
pub fn root(tree: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> AnyElement {
    let element = node(tree, cx, window, app);
    framed(&tree.id, element.w_full())
}

/// A node's box, wrapped so that its frame can be recorded.
pub fn framed(id: &str, element: impl IntoElement) -> AnyElement {
    Framed::new(id, element).into_any_element()
}

/// The box of a node, which its parent sizes and places.
pub fn node(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = match &node.kind {
        UiKind::Col { .. }
        | UiKind::Row { .. }
        | UiKind::Box { .. }
        | UiKind::Grid { .. }
        | UiKind::Card { .. }
        | UiKind::Carousel { .. }
        | UiKind::Popover { .. }
        | UiKind::Divider {}
        | UiKind::Spacer {} => layout::layout(node, cx, window, app),
        UiKind::Title { .. }
        | UiKind::Text { .. }
        | UiKind::Caption { .. }
        | UiKind::Label { .. }
        | UiKind::Badge { .. }
        | UiKind::Link { .. }
        | UiKind::List { .. }
        | UiKind::ListItem { .. }
        | UiKind::Markdown { .. }
        | UiKind::Quote { .. }
        | UiKind::Code { .. } => text::text(node, cx, window, app),
        UiKind::Button { .. }
        | UiKind::IconButton { .. }
        | UiKind::Pressable { .. }
        | UiKind::Input { .. }
        | UiKind::TextArea { .. }
        | UiKind::Select { .. }
        | UiKind::SegmentedControl { .. }
        | UiKind::Slider { .. }
        | UiKind::Checkbox { .. }
        | UiKind::RadioGroup { .. }
        | UiKind::DatePicker { .. }
        | UiKind::Timer { .. }
        | UiKind::Form { .. } => inputs::input(node, cx, window, app),
        UiKind::SearchImage { .. }
        | UiKind::Image { .. }
        | UiKind::Icon { .. }
        | UiKind::Table { .. }
        | UiKind::Map { .. }
        | UiKind::MapMarker { .. }
        | UiKind::YouTube { .. } => div().id(cx.id(&node.id, "node")),
        UiKind::Math { .. }
        | UiKind::Chart { .. }
        | UiKind::Flashcard { .. }
        | UiKind::Celebration { .. }
        | UiKind::Svg { .. }
        | UiKind::G { .. }
        | UiKind::Circle { .. }
        | UiKind::Ellipse { .. }
        | UiKind::Rect { .. }
        | UiKind::Line { .. }
        | UiKind::Path { .. }
        | UiKind::Polyline { .. }
        | UiKind::Polygon { .. }
        | UiKind::SvgText { .. }
        | UiKind::HtmlView { .. } => div().id(cx.id(&node.id, "node")),
    };
    accessible(element, node.a11y.as_ref())
}

pub(super) fn accessible(element: Stateful<Div>, a11y: Option<&A11y>) -> Stateful<Div> {
    let Some(a11y) = a11y else { return element };
    let role = match a11y.role {
        UiRole::Group => Role::Group,
        UiRole::Button => Role::Button,
        UiRole::Dialog => Role::Dialog,
        UiRole::Heading => Role::Heading,
        UiRole::Text => Role::Label,
        UiRole::Link => Role::Link,
        UiRole::List => Role::List,
        UiRole::ListItem => Role::ListItem,
        UiRole::Blockquote => Role::Blockquote,
        UiRole::Code => Role::Code,
        UiRole::Image => Role::Image,
        UiRole::TextField => Role::TextInput,
        UiRole::PopUpButton => Role::ComboBox,
        UiRole::RadioGroup => Role::RadioGroup,
        UiRole::Adjustable => Role::Slider,
        UiRole::Checkbox => Role::CheckBox,
        UiRole::Timer => Role::Timer,
        UiRole::Form => Role::Form,
        UiRole::Table => Role::Table,
        UiRole::Region => Role::Region,
        UiRole::Hidden => return element,
    };
    let element = element
        .role(role)
        .when_some(a11y.label.clone(), |element, label| element.aria_label(label))
        .when_some(a11y.value.clone(), |element, value| element.aria_value(value))
        .when_some(a11y.level, |element, level| element.aria_level(level as usize));
    match (a11y.role, a11y.on) {
        (UiRole::Checkbox, Some(on)) => element.aria_toggled(if on { Toggled::True } else { Toggled::False }),
        (UiRole::Button, Some(on)) => element.aria_toggled(if on { Toggled::True } else { Toggled::False }),
        (_, Some(on)) => element.aria_expanded(on),
        (_, None) => element,
    }
}
