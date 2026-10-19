//! A field that holds a day: it looks like a field and opens a month to pick the day from, in a
//! popover under it.

#![allow(dead_code, reason = "a component offers what the Mac app's does, whether a view uses it yet or not")]

use std::rc::Rc;

use chrono::{Datelike, Local, NaiveDate};
use gpui_kit::component::calendar::{Calendar, CalendarEvent, CalendarState, Date, Matcher};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Opacity, Surface, colors};
use crate::ui::control::field_fill;
use crate::ui::menu::Anchor;
use crate::ui::{dropdown_card, icons, motion};

type OnPick = Rc<dyn Fn(NaiveDate, &mut Window, &mut App)>;

/// The day `day` is shown at, or today, held to `min`…`max`.
pub fn shown_day(
    day: Option<NaiveDate>,
    today: NaiveDate,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
) -> NaiveDate {
    let day = day.unwrap_or(today);
    let day = min.map_or(day, |min| day.max(min));
    max.map_or(day, |max| day.min(max))
}

/// Whether `day` lies outside `min`…`max`, where it can't be picked.
pub fn out_of_range(day: NaiveDate, min: Option<NaiveDate>, max: Option<NaiveDate>) -> bool {
    min.is_some_and(|min| day < min) || max.is_some_and(|max| day > max)
}

/// Whether the popover is open, where the field and the popover were drawn, the month in it, and
/// what a picked day is handed to.
struct Popover {
    open: bool,
    field: Anchor,
    card: Anchor,
    calendar: Entity<CalendarState>,
    on_pick: Option<OnPick>,
    _picked: Subscription,
}

/// A day, in a field that looks like `InputField`: the day's words, or the placeholder, and the
/// calendar symbol at its end. A press, or Space or Return, opens a month under it to pick a day
/// from, held to `min`…`max`; picking one hands it to `on_pick` and closes it, as Esc and a click
/// anywhere else do.
#[derive(IntoElement)]
pub struct DateField {
    id: ElementId,
    text: Option<SharedString>,
    placeholder: SharedString,
    label: Option<SharedString>,
    day: Option<NaiveDate>,
    min: Option<NaiveDate>,
    max: Option<NaiveDate>,
    size: ControlSize,
    surface: Surface,
    disabled: bool,
    on_pick: Option<OnPick>,
}

impl DateField {
    const GAP: f32 = 4.;
    const MARGIN: f32 = 8.;
    const CALENDAR_WIDTH: f32 = 224.;
    const PADDING: f32 = 12.;

    pub fn new(id: impl Into<ElementId>, day: Option<NaiveDate>) -> Self {
        Self {
            id: id.into(),
            text: None,
            placeholder: "Date".into(),
            label: None,
            day,
            min: None,
            max: None,
            size: ControlSize::Regular,
            surface: Surface::Background,
            disabled: false,
            on_pick: None,
        }
    }

    /// The day as words.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// What the field is named to assistive technology, where it isn't its placeholder.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The days it can be set to, from `min` to `max`.
    pub fn range(mut self, min: Option<NaiveDate>, max: Option<NaiveDate>) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_pick(mut self, handler: impl Fn(NaiveDate, &mut Window, &mut App) + 'static) -> Self {
        self.on_pick = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for DateField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled || crate::ui::inert(cx);
        let id = self.id.clone();
        let named = |name: &'static str| ElementId::NamedChild(std::sync::Arc::new(id.clone()), name.into());
        let focus = crate::ui::focus_state(&id, window, cx);
        let state = window.use_keyed_state(named("popover"), cx, |window, cx| {
            let calendar = cx.new(|cx| CalendarState::new(window, cx));
            let picked = cx.subscribe_in(&calendar, window, |popover: &mut Popover, _, event, window, cx| {
                let CalendarEvent::Selected(date) = event;
                let Some(day) = date.start() else { return };
                popover.open = false;
                cx.notify();
                window.refresh();
                let Some(on_pick) = popover.on_pick.clone() else { return };
                on_pick(day, window, cx);
            });
            Popover {
                open: false,
                field: Anchor::default(),
                card: Anchor::default(),
                calendar,
                on_pick: None,
                _picked: picked,
            }
        });
        let on_pick = self.on_pick.clone();
        state.update(cx, |popover, _| popover.on_pick = on_pick);
        let (open, field_anchor, card_anchor, calendar) = {
            let popover = state.read(cx);
            (popover.open, popover.field.clone(), popover.card.clone(), popover.calendar.clone())
        };
        let (day, min, max) = (self.day, self.min, self.max);
        let show = {
            let (state, focus, calendar) = (state.clone(), focus.clone(), calendar.clone());
            move |shown: bool, window: &mut Window, cx: &mut App| {
                state.update(cx, |popover, cx| {
                    popover.open = shown;
                    cx.notify();
                });
                window.refresh();
                if !shown {
                    window.focus(&focus, cx);
                    return;
                }
                crate::ui::overlay_opened(cx);
                let month = shown_day(day, Local::now().date_naive(), min, max);
                calendar.update(cx, |calendar, cx| {
                    calendar.set_disabled_matcher(Matcher::custom(move |day| out_of_range(*day, min, max)), window, cx);
                    calendar.apply_date(Date::Single(day));
                    calendar.select_year(month.year());
                    calendar.select_month(month.month() as u8);
                    cx.notify();
                });
                let keys = calendar.read(cx).focus_handle.clone();
                window.focus(&keys, cx);
            }
        };
        let show = Rc::new(show);
        let c = colors(cx);
        let size = self.size;
        let ring = !disabled && !open && crate::ui::focus_shown(&focus, window);
        let words = match &self.text {
            Some(text) => div().text_color(c.foreground).child(text.clone()),
            None => div().text_color(c.muted_stronger_foreground).child(self.placeholder.clone()),
        };
        let field = div()
            .id(id.clone())
            .relative()
            .w_full()
            .h(px(size.height()))
            .px(px(size.padding() - 2.))
            .rounded(px(size.radius()))
            .bg(field_fill(self.surface, c))
            .flex()
            .items_center()
            .gap(px(size.gap()))
            .cursor_default()
            .role(Role::Button)
            .aria_label(self.label.clone().unwrap_or_else(|| self.placeholder.clone()))
            .when_some(self.text.clone(), |field, text| field.aria_value(text))
            .when(disabled, |field| field.opacity(c.opacity(Opacity::Disabled)))
            .child(field_anchor.track())
            .child(
                words
                    .flex_1()
                    .min_w_0()
                    .text_size(px(size.text_size() + 0.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis(),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(c.muted_stronger_foreground)
                    .child(icons::symbol("calendar", size.small_symbol())),
            )
            .when(ring, |field| field.child(crate::ui::focus_ring(size.radius(), cx)))
            .when(!disabled, |field| {
                let (click, keys) = (show.clone(), show.clone());
                field
                    .track_focus(&focus)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        click(true, window, cx)
                    })
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if !matches!(event.keystroke.key.as_str(), "space" | "enter") {
                            return;
                        }
                        cx.stop_propagation();
                        keys(true, window, cx)
                    })
            });
        let shown = motion::eased_from(named("shown"), 0., if open { 1. } else { 0. }, motion::QUICK, window, cx);
        let popover = (shown > 0.).then(|| {
            let bounds = field_anchor.bounds();
            let content = crate::window_frame::content_bounds(window);
            let height = card_anchor.bounds().size.height;
            let below = bounds.bottom() + px(Self::GAP);
            let above = bounds.top() - px(Self::GAP) - height;
            let fits_below = below + height <= content.bottom() - px(Self::MARGIN);
            let top = if fits_below || above < content.top() + px(Self::MARGIN) { below } else { above };
            let card = dropdown_card(div(), cx)
                .id(named("card"))
                .absolute()
                .left(bounds.left() - content.left())
                .top(top - content.top())
                .p(px(Self::PADDING))
                .opacity(shown)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(card_anchor.track())
                .child(Calendar::new(&calendar).border_0().rounded_none().p_0().w(px(Self::CALENDAR_WIDTH)));
            let (close, escape) = (show.clone(), show.clone());
            let layer = div()
                .id(named("layer"))
                .w(content.size.width)
                .h(content.size.height)
                .when(open, |layer| {
                    layer
                        .occlude()
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
        div().relative().w_full().child(field).children(popover)
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{out_of_range, shown_day};

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn opens_on_the_day_or_today_held_to_the_range() {
        let today = day(2026, 10, 19);
        assert_eq!(shown_day(Some(day(2026, 1, 2)), today, None, None), day(2026, 1, 2));
        assert_eq!(shown_day(None, today, None, None), today);
        assert_eq!(shown_day(None, today, Some(day(2027, 3, 1)), None), day(2027, 3, 1));
        assert_eq!(shown_day(None, today, None, Some(day(2025, 5, 5))), day(2025, 5, 5));
    }

    #[test]
    fn days_outside_the_range_are_out() {
        let (min, max) = (Some(day(2026, 1, 10)), Some(day(2026, 1, 20)));
        assert!(out_of_range(day(2026, 1, 9), min, max));
        assert!(!out_of_range(day(2026, 1, 10), min, max));
        assert!(!out_of_range(day(2026, 1, 20), min, max));
        assert!(out_of_range(day(2026, 1, 21), min, max));
        assert!(!out_of_range(day(1999, 1, 1), None, None));
    }
}
