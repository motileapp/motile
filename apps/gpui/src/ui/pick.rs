//! Choices as a whole row or a card: a checkbox or a radio with its words, and an option drawn as
//! a card that is chosen by a press anywhere on it.

#![allow(dead_code, reason = "a component offers what the Mac app's does, whether a view uses it yet or not")]

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Layer, Opacity, Radius, Surface, colors};
use crate::ui::{PickMark, motion};

type OnPress = Rc<dyn Fn(&mut Window, &mut App)>;

const DETAIL_GAP: f32 = 2.;
const DETAIL_SIZE: f32 = 13.;
const DETAIL_LINE: f32 = 20.;

/// The words under a choice's title that say more of it.
fn detail_text(detail: SharedString, cx: &App) -> Div {
    div()
        .mt(px(DETAIL_GAP))
        .text_size(px(DETAIL_SIZE))
        .line_height(px(DETAIL_LINE))
        .text_color(colors(cx).muted_foreground)
        .child(detail)
}

/// Makes the element one press target that Space presses too while it has the keys.
fn pressable(element: Stateful<Div>, focus: &FocusHandle, on_press: Option<OnPress>) -> Stateful<Div> {
    let Some(on_press) = on_press else { return element };
    let on_key = on_press.clone();
    element
        .track_focus(focus)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_press(window, cx)
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key != "space" {
                return;
            }
            cx.stop_propagation();
            on_key(window, cx)
        })
}

/// A checkbox where any are picked (`multiple`) or a radio where one is, as a row: the mark, then
/// its title and the detail under it. The whole row is one press target, with no light.
#[derive(IntoElement)]
pub struct PickRow {
    id: ElementId,
    title: SharedString,
    detail: Option<SharedString>,
    picked: bool,
    multiple: bool,
    struck: bool,
    size: ControlSize,
    disabled: bool,
    on_press: Option<OnPress>,
}

impl PickRow {
    const GAP: f32 = 8.;
    const TITLE_LINE: f32 = crate::theme::PROSE_LINE_HEIGHT;

    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, picked: bool, multiple: bool) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            picked,
            multiple,
            struck: false,
            size: ControlSize::Regular,
            disabled: false,
            on_press: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Its title struck through and muted, as a task that is done.
    pub fn struck(mut self, struck: bool) -> Self {
        self.struck = struck;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }

    /// How wide its words are in a row `width` wide.
    pub fn text_width(width: f32) -> f32 {
        width - PickMark::SIZE - Self::GAP
    }

    /// How tall it is with its title and detail as tall as they wrap to.
    pub fn height(size: ControlSize, title_height: f32, detail_height: Option<f32>) -> f32 {
        let words = title_height + detail_height.map_or(0., |detail| DETAIL_GAP + detail);
        words.max(size.height())
    }
}

impl RenderOnce for PickRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled || crate::ui::inert(cx);
        let focus = crate::ui::focus_state(&self.id, window, cx);
        let ring = !disabled && crate::ui::focus_shown(&focus, window);
        let c = colors(cx);
        let mark_id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "mark".into());
        let mark_radius = if self.multiple { Radius::XS } else { PickMark::SIZE / 2. };
        let mark = div()
            .relative()
            .flex_shrink_0()
            .child(PickMark::new(mark_id, self.picked, self.multiple))
            .when(ring, |mark| mark.child(crate::ui::focus_ring(mark_radius, cx)));
        let title = div()
            .text_size(px(crate::theme::PROSE_SIZE))
            .line_height(px(Self::TITLE_LINE))
            .text_color(if self.struck { c.muted_foreground } else { c.foreground })
            .when(self.struck, |title| title.line_through())
            .child(self.title.clone());
        let row = div()
            .id(self.id)
            .w_full()
            .min_h(px(self.size.height()))
            .flex()
            .items_center()
            .gap(px(Self::GAP))
            .cursor_default()
            .role(if self.multiple { Role::CheckBox } else { Role::RadioButton })
            .aria_label(self.title)
            .aria_toggled(self.picked.into())
            .when(disabled, |row| row.opacity(c.opacity(Opacity::Disabled)))
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(title)
                    .children(self.detail.map(|detail| detail_text(detail, cx))),
            );
        pressable(row, &focus, self.on_press.filter(|_| !disabled))
    }
}

/// An option drawn as a card: its title and the detail under it, in the surface's border, lit
/// under the pointer, and in the primary colour's border and wash once chosen.
#[derive(IntoElement)]
pub struct OptionCard {
    id: ElementId,
    title: SharedString,
    detail: Option<SharedString>,
    selected: bool,
    surface: Surface,
    radius: f32,
    disabled: bool,
    on_press: Option<OnPress>,
}

impl OptionCard {
    const PADDING_Y: f32 = 14.;
    const PADDING_X: f32 = 16.;
    const BORDER: f32 = 1.;
    const TITLE_LINE: f32 = 20.;

    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, selected: bool) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            selected,
            surface: Surface::Background,
            radius: Radius::LG,
            disabled: false,
            on_press: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }

    /// How wide its words are in a card `width` wide.
    pub fn text_width(width: f32) -> f32 {
        width - 2. * Self::PADDING_X
    }

    /// How tall it is with its title and detail as tall as they wrap to.
    pub fn height(title_height: f32, detail_height: Option<f32>) -> f32 {
        Self::PADDING_Y + title_height + detail_height.map_or(0., |detail| DETAIL_GAP + detail) + Self::PADDING_Y
    }
}

impl RenderOnce for OptionCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled || crate::ui::inert(cx);
        let focus = crate::ui::focus_state(&self.id, window, cx);
        let hovered = crate::ui::hover_state(&self.id, window, cx);
        let named = |name: &'static str| ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), name.into());
        let lit_target = if !disabled && *hovered.read(cx) { 1. } else { 0. };
        let lit = motion::eased(named("lit"), lit_target, motion::QUICK, window, cx);
        let chosen = motion::eased(named("chosen"), if self.selected { 1. } else { 0. }, motion::QUICK, window, cx);
        let ring = !disabled && crate::ui::focus_shown(&focus, window);
        let c = colors(cx);
        let row = self.surface.layer(Layer::Row, c);
        let fill = motion::mix(motion::mix(row.opacity(0.), row, lit), c.wash(c.primary, false), chosen);
        let border = motion::mix(self.surface.border(c), c.primary, chosen);
        let pressed = self.surface.layer(Layer::RowSelected, c);
        let selected = self.selected;
        let inset = |padding: f32| px(padding - Self::BORDER);
        let card = div()
            .id(named("card"))
            .w_full()
            .pt(inset(Self::PADDING_Y))
            .pb(inset(Self::PADDING_Y))
            .pl(inset(Self::PADDING_X))
            .pr(inset(Self::PADDING_X))
            .rounded(px(self.radius))
            .border(px(Self::BORDER))
            .border_color(border)
            .bg(fill)
            .when(!disabled && !selected, |card| card.active(move |card| card.bg(pressed)))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(crate::theme::PROSE_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(px(Self::TITLE_LINE))
                    .text_color(c.foreground)
                    .child(self.title.clone()),
            )
            .children(self.detail.map(|detail| detail_text(detail, cx)));
        let option = div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .cursor_default()
            .role(Role::RadioButton)
            .aria_label(self.title)
            .aria_toggled(selected.into())
            .when(disabled, |option| option.opacity(c.opacity(Opacity::Disabled)))
            .on_hover(crate::ui::track_hover(&hovered))
            .child(card)
            .when(ring, |option| option.child(crate::ui::focus_ring(self.radius, cx)));
        pressable(option, &focus, self.on_press.filter(|_| !disabled))
    }
}

#[cfg(test)]
mod tests {
    use super::{OptionCard, PickRow};
    use crate::theme::ControlSize;

    #[test]
    fn a_pick_row_is_as_tall_as_its_words_or_its_size() {
        assert_eq!(PickRow::height(ControlSize::Regular, 24., None), 28.);
        assert_eq!(PickRow::height(ControlSize::Regular, 48., Some(20.)), 70.);
        assert_eq!(PickRow::text_width(300.), 276.);
    }

    #[test]
    fn an_option_card_pads_its_words() {
        assert_eq!(OptionCard::height(20., None), 48.);
        assert_eq!(OptionCard::height(40., Some(20.)), 90.);
        assert_eq!(OptionCard::text_width(200.), 168.);
    }
}
