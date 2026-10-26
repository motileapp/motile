//! The small pieces every screen is made of, as the Mac app's `Views/UI` and `Views/Shared` draw
//! them: the light under the pointer, the controls, modals, and the icons of agents and projects.

pub mod agent_avatar;
pub mod control;
pub mod date_field;
pub mod icons;
pub mod logos;
pub mod menu;
pub mod modal;
pub mod motion;
pub mod parts;
pub mod pick;
pub mod rich_text;
pub mod skill_chip;
pub mod slider;

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{IconSize, Layer, Opacity, Radius, Surface, colors};
pub use agent_avatar::{AgentAvatar, AgentAvatars};
pub use control::{
    ActionButton, ActionMenu, Chip, InputField, InputVariant, Segmented, Select, Spinner, Switch, TextArea, Variant,
    card_default, card_slice, text_area_state,
};
pub use date_field::DateField;
pub use parts::{
    CommandBox, CopyButton, FadingScroll, GlassButton, KeyCaps, PickMark, anchored_popover, dropdown_card,
    popover_card, popover_panel, tracks_bounds,
};
pub use pick::{OptionCard, PickRow};
pub use slider::Slider;

pub type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The light of a row, a tab or a choice: under what the pointer is over or presses, what is
/// selected and what is `lit`. What is selected is lit no further. The light is drawn `inset`
/// from the edges of the element its group names: that margin looks empty but is the element's,
/// so neighbours leave no gap to miss. What is `small`, like a tab, lights as a control does, not
/// as a row. A `fill` lights it in that colour instead of the surface's accent. What is `opaque`
/// lies on the surface's own colour when unlit, so what slides under it stays hidden.
#[derive(Clone, Copy)]
pub struct Highlight {
    radius: f32,
    inset: Edges<f32>,
    selected: bool,
    lit: bool,
    /// The pointer is followed by whoever made it, which says so with `hovered`.
    follows_pointer: bool,
    small: bool,
    enabled: bool,
    surface: Surface,
    fill: Option<Hsla>,
    opaque: bool,
}

impl Default for Highlight {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlight {
    pub fn new() -> Self {
        Self {
            radius: Radius::MD,
            inset: Edges::default(),
            selected: false,
            lit: false,
            follows_pointer: true,
            small: false,
            enabled: true,
            surface: Surface::Background,
            fill: None,
            opaque: false,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn inset(mut self, inset: Edges<f32>) -> Self {
        self.inset = inset;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Lit without the pointer, as the row the arrow keys are on.
    pub fn lit(mut self, lit: bool) -> Self {
        self.lit = lit;
        self
    }

    /// Lit only while `hovered` says the pointer is on it: a row that follows the pointer itself.
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.lit = hovered;
        self.follows_pointer = false;
        self
    }

    pub fn small(mut self, small: bool) -> Self {
        self.small = small;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn fill(mut self, fill: Hsla) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn opaque(mut self, opaque: bool) -> Self {
        self.opaque = opaque;
        self
    }

    fn lit_layer(&self) -> Layer {
        if self.small { Layer::Control } else { Layer::Row }
    }

    /// The light it has without the pointer.
    pub fn layer(&self) -> Option<Layer> {
        if self.selected {
            return Some(if self.small { Layer::ControlLit } else { Layer::RowSelected });
        }
        (self.enabled && self.lit).then(|| self.lit_layer())
    }

    /// The light a control on it lies on while the pointer is over the control, and so over it.
    #[expect(dead_code, reason = "for the views still to be ported")]
    pub fn row(&self) -> Option<Layer> {
        if !self.enabled {
            return None;
        }
        Some(self.layer().unwrap_or_else(|| self.lit_layer()))
    }

    fn color(&self, layer: Layer, cx: &App) -> Hsla {
        self.fill.unwrap_or_else(|| self.surface.layer(layer, colors(cx)))
    }

    /// The light itself, to lie behind what the element `group` names shows.
    pub fn render(self, group: impl Into<SharedString>, cx: &App) -> Div {
        let group = group.into();
        let lit = self.color(self.lit_layer(), cx);
        let inset = self.inset;
        div()
            .absolute()
            .top(px(inset.top))
            .left(px(inset.left))
            .right(px(inset.right))
            .bottom(px(inset.bottom))
            .rounded(px(self.radius))
            .when(self.opaque, |light| light.bg(self.surface.color(colors(cx))))
            .when_some(self.layer(), |light, layer| light.bg(self.color(layer, cx)))
            .when(self.layer().is_none() && self.enabled && self.follows_pointer, |light| {
                light.group_hover(group.clone(), move |light| light.bg(lit))
            })
    }

    /// What is faded is in the muted colour until it lights up: what is under the pointer has to
    /// be `lit`, from `hover_state`, as a text's colour is set before the pointer is known.
    pub fn faded(self, element: Div, cx: &App) -> Div {
        let c = colors(cx);
        if !self.enabled {
            return element.text_color(c.muted_stronger_foreground);
        }
        element.text_color(if self.layer().is_some() { c.foreground } else { c.muted_foreground })
    }
}

/// Whether the pointer is over the element `id` names, kept from frame to frame. GPUI sets a
/// text's colour as it lays the text out, before it knows what the pointer is over, so text that
/// lights up under the pointer reads this, and the element keeps it with `track_hover`.
pub fn hover_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<bool> {
    window.use_keyed_state(ElementId::NamedChild(std::sync::Arc::new(id.clone()), "hovered".into()), cx, |_, _| false)
}

/// The focus handle of the control `id` names, kept from frame to frame, which Tab reaches.
pub fn focus_state(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    let key = ElementId::NamedChild(std::sync::Arc::new(id.clone()), "focus".into());
    window.use_keyed_state(key, cx, |_, cx| cx.focus_handle().tab_stop(true)).read(cx).clone()
}

/// Whether the control has the keys and was reached by them, so that it shows its focus ring.
pub fn focus_shown(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}

/// The focus ring around a shape of `radius` that fills its parent: 2 wide in the ring colour,
/// just outside it.
pub fn focus_ring(radius: f32, cx: &App) -> Div {
    div()
        .absolute()
        .top(px(-FOCUS_RING))
        .left(px(-FOCUS_RING))
        .right(px(-FOCUS_RING))
        .bottom(px(-FOCUS_RING))
        .rounded(px(radius + FOCUS_RING))
        .border(px(FOCUS_RING))
        .border_color(colors(cx).ring)
}

const FOCUS_RING: f32 = 2.;

/// The `on_hover` of the element whose `hover_state` this is.
pub fn track_hover(state: &Entity<bool>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |hovered, _, cx| {
        state.update(cx, |held, cx| {
            if *held != *hovered {
                *held = *hovered;
                cx.notify();
            }
        })
    }
}

/// The light of a row on the background: under the pointer, and under what is selected.
pub fn highlight(group: impl Into<SharedString>, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    Highlight::new().radius(radius).inset(inset).selected(selected).render(group, cx)
}

/// Makes the element a button whose own look says what it does, like a picture: pressed, it is
/// at opacity lit.
pub fn dims<E: StatefulInteractiveElement + Styled>(element: E, cx: &App) -> E {
    let lit = colors(cx).opacity(Opacity::Lit);
    element.active(move |element| element.opacity(lit))
}

/// How wide a line of the interface's type is drawn.
pub fn text_width(text: &str, size: f32, weight: FontWeight, window: &mut Window) -> f32 {
    line_width(text, size, crate::transcript::rows::font(crate::theme::UI_FONT, weight, false), window)
}

/// How wide a line of the interface's type is drawn with `tabular_numbers`.
pub fn tabular_text_width(text: &str, size: f32, weight: FontWeight, window: &mut Window) -> f32 {
    let font = crate::transcript::rows::font(crate::theme::UI_FONT, weight, false);
    line_width(text, size, Font { features: tabular_numbers(), ..font }, window)
}

fn line_width(text: &str, size: f32, font: Font, window: &mut Window) -> f32 {
    let run = TextRun {
        len: text.len(),
        font,
        color: gpui_kit::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(window.text_system().shape_line(text.to_string().into(), px(size), &[run], None).width).ceil()
}

/// One line of text that gives up its middle for an ellipsis when it doesn't fit, as the Mac's
/// `.truncationMode(.middle)`: a branch, a path or a file's name keeps its start and its end.
pub fn middle_truncated(text: impl Into<SharedString>) -> Div {
    div().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis_middle().child(text.into())
}

/// The elided lines painted by hand, by text, font, size and width.
#[derive(Default)]
struct Elided(std::collections::HashMap<(SharedString, Font, u32, u32), SharedString>);

impl Global for Elided {}

/// The text with its middle given up for an ellipsis so that it fits `width`, for text that is
/// painted rather than laid out. Each answer is kept, so a frame never measures it again.
pub fn elide_middle(text: &SharedString, font: &Font, size: Pixels, width: Pixels, cx: &mut App) -> SharedString {
    let key = (text.clone(), font.clone(), f32::from(size).to_bits(), f32::from(width).round() as u32);
    if let Some(elided) = cx.default_global::<Elided>().0.get(&key) {
        return elided.clone();
    }
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: gpui_kit::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut wrapper = cx.text_system().line_wrapper(font.clone(), size);
    let (elided, _) = wrapper.truncate_line(text.clone(), width, "…", &[run], TruncateFrom::Middle);
    let kept = &mut cx.global_mut::<Elided>().0;
    if kept.len() > 4096 {
        kept.clear();
    }
    kept.insert(key, elided.clone());
    elided
}

/// Text in several runs with its middle given up for an ellipsis so that it fits `width`, the
/// runs cut with it. It is measured in its first run's font.
pub fn elide_middle_runs(
    text: SharedString,
    runs: Vec<TextRun>,
    size: Pixels,
    width: Pixels,
    cx: &mut App,
) -> (SharedString, Vec<TextRun>) {
    let Some(first) = runs.first() else { return (text, runs) };
    let elided = elide_middle(&text, &first.font.clone(), size, width, cx);
    if elided == text {
        return (text, runs);
    }
    let front = text.bytes().zip(elided.bytes()).take_while(|(a, b)| a == b).count();
    let front = (0..=front).rev().find(|&at| text.is_char_boundary(at) && elided.is_char_boundary(at)).unwrap_or(0);
    let ellipsis = elided[front..].chars().next().map_or(0, char::len_utf8);
    let back = text.len() + front + ellipsis - elided.len();
    let mut cut = Vec::new();
    let keep = |range: std::ops::Range<usize>, cut: &mut Vec<TextRun>| {
        let mut at = 0;
        for run in &runs {
            let kept = range.start.max(at)..range.end.min(at + run.len);
            at += run.len;
            if !kept.is_empty() {
                cut.push(TextRun { len: kept.len(), ..run.clone() });
            }
        }
    };
    keep(0..front, &mut cut);
    let mut at = 0;
    if let Some(run) = runs.iter().find(|run| {
        at += run.len;
        at > front
    }) {
        cut.push(TextRun { len: ellipsis, ..run.clone() });
    }
    keep(back..text.len(), &mut cut);
    (elided, cut)
}

/// Tells the list which of its rows are others now, by their keys, so that only those are laid out
/// again and the list keeps its place, as the Mac's lists do: the rows around the change keep
/// theirs, and a top row that changed stays as far down.
pub fn relist(list: &ListState, listed: &mut Vec<String>, keys: Vec<String>) {
    if *listed == keys && list.item_count() == keys.len() {
        return;
    }
    if list.item_count() != listed.len() {
        list.reset(keys.len());
        *listed = keys;
        return;
    }
    let same_start = listed.iter().zip(&keys).take_while(|(old, new)| old == new).count();
    let same_end = listed[same_start..]
        .iter()
        .rev()
        .zip(keys[same_start..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let changed = same_start..listed.len() - same_end;
    let count = keys.len() - same_start - same_end;
    let top = list.logical_scroll_top();
    list.splice(changed.clone(), count);
    if changed.contains(&top.item_ix) && !keys.is_empty() {
        list.scroll_to(ListOffset { item_ix: top.item_ix.min(keys.len() - 1), ..top });
    }
    *listed = keys;
}

/// Digits of one width, so that numbers that change don't move what is beside them.
pub fn tabular_numbers() -> FontFeatures {
    FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
}

/// SwiftUI's ease-out curve, near enough.
pub fn ease_out(delta: f32) -> f32 {
    1. - (1. - delta) * (1. - delta)
}

pub fn edges(top: f32, left: f32, bottom: f32, right: f32) -> Edges<f32> {
    Edges { top, left, bottom, right }
}

pub fn even(all: f32) -> Edges<f32> {
    edges(all, all, all, all)
}

/// The tooltip that says `text`. It isn't shown once a menu, a dropdown, a modal, a popover, the
/// viewer or the command panel has opened over what it belongs to, nor when that was pressed while
/// it waited to show, as the system's tooltips aren't.
pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| {
        let shown = component::tooltip::Tooltip::new(text.clone()).build(window, cx);
        let overlays = cx.default_global::<Overlays>();
        let pressed_while_waiting = overlays.pressed.is_some_and(|pressed| pressed.elapsed() < TOOLTIP_DELAY);
        let opened = overlays.opened;
        cx.new(|_| HidingTooltip { shown, opened, hidden: pressed_while_waiting }).into()
    }
}

/// How long the pointer rests on something before its tooltip shows: GPUI's own delay, and a
/// little more for a press that came with the pointer.
const TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// How many menus, dropdowns, modals and command panels have opened, the popovers open now, and
/// when the mouse was last pressed: what a tooltip goes away for.
#[derive(Default)]
struct Overlays {
    opened: u64,
    open: std::collections::HashSet<&'static str>,
    pressed: Option<std::time::Instant>,
}

impl Global for Overlays {}

/// Says that a menu, a dropdown, a modal, a popover, the viewer or the command panel opened over
/// the window.
pub fn overlay_opened(cx: &mut App) {
    cx.default_global::<Overlays>().opened += 1;
}

/// Says that a popover the app draws, or the viewer, is open or went. No tooltip shows while one is.
pub fn popover_shown(name: &'static str, shown: bool, cx: &mut App) {
    let overlays = cx.default_global::<Overlays>();
    if !shown {
        overlays.open.remove(name);
    } else if overlays.open.insert(name) {
        overlays.opened += 1;
    }
}

/// Says that the mouse was pressed. The window's root says it of every press.
pub fn mouse_pressed(cx: &mut App) {
    cx.default_global::<Overlays>().pressed = Some(std::time::Instant::now());
}

struct HidingTooltip {
    shown: AnyView,
    /// How many overlays had opened when it was made.
    opened: u64,
    hidden: bool,
}

impl Render for HidingTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlays = cx.default_global::<Overlays>();
        let covered = overlays.opened != self.opened || !overlays.open.is_empty() || menu::is_open(cx);
        self.hidden |= covered;
        div().when(!self.hidden, |tooltip| tooltip.child(self.shown.clone()))
    }
}

/// Whether the controls drawn now are under the top modal, where they wait and look disabled.
pub fn inert(cx: &App) -> bool {
    cx.try_global::<Inert>().is_some_and(|inert| inert.0)
}

#[derive(Default)]
struct Inert(bool);

impl Global for Inert {}

/// Lays `child` out with its controls disabled while `inert`, as SwiftUI's `.disabled` does to
/// everything inside it.
pub struct InertScope {
    inert: bool,
    child: AnyElement,
}

pub fn inert_scope(inert: bool, child: impl IntoElement) -> InertScope {
    InertScope { inert, child: child.into_any_element() }
}

impl IntoElement for InertScope {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for InertScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let outer = std::mem::replace(&mut cx.default_global::<Inert>().0, self.inert);
        let layout = self.child.request_layout(window, cx);
        cx.default_global::<Inert>().0 = outer;
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// A button in the window's top bar. The buttons touch: the space seen between them is theirs.
pub const TOOLBAR_MARGIN: f32 = 2.;
pub const TOOLBAR_WIDTH: f32 = 28. + 2. * TOOLBAR_MARGIN;
/// Larger than a regular button's, which matches text these have none of.
pub const TOOLBAR_SYMBOL: IconSize = IconSize::Md;

/// A line across the background, in its border.
pub fn divider(cx: &App) -> Div {
    divider_on(Surface::Background, cx)
}

/// A line across the surface it lies on, in that surface's border.
pub fn divider_on(surface: Surface, cx: &App) -> Div {
    div().h(px(1.)).w_full().flex_shrink_0().bg(surface.border(colors(cx)))
}

/// The spinner of a small control.
pub fn spinner(size: IconSize, cx: &App) -> impl IntoElement {
    Spinner::new(size).render(cx)
}

/// The window's top bar, which moves the window when it is dragged and zooms it when it is
/// double-clicked, as the Mac's does, and shows the window's menu on a right click on Linux.
/// What lies on it takes its own clicks.
pub fn window_drag_strip(id: impl Into<ElementId>, height: f32, armed: &Rc<std::cell::Cell<bool>>) -> Stateful<Div> {
    let armed = armed.clone();
    let pressed = armed.clone();
    div()
        .id(id)
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(height))
        .on_mouse_down(MouseButton::Left, move |event, window, _| {
            if event.click_count == 2 {
                pressed.set(false);
                if cfg!(target_os = "macos") {
                    window.titlebar_double_click();
                } else {
                    window.zoom_window();
                }
            } else {
                pressed.set(true);
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let armed = armed.clone();
            move |_, _, _| armed.set(false)
        })
        .when(cfg!(target_os = "linux"), |strip| {
            strip.on_mouse_down(MouseButton::Right, |event, window, _| window.show_window_menu(event.position))
        })
        .on_mouse_move(move |event, window, _| {
            if armed.get() && event.pressed_button == Some(MouseButton::Left) {
                armed.set(false);
                window.start_window_move();
            }
        })
}
