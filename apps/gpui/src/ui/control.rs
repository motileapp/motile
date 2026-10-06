//! The controls every view is made of, as the Mac app's `Views/UI` draws them: the button and the
//! menu that looks like it, the spinner, the chip, the switch, the segmented choice, the field
//! and the card. Their sizes are a `ControlSize` and their corners a `Radius`; nothing here is
//! styled by hand.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Radius, Surface, colors};
use crate::ui::icons;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{OnClick, tooltip};

/// What a button is for, which is how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Variant {
    /// The one thing the view calls for.
    Primary,
    /// The others beside it.
    #[default]
    Secondary,
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
}

/// What a button shows before its words: a symbol, or a picture of its own like a logo.
pub enum ControlIcon {
    Symbol(SharedString),
    Picture(AnyElement),
}

/// `overlay` over `base`, both opaque enough to be seen.
pub fn over(base: Hsla, overlay: Hsla) -> Hsla {
    base.blend(overlay)
}

/// The colours and the shape of a button or a menu. Pressed looks as under the pointer does.
#[derive(Clone, Copy)]
pub struct Look {
    pub variant: Variant,
    pub size: ControlSize,
    pub selected: bool,
    pub surface: Surface,
    /// Round ends, for the composer's buttons.
    pub round: bool,
    /// The sides another control touches, which stay square: (leading, trailing).
    pub joined: (bool, bool),
    /// A colour that says something, like a pull request's state, in place of the quiet ones.
    pub tint: Option<Hsla>,
    /// Only a symbol: too small for the next layer to show under the pointer.
    pub wordless: bool,
}

impl Look {
    pub fn new(variant: Variant, size: ControlSize) -> Self {
        Self {
            variant,
            size,
            selected: false,
            surface: Surface::Background,
            round: false,
            joined: (false, false),
            tint: None,
            wordless: false,
        }
    }

    pub fn foreground(&self, lit: bool, c: &crate::theme::Colors) -> Hsla {
        if let Some(tint) = self.tint
            && matches!(self.variant, Variant::Ghost | Variant::Secondary)
        {
            return tint;
        }
        match self.variant {
            Variant::Secondary => c.text,
            Variant::Primary | Variant::Danger => white(),
            Variant::Warning => c.background,
            Variant::Ghost => {
                if self.selected || lit {
                    c.text
                } else {
                    c.secondary
                }
            }
            Variant::Link | Variant::Accent => c.link,
        }
    }

    pub fn fill(&self, lit: bool, c: &crate::theme::Colors) -> Hsla {
        let wash = |color: Hsla| if lit { over(color, hsla(0., 0., 1., 0.12)) } else { color };
        match self.variant {
            Variant::Primary => wash(c.primary),
            Variant::Secondary => {
                if self.selected || lit {
                    self.surface.further().color(c)
                } else {
                    self.surface.next().color(c)
                }
            }
            Variant::Danger => wash(c.danger_fill),
            Variant::Warning => wash(c.warning),
            Variant::Ghost => {
                if self.selected {
                    self.surface.further().color(c)
                } else if lit {
                    self.ghost_lit(c)
                } else {
                    transparent_black()
                }
            }
            Variant::Link => {
                if lit {
                    c.link_hover
                } else {
                    transparent_black()
                }
            }
            Variant::Accent => c.link.opacity(if lit { 0.22 } else { 0.14 }),
        }
    }

    fn ghost_lit(&self, c: &crate::theme::Colors) -> Hsla {
        if !self.wordless {
            return self.surface.next().color(c);
        }
        match self.surface {
            Surface::Background => Surface::Tertiary.color(c),
            Surface::Popover => c.border_secondary,
            _ => self.surface.next().color(c),
        }
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

/// What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands
/// where the symbol is, or over the words when there is none, and the width stays. With a
/// `pending_title` it says that beside the spinner instead.
pub struct ControlLabel {
    pub title: Option<SharedString>,
    pub icon: Option<ControlIcon>,
    pub size: ControlSize,
    pub chevron: bool,
    pub pending: bool,
    pub pending_title: Option<SharedString>,
    pub fills: bool,
    /// The symbol's size where it isn't the one that goes with `size`.
    pub symbol_size: Option<f32>,
}

impl ControlLabel {
    fn wordless(&self) -> bool {
        self.title.is_none() && !self.chevron
    }

    fn mark_size(&self) -> f32 {
        self.symbol_size.unwrap_or(self.size.symbol())
    }

    pub fn render(self, cx: &App) -> Div {
        let size = self.size;
        let wordless = self.wordless();
        let has_icon = self.icon.is_some();
        let mark_size = self.mark_size();
        let waiting_title = if self.pending { self.pending_title.clone() } else { None };
        let spins = self.pending && has_icon;
        let words = |title: Option<SharedString>, spins: bool, icon: Option<ControlIcon>, hidden: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(size.gap()))
                .when(spins, |row| row.child(Spinner::new(mark_size).render(cx)))
                .when(!spins, |row| {
                    row.when_some(icon, |row, icon| match icon {
                        ControlIcon::Symbol(name) => row.child(icons::symbol(name, mark_size)),
                        ControlIcon::Picture(picture) => row.child(
                            div()
                                .size(px(size.symbol_side()))
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
                            .text_size(px(size.text_size()))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .when(hidden, |text| text.opacity(0.))
                            .child(title),
                    )
                })
        };
        let overlay_spinner = self.pending && !has_icon && waiting_title.is_none();
        let text_hidden = self.pending && !spins;
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(size.gap()))
            .when(self.fills, |label| label.w_full().justify_center())
            .min_w(px(size.height()))
            .h(px(size.height()))
            .pl(px(if wordless { 0. } else { size.padding() - if has_icon { size.symbol_outset() } else { 0. } }))
            .pr(px(if wordless { 0. } else { size.padding() }))
            .when(wordless, |label| label.justify_center())
            .when(has_icon || self.title.is_some(), |label| {
                let stack = div()
                    .relative()
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
                label.child(div().opacity(0.6).child(icons::symbol("chevron-down", size.text_size())))
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

/// The button. It says what it does with words, a symbol or both, in one of three sizes. While
/// `pending` it shows the spinner and takes no presses. `margin` is room around it that is its
/// own, so that neighbours leave no gap to miss.
#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    title: Option<SharedString>,
    icon: Option<ControlIcon>,
    help: Option<SharedString>,
    look: Look,
    chevron: bool,
    pending: bool,
    pending_title: Option<SharedString>,
    fills: bool,
    symbol_size: Option<f32>,
    margin: Edges<f32>,
    disabled: bool,
    on_click: Option<OnClick>,
}

impl ActionButton {
    /// A button with words, and a symbol before them when it has one.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        let title: SharedString = title.into();
        Self {
            id: id.into(),
            title: (!title.is_empty()).then_some(title),
            icon: None,
            help: None,
            look: Look::new(Variant::Secondary, ControlSize::Regular),
            chevron: false,
            pending: false,
            pending_title: None,
            fills: false,
            symbol_size: None,
            margin: Edges::default(),
            disabled: false,
            on_click: None,
        }
    }

    /// A button that is only a symbol says what it does in `help`.
    pub fn icon(id: impl Into<ElementId>, symbol: impl Into<SharedString>, help: impl Into<SharedString>) -> Self {
        let mut button = Self::new(id, "");
        button.icon = Some(ControlIcon::Symbol(symbol.into()));
        button.help = Some(help.into());
        button.look = Look::new(Variant::Ghost, ControlSize::Regular);
        button.look.wordless = true;
        button
    }

    /// The half of a split button that opens what the other half doesn't do: only a chevron.
    pub fn chevron(id: impl Into<ElementId>, help: impl Into<SharedString>) -> Self {
        let mut button = Self::new(id, "");
        button.help = Some(help.into());
        button.chevron = true;
        button.look = Look::new(Variant::Ghost, ControlSize::Regular);
        button
    }

    pub fn symbol(mut self, symbol: impl Into<SharedString>) -> Self {
        self.icon = Some(ControlIcon::Symbol(symbol.into()));
        self
    }

    pub fn picture(mut self, picture: impl IntoElement) -> Self {
        self.icon = Some(ControlIcon::Picture(picture.into_any_element()));
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

    pub fn danger(self) -> Self {
        self.variant(Variant::Danger)
    }

    pub fn link(self) -> Self {
        self.variant(Variant::Link)
    }

    /// The symbol in another size in a button of the same size.
    pub fn symbol_size(mut self, size: f32) -> Self {
        self.symbol_size = Some(size);
        self
    }

    pub fn pending(mut self, pending: bool) -> Self {
        self.pending = pending;
        self
    }

    pub fn pending_title(mut self, title: impl Into<SharedString>) -> Self {
        self.pending_title = Some(title.into());
        self
    }

    pub fn round(mut self, round: bool) -> Self {
        self.look.round = round;
        self
    }

    /// It stretches to the width it is given.
    pub fn fills(mut self, fills: bool) -> Self {
        self.fills = fills;
        self
    }

    /// It opens something to choose from, and shows a chevron for it.
    pub fn opens(mut self, opens: bool) -> Self {
        self.chevron = opens;
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

    pub fn margin(mut self, margin: Edges<f32>) -> Self {
        self.margin = margin;
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let look = self.look;
        let active = !self.disabled && !self.pending;
        let group: SharedString = format!("control-{:?}", self.id).into();
        let label = ControlLabel {
            title: self.title,
            icon: self.icon,
            size: look.size,
            chevron: self.chevron,
            pending: self.pending,
            pending_title: self.pending_title,
            fills: self.fills,
            symbol_size: self.symbol_size,
        };
        let fill = look.fill(false, c);
        let lit = look.fill(true, c);
        let foreground = look.foreground(false, c);
        let lit_foreground = look.foreground(true, c);
        let corners = look.corners();
        div()
            .id(self.id)
            .group(group.clone())
            .flex_shrink_0()
            .when(self.fills, |button| button.w_full())
            .pt(px(self.margin.top))
            .pl(px(self.margin.left))
            .pb(px(self.margin.bottom))
            .pr(px(self.margin.right))
            .when(!self.disabled || self.pending, |button| button.opacity(1.))
            .when(self.disabled && !self.pending, |button| button.opacity(0.45))
            .when(active, |button| button.cursor_default())
            .child(
                div()
                    .when(self.fills, |inner| inner.w_full())
                    .rounded_tl(corners.top_left)
                    .rounded_tr(corners.top_right)
                    .rounded_bl(corners.bottom_left)
                    .rounded_br(corners.bottom_right)
                    .bg(fill)
                    .text_color(foreground)
                    .when(active, |inner| {
                        inner.group_hover(group, move |inner| inner.bg(lit).text_color(lit_foreground))
                    })
                    .child(label.render(cx)),
            )
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
}

impl ActionMenu {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        build: impl Fn(&mut Window, &mut App) -> Menu + 'static,
    ) -> Self {
        let button = ActionButton::new(id, title).variant(Variant::Ghost).opens(true);
        Self { button, anchor: Anchor::default(), build: Rc::new(build) }
    }

    pub fn icon(
        id: impl Into<ElementId>,
        symbol: impl Into<SharedString>,
        help: impl Into<SharedString>,
        build: impl Fn(&mut Window, &mut App) -> Menu + 'static,
    ) -> Self {
        Self { button: ActionButton::icon(id, symbol, help), anchor: Anchor::default(), build: Rc::new(build) }
    }

    /// Changes the button the menu opens from.
    pub fn button(mut self, change: impl FnOnce(ActionButton) -> ActionButton) -> Self {
        self.button = change(self.button);
        self
    }
}

impl RenderOnce for ActionMenu {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let anchor = self.anchor.clone();
        let opens = self.anchor.clone();
        let build = self.build;
        let _ = window;
        div().relative().flex_shrink_0().child(anchor.track()).child(self.button.on_click(move |_, window, cx| {
            let menu = build(window, cx);
            if menu.is_empty() {
                return;
            }
            menu.show(opens.below(), window, cx);
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
                Animation::new(std::time::Duration::from_millis(1200)).repeat(),
                |icon, delta| icon.rotate(delta * 360.),
            ),
        )
    }
}

/// A word or two that says what something is: a state, a label, a name. With a `tone` it is in
/// that colour on a wash of it; with a `dot` the colour is the dot's and the words stay plain.
#[derive(IntoElement)]
pub struct Chip {
    title: SharedString,
    icon: Option<SharedString>,
    dot: Option<Hsla>,
    tone: Option<Hsla>,
    monospaced: bool,
    surface: Surface,
}

impl Chip {
    pub const HEIGHT: f32 = 20.;

    pub fn new(title: impl Into<SharedString>) -> Self {
        Self { title: title.into(), icon: None, dot: None, tone: None, monospaced: false, surface: Surface::Background }
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
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let foreground = if self.dot.is_some() { c.text } else { self.tone.unwrap_or(c.text) };
        let background = self.tone.map(|tone| tone.opacity(0.14)).unwrap_or_else(|| self.surface.next().color(c));
        div()
            .flex_shrink_0()
            .h(px(Self::HEIGHT))
            .px(px(7.))
            .rounded_full()
            .bg(background)
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
                    .when(self.monospaced, |text| text.font_family(crate::theme::MONO_FONT))
                    .child(self.title),
            )
    }
}

/// On or off: a knob at one end of a track, which is in the primary colour while it is on.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    disabled: bool,
    surface: Surface,
    on_change: Option<OnChange>,
}

impl Switch {
    const HEIGHT: f32 = 20.;
    const WIDTH: f32 = 34.;
    const INSET: f32 = 2.;

    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self { id: id.into(), on, disabled: false, surface: Surface::Background, on_change: None }
    }

    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let on = self.on;
        let knob = Self::HEIGHT - 2. * Self::INSET;
        div()
            .id(self.id)
            .flex_shrink_0()
            .w(px(Self::WIDTH))
            .h(px(Self::HEIGHT))
            .rounded_full()
            .bg(if on { c.primary } else { self.surface.next().next().color(c) })
            .relative()
            .when(self.disabled, |switch| switch.opacity(0.5))
            .when(!self.disabled, |switch| switch.active(|switch| switch.opacity(0.85)))
            .child(
                div()
                    .absolute()
                    .top(px(Self::INSET))
                    .when(on, |knob| knob.right(px(Self::INSET)))
                    .when(!on, |knob| knob.left(px(Self::INSET)))
                    .size(px(knob))
                    .rounded_full()
                    .bg(white())
                    .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.18), 1., 2.)),
            )
            .when_some(self.on_change.filter(|_| !self.disabled), |switch, on_change| {
                switch.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_change(!on, window, cx)
                })
            })
    }
}

/// A short row of choices of which one is on, each a control of its size, in a track that is a
/// card around them: the one that is on stands on the border colour, the others are in the
/// secondary colour until the pointer is over them.
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    options: Vec<SharedString>,
    selected: usize,
    size: ControlSize,
    on_select: Option<OnSelect>,
}

impl Segmented {
    /// How far the choices stand from the track's edge, which has the border in it.
    pub const INSET: f32 = 4.;

    pub fn new(id: impl Into<ElementId>, options: Vec<SharedString>, selected: usize) -> Self {
        Self { id: id.into(), options, selected, size: ControlSize::Regular, on_select: None }
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = self.size;
        let on_select = self.on_select;
        card(div(), size.radius() + Self::INSET, cx)
            .id(self.id)
            .flex_shrink_0()
            .p(px(Self::INSET))
            .flex()
            .items_center()
            .gap(px(Self::INSET))
            .children(self.options.into_iter().enumerate().map(|(index, title)| {
                let selected = index == self.selected;
                let on_select = on_select.clone();
                div()
                    .id(("segment", index))
                    .h(px(size.height()))
                    .px(px(size.padding()))
                    .rounded(px(size.radius()))
                    .flex()
                    .items_center()
                    .text_size(px(size.text_size()))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .text_color(if selected { c.text } else { c.secondary })
                    .when(selected, |segment| segment.bg(c.border))
                    .when(!selected, |segment| {
                        segment.hover(move |segment| segment.bg(c.border.opacity(0.5)).text_color(c.text))
                    })
                    .child(title)
                    .when_some(on_select, |segment, on_select| {
                        segment.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_select(index, window, cx)
                        })
                    })
            }))
    }
}

/// A box on the secondary background with the border around it: a group of settings, a table,
/// a note, the track of a `Segmented`. What is inside lies on the secondary surface.
pub fn card(element: Div, radius: f32, cx: &App) -> Div {
    let c = colors(cx);
    element.bg(c.background_secondary).border_1().border_color(c.border).rounded(px(radius))
}

pub fn card_default(element: Div, cx: &App) -> Div {
    card(element, Radius::CARD, cx)
}

/// How a field is set off from what it lies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputVariant {
    /// A fill with a border around it.
    #[default]
    Outlined,
    /// A fill alone: a search above a list.
    Filled,
    /// No background or height of its own, for a field that lies on glass.
    Bare,
}

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`.
#[derive(IntoElement)]
pub struct InputField {
    state: Entity<component::input::InputState>,
    icon: Option<SharedString>,
    variant: InputVariant,
    size: ControlSize,
    clearable: bool,
    monospaced: bool,
    surface: Surface,
}

impl InputField {
    pub fn new(state: &Entity<component::input::InputState>) -> Self {
        Self {
            state: state.clone(),
            icon: None,
            variant: InputVariant::Outlined,
            size: ControlSize::Regular,
            clearable: false,
            monospaced: false,
            surface: Surface::Background,
        }
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

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }
}

impl RenderOnce for InputField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = self.size;
        let bare = self.variant == InputVariant::Bare;
        let fill = match self.variant {
            InputVariant::Outlined | InputVariant::Filled => self.surface.next().color(c),
            InputVariant::Bare => transparent_black(),
        };
        let empty = self.state.read(cx).value().is_empty();
        let clear = self.state.clone();
        let clear_inset = (size.height() - ControlSize::Small.height()) / 2.;
        div()
            .w_full()
            .when(!bare, |field| field.h(px(size.height())))
            .when(bare, |field| field.h_full())
            .px(px(size.padding() - 2.))
            .rounded(px(size.radius()))
            .bg(fill)
            .when(self.variant == InputVariant::Outlined, |field| field.border_1().border_color(self.surface.border(c)))
            .flex()
            .items_center()
            .gap(px(size.gap()))
            .cursor_text()
            .when_some(self.icon, |field, icon| {
                field
                    .child(div().text_color(c.tertiary).flex_shrink_0().child(icons::symbol(icon, size.small_symbol())))
            })
            .child(
                div().flex_1().min_w_0().child(
                    component::input::Input::new(&self.state)
                        .appearance(false)
                        .px_0()
                        .py_0()
                        .h(px(size.height() - 2.))
                        .text_size(px(size.text_size() + 0.5))
                        .when(self.monospaced, |input| input.font_family(crate::theme::MONO_FONT)),
                ),
            )
            .when(self.clearable && !empty, |field| {
                field.child(
                    ActionButton::icon("clear", "x", "Clear")
                        .small()
                        .surface(self.surface.next())
                        .margin(Edges {
                            top: 0.,
                            left: 0.,
                            bottom: 0.,
                            right: (clear_inset - size.padding() + 2.).max(0.),
                        })
                        .on_click(move |_, window, cx| {
                            clear.update(cx, |state, cx| state.set_value("", window, cx));
                        }),
                )
            })
    }
}
