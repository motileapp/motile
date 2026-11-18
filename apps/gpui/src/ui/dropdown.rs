//! What drops from a button the app draws, as the Mac's `dropdown` modifier has it: under the
//! button, or over it where it fits better there, lined up with its leading or its trailing edge
//! and kept inside the window. `ChoiceDropdown` is a button with a short list to pick one from.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{TextSize, colors};
use crate::ui::dropdown_list as list;
use crate::ui::menu::Anchor;
use crate::ui::{ActionButton, Variant, dropdown_card, icons};
use crate::window_frame::content_bounds;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// The room between a dropdown and its button's light.
pub const GAP: f32 = 6.;
/// The least room a dropdown leaves between itself and the window's edges.
const MARGIN: f32 = 8.;

/// A dropdown against the button whose light is `button`, over the window's content, which the
/// caller fills with it. It opens under the button, or over it when it doesn't fit under and has
/// more room there, lines up with the button's `trailing` edge or its leading one, and slides back
/// inside the window where it would leave it.
pub fn drop_from(button: Bounds<Pixels>, trailing: bool, window: &Window) -> Floating {
    Floating { button, trailing, area: content_bounds(window), children: Vec::new() }
}

/// The most a dropdown from `button` can be tall, on whichever side of it has more room; one
/// taller scrolls.
pub fn room(button: Bounds<Pixels>, window: &Window) -> Pixels {
    let area = content_bounds(window);
    let below = area.bottom() - button.bottom();
    let above = button.top() - area.top();
    (below.max(above) - px(GAP + MARGIN)).max(px(0.))
}

/// Where a card of `card` size stands against `button` inside `area`, as `drop_from` places it.
fn place(button: Bounds<Pixels>, card: Size<Pixels>, area: Bounds<Pixels>, trailing: bool) -> Point<Pixels> {
    let (gap, margin) = (px(GAP), px(MARGIN));
    let below = area.bottom() - margin - button.bottom() - gap;
    let above = button.top() - gap - area.top() - margin;
    let up = card.height > below && above > below;
    let y = if up { button.top() - gap - card.height } else { button.bottom() + gap };
    let x = if trailing { button.right() - card.width } else { button.left() };
    let x = x.min(area.right() - margin - card.width).max(area.left() + margin);
    point(x, y.min(area.bottom() - margin - card.height).max(area.top() + margin))
}

/// What `drop_from` makes: its card, placed once its size is known.
pub struct Floating {
    button: Bounds<Pixels>,
    trailing: bool,
    area: Bounds<Pixels>,
    children: Vec<AnyElement>,
}

impl ParentElement for Floating {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl IntoElement for Floating {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Floating {
    type RequestLayoutState = Vec<LayoutId>;
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
    ) -> (LayoutId, Vec<LayoutId>) {
        let children: Vec<_> = self.children.iter_mut().map(|child| child.request_layout(window, cx)).collect();
        let style = Style { position: Position::Absolute, display: Display::Flex, ..Style::default() };
        (window.request_layout(style, children.iter().copied(), cx), children)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        children: &mut Vec<LayoutId>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(card) = children.iter().map(|id| window.layout_bounds(*id)).reduce(|all, one| all.union(&one)) else {
            return;
        };
        let at = place(self.button, card.size, self.area, self.trailing);
        let offset = at - bounds.origin;
        window.with_element_offset(point(offset.x.round(), offset.y.round()), |window| {
            for child in &mut self.children {
                child.prepaint(window, cx);
            }
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Vec<LayoutId>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        for child in &mut self.children {
            child.paint(window, cx);
        }
    }
}

/// What a dropdown lies on: the window's content around it, as a menu owns the pointer. A press on
/// it only closes the dropdown, and what lies under it is not lit, scrolled or given the cursor.
pub fn backdrop(
    id: impl Into<ElementId>,
    close: impl Fn(&mut Window, &mut App) + 'static,
    window: &Window,
) -> Stateful<Div> {
    let content = content_bounds(window).size;
    let close: Run = Rc::new(close);
    let right = close.clone();
    div()
        .id(id)
        .w(content.width)
        .h(content.height)
        .occlude()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            close(window, cx)
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            cx.stop_propagation();
            right(window, cx)
        })
}

/// A layer as big as the window's content, over everything but the menus.
pub fn over_content(layer: impl IntoElement, window: &Window) -> Deferred {
    deferred(anchored().position(crate::window_frame::content_origin(window)).child(layer)).with_priority(1)
}

/// The light of a button, `margin` inside the bounds that take its presses.
pub fn light(bounds: Bounds<Pixels>, margin: Edges<f32>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(bounds.left() + px(margin.left), bounds.top() + px(margin.top)),
        point(bounds.right() - px(margin.right), bounds.bottom() - px(margin.bottom)),
    )
}

/// One of what a `ChoiceDropdown` offers: its symbol or picture, its name, a line about it, and
/// whether it is the one chosen.
pub struct DropdownChoice {
    title: SharedString,
    detail: Option<SharedString>,
    icon: Option<AnyElement>,
    chosen: bool,
    choose: Run,
}

impl DropdownChoice {
    pub fn new(title: impl Into<SharedString>, chosen: bool, choose: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { title: title.into(), detail: None, icon: None, chosen, choose: Rc::new(choose) }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn symbol(mut self, symbol: &'static str) -> Self {
        self.icon = Some(icons::symbol(symbol, list::SYMBOL).into_any_element());
        self
    }

    pub fn picture(mut self, picture: impl IntoElement) -> Self {
        self.icon = Some(picture.into_any_element());
        self
    }
}

/// A ghost button that drops a short list to pick one from, under a title, as the composer's
/// strip offers its server and its workspace. Picking one, Esc or a press elsewhere closes it.
#[derive(IntoElement)]
pub struct ChoiceDropdown {
    id: ElementId,
    button: ActionButton,
    title: SharedString,
    choices: Vec<DropdownChoice>,
    trailing: bool,
    shortcut: Option<&'static str>,
    on_close: Option<Run>,
}

/// Whether a `ChoiceDropdown` is open, where its button was drawn, and what has the keys while
/// it is.
struct Dropped {
    open: bool,
    anchor: Anchor,
    focus: FocusHandle,
}

impl ChoiceDropdown {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        title: impl Into<SharedString>,
        choices: Vec<DropdownChoice>,
    ) -> Self {
        let id = id.into();
        let button = ActionButton::new(id.clone(), label).variant(Variant::Ghost).opens(true);
        Self { id, button, title: title.into(), choices, trailing: false, shortcut: None, on_close: None }
    }

    /// Changes the button it drops from.
    pub fn button(mut self, change: impl FnOnce(ActionButton) -> ActionButton) -> Self {
        self.button = change(self.button);
        self
    }

    /// It lines up with its button's trailing edge.
    pub fn trailing(mut self) -> Self {
        self.trailing = true;
        self
    }

    /// Makes the button the one the command's keys press.
    pub fn shortcut(mut self, command: &'static str) -> Self {
        self.shortcut = Some(command);
        self
    }

    /// What runs once it closes, as giving the keys back to what had them.
    pub fn on_close(mut self, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(on_close));
        self
    }

    /// How wide its button is when nothing cuts its words short.
    pub fn natural_width(&self, window: &mut Window, cx: &App) -> f32 {
        self.button.natural_width(window, cx)
    }

    fn row(index: usize, choice: DropdownChoice, close: Run, cx: &App) -> Stateful<Div> {
        let c = colors(cx);
        let group = SharedString::from(format!("choice-{index}"));
        let choose = choice.choose;
        list::row(("choice", index), group.clone(), cx)
            .when(choice.detail.is_some(), |row| row.py(px(list::DETAIL_PADDING)))
            .child(list::light(group, false, cx))
            .children(
                choice
                    .icon
                    .map(|icon| div().relative().flex_shrink_0().flex().text_color(c.muted_foreground).child(icon)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .whitespace_nowrap()
                    .child(choice.title)
                    .children(
                        choice
                            .detail
                            .map(|detail| div().text_size(TextSize::Xs).text_color(c.muted_foreground).child(detail)),
                    ),
            )
            .child(list::check(choice.chosen).ml(px(4.)))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                choose(window, cx);
                close(window, cx);
            })
    }
}

impl RenderOnce for ChoiceDropdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "dropdown".into());
        let state = window.use_keyed_state(key, cx, |_, cx| Dropped {
            open: false,
            anchor: Anchor::default(),
            focus: cx.focus_handle(),
        });
        let (open, anchor, focus) = {
            let dropped = state.read(cx);
            (dropped.open, dropped.anchor.clone(), dropped.focus.clone())
        };
        let on_close = self.on_close.clone();
        let show = Rc::new({
            let state = state.clone();
            move |shown: bool, window: &mut Window, cx: &mut App| {
                let focus = state.update(cx, |dropped, _| {
                    dropped.open = shown;
                    dropped.focus.clone()
                });
                if shown {
                    crate::ui::overlay_opened(cx);
                    window.focus(&focus, cx);
                } else if let Some(on_close) = &on_close {
                    on_close(window, cx);
                }
                window.refresh();
            }
        });
        let margin = self.button.margin_edges();
        let button = self.button.open(open).on_click({
            let show = show.clone();
            move |_, window, cx| show(true, window, cx)
        });
        let pressed = {
            let show = show.clone();
            move |window: &mut Window, cx: &mut App| show(!open, window, cx)
        };
        let dropdown = open.then(|| {
            let close: Run = {
                let show = show.clone();
                Rc::new(move |window, cx| show(false, window, cx))
            };
            let rows =
                self.choices.into_iter().enumerate().map(|(index, choice)| Self::row(index, choice, close.clone(), cx));
            let card = dropdown_card(div(), cx)
                .id("choice-dropdown")
                .min_w(px(200.))
                .p(px(list::PADDING))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(list::title(self.title, None, cx))
                .children(rows);
            let escape = close.clone();
            let layer = backdrop("choice-dropdown-layer", move |window, cx| close(window, cx), window)
                .track_focus(&focus)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    cx.stop_propagation();
                    escape(window, cx)
                })
                .child(drop_from(light(anchor.bounds(), margin), self.trailing, window).child(card));
            over_content(layer, window)
        });
        div()
            .relative()
            .flex_shrink_0()
            .child(anchor.track())
            .children(self.shortcut.map(|command| crate::keyboard::target_pressing(command, pressed)))
            .child(button)
            .children(dropdown)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, Pixels, point, px, size};

    use super::{GAP, place};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    const WINDOW: (f32, f32) = (1000., 800.);

    fn placed(button: Bounds<Pixels>, card: (f32, f32), trailing: bool) -> (f32, f32) {
        let at = place(button, size(px(card.0), px(card.1)), rect(0., 0., WINDOW.0, WINDOW.1), trailing);
        (f32::from(at.x), f32::from(at.y))
    }

    #[test]
    fn hangs_under_its_button_by_its_leading_edge() {
        assert_eq!(placed(rect(100., 100., 40., 40.), (300., 200.), false), (100., 140. + GAP));
    }

    #[test]
    fn lines_up_with_the_trailing_edge() {
        assert_eq!(placed(rect(500., 100., 40., 40.), (300., 200.), true), (240., 140. + GAP));
    }

    #[test]
    fn slides_back_inside_the_window_at_either_side() {
        assert_eq!(placed(rect(900., 100., 40., 40.), (300., 200.), false).0, 692.);
        assert_eq!(placed(rect(20., 100., 40., 40.), (300., 200.), true).0, 8.);
    }

    #[test]
    fn opens_over_its_button_only_when_it_does_not_fit_under_it() {
        assert_eq!(placed(rect(100., 650., 40., 40.), (300., 90.), false).1, 690. + GAP);
        assert_eq!(placed(rect(100., 650., 40., 40.), (300., 200.), false).1, 450. - GAP);
    }
}
