//! The controls every view is made of, as the Mac app's `Views/UI` draws them: the button and the
//! menu that looks like it, the spinner, the chip, the switch, the segmented choice, the field
//! and the card. Their sizes are a `ControlSize` and their corners a `Radius`; nothing here is
//! styled by hand.

#![allow(dead_code, reason = "a component offers what the Mac app's does, whether a view uses it yet or not")]

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{Colors, ControlSize, Layer, Opacity, Radius, Surface, colors};
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{OnClick, tooltip};
use crate::ui::{icons, motion};

/// What a button is for, which is how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Variant {
    /// The one thing the view calls for.
    Primary,
    /// The others beside it.
    #[default]
    Secondary,
    /// The page's colour inside its border: the buttons of settings rows.
    Outline,
    /// What can't be taken back.
    Danger,
    /// What the user is asked to let happen.
    Warning,
    /// Nothing until the pointer is over it: the buttons of bars and rows.
    Ghost,
    /// In the colour of a link.
    Link,
    /// A link's colour on a wash of it: a choice that is on.
    Accent,
    /// A secondary button as it is in the dark, over a picture or a video.
    Overlay,
}

/// What a button shows before its words: a symbol, or a picture of its own like a logo.
pub enum ControlIcon {
    Symbol(SharedString),
    Picture(AnyElement),
}

/// The colours and the shape of a button or a menu. Pressed looks as under the pointer does.
#[derive(Clone, Copy)]
pub struct Look {
    pub variant: Variant,
    pub size: ControlSize,
    pub selected: bool,
    /// The layer it lies on.
    pub surface: Surface,
    /// The row it lies on, as it is lit when the pointer is over the button.
    pub row: Option<Layer>,
    /// Round ends, for the composer's buttons.
    pub round: bool,
    /// The sides another control touches, which stay square: (leading, trailing).
    pub joined: (bool, bool),
    /// A colour that says something, like a pull request's state, in place of the quiet ones.
    pub tint: Option<Hsla>,
}

impl Look {
    pub fn new(variant: Variant, size: ControlSize) -> Self {
        Self {
            variant,
            size,
            selected: false,
            surface: Surface::Background,
            row: None,
            round: false,
            joined: (false, false),
            tint: None,
        }
    }

    pub fn foreground(&self, lit: bool, c: &Colors) -> Hsla {
        if let Some(tint) = self.tint
            && matches!(self.variant, Variant::Ghost | Variant::Secondary)
        {
            return tint;
        }
        match self.variant {
            Variant::Secondary | Variant::Outline => c.foreground,
            Variant::Primary => c.primary_foreground,
            Variant::Danger => c.destructive_foreground,
            Variant::Warning => c.warning_foreground,
            Variant::Ghost => {
                if self.selected || lit {
                    c.foreground
                } else {
                    c.muted_foreground
                }
            }
            Variant::Link | Variant::Accent => c.primary,
            Variant::Overlay => c.foreground_over_picture,
        }
    }

    /// A button filled with a colour of its own lights by that colour at opacity lit.
    pub fn fill(&self, lit: bool, c: &Colors) -> Hsla {
        let filled = |color: Hsla| if lit { c.at(color, Opacity::Lit) } else { color };
        let on = self.selected || lit;
        match self.variant {
            Variant::Primary => filled(c.primary),
            Variant::Secondary => self.surface.layer(if on { Layer::ControlLit } else { Layer::Control }, c),
            Variant::Outline => {
                if on {
                    self.surface.layer(Layer::Control, c)
                } else {
                    c.background
                }
            }
            Variant::Danger => filled(c.destructive),
            Variant::Warning => filled(c.warning),
            Variant::Ghost => {
                if self.selected {
                    self.surface.layer(Layer::ControlLit, c)
                } else if lit {
                    self.ghost_lit(c)
                } else {
                    transparent_black()
                }
            }
            Variant::Link => {
                if lit {
                    c.wash(c.primary, false)
                } else {
                    transparent_black()
                }
            }
            Variant::Accent => c.wash(c.primary, lit),
            Variant::Overlay => {
                if lit {
                    c.control_lit_over_picture
                } else {
                    c.control_over_picture
                }
            }
        }
    }

    /// A ghost button lights a step above the row it lies on.
    fn ghost_lit(&self, c: &Colors) -> Hsla {
        let layer = match self.row {
            Some(Layer::RowSelected | Layer::ControlLit) => Layer::ControlLitOnSelected,
            Some(Layer::Row | Layer::Control) => Layer::ControlLit,
            Some(Layer::Chip) => return c.border_accent,
            _ => Layer::Control,
        };
        self.surface.layer(layer, c)
    }

    /// The line around an outline button.
    pub fn border(&self, c: &Colors) -> Option<Hsla> {
        (self.variant == Variant::Outline).then(|| self.surface.border(c))
    }

    pub fn radius(&self) -> f32 {
        if self.round { self.size.height() / 2. } else { self.size.radius() }
    }

    /// The corners: the joined sides stay square.
    pub fn corners(&self) -> Corners<Pixels> {
        let radius = px(self.radius());
        let leading = if self.joined.0 { px(0.) } else { radius };
        let trailing = if self.joined.1 { px(0.) } else { radius };
        Corners { top_left: leading, bottom_left: leading, top_right: trailing, bottom_right: trailing }
    }
}

/// The text with the dots between its parts muted, as "Opus 5.5 · Work".
pub fn dotted(text: impl Into<SharedString>, c: &Colors) -> StyledText {
    let text: SharedString = text.into();
    let muted = HighlightStyle { color: Some(c.muted_stronger_foreground), ..Default::default() };
    let dots: Vec<_> = text.match_indices(" · ").map(|(start, dot)| (start..start + dot.len(), muted)).collect();
    StyledText::new(text).with_highlights(dots)
}

/// Where the words of a label that fills sit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Alignment {
    Leading,
    #[default]
    Center,
}

/// What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands
/// where the symbol is, or over the words when there is none, and the width stays. A symbol that
/// turns turns itself instead. With a `pending_title` it says that beside the spinner instead.
pub struct ControlLabel {
    pub title: Option<SharedString>,
    pub icon: Option<ControlIcon>,
    pub size: ControlSize,
    pub chevron: bool,
    pub pending: bool,
    pub pending_title: Option<SharedString>,
    pub fills: bool,
    /// Where its words sit when it fills.
    pub alignment: Alignment,
    /// The symbol's or the picture's size where it isn't the one that goes with `size`.
    pub symbol_size: Option<f32>,
    /// The room between its symbol and its words where it isn't the one that goes with `size`.
    pub gap: Option<f32>,
    /// The room between its words and its sides where it isn't the one that goes with `size`.
    pub padding: Option<f32>,
    /// Words cut short lose their middle, as a branch's name does, rather than their end.
    pub middle: bool,
    /// Its digits are all one width, as a count that changes.
    pub tabular: bool,
}

impl ControlLabel {
    pub fn new(size: ControlSize) -> Self {
        Self {
            title: None,
            icon: None,
            size,
            chevron: false,
            pending: false,
            pending_title: None,
            fills: false,
            alignment: Alignment::Center,
            symbol_size: None,
            gap: None,
            padding: None,
            middle: false,
            tabular: false,
        }
    }

    fn wordless(&self) -> bool {
        self.title.is_none() && !self.chevron
    }

    fn mark_size(&self) -> f32 {
        self.symbol_size.unwrap_or(self.size.symbol())
    }

    /// How wide it is drawn when nothing cuts its words short, as `render` lays it out.
    pub fn natural_width(&self, window: &mut Window) -> f32 {
        let size = self.size;
        let wordless = self.wordless();
        let padding = self.padding.unwrap_or(size.padding());
        let leading = if wordless { 0. } else { padding - if self.icon.is_some() { size.symbol_outset() } else { 0. } };
        let trailing = if wordless { 0. } else { padding - if self.chevron { size.chevron_outset() } else { 0. } };
        let mark = self.icon.as_ref().map(|icon| match icon {
            ControlIcon::Picture(_) => self.symbol_size.unwrap_or(size.symbol_side()),
            ControlIcon::Symbol(_) => crate::theme::symbol_side(self.mark_size()),
        });
        let words =
            self.title.as_ref().map(|title| crate::ui::text_width(title, size.text_size(), FontWeight::MEDIUM, window));
        let stack = match (mark, words) {
            (Some(mark), Some(words)) => mark + self.gap.unwrap_or(size.gap()) + words,
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => 0.,
        };
        let has_stack = mark.is_some() || words.is_some();
        let chevron = match self.chevron {
            true => crate::theme::symbol_side(size.text_size()) * 14. / 24. + if has_stack { size.gap() } else { 0. },
            false => 0.,
        };
        (leading + self.picture_inset() + stack + chevron + trailing).max(size.height())
    }

    /// A picture smaller than the square stays centred where the square would be.
    fn picture_inset(&self) -> f32 {
        match (&self.icon, self.symbol_size) {
            (Some(ControlIcon::Picture(_)), Some(size)) => (self.size.symbol_side() - size) / 2.,
            _ => 0.,
        }
    }

    pub fn render(self, cx: &App) -> Div {
        let c = colors(cx);
        let size = self.size;
        let wordless = self.wordless();
        let has_icon = self.icon.is_some();
        let mark_size = self.mark_size();
        let side_padding = self.padding.unwrap_or(size.padding());
        let picture_inset = self.picture_inset();
        let picture_side = self.symbol_size.unwrap_or(size.symbol_side());
        let gap = self.gap.unwrap_or(size.gap());
        let waiting_title = if self.pending { self.pending_title.clone() } else { None };
        let spins = self.pending && has_icon;
        let middle = self.middle;
        let tabular = self.tabular;
        let words = |title: Option<SharedString>, spins: bool, icon: Option<ControlIcon>, hidden: bool| {
            let turning = matches!(&icon, Some(ControlIcon::Symbol(symbol)) if icons::turns(symbol));
            div()
                .flex()
                .items_center()
                .gap(px(gap))
                .when(spins && !turning, |row| row.child(Spinner::new(mark_size).render(cx)))
                .when(!spins || turning, |row| {
                    row.when_some(icon, |row, icon| match icon {
                        ControlIcon::Symbol(name) if turning => row.child(turning_symbol(name, mark_size, spins)),
                        ControlIcon::Symbol(name) => row.child(icons::symbol(name, mark_size)),
                        ControlIcon::Picture(picture) => row.child(
                            div()
                                .size(px(picture_side))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .child(picture),
                        ),
                    })
                })
                .when_some(title, |row, title| {
                    row.child(
                        div()
                            .min_w_0()
                            .text_size(px(size.text_size()))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .map(|text| if middle { text.text_ellipsis_middle() } else { text.text_ellipsis() })
                            .when(tabular, |text| text.font_features(crate::ui::tabular_numbers()))
                            .when(hidden, |text| text.opacity(0.))
                            .child(dotted(title, c)),
                    )
                })
        };
        let overlay_spinner = self.pending && !has_icon && waiting_title.is_none();
        let text_hidden = self.pending && !spins;
        let leading = if wordless { 0. } else { side_padding - if has_icon { size.symbol_outset() } else { 0. } };
        let trailing = if wordless { 0. } else { side_padding - if self.chevron { size.chevron_outset() } else { 0. } };
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(size.gap()))
            .when(self.fills, |label| {
                label.w_full().when(self.alignment == Alignment::Center, |label| label.justify_center())
            })
            .min_w(px(size.height()))
            .h(px(size.height()))
            .pl(px(leading + picture_inset))
            .pr(px(trailing))
            .when(wordless, |label| label.justify_center())
            .when(has_icon || self.title.is_some(), |label| {
                let stack = div()
                    .relative()
                    .min_w_0()
                    .child(
                        words(self.title.clone(), spins, self.icon, text_hidden)
                            .when(waiting_title.is_some(), |w| w.opacity(0.)),
                    )
                    .when_some(waiting_title, |stack, waiting| {
                        stack.child(div().absolute().inset_0().flex().items_center().child(words(
                            Some(waiting),
                            true,
                            None,
                            false,
                        )))
                    });
                label.child(stack)
            })
            .when(self.chevron, |label| {
                label.child(icons::trimmed_chevron(size.text_size()).text_color(c.muted_stronger_foreground))
            })
            .when(overlay_spinner, |label| {
                label.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Spinner::new(mark_size).render(cx)),
                )
            })
    }
}

/// A symbol that turns while `turning`, as an arrow that goes round does in place of the spinner,
/// and finishes the turn it is on once that ends.
pub fn turning_symbol(name: SharedString, size: f32, turning: bool) -> TurningSymbol {
    TurningSymbol { name, size, turning }
}

#[derive(IntoElement)]
pub struct TurningSymbol {
    name: SharedString,
    size: f32,
    turning: bool,
}

/// Where a turning symbol's angle comes from: the angle it was at when `turning` last changed,
/// and since when.
struct Turn {
    turning: bool,
    from: f32,
    since: std::time::Instant,
}

impl Turn {
    /// Its angle now, in degrees: going on while it turns, and stopped at the next whole turn
    /// once it doesn't.
    fn angle(&self) -> f32 {
        let turned = self.from + 360. * self.since.elapsed().as_secs_f32() / Spinner::TURN;
        if self.turning { turned } else { turned.min((self.from / 360.).ceil() * 360.) }
    }
}

impl RenderOnce for TurningSymbol {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = SharedString::from(format!("turning-{}", self.name));
        let turning = self.turning;
        let state =
            window.use_keyed_state(key, cx, |_, _| Turn { turning, from: 0., since: std::time::Instant::now() });
        if state.read(cx).turning != turning {
            state.update(cx, |turn, _| {
                let from = turn.angle() % 360.;
                *turn = Turn { turning, from, since: std::time::Instant::now() };
            });
        }
        let angle = state.read(cx).angle();
        let stopped = !turning && angle % 360. == 0.;
        if !stopped {
            window.request_animation_frame();
        }
        icons::symbol(self.name, self.size).rotate(if stopped { 0. } else { angle % 360. })
    }
}

/// How far an inline button's light reaches above and below its words.
const INLINE_LIGHT_ROOM: f32 = 3.;

/// The button. It says what it does with words, a symbol or both, in one of three sizes. While
/// `pending` it shows the spinner and takes no presses. `margin` is room around it that is its
/// own, so that neighbours leave no gap to miss.
#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    help: Option<SharedString>,
    look: Look,
    label: ControlLabel,
    margin: Edges<f32>,
    disabled: bool,
    /// It gives up room when there is too little, cutting its words short.
    truncates: bool,
    /// It stands in a line of text: its symbol or words start where the line does, and it is no
    /// taller than its words, so that nothing moves for it. Its light and its press reach around.
    inline: bool,
    on_click: Option<OnClick>,
}

impl ActionButton {
    /// A button with words, and a symbol before them when it has one.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        let title: SharedString = title.into();
        let mut label = ControlLabel::new(ControlSize::Regular);
        label.title = (!title.is_empty()).then_some(title);
        Self {
            id: id.into(),
            help: None,
            look: Look::new(Variant::Secondary, ControlSize::Regular),
            label,
            margin: Edges::default(),
            disabled: false,
            truncates: false,
            inline: false,
            on_click: None,
        }
    }

    /// A button that is only a symbol says what it does in `help`.
    pub fn icon(id: impl Into<ElementId>, symbol: impl Into<SharedString>, help: impl Into<SharedString>) -> Self {
        let mut button = Self::new(id, "");
        button.label.icon = Some(ControlIcon::Symbol(symbol.into()));
        button.help = Some(help.into());
        button.look.variant = Variant::Ghost;
        button
    }

    /// The half of a split button that opens what the other half doesn't do: only a chevron.
    pub fn chevron(id: impl Into<ElementId>, help: impl Into<SharedString>) -> Self {
        let mut button = Self::new(id, "");
        button.help = Some(help.into());
        button.label.chevron = true;
        button.look.variant = Variant::Ghost;
        button
    }

    pub fn symbol(mut self, symbol: impl Into<SharedString>) -> Self {
        self.label.icon = Some(ControlIcon::Symbol(symbol.into()));
        self
    }

    pub fn picture(mut self, picture: impl IntoElement) -> Self {
        self.label.icon = Some(ControlIcon::Picture(picture.into_any_element()));
        self
    }

    pub fn help(mut self, help: impl Into<SharedString>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.look.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.look.size = size;
        self.label.size = size;
        self
    }

    pub fn small(self) -> Self {
        self.size(ControlSize::Small)
    }

    pub fn large(self) -> Self {
        self.size(ControlSize::Large)
    }

    pub fn primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    pub fn ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }

    pub fn outline(self) -> Self {
        self.variant(Variant::Outline)
    }

    pub fn overlay(self) -> Self {
        self.variant(Variant::Overlay)
    }

    pub fn danger(self) -> Self {
        self.variant(Variant::Danger)
    }

    pub fn link(self) -> Self {
        self.variant(Variant::Link)
    }

    /// The symbol in another size in a button of the same size.
    pub fn symbol_size(mut self, size: f32) -> Self {
        self.label.symbol_size = Some(size);
        self
    }

    /// The room between its symbol and its words where it isn't the one that goes with its size.
    pub fn gap(mut self, gap: f32) -> Self {
        self.label.gap = Some(gap);
        self
    }

    /// The room between its words and its sides where it isn't the one that goes with its size.
    pub fn padding(mut self, padding: f32) -> Self {
        self.label.padding = Some(padding);
        self
    }

    pub fn pending(mut self, pending: bool) -> Self {
        self.label.pending = pending;
        self
    }

    pub fn pending_title(mut self, title: impl Into<SharedString>) -> Self {
        self.label.pending_title = Some(title.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.look.selected = selected;
        self
    }

    pub fn round(mut self, round: bool) -> Self {
        self.look.round = round;
        self
    }

    /// It stretches to the width it is given.
    pub fn fills(mut self, fills: bool) -> Self {
        self.label.fills = fills;
        self
    }

    /// Where its words sit when it fills.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.label.alignment = alignment;
        self
    }

    /// It opens something to choose from, and shows a chevron for it.
    pub fn opens(mut self, opens: bool) -> Self {
        self.label.chevron = opens;
        self
    }

    pub fn joined(mut self, leading: bool, trailing: bool) -> Self {
        self.look.joined = (leading, trailing);
        self
    }

    pub fn tint(mut self, tint: Option<Hsla>) -> Self {
        self.look.tint = tint;
        self
    }

    /// The layer the button lies on.
    pub fn surface(mut self, surface: Surface) -> Self {
        self.look.surface = surface;
        self
    }

    /// The row it lies on, as it is lit, so that it lights a step above it.
    pub fn row(mut self, row: Option<Layer>) -> Self {
        self.look.row = row;
        self
    }

    pub fn margin(mut self, margin: Edges<f32>) -> Self {
        self.margin = margin;
        self
    }

    /// It gives up room when there is too little, cutting its words short.
    pub fn truncates(mut self, truncates: bool) -> Self {
        self.truncates = truncates;
        self
    }

    /// It stands in a line of text, taking no more room than its symbol and words.
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }

    /// The room its light takes around its symbol and words when inline, which the line doesn't
    /// give it: before them, and above and below.
    fn inline_bleed(&self, window: &Window) -> (f32, f32) {
        let size = self.label.size;
        let line = window.text_style().line_height.to_pixels(px(size.text_size()).into(), window.rem_size());
        let vertical = (size.height() - f32::from(line)) / 2.;
        let leading = match self.label.wordless() {
            true => (size.height() - size.symbol_side()) / 2.,
            false => {
                self.label.padding.unwrap_or(size.padding())
                    - if self.label.icon.is_some() { size.symbol_outset() } else { 0. }
            }
        };
        (leading, vertical.max(0.))
    }

    /// How wide it is with its margin when nothing cuts its words short.
    pub fn natural_width(&self, window: &mut Window, cx: &App) -> f32 {
        let border = if self.look.border(colors(cx)).is_some() { 2. } else { 0. };
        self.margin.left + self.margin.right + self.label.natural_width(window) + border
    }

    /// Its digits are all one width, as a count that changes.
    pub fn monospaced_digits(mut self) -> Self {
        self.label.tabular = true;
        self
    }

    /// It gives up room when there is too little, cutting the middle of its words, as a branch's
    /// name is.
    pub fn truncates_middle(mut self) -> Self {
        self.truncates = true;
        self.label.middle = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ActionButton {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.disabled |= crate::ui::inert(cx);
        let look = self.look;
        let pending = self.label.pending;
        let fills = self.label.fills;
        let active = !self.disabled && !pending;
        let hovered = crate::ui::hover_state(&self.id, window, cx);
        let c = colors(cx);
        let group: SharedString = format!("control-{:?}", self.id).into();
        let fill = look.fill(false, c);
        let lit = look.fill(true, c);
        let foreground = look.foreground(active && *hovered.read(cx), c);
        let corners = look.corners();
        let truncates = self.truncates;
        let bleed = self.inline.then(|| self.inline_bleed(window));
        // An inline button's light hugs its words, while its press keeps the control's height.
        let light_inset = bleed.map_or(0., |(_, vertical)| (vertical - INLINE_LIGHT_ROOM).max(0.));
        let label = self.label.render(cx).when(truncates, |label| label.min_w_0()).my(px(-light_inset));
        div()
            .id(self.id.clone())
            .group(group.clone())
            .when(truncates, |button| button.min_w_0().flex())
            .when(!truncates, |button| button.flex_shrink_0())
            .when(fills, |button| button.w_full())
            .pt(px(self.margin.top))
            .pl(px(self.margin.left))
            .pb(px(self.margin.bottom))
            .pr(px(self.margin.right))
            .when_some(bleed, |button, (leading, vertical)| button.ml(px(-leading)).my(px(-vertical)))
            .when(self.disabled && !pending, |button| button.opacity(c.opacity(Opacity::Disabled)))
            .cursor_default()
            .child(
                div()
                    .id("face")
                    .my(px(light_inset))
                    .when(fills, |inner| inner.w_full())
                    .when(truncates, |inner| inner.min_w_0())
                    .rounded_tl(corners.top_left)
                    .rounded_tr(corners.top_right)
                    .rounded_bl(corners.bottom_left)
                    .rounded_br(corners.bottom_right)
                    .bg(fill)
                    .when_some(look.border(c), |inner, border| inner.border_1().border_color(border))
                    .text_color(foreground)
                    .when(active, |inner| {
                        inner
                            .group_hover(group.clone(), move |inner| inner.bg(lit))
                            .group_active(group, move |inner| inner.bg(lit))
                    })
                    .child(label),
            )
            .on_hover(crate::ui::track_hover(&hovered))
            .when_some(self.help, |button, help| button.tooltip(tooltip(help)))
            .when_some(self.on_click.filter(|_| active), |button, on_click| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}

type BuildMenu = Rc<dyn Fn(&mut Window, &mut App) -> Menu>;
type OnChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type OnSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A menu that looks and is sized as the button is. With words it shows what is chosen, and a
/// chevron after it. The menu is built when it opens, under the button.
#[derive(IntoElement)]
pub struct ActionMenu {
    button: ActionButton,
    anchor: Anchor,
    build: BuildMenu,
    shortcut: Option<&'static str>,
}

impl ActionMenu {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        build: impl Fn(&mut Window, &mut App) -> Menu + 'static,
    ) -> Self {
        let button = ActionButton::new(id, title).variant(Variant::Ghost).opens(true);
        Self { button, anchor: Anchor::default(), build: Rc::new(build), shortcut: None }
    }

    pub fn icon(
        id: impl Into<ElementId>,
        symbol: impl Into<SharedString>,
        help: impl Into<SharedString>,
        build: impl Fn(&mut Window, &mut App) -> Menu + 'static,
    ) -> Self {
        Self {
            button: ActionButton::icon(id, symbol, help),
            anchor: Anchor::default(),
            build: Rc::new(build),
            shortcut: None,
        }
    }

    /// Changes the button the menu opens from.
    pub fn button(mut self, change: impl FnOnce(ActionButton) -> ActionButton) -> Self {
        self.button = change(self.button);
        self
    }

    /// Makes the button the one the command's keys press.
    pub fn shortcut(mut self, command: &'static str) -> Self {
        self.shortcut = Some(command);
        self
    }

    /// How wide its button is when nothing cuts its words short.
    pub fn natural_width(&self, window: &mut Window, cx: &App) -> f32 {
        self.button.natural_width(window, cx)
    }
}

impl RenderOnce for ActionMenu {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let anchor = self.anchor.clone();
        let opens = self.anchor.clone();
        let build = self.build;
        let _ = window;
        div()
            .relative()
            .flex_shrink_0()
            .child(anchor.track())
            .children(self.shortcut.map(crate::keyboard::target))
            .child(self.button.on_click(move |_, window, cx| {
                let menu = build(window, cx);
                if menu.is_empty() {
                    return;
                }
                menu.show_under(&opens, 4., window, cx);
            }))
    }
}

/// What is under way: Lucide's loader, turning, in the colour of the text around it.
pub struct Spinner {
    size: f32,
}

impl Spinner {
    /// The loader reaches every side of its square, so it is drawn smaller in it to look as
    /// large as the symbols it stands in for.
    const FILL: f32 = 0.85;
    /// How long a turn takes, in seconds.
    pub const TURN: f32 = 1.2;

    pub fn new(size: f32) -> Self {
        Self { size }
    }

    pub fn regular() -> Self {
        Self::new(ControlSize::Regular.symbol())
    }

    pub fn render(self, _: &App) -> impl IntoElement {
        let side = crate::theme::symbol_side(self.size);
        div().size(px(side)).flex_shrink_0().flex().items_center().justify_center().child(
            icons::symbol("loader", self.size * Self::FILL).with_animation(
                "spinner",
                Animation::new(std::time::Duration::from_secs_f32(Self::TURN)).repeat(),
                |icon, delta| icon.rotate(delta * 360.),
            ),
        )
    }
}

/// A word or two that says what something is: a state, a label, a name. With a `tone` it is in
/// that colour on a wash of it; without, it is bordered. With a `dot` the colour is the dot's and
/// the words stay plain.
#[derive(IntoElement)]
pub struct Chip {
    title: SharedString,
    icon: Option<SharedString>,
    dot: Option<Hsla>,
    tone: Option<Hsla>,
    monospaced: bool,
    surface: Surface,
    /// The row it lies on, as it is lit, which it stands a step above.
    row: Option<Layer>,
    /// Its words lose their middle when there is too little room, as a branch's name does.
    middle: bool,
    /// Its words can be selected and copied, as part of the window's selection.
    selectable: Option<ElementId>,
}

impl Chip {
    pub const HEIGHT: f32 = 20.;

    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            dot: None,
            tone: None,
            monospaced: false,
            surface: Surface::Background,
            row: None,
            middle: false,
            selectable: None,
        }
    }

    /// A branch's name: in the code font, cut in its middle, and selectable.
    pub fn branch(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        let mut chip = Self::new(name).monospaced(true);
        chip.middle = true;
        chip.selectable = Some(id.into());
        chip
    }

    pub fn icon(mut self, symbol: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol.into());
        self
    }

    pub fn dot(mut self, color: Hsla) -> Self {
        self.dot = Some(color);
        self
    }

    pub fn tone(mut self, color: Hsla) -> Self {
        self.tone = Some(color);
        self
    }

    pub fn monospaced(mut self, monospaced: bool) -> Self {
        self.monospaced = monospaced;
        self
    }

    /// The layer the chip lies on.
    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn row(mut self, row: Option<Layer>) -> Self {
        self.row = row;
        self
    }

    fn layer(&self) -> Layer {
        match self.row {
            Some(Layer::RowSelected | Layer::ControlLit | Layer::ControlLitOnSelected) => Layer::ControlLitOnSelected,
            Some(Layer::Row | Layer::Control | Layer::Chip) => Layer::ControlLit,
            Some(Layer::Box) | None => Layer::Control,
        }
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let foreground = if self.dot.is_some() { c.foreground } else { self.tone.unwrap_or(c.foreground) };
        let background = match self.tone {
            Some(tone) => c.wash(tone, false),
            None => self.surface.layer(self.layer(), c),
        };
        let words: AnyElement = match self.selectable {
            Some(id) => crate::ui::rich_text::RichText::new(id, self.title).into_any_element(),
            None => self.title.into_any_element(),
        };
        div()
            .when(!self.middle, |chip| chip.flex_shrink_0())
            .when(self.middle, |chip| chip.min_w_0())
            .h(px(Self::HEIGHT))
            .px(px(7.))
            .rounded_full()
            .bg(background)
            .when(self.tone.is_none(), |chip| chip.border_1().border_color(self.surface.border(c)))
            .flex()
            .items_center()
            .gap(px(5.))
            .text_color(foreground)
            .when_some(self.dot, |chip, dot| chip.child(div().size(px(7.)).rounded_full().bg(dot).flex_shrink_0()))
            .when_some(self.icon, |chip, icon| chip.child(icons::symbol(icon, 11.)))
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .when(self.middle, |text| text.min_w_0().overflow_hidden().text_ellipsis_middle())
                    .when(self.monospaced, |text| text.font_family(crate::theme::MONO_FONT))
                    .child(words),
            )
    }
}

/// On or off: the Mac's small switch, in the primary colour while it is on. Off, its track is
/// the emphasized foreground's wash, so it shows on every surface as the system's does.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    disabled: bool,
    on_change: Option<OnChange>,
}

impl Switch {
    const HEIGHT: f32 = 18.;
    const WIDTH: f32 = 44.;
    const KNOB_WIDTH: f32 = 26.;
    const INSET: f32 = 2.;
    /// How long the knob takes to slide across, as the system's does.
    const SLIDE: std::time::Duration = std::time::Duration::from_millis(200);

    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self { id: id.into(), on, disabled: false, on_change: None }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let on = self.on;
        let disabled = self.disabled || crate::ui::inert(cx);
        let id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "knob".into());
        let shown = motion::eased(id, if on { 1. } else { 0. }, Self::SLIDE, window, cx);
        let track = motion::mix(c.wash(c.emphasized_foreground, false), c.primary, shown);
        // The system's knob is a little grey while off in the dark.
        let off_knob = if c.dark { c.at(c.primary_foreground, Opacity::Lit) } else { c.primary_foreground };
        let knob = motion::mix(off_knob, c.primary_foreground, shown);
        let travel = Self::WIDTH - Self::KNOB_WIDTH - 2. * Self::INSET;
        div()
            .id(self.id)
            .flex_shrink_0()
            .w(px(Self::WIDTH))
            .h(px(Self::HEIGHT))
            .rounded_full()
            .bg(track)
            .relative()
            .when(disabled, |switch| switch.opacity(c.opacity(Opacity::Disabled)))
            .child(
                div()
                    .absolute()
                    .top(px(Self::INSET))
                    .left(px(Self::INSET + travel * shown))
                    .w(px(Self::KNOB_WIDTH))
                    .h(px(Self::HEIGHT - 2. * Self::INSET))
                    .rounded_full()
                    .bg(knob)
                    .shadow(c.shadow(crate::theme::ShadowSize::Sm, Opacity::Shadow)),
            )
            .when_some(self.on_change.filter(|_| !disabled), |switch, on_change| {
                switch.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_change(!on, window, cx)
                })
            })
    }
}

/// A short row of choices of which one is on, in a track on the background with the border
/// around it. The choices touch each other and the track's edge, so no click falls between them;
/// their light is drawn inset from their edges, as the sidebar's rows are.
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    options: Vec<SharedString>,
    selected: usize,
    size: ControlSize,
    fills: bool,
    disabled: bool,
    on_select: Option<OnSelect>,
}

impl Segmented {
    /// How far a choice's light stands from the track's border, and from the next one's.
    pub const INSET: f32 = 4.;
    pub const BORDER: f32 = 1.;

    pub fn new(id: impl Into<ElementId>, options: Vec<SharedString>, selected: usize) -> Self {
        Self {
            id: id.into(),
            options,
            selected,
            size: ControlSize::Regular,
            fills: false,
            disabled: false,
            on_select: None,
        }
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// The choices share the whole width they are given.
    pub fn fills(mut self, fills: bool) -> Self {
        self.fills = fills;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled || crate::ui::inert(cx);
        let hovers: Vec<Entity<bool>> = (0..self.options.len())
            .map(|index| crate::ui::hover_state(&(self.id.clone(), format!("segment-{index}")).into(), window, cx))
            .collect();
        let lights: Vec<f32> = (0..self.options.len())
            .map(|index| {
                let id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), format!("light-{index}").into());
                motion::eased(id, if index == self.selected { 1. } else { 0. }, motion::QUICK, window, cx)
            })
            .collect();
        let c = colors(cx);
        let size = self.size;
        let count = self.options.len();
        let on_select = self.on_select.filter(|_| !disabled);
        let half = Self::INSET / 2.;
        let group_of = |index: usize| SharedString::from(format!("segment-{:?}-{index}", self.id));
        div()
            .id(self.id.clone())
            .flex_shrink_0()
            .when(self.fills, |track| track.w_full())
            .flex()
            .items_center()
            .bg(c.background)
            .border_1()
            .border_color(c.border)
            .rounded(px(size.radius() + Self::INSET + Self::BORDER))
            .when(disabled, |track| track.opacity(c.opacity(Opacity::Disabled)))
            .children(self.options.into_iter().enumerate().map(|(index, title)| {
                let selected = index == self.selected;
                let on_select = on_select.clone();
                let margin = crate::ui::edges(
                    Self::INSET,
                    if index == 0 { Self::INSET } else { half },
                    Self::INSET,
                    if index == count - 1 { Self::INSET } else { half },
                );
                let group = group_of(index);
                let hovered = *hovers[index].read(cx) && !disabled;
                let light = lights[index];
                // The light of the one chosen fades from the last one chosen to it.
                let hover = crate::ui::Highlight::new()
                    .radius(size.radius())
                    .inset(margin)
                    .small(true)
                    .enabled(!disabled && !selected);
                let chosen = div()
                    .absolute()
                    .top(px(margin.top))
                    .left(px(margin.left))
                    .right(px(margin.right))
                    .bottom(px(margin.bottom))
                    .rounded(px(size.radius()))
                    .bg(Surface::Background.layer(Layer::ControlLit, c))
                    .opacity(light);
                let text = match (disabled, hovered) {
                    (true, _) => c.muted_stronger_foreground,
                    (false, true) => c.foreground,
                    (false, false) => motion::mix(c.muted_foreground, c.foreground, light),
                };
                div()
                    .id(("segment", index))
                    .group(group.clone())
                    .relative()
                    .when(self.fills, |segment| segment.flex_1())
                    .pt(px(margin.top))
                    .pb(px(margin.bottom))
                    .pl(px(margin.left))
                    .pr(px(margin.right))
                    .child(hover.render(group.clone(), cx))
                    .when(light > 0., |segment| segment.child(chosen))
                    .on_hover(crate::ui::track_hover(&hovers[index]))
                    .child(
                        div()
                            .relative()
                            .h(px(size.height()))
                            .px(px(size.padding()))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(size.text_size()))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .text_color(text)
                            .child(title),
                    )
                    .when_some(on_select, |segment, on_select| {
                        segment.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_select(index, window, cx)
                        })
                    })
            }))
    }
}

/// A card: a box of its own with its edge around it, like a group of settings, a table or a
/// note. What is inside lies on `Surface::Card`.
pub fn card(element: Div, radius: f32, cx: &App) -> Div {
    let c = colors(cx);
    element.bg(c.card).border_1().border_color(c.border_card).rounded(px(radius))
}

pub fn card_default(element: Div, cx: &App) -> Div {
    card(element, Radius::LG, cx)
}

/// How a field is set off from what it lies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputVariant {
    /// A fill alone.
    #[default]
    Filled,
    /// No background or height of its own, for a field that lies on glass, with its symbol in the
    /// foreground.
    Bare,
}

/// The fill of a field: the input colour, or on a card, which is as dark as input, the card's
/// accent.
fn field_fill(surface: Surface, c: &Colors) -> Hsla {
    if surface == Surface::Card { surface.layer(Layer::Box, c) } else { c.input }
}

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`, as Esc does while it has text. A
/// `secure` one shows dots for what is typed.
#[derive(IntoElement)]
pub struct InputField {
    state: Entity<component::input::InputState>,
    icon: Option<SharedString>,
    variant: InputVariant,
    size: ControlSize,
    clearable: bool,
    monospaced: bool,
    secure: bool,
    surface: Surface,
    /// What is typed is this large instead of the size's text, like a search that is what a
    /// panel is about.
    text_size: Option<f32>,
}

impl InputField {
    pub fn new(state: &Entity<component::input::InputState>) -> Self {
        Self {
            state: state.clone(),
            icon: None,
            variant: InputVariant::Filled,
            size: ControlSize::Regular,
            clearable: false,
            monospaced: false,
            secure: false,
            surface: Surface::Background,
            text_size: None,
        }
    }

    pub fn text_size(mut self, text_size: f32) -> Self {
        self.text_size = Some(text_size);
        self
    }

    pub fn icon(mut self, symbol: impl Into<SharedString>) -> Self {
        self.icon = Some(symbol.into());
        self
    }

    pub fn variant(mut self, variant: InputVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    pub fn monospaced(mut self, monospaced: bool) -> Self {
        self.monospaced = monospaced;
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }
}

impl RenderOnce for InputField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = self.size;
        let bare = self.variant == InputVariant::Bare;
        let fill = if bare { transparent_black() } else { field_fill(self.surface, c) };
        if self.state.read(cx).presentation().is_masked() != self.secure {
            let secure = self.secure;
            self.state.update(cx, |state, cx| state.set_masked(secure, window, cx));
        }
        let empty = self.state.read(cx).value().is_empty();
        let clears = self.clearable && !empty;
        let clear = self.state.clone();
        let escape = self.state.clone();
        let focus = self.state.read(cx).focus_handle(cx);
        let clear_inset = (size.height() - ControlSize::Small.height()) / 2.;
        div()
            .w_full()
            .when(!bare, |field| field.h(px(size.height())))
            .when(bare, |field| field.h_full())
            .px(px(size.padding() - 2.))
            .rounded(px(size.radius()))
            .bg(fill)
            .flex()
            .items_center()
            .gap(px(size.gap()))
            .cursor_text()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| focus.focus(window, cx))
            .when(clears, |field| {
                field.on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    cx.stop_propagation();
                    escape.update(cx, |state, cx| clear_field(state, window, cx));
                })
            })
            .when_some(self.icon, |field, icon| {
                let tint = if bare { c.foreground } else { c.muted_stronger_foreground };
                field.child(div().text_color(tint).flex_shrink_0().child(icons::symbol(icon, size.small_symbol())))
            })
            .child(
                div().flex_1().min_w_0().child(
                    component::input::Input::new(&self.state)
                        .appearance(false)
                        .px_0()
                        .py_0()
                        .h(px(size.height() - 2.))
                        .text_size(px(self.text_size.unwrap_or(size.text_size() + 0.5)))
                        .when(self.monospaced, |input| input.font_family(crate::theme::MONO_FONT)),
                ),
            )
            .when(clears, |field| {
                field.child(
                    ActionButton::icon("clear", "x", "Clear")
                        .small()
                        .surface(self.surface)
                        .margin(Edges {
                            top: 0.,
                            left: 0.,
                            bottom: 0.,
                            right: (clear_inset - size.padding() + 2.).max(0.),
                        })
                        .on_click(move |_, window, cx| {
                            clear.update(cx, |state, cx| clear_field(state, window, cx));
                        }),
                )
            })
    }
}

/// Empties the field, and says so as typing would.
fn clear_field(
    state: &mut component::input::InputState,
    window: &mut Window,
    cx: &mut Context<component::input::InputState>,
) {
    state.set_value("", window, cx);
    cx.emit(component::input::InputEvent::Change);
}

/// The state of a `TextArea`: `lines` tall while empty, growing with what is written up to
/// `max_lines`, past which it scrolls.
pub fn text_area_state(
    placeholder: impl Into<SharedString>,
    lines: usize,
    max_lines: usize,
    window: &mut Window,
    cx: &mut App,
) -> Entity<component::input::TextareaState> {
    let placeholder = placeholder.into();
    cx.new(|cx| {
        component::input::TextareaState::new(window, cx).placeholder(placeholder).auto_grow(lines, max_lines.max(lines))
    })
}

/// A box to write more than a line in, in a field's frame. One that `fills` is as tall as there
/// is room instead. Return is a line break.
#[derive(IntoElement)]
pub struct TextArea {
    state: Entity<component::input::TextareaState>,
    size: ControlSize,
    monospaced: bool,
    fills: bool,
    surface: Surface,
}

impl TextArea {
    pub fn new(state: &Entity<component::input::TextareaState>) -> Self {
        Self {
            state: state.clone(),
            size: ControlSize::Regular,
            monospaced: false,
            fills: false,
            surface: Surface::Background,
        }
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn monospaced(mut self, monospaced: bool) -> Self {
        self.monospaced = monospaced;
        self
    }

    pub fn fills(mut self, fills: bool) -> Self {
        self.fills = fills;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }
}

impl RenderOnce for TextArea {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = self.size;
        div()
            .w_full()
            .when(self.fills, |area| area.h_full())
            .px(px(size.padding() - 2.))
            .py(px((size.padding() - 4.).max(0.)))
            .rounded(px(size.radius()))
            .bg(field_fill(self.surface, c))
            .cursor_text()
            .child(
                component::input::Textarea::new(&self.state)
                    .appearance(false)
                    .when(self.fills, |area| area.h(relative(1.)))
                    .text_size(px(size.text_size() + 0.5))
                    .when(self.monospaced, |area| area.font_family(crate::theme::MONO_FONT)),
            )
    }
}
