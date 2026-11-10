//! Snoozes threads until a day and a time the user picks: the Mac app's `CustomSnoozeSheet`.

use chrono::{Duration, Local, NaiveDate, TimeZone};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::{CustomSnooze, Store};
use crate::theme::{TextSize, colors};
use crate::ui::{ActionButton, DateField, InputField, beside};

pub struct CustomSnoozeSheet {
    store: Entity<Store>,
    snooze: CustomSnooze,
    day: NaiveDate,
    time: Entity<InputState>,
    error: Option<String>,
    _subscription: Subscription,
}

impl CustomSnoozeSheet {
    pub fn new(store: Entity<Store>, snooze: CustomSnooze, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let time = cx.new(|cx| InputState::new(window, cx).default_value("9:00").placeholder("Time"));
        let subscription = cx.subscribe_in(&time, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { .. } => this.submit(cx),
            InputEvent::Change => {
                this.error = None;
                cx.notify();
            }
            _ => {}
        });
        let day = Local::now().date_naive() + Duration::days(1);
        Self { store, snooze, day, time, error: None, _subscription: subscription }
    }

    /// Snoozes the threads until the day and the time written, or says why it can't.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let day = self.day.format("%Y-%m-%d").to_string();
        let time = self.time.read(cx).value().to_string();
        match motile_core::snooze::custom(&day, &time, Local::now()) {
            Ok(until) => {
                let CustomSnooze { thread_ids, from_sidebar } = self.snooze.clone();
                self.store.update(cx, |store, cx| {
                    store.custom_snooze = None;
                    store.snooze(&thread_ids, until, from_sidebar, cx);
                    cx.notify();
                });
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.custom_snooze = None;
            cx.notify();
        });
    }
}

impl Render for CustomSnoozeSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let today = Local::now().date_naive();
        let noon = self.day.and_hms_opt(12, 0, 0).and_then(|noon| Local.from_local_datetime(&noon).earliest());
        let day = crate::format::date(noon.map_or(0., |noon| noon.timestamp() as f64), "yMMMd", "%b %-d, %Y");
        let (cancel, submit, pick) = (cx.entity().downgrade(), cx.entity().downgrade(), cx.entity().downgrade());
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(div().text_size(TextSize::Lg).font_weight(FontWeight::SEMIBOLD).child("Snooze Until"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(
                        DateField::new("snooze-day", Some(self.day)).text(day).range(Some(today), None).on_pick(
                            move |day, _, cx| {
                                let _ = pick.update(cx, |this, cx| {
                                    this.day = day;
                                    this.error = None;
                                    cx.notify();
                                });
                            },
                        ),
                    ))
                    .child(div().w(px(110.)).child(InputField::new(&self.time))),
            )
            .children(
                self.error.clone().map(|error| div().text_size(TextSize::Xs).text_color(c.destructive).child(error)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(ActionButton::new("snooze-cancel", "Cancel").margin(beside(0., 4.)).on_click(
                        move |_, _, cx| {
                            let _ = cancel.update(cx, |this, cx| this.close(cx));
                        },
                    ))
                    .child(ActionButton::new("snooze", "Snooze").primary().margin(beside(4., 0.)).on_click(
                        move |_, _, cx| {
                            let _ = submit.update(cx, |this, cx| this.submit(cx));
                        },
                    )),
            )
    }
}
