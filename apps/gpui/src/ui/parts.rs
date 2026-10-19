//! The smaller components of the Mac app's `Views/UI`: key caps, the button that copies, a
//! command to run, the mark of a picked row, a scroll that fades its hidden edges, and a menu of
//! things to turn on and off that stays open while they are picked.

#![allow(dead_code, reason = "a component offers what the Mac app's does, whether a view uses it yet or not")]

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Layer, Opacity, Radius, ShadowSize, Surface, colors};
use crate::ui::control::{ActionButton, Variant};
use crate::ui::menu::Anchor;
use crate::ui::{icons, motion};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// Keys to press, each on a cap of its own, or all on one when `joined`.
#[derive(IntoElement)]
pub struct KeyCaps {
    caps: Vec<SharedString>,
    joined: bool,
    surface: Surface,
}

impl KeyCaps {
    pub const HEIGHT: f32 = 20.;

    pub fn new(caps: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self { caps: caps.into_iter().map(Into::into).collect(), joined: false, surface: Surface::Background }
    }

    pub fn joined(mut self, joined: bool) -> Self {
        self.joined = joined;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }
}

impl RenderOnce for KeyCaps {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let fill = self.surface.layer(Layer::Control, colors(cx));
        let shown = if self.joined {
            vec![SharedString::from(crate::store::shortcuts::join_caps(&self.caps, std::env::consts::OS))]
        } else {
            self.caps
        };
        div().flex_shrink_0().flex().items_center().gap(px(5.)).children(shown.into_iter().map(move |cap| {
            div()
                .h(px(Self::HEIGHT))
                .min_w(px(22.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(Radius::XS))
                .bg(fill)
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .child(cap)
        }))
    }
}

/// How long the button that copies shows its check mark.
const COPIED_FOR: Duration = Duration::from_millis(1200);

/// When a button that copies last copied, and how many copies it had seen then.
struct Copied {
    at: Option<Instant>,
    copies: usize,
}

/// The button that copies. It shows a check mark for a moment once it has, also when `copies`
/// goes up because something else, like a shortcut, copied.
#[derive(IntoElement)]
pub struct CopyButton {
    id: ElementId,
    help: SharedString,
    variant: Variant,
    size: ControlSize,
    symbol_size: Option<f32>,
    round: bool,
    copies: usize,
    surface: Surface,
    row: Option<Layer>,
    disabled: bool,
    copy: Run,
}

impl CopyButton {
    pub fn new(
        id: impl Into<ElementId>,
        help: impl Into<SharedString>,
        copy: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            help: help.into(),
            variant: Variant::Ghost,
            size: ControlSize::Regular,
            symbol_size: None,
            round: false,
            copies: 0,
            surface: Surface::Background,
            row: None,
            disabled: false,
            copy: Rc::new(copy),
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn symbol_size(mut self, size: f32) -> Self {
        self.symbol_size = Some(size);
        self
    }

    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    /// How many times something else has copied what the button copies.
    pub fn copies(mut self, copies: usize) -> Self {
        self.copies = copies;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn row(mut self, row: Option<Layer>) -> Self {
        self.row = row;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Draws the window again once a copy's check mark is to go.
fn refresh_after_copy(window: &mut Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            cx.update(|window, _| window.refresh()).ok();
        })
        .detach();
}

impl RenderOnce for CopyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let copies = self.copies;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| Copied { at: None, copies });
        if state.read(cx).copies != copies {
            state.update(cx, |copied, _| *copied = Copied { at: Some(Instant::now()), copies });
            refresh_after_copy(window, cx);
        }
        let copied = state.read(cx).at.is_some_and(|at| at.elapsed() < COPIED_FOR);
        let copy = self.copy;
        let mut button = ActionButton::icon(self.id, if copied { "check" } else { "copy" }, self.help)
            .variant(self.variant)
            .size(self.size)
            .round(self.round)
            .surface(self.surface)
            .row(self.row)
            .disabled(self.disabled)
            .on_click(move |_, window, cx| {
                copy(window, cx);
                state.update(cx, |copied, _| copied.at = Some(Instant::now()));
                refresh_after_copy(window, cx);
            });
        if let Some(size) = self.symbol_size {
            button = button.symbol_size(size);
        }
        button
    }
}

/// A command to run in a terminal, in the theme's code colours, with the button that copies it.
/// Without a command yet it says what it waits for. A link is shown on one line, cut short, with
/// the button that opens it.
#[derive(IntoElement)]
pub struct CommandBox {
    id: ElementId,
    command: Option<SharedString>,
    /// What `Colors::syntax` colours each stretch: its start, its length and the colour, in
    /// threes, counted in UTF-16 as the core counts them.
    spans: Vec<u32>,
    placeholder: SharedString,
    surface: Surface,
    link: Option<SharedString>,
}

impl CommandBox {
    const TEXT_SIZE: f32 = 12.5;
    const LINE_HEIGHT: f32 = 18.;

    pub fn new(id: impl Into<ElementId>, command: Option<impl Into<SharedString>>) -> Self {
        Self {
            id: id.into(),
            command: command.map(Into::into),
            spans: Vec::new(),
            placeholder: "Preparing the command…".into(),
            surface: Surface::Background,
            link: None,
        }
    }

    pub fn link(mut self, link: impl Into<SharedString>) -> Self {
        self.link = Some(link.into());
        self
    }

    pub fn spans(mut self, spans: Vec<u32>) -> Self {
        self.spans = spans;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    /// The command in its colours. It wraps between any two characters, as CSS's `break-all`
    /// does, so a zero-width space follows each one; the copy takes the command as it is.
    fn highlighted(command: &str, spans: &[u32], palette: &[Hsla; 16]) -> StyledText {
        let units = command.encode_utf16().count();
        let mut colours = vec![0u32; units];
        for span in spans.as_chunks::<3>().0 {
            let (start, end) = (span[0] as usize, (span[0] + span[1]) as usize);
            let Some(stretch) = colours.get_mut(start..end) else { continue };
            stretch.fill(span[2]);
        }
        let mut text = String::with_capacity(command.len() * 4);
        let mut runs = Vec::new();
        let mut offset = 0;
        for character in command.chars() {
            let start = text.len();
            text.push(character);
            text.push('\u{200B}');
            let colour = palette.get(colours[offset] as usize).copied().unwrap_or(palette[0]);
            runs.push((start..text.len(), HighlightStyle { color: Some(colour), ..Default::default() }));
            offset += character.len_utf16();
        }
        StyledText::new(text).with_highlights(runs)
    }
}

impl RenderOnce for CommandBox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let inside = self.surface.in_box();
        let inset = ((ControlSize::Regular.height() - Self::LINE_HEIGHT) / 2.).round();
        let text = match (&self.command, &self.link) {
            (Some(command), Some(_)) => {
                div().whitespace_nowrap().overflow_hidden().text_ellipsis().child(command.clone()).into_any_element()
            }
            (Some(command), None) => Self::highlighted(command, &self.spans, &c.syntax).into_any_element(),
            (None, _) => div().text_color(c.muted_stronger_foreground).child(self.placeholder).into_any_element(),
        };
        let noun = if self.link.is_some() { "link" } else { "command" };
        let open = self.link.clone().map(|link| {
            ActionButton::new(SharedString::from(format!("open-{:?}", self.id)), "Open")
                .symbol("square-arrow-out-up-right")
                .ghost()
                .symbol_size(12.)
                .surface(inside)
                .on_click(move |_, _, cx| cx.open_url(&link))
        });
        let copied = self.command.clone();
        div()
            .id(self.id.clone())
            .w_full()
            .p(px(4.))
            .flex()
            .items_start()
            .gap(px(8.))
            .rounded(px(Radius::LG))
            .bg(self.surface.layer(Layer::Box, c))
            .border_1()
            .border_color(self.surface.border(c))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .py(px(inset))
                    .pl(px(inset + 2.))
                    .font_family(crate::theme::MONO_FONT)
                    .text_size(px(Self::TEXT_SIZE))
                    .line_height(px(Self::LINE_HEIGHT))
                    .text_color(c.foreground)
                    .child(text),
            )
            .child(
                div().flex().gap(px(4.)).children(open).child(
                    CopyButton::new(
                        SharedString::from(format!("copy-{:?}", self.id)),
                        format!("Copy the {noun}"),
                        move |_, cx| {
                            let Some(command) = &copied else { return };
                            cx.write_to_clipboard(ClipboardItem::new_string(command.to_string()));
                        },
                    )
                    .symbol_size(12.)
                    .surface(inside)
                    .disabled(self.command.is_none()),
                ),
            )
    }
}

/// Whether a row of a list is picked: a radio's ring where one of the list is picked, a box where
/// any are, filled in the primary colour once it is.
#[derive(IntoElement)]
pub struct PickMark {
    id: ElementId,
    picked: bool,
    multiple: bool,
}

impl PickMark {
    pub const SIZE: f32 = 16.;

    pub fn new(id: impl Into<ElementId>, picked: bool, multiple: bool) -> Self {
        Self { id: id.into(), picked, multiple }
    }
}

impl RenderOnce for PickMark {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let filled = motion::eased(self.id, if self.picked { 1. } else { 0. }, motion::QUICK, window, cx);
        let c = colors(cx);
        let stroke = motion::mix(c.muted_strongest_foreground, c.primary, filled);
        div()
            .size(px(Self::SIZE))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .border(px(1.5))
            .border_color(stroke)
            .bg(c.primary.opacity(filled))
            .when(self.multiple, |mark| mark.rounded(px(Radius::XS)))
            .when(!self.multiple, |mark| mark.rounded_full())
            .when(filled > 0. && self.multiple, |mark| {
                mark.child(div().opacity(filled).child(icons::symbol("check", 10.).text_color(c.primary_foreground)))
            })
            .when(filled > 0. && !self.multiple, |mark| {
                mark.child(div().size(px(Self::SIZE * 0.375)).rounded_full().bg(c.primary_foreground).opacity(filled))
            })
    }
}

/// How far a scroll fades out at an edge with more behind it.
pub const EDGE_FADE: f32 = 24.;

/// The fade over a scroll's edge, by how much of its content is hidden past it, up to
/// `EDGE_FADE`, into the colour of what it lies on.
pub fn edge_fade(top: bool, hidden: f32, color: Hsla) -> Div {
    let shown = color.opacity((hidden / EDGE_FADE).clamp(0., 1.));
    let (from, to) = if top { (shown, color.opacity(0.)) } else { (color.opacity(0.), shown) };
    div()
        .absolute()
        .left_0()
        .right_0()
        .h(px(EDGE_FADE))
        .when(top, |fade| fade.top_0())
        .when(!top, |fade| fade.bottom_0())
        .bg(linear_gradient(180., linear_color_stop(from, 0.), linear_color_stop(to, 1.)))
}

/// Content as tall as it is up to `max_height`, past which it scrolls, fading out at an edge
/// with more behind it. `padding` insets the content inside the scrolling edges.
#[derive(IntoElement)]
pub struct FadingScroll {
    id: ElementId,
    max_height: f32,
    padding: f32,
    starts_at_end: bool,
    fill: Option<Hsla>,
    children: Vec<AnyElement>,
}

impl FadingScroll {
    pub fn new(id: impl Into<ElementId>, max_height: f32) -> Self {
        Self { id: id.into(), max_height, padding: 0., starts_at_end: false, fill: None, children: Vec::new() }
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// It starts scrolled to its end.
    pub fn starts_at_end(mut self, starts_at_end: bool) -> Self {
        self.starts_at_end = starts_at_end;
        self
    }

    /// The colour of what it lies on, which its edges fade into: the background unless it is
    /// given one.
    pub fn fill(mut self, fill: Hsla) -> Self {
        self.fill = Some(fill);
        self
    }
}

impl ParentElement for FadingScroll {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for FadingScroll {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let starts_at_end = self.starts_at_end;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| (ScrollHandle::new(), starts_at_end));
        let (handle, unscrolled) = state.read(cx).clone();
        if unscrolled {
            state.update(cx, |state, _| state.1 = false);
            handle.scroll_to_bottom();
        }
        let above = (-handle.offset().y).max(px(0.));
        let below = (handle.max_offset().y - above).max(px(0.));
        let fill = self.fill.unwrap_or(colors(cx).background);
        div()
            .relative()
            .w_full()
            .max_h(px(self.max_height))
            .flex()
            .flex_col()
            .child(
                div()
                    .id(self.id)
                    .w_full()
                    .max_h(px(self.max_height))
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .p(px(self.padding))
                    .on_scroll_wheel(|_, window, _| window.refresh())
                    .children(self.children),
            )
            .when(above > px(0.), |scroll| scroll.child(edge_fade(true, f32::from(above), fill)))
            .when(below > px(0.), |scroll| scroll.child(edge_fade(false, f32::from(below), fill)))
    }
}

/// The look of what drops from a button or lies over the window: the popover's colour, a line
/// around it and a faint shadow under it. What is inside lies on `Surface::Popover`.
pub fn dropdown_card(element: Div, cx: &App) -> Div {
    popover_card(element, Radius::MD, cx)
}

/// `dropdown_card` with corners of `radius`, as a chart's tooltip has.
pub fn popover_card(element: Div, radius: f32, cx: &App) -> Div {
    let c = colors(cx);
    element
        .bg(c.popover)
        .rounded(px(radius))
        .border_1()
        .border_color(c.border_popover)
        .shadow(c.shadow(ShadowSize::Md, Opacity::Shadow))
}

const POPOVER_GAP: f32 = 8.;
const POPOVER_MARGIN: f32 = 12.;

/// Where the trigger of a popover was last painted, in the window.
pub type TriggerBounds = Entity<Option<Bounds<Pixels>>>;

/// Keeps `bounds` up to date with the frame of the element it is a child of.
pub fn tracks_bounds(bounds: TriggerBounds) -> impl IntoElement {
    canvas(move |frame, _, cx| bounds.update(cx, |bounds, _| *bounds = Some(frame)), |_, _, _, _| {})
        .absolute()
        .inset_0()
}

/// Motile's popover panel, at most `width` wide, padded 12 and 14, its children 8 apart. A press
/// outside it, Esc while it has `focus`, or a scroll outside it calls `close`.
pub fn popover_panel(id: ElementId, width: f32, focus: &FocusHandle, close: Run, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let (escapes, scrolls) = (close.clone(), close.clone());
    let closes_on_scroll = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (scrolls, sent) = (scrolls.clone(), std::cell::Cell::new(false));
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || bounds.contains(&event.position) || sent.replace(true) {
                    return;
                }
                scrolls(window, cx);
            });
        },
    )
    .absolute()
    .inset_0();
    popover_card(
        div().py(px(12.)).px(px(14.)).max_w(px(width)).flex().flex_col().gap(px(8.)).text_color(c.foreground),
        Radius::LG,
        cx,
    )
    .id(id)
    .role(Role::Dialog)
    .occlude()
    .child(closes_on_scroll)
    .track_focus(focus)
    .on_key_down(move |event: &KeyDownEvent, window, cx| {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            escapes(window, cx);
        }
    })
    .on_mouse_down_out(move |_, window, cx| close(window, cx))
}

/// A popover's panel by its trigger, a child of it: 8 under it with their leading edges together,
/// or 8 over it when fewer than 220 are left under it and more than 240 over it, kept 12 inside
/// the window.
pub fn anchored_popover(trigger: Option<Bounds<Pixels>>, panel: impl IntoElement, window: &Window) -> Div {
    let room = f32::from(window.viewport_size().height);
    let up = trigger.is_some_and(|trigger| {
        let below = room - f32::from(trigger.bottom());
        below < 220. + POPOVER_GAP && f32::from(trigger.top()) > 240.
    });
    let (place, corner, offset) = if up {
        (div().absolute().left_0().bottom_full(), gpui_kit::Anchor::BottomLeft, -POPOVER_GAP)
    } else {
        (div().absolute().left_0().top_full(), gpui_kit::Anchor::TopLeft, POPOVER_GAP)
    };
    place.child(
        deferred(
            anchored()
                .anchor(corner)
                .snap_to_window_with_margin(px(POPOVER_MARGIN))
                .offset(point(px(0.), px(offset)))
                .child(panel),
        )
        .with_priority(1),
    )
}

/// One of the things a `CheckMenu` turns on and off.
pub struct Check {
    pub title: SharedString,
    pub checked: bool,
    pub toggle: Run,
}

impl Check {
    pub fn new(
        title: impl Into<SharedString>,
        checked: bool,
        toggle: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self { title: title.into(), checked, toggle: Rc::new(toggle) }
    }
}

/// A menu of things to turn on and off. It stays open while they are picked, and closes on Esc
/// or a click anywhere else.
#[derive(IntoElement)]
pub struct CheckMenu {
    id: ElementId,
    title: SharedString,
    icon: Option<SharedString>,
    help: SharedString,
    variant: Variant,
    size: ControlSize,
    tint: Option<Hsla>,
    checks: Vec<Check>,
}

impl CheckMenu {
    const PADDING: f32 = 4.;
    const GAP: f32 = 4.;
    const ROW_HEIGHT: f32 = 28.;

    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        help: impl Into<SharedString>,
        checks: Vec<Check>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: None,
            help: help.into(),
            variant: Variant::Ghost,
            size: ControlSize::Regular,
            tint: None,
            checks,
        }
    }

    pub fn icon(mut self, symbol: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol.into());
        self
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn tint(mut self, tint: Option<Hsla>) -> Self {
        self.tint = tint;
        self
    }
}

/// Whether a `CheckMenu` is open, where its button was drawn, and what has the keys while it is.
struct Dropdown {
    open: bool,
    anchor: Anchor,
    focus: FocusHandle,
}

impl RenderOnce for CheckMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| Dropdown {
            open: false,
            anchor: Anchor::default(),
            focus: cx.focus_handle(),
        });
        let (open, anchor, focus) = {
            let dropdown = state.read(cx);
            (dropdown.open, dropdown.anchor.clone(), dropdown.focus.clone())
        };
        let show = {
            let state = state.clone();
            move |shown: bool, window: &mut Window, cx: &mut App| {
                state.update(cx, |dropdown, _| dropdown.open = shown);
                if shown {
                    crate::ui::overlay_opened(cx);
                    let focus = state.read(cx).focus.clone();
                    window.focus(&focus, cx);
                }
                window.refresh();
            }
        };
        let id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "dropdown".into());
        let shown = motion::eased_from(id, 0., if open { 1. } else { 0. }, motion::QUICK, window, cx);
        let show = Rc::new(show);
        let mut button = ActionButton::new(self.id.clone(), self.title)
            .help(self.help)
            .variant(self.variant)
            .size(self.size)
            .selected(open)
            .opens(true)
            .tint(self.tint)
            .on_click({
                let show = show.clone();
                move |_, window, cx| show(true, window, cx)
            });
        if let Some(icon) = self.icon {
            button = button.symbol(icon);
        }
        // Under the button, always: it scrolls when there is too little room below it.
        let dropdown = (shown > 0.).then(|| {
            let c = colors(cx);
            let bounds = anchor.bounds();
            let content = crate::window_frame::content_bounds(window);
            let viewport = content.size;
            let top = bounds.bottom() + px(Self::GAP);
            let room = (content.bottom() - top - px(2. * Self::GAP)).max(px(0.));
            let rows = self.checks.into_iter().enumerate().map(|(index, check)| {
                let group = SharedString::from(format!("check-{index}"));
                let toggle = check.toggle;
                div()
                    .id(("check", index))
                    .group(group.clone())
                    .relative()
                    .h(px(Self::ROW_HEIGHT))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(c.foreground)
                    .child(crate::ui::Highlight::new().radius(Radius::SM).surface(Surface::Popover).render(group, cx))
                    .child(
                        div()
                            .relative()
                            .when(!check.checked, |mark| mark.opacity(0.))
                            .child(icons::symbol("check", 13.)),
                    )
                    .child(div().relative().flex_1().whitespace_nowrap().pr(px(12.)).child(check.title))
                    .when(open, |row| {
                        row.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            toggle(window, cx)
                        })
                    })
            });
            let card = dropdown_card(div(), cx)
                .id("check-menu")
                .absolute()
                .left(bounds.origin.x)
                .top(top)
                .min_w(px(160.))
                .max_h(room)
                .overflow_y_scroll()
                .p(px(Self::PADDING))
                .opacity(shown)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(rows);
            let (close, escape) = (show.clone(), show.clone());
            // While it fades out it takes no more clicks or keys.
            let layer = div()
                .id("check-menu-layer")
                .w(viewport.width)
                .h(viewport.height)
                .when(open, |layer| {
                    layer
                        .occlude()
                        .track_focus(&focus)
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key != "escape" {
                                return;
                            }
                            cx.stop_propagation();
                            escape(false, window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| close(false, window, cx))
                })
                .child(card);
            deferred(anchored().position(crate::window_frame::content_origin(window)).child(layer))
                .with_priority(crate::ui::menu::PRIORITY)
        });
        div().relative().flex_shrink_0().child(anchor.track()).child(button).children(dropdown)
    }
}

/// A round symbol's button that floats on glass over what scrolls under it, as the map's controls
/// do: the surface's colour at `lit` in its edge, with a small shadow.
#[derive(IntoElement)]
pub struct GlassButton {
    id: ElementId,
    symbol: SharedString,
    help: SharedString,
    surface: Surface,
    on_click: Option<crate::ui::OnClick>,
}

impl GlassButton {
    pub const SIDE: f32 = 34.;
    const SYMBOL: f32 = 18.;

    pub fn new(id: impl Into<ElementId>, symbol: impl Into<SharedString>, help: impl Into<SharedString>) -> Self {
        Self { id: id.into(), symbol: symbol.into(), help: help.into(), surface: Surface::Background, on_click: None }
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for GlassButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let base = self.surface.color(c);
        let on_click = self.on_click;
        div()
            .id(self.id)
            .size(px(Self::SIDE))
            .flex_none()
            .rounded_full()
            .bg(c.at(base, Opacity::Lit))
            .border_1()
            .border_color(self.surface.border(c))
            .shadow(c.shadow(ShadowSize::Sm, Opacity::ShadowStronger))
            .flex()
            .items_center()
            .justify_center()
            .text_color(c.foreground)
            .cursor_pointer()
            .hover(move |button| button.bg(base))
            .role(Role::Button)
            .aria_label(self.help.clone())
            .tooltip(crate::ui::tooltip(self.help))
            .child(icons::symbol(self.symbol, Self::SYMBOL / crate::theme::SYMBOL_SCALE))
            .when_some(on_click, |button, on_click| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}
