//! What the user and the agents did on the servers counted, the Activity tab of the usage route:
//! the headline numbers, the work over time by where it came from, the clients it came from, the
//! hours of the week, how the turns went and the features used.

use std::collections::HashSet;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::activity::{duration, number};
use serde::{Deserialize, Serialize};

use super::spending::{CAPTION, CHART_HEIGHT, X_GUTTER, Y_GUTTER, axis_label, axis_ticks, caption, dot, ticks};
use crate::format;
use crate::models::ago;
use crate::store::Store;
use crate::store::usage::{
    ActivityClient, ActivityFact, ActivityHour, ActivityLine, ActivityReport, ActivitySource, FeatureLine, UsagePeriod,
};
use crate::theme::{Colors, ControlSize, IconSize, Layer, Opacity, Radius, Surface, TextSize, colors};
use crate::ui::control::dotted;
use crate::ui::menu::Anchor;
use crate::ui::{
    ActionButton, Chip, Segmented, card_default, divider_on, icons, logos, middle_truncated, tabular_numbers,
    tabular_text_width, text_width,
};

/// Narrower than this, the tiles go two to a row and the turns' cards under each other.
const BESIDE_WIDTH: f32 = 640.;
/// How much of its bucket's width a bar takes.
const BAR_FILL: f32 = 0.7;
const BADGE: f32 = 32.;
const CLIENT_BAR_WIDTH: f32 = 96.;
const FEATURE_BAR_WIDTH: f32 = 56.;
/// How many features a group shows until all are asked for.
const FEATURES_SHOWN: usize = 5;
const HEATMAP_PADDING: f32 = 16.;
const HEATMAP_GAP: f32 = 3.;
const DAY_LABELS: f32 = 30.;
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MEASURE_KEY: &str = "activity.measure";
const BREAKDOWN_KEY: &str = "activity.breakdown";
const WHO_KEY: &str = "activity.who";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Measure {
    #[default]
    Messages,
    Turns,
    Time,
}

impl Measure {
    const ALL: [Measure; 3] = [Measure::Messages, Measure::Turns, Measure::Time];

    fn label(self) -> &'static str {
        match self {
            Measure::Messages => "Messages",
            Measure::Turns => "Turns",
            Measure::Time => "Agent time",
        }
    }

    /// Agent time is charted in hours, so that its axis falls on whole hours.
    fn total(self, source: &ActivitySource) -> f64 {
        match self {
            Measure::Messages => f64::from(source.messages),
            Measure::Turns => f64::from(source.turns),
            Measure::Time => source.seconds / 3600.,
        }
    }

    fn at(self, source: &ActivitySource, index: usize) -> f64 {
        match self {
            Measure::Messages => source.message_points.get(index).map_or(0., |value| f64::from(*value)),
            Measure::Turns => source.turn_points.get(index).map_or(0., |value| f64::from(*value)),
            Measure::Time => source.second_points.get(index).map_or(0., |seconds| seconds / 3600.),
        }
    }

    fn amount(self, value: f64) -> String {
        match self {
            Measure::Messages | Measure::Turns => number(value.max(0.).round() as u32),
            Measure::Time => duration(value * 3600.),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Breakdown {
    #[default]
    Agents,
    Models,
    Projects,
}

impl Breakdown {
    const ALL: [Breakdown; 3] = [Breakdown::Agents, Breakdown::Models, Breakdown::Projects];

    fn label(self) -> &'static str {
        match self {
            Breakdown::Agents => "Agents",
            Breakdown::Models => "Models",
            Breakdown::Projects => "Projects",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Who {
    #[default]
    You,
    Agents,
}

impl Who {
    fn label(self) -> &'static str {
        match self {
            Who::You => "You",
            Who::Agents => "Agents",
        }
    }
}

pub struct ActivityView {
    store: Entity<Store>,
    /// The bucket the pointer is over.
    pointed: Option<usize>,
    /// Where the plot was drawn, to tell which bucket the pointer is over.
    plot: Anchor,
    /// The hour of the week the pointer is over.
    chosen: Option<usize>,
    /// The feature groups that show all their features.
    expanded: HashSet<SharedString>,
    /// How wide the content is, which lays it out.
    pub width: f32,
    period: UsagePeriod,
}

impl ActivityView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let period = store.read(cx).usage_period();
        Self {
            store,
            pointed: None,
            plot: Anchor::default(),
            chosen: None,
            expanded: HashSet::new(),
            width: 0.,
            period,
        }
    }

    fn beside(&self) -> bool {
        self.width >= BESIDE_WIDTH
    }

    fn pref<T: serde::de::DeserializeOwned + Default>(&self, key: &str, cx: &App) -> T {
        self.store.read(cx).prefs.get::<T>(key).unwrap_or_default()
    }

    /// A row of choices kept under `key`, of which `current` is on.
    fn picker<T: Copy + PartialEq + Serialize + 'static>(
        &self,
        key: &'static str,
        offered: Vec<T>,
        current: T,
        label: fn(T) -> &'static str,
    ) -> Segmented {
        let store = self.store.clone();
        Segmented::new(
            key,
            offered.iter().map(|choice| SharedString::from(label(*choice))).collect(),
            offered.iter().position(|choice| *choice == current).unwrap_or(0),
        )
        .size(ControlSize::Small)
        .on_select(move |index, _, cx| {
            store.update(cx, |store, cx| {
                store.prefs.set(key, offered[index]);
                cx.notify();
            })
        })
    }

    fn point_at(&mut self, position: Point<Pixels>, count: usize, cx: &mut Context<Self>) {
        let bounds = self.plot.bounds();
        let width = f32::from(bounds.size.width);
        let pointed = if !bounds.contains(&position) || count == 0 || width <= 0. {
            None
        } else {
            let fraction = (f32::from(position.x - bounds.origin.x) / width).clamp(0., 1.);
            Some(((fraction * count as f32) as usize).min(count - 1))
        };
        if self.pointed != pointed {
            self.pointed = pointed;
            cx.notify();
        }
    }

    fn columns(&self) -> usize {
        if self.beside() { 4 } else { 2 }
    }

    fn tiles(&self, report: &ActivityReport, cx: &App) -> AnyElement {
        stat_grid(&report.tiles, self.columns(), TextSize::Xl, cx).into_any_element()
    }

    fn over_time(&self, report: &ActivityReport, cx: &mut Context<Self>) -> AnyElement {
        let measure: Measure = self.pref(MEASURE_KEY, cx);
        let picker = self.picker(MEASURE_KEY, Measure::ALL.to_vec(), measure, Measure::label).fills(!self.beside());
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Over time", None, Some(picker.into_any_element()), !self.beside(), cx))
            .child(self.legend(report, measure, cx))
            .child(self.chart(report, measure, cx))
            .into_any_element()
    }

    /// Each source with its total, or what it did in the bucket the pointer is over, whose date
    /// ends the line.
    fn legend(&self, report: &ActivityReport, measure: Measure, cx: &App) -> AnyElement {
        let c = colors(cx);
        let index = self.pointed.filter(|index| *index < report.starts.len());
        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .text_size(TextSize::Xs)
            .child(div().flex_1().min_w_0().flex().flex_wrap().gap_x(px(14.)).gap_y(px(8.)).children(
                shown_sources(report, measure).map(|source| {
                    let shown = index.map_or_else(|| measure.total(source), |index| measure.at(source, index));
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .whitespace_nowrap()
                        .child(dot(chart_color(source.color, c)))
                        .child(icons::symbol(symbol_name(&source.symbol), IconSize::Xs).text_color(c.muted_foreground))
                        .child(div().text_color(c.foreground).child(source.label.clone()))
                        .child(
                            div()
                                .text_color(c.muted_foreground)
                                .font_features(tabular_numbers())
                                .child(measure.amount(shown)),
                        )
                }),
            ))
            .when_some(index, |legend, index| {
                legend.child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(c.muted_foreground)
                        .child(pointed_label(report.starts[index], self.period)),
                )
            })
            .into_any_element()
    }

    /// A bar for each bucket, its sources stacked in their colours.
    fn chart(&self, report: &ActivityReport, measure: Measure, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let count = report.starts.len();
        let series: Vec<(Hsla, Vec<f64>)> = shown_sources(report, measure)
            .map(|source| (chart_color(source.color, c), (0..count).map(|index| measure.at(source, index)).collect()))
            .collect();
        let max =
            (0..count).map(|index| series.iter().map(|(_, values)| values[index]).sum::<f64>()).fold(0., f64::max);
        let (step, top) = ticks(max);
        let tick_count = (top / step).round() as usize;
        let labelled = axis_ticks(&report.starts, self.period);
        let center = move |index: usize| relative((index as f32 + 0.5) / count.max(1) as f32);
        let pointed = self.pointed.filter(|index| *index < count);
        let dimmed = c.opacity(Opacity::Disabled);
        div()
            .w_full()
            .h(px(CHART_HEIGHT))
            .relative()
            .child(
                div()
                    .id("activity-plot")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right(px(Y_GUTTER))
                    .bottom(px(X_GUTTER))
                    .child(self.plot.track())
                    .children((0..=tick_count).map(|tick| {
                        let value = tick as f64 * step;
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(relative(1. - (value / top) as f32))
                            .h(px(1.))
                            .bg(c.border)
                            .child(
                                div()
                                    .absolute()
                                    .left_full()
                                    .top(px(-7.))
                                    .w(px(Y_GUTTER))
                                    .pl(px(6.))
                                    .text_size(CAPTION)
                                    .text_color(c.muted_foreground)
                                    .whitespace_nowrap()
                                    .child(measure.amount(value)),
                            )
                    }))
                    .child(
                        div().absolute().left_0().right_0().top_0().bottom_0().child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| paint_bars(bounds, &series, top, pointed, dimmed, window),
                            )
                            .size_full(),
                        ),
                    )
                    .child(div().absolute().left_0().right_0().top_full().h(px(X_GUTTER)).children(
                        labelled.into_iter().map(|index| {
                            div()
                                .absolute()
                                .left(center(index))
                                .ml(px(-30.))
                                .w(px(60.))
                                .pt(px(4.))
                                .flex()
                                .justify_center()
                                .text_size(CAPTION)
                                .text_color(c.muted_foreground)
                                .whitespace_nowrap()
                                .child(axis_label(report.starts[index], self.period))
                        }),
                    ))
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                            this.point_at(event.position, count, cx)
                        }),
                    )
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !hovered && this.pointed.is_some() {
                            this.pointed = None;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }

    fn clients(&self, report: &ActivityReport, cx: &App) -> AnyElement {
        let rows = separated(report.clients.iter().map(|client| self.client_row(client, cx)).collect(), cx);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Clients", Some("Where your messages came from".into()), None, false, cx))
            .child(card_default(div().flex().flex_col(), cx).children(rows))
            .into_any_element()
    }

    /// A client: its device, what it is, how it reaches the servers, and its part of the messages.
    fn client_row(&self, client: &ActivityClient, cx: &App) -> AnyElement {
        let c = colors(cx);
        let beside = self.beside();
        let color = chart_color(client.color, c);
        let badge = div()
            .flex_shrink_0()
            .size(px(BADGE))
            .rounded(px(Radius::MD))
            .bg(c.wash(color, false))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::symbol(symbol_name(&client.symbol), IconSize::Md).text_color(color));
        let name = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                middle_truncated(client.name.clone())
                    .text_size(TextSize::Sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground),
            )
            .when(client.this_device, |name| {
                name.child(div().flex_shrink_0().child(Chip::new("This device").small(true).surface(Surface::Card)))
            })
            .when(client.removed, |name| {
                name.child(
                    div().flex_shrink_0().child(Chip::new("Removed").muted(true).small(true).surface(Surface::Card)),
                )
            });
        let about = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(3.))
            .text_size(TextSize::Xs)
            .text_color(c.muted_foreground)
            .child(name)
            .when(beside, |about| {
                let line = if client.detail.is_empty() {
                    client.client.clone()
                } else {
                    format!("{} · {}", client.client, client.detail)
                };
                about.child(div().whitespace_nowrap().overflow_hidden().text_ellipsis().child(dotted(line, c)))
            })
            .when(!beside, |about| {
                about
                    .child(client.client.clone())
                    .when(!client.detail.is_empty(), |about| about.child(div().child(client.detail.clone())))
            });
        let messages = if client.messages == 1 {
            "1 message".to_string()
        } else {
            format!("{} messages", number(client.messages))
        };
        let seen = if client.active {
            Some("Active now".to_string())
        } else {
            client.last_seen.map(|seen| {
                let ago = ago(seen);
                if ago == "now" { "Seen just now".to_string() } else { format!("Seen {ago} ago") }
            })
        };
        let main = div().flex().items_center().gap(px(12.)).child(badge).child(about);
        if !beside {
            let numbers = div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .text_size(TextSize::Xxs)
                .whitespace_nowrap()
                .font_features(tabular_numbers())
                .child(div().text_color(c.foreground).child(messages))
                .when_some(seen, |numbers, seen| {
                    numbers.child(div().text_color(c.muted_stronger_foreground).child(seen))
                });
            return div()
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(main)
                .child(
                    div()
                        .pl(px(BADGE + 12.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(share_bar(client.share, color, c))
                        .child(numbers),
                )
                .into_any_element();
        }
        let numbers = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(3.))
            .whitespace_nowrap()
            .font_features(tabular_numbers())
            .child(div().text_size(TextSize::Xs).text_color(c.foreground).child(messages))
            .when_some(seen, |numbers, seen| {
                numbers.child(div().text_size(TextSize::Xxs).text_color(c.muted_stronger_foreground).child(seen))
            });
        div()
            .px(px(14.))
            .py(px(12.))
            .child(main.child(share_bar(client.share, color, c).w(px(CLIENT_BAR_WIDTH)).flex_shrink_0()).child(numbers))
            .into_any_element()
    }

    fn hours(&self, report: &ActivityReport, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("When you work", report.busiest.clone(), None, false, cx))
            .child(card_default(div().p(px(HEATMAP_PADDING)), cx).child(self.heatmap(&report.hours, window, cx)))
            .into_any_element()
    }

    /// The hours of the week, a row a day, each as busy as its colour is strong. The hour the
    /// pointer is over says what was done in it.
    fn heatmap(&self, hours: &[ActivityHour], window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let inner = self.width - 2. * HEATMAP_PADDING - 2.;
        let side = ((inner - DAY_LABELS - HEATMAP_GAP * 24.) / 24.).max(4.);
        let tall = side.min(18.);
        let corner = Radius::XS.min(side / 4.);
        let rows = (0..7).map(|day| {
            div()
                .flex()
                .items_center()
                .gap(px(HEATMAP_GAP))
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(DAY_LABELS))
                        .text_size(TextSize::Xxs)
                        .text_color(c.muted_foreground)
                        .child(DAYS[day]),
                )
                .children((0..24).map(|hour| {
                    let index = day * 24 + hour;
                    let level = hours.get(index).map_or(0, |hour| hour.level);
                    div()
                        .id(("activity-hour", index))
                        .flex_shrink_0()
                        .w(px(side))
                        .h(px(tall))
                        .rounded(px(corner))
                        .bg(level_fill(level, c))
                        .when(self.chosen == Some(index), |cell| cell.border_1().border_color(c.foreground))
                        .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                            let chosen = match (*inside, this.chosen) {
                                (true, _) => Some(index),
                                (false, Some(shown)) if shown == index => None,
                                (false, shown) => shown,
                            };
                            if this.chosen != chosen {
                                this.chosen = chosen;
                                cx.notify();
                            }
                        }))
                }))
        });
        let hour_labels =
            div().pt(px(2.)).flex().gap(px(HEATMAP_GAP)).child(div().flex_shrink_0().w(px(DAY_LABELS))).children(
                [0, 6, 12, 18].map(|hour| {
                    div()
                        .flex_shrink_0()
                        .w(px(side * 6. + HEATMAP_GAP * 5.))
                        .text_size(TextSize::Xxs)
                        .text_color(c.muted_foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(hour_label(hour))
                }),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().flex().flex_col().gap(px(HEATMAP_GAP)).children(rows).child(hour_labels))
            .child(self.heatmap_caption(hours, inner, window, c))
            .into_any_element()
    }

    /// What the chosen hour holds, or how to choose one, and the key to the colours: beside each
    /// other, or the key under it where they don't fit.
    fn heatmap_caption(&self, hours: &[ActivityHour], room: f32, window: &mut Window, c: &Colors) -> Div {
        let chosen = self.chosen.and_then(|index| hours.get(index));
        let (said, color) = match chosen {
            Some(hour) => {
                let messages =
                    if hour.messages == 1 { "1 message".into() } else { format!("{} messages", number(hour.messages)) };
                let turns = if hour.turns == 1 { "1 turn".into() } else { format!("{} turns", number(hour.turns)) };
                let when = format!(
                    "{}, {}–{}",
                    WEEKDAYS[usize::from(hour.weekday).min(6)],
                    hour_label(hour.hour),
                    hour_label((hour.hour + 1) % 24)
                );
                (format!("{when} · {messages} · {turns}"), c.muted_foreground)
            }
            None => ("Point at an hour to see what was done in it".to_string(), c.muted_stronger_foreground),
        };
        let words = |text: &str, window: &mut Window| text_width(text, TextSize::Xxs, FontWeight::NORMAL, window);
        let key_width = words("Less", window) + words("More", window) + 2. * 3. + 5. * 10. + 6. * HEATMAP_GAP;
        let fits = tabular_text_width(&said, TextSize::Xxs, FontWeight::NORMAL, window) + 8. + key_width <= room;
        let said = div().min_w_0().overflow_hidden().text_ellipsis().text_color(color).child(dotted(said, c));
        let key = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(HEATMAP_GAP))
            .child(div().pr(px(3.)).text_color(c.muted_foreground).child("Less"))
            .children(
                (0..5).map(|level| div().size(px(10.)).rounded(px(Radius::XS.min(10. / 4.))).bg(level_fill(level, c))),
            )
            .child(div().pl(px(3.)).text_color(c.muted_foreground).child("More"));
        let caption = div().flex().text_size(TextSize::Xxs).font_features(tabular_numbers()).whitespace_nowrap();
        if !fits {
            return caption.flex_col().items_start().gap(px(8.)).child(said).child(key);
        }
        caption.items_center().gap(px(8.)).child(said).child(div().flex_1().min_w(px(8.))).child(key)
    }

    fn turns(&self, report: &ActivityReport, cx: &App) -> AnyElement {
        let outcomes = self.outcomes(report, cx);
        let breakdown = self.breakdown(report, cx);
        let cards = if self.beside() {
            div().flex().gap(px(12.)).child(outcomes.flex_1().min_w_0()).child(breakdown.flex_1().min_w_0())
        } else {
            div().flex().flex_col().gap(px(12.)).child(outcomes).child(breakdown)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Turns", None, None, false, cx))
            .child(cards)
            .when(!report.facts.is_empty(), |turns| {
                turns.child(stat_grid(&report.facts, self.columns(), TextSize::Lg, cx))
            })
            .into_any_element()
    }

    fn outcomes(&self, report: &ActivityReport, cx: &App) -> Div {
        let c = colors(cx);
        let rows = separated(
            report.outcomes.iter().map(|line| line_row(line, Some(outcome_color(&line.name, c)), cx)).collect(),
            cx,
        );
        card_default(div().flex().flex_col(), cx)
            .child(
                table_header(cx)
                    .px(px(14.))
                    .text_size(TextSize::Sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground)
                    .child("How they ended"),
            )
            .child(divider_on(Surface::Card, cx))
            .children(rows)
    }

    fn breakdown(&self, report: &ActivityReport, cx: &App) -> Div {
        let offered: Vec<Breakdown> = Breakdown::ALL
            .into_iter()
            .filter(|breakdown| *breakdown != Breakdown::Projects || report.projects.len() > 1)
            .collect();
        let chosen: Breakdown = self.pref(BREAKDOWN_KEY, cx);
        let current = if offered.contains(&chosen) { chosen } else { Breakdown::Agents };
        let shown = match current {
            Breakdown::Agents => &report.agents,
            Breakdown::Models => &report.models,
            Breakdown::Projects => &report.projects,
        };
        let rows = separated(shown.iter().map(|line| line_row(line, None, cx)).collect(), cx);
        let picker = self.picker(BREAKDOWN_KEY, offered, current, Breakdown::label).fills(true);
        card_default(div().flex().flex_col(), cx)
            .child(div().min_h(px(40.)).px(px(8.)).flex().items_center().child(picker))
            .child(divider_on(Surface::Card, cx))
            .children(rows)
    }

    fn features(&self, report: &ActivityReport, cx: &mut Context<Self>) -> AnyElement {
        let offered: Vec<Who> = [Who::You, Who::Agents]
            .into_iter()
            .filter(|who| match who {
                Who::You => !report.features.is_empty(),
                Who::Agents => !report.agent_features.is_empty(),
            })
            .collect();
        let chosen: Who = self.pref(WHO_KEY, cx);
        let current = if offered.contains(&chosen) { chosen } else { offered.first().copied().unwrap_or_default() };
        let stacked = !self.beside() && offered.len() > 1;
        let picker = (offered.len() > 1)
            .then(|| self.picker(WHO_KEY, offered, current, Who::label).fills(!self.beside()).into_any_element());
        let shown = match current {
            Who::You => {
                let cards = report
                    .features
                    .iter()
                    .map(|group| {
                        let key = format!("you:{}", group.name).into();
                        self.feature_card(key, &group.name, group.count, &group.features, cx)
                    })
                    .collect();
                grid(cards, if self.beside() { 2 } else { 1 }, 12., false)
            }
            Who::Agents => {
                let count = report.agent_features.iter().map(|feature| feature.count).sum();
                self.feature_card("agents".into(), "Motile tools", count, &report.agent_features, cx)
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Features", None, picker, stacked, cx))
            .child(shown)
            .into_any_element()
    }

    /// The features of a group as a table under a header of its name and total, each with how
    /// often it was used: the five used most, and the rest once asked for. `key` keeps which groups show them all.
    fn feature_card(
        &self,
        key: SharedString,
        title: &str,
        count: u32,
        features: &[FeatureLine],
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let all = self.expanded.contains(&key);
        let listed = if all { features } else { &features[..features.len().min(FEATURES_SHOWN)] };
        let rows = separated(listed.iter().map(|feature| feature_row(feature, cx)).collect(), cx);
        let more = (features.len() > FEATURES_SHOWN).then(|| {
            let label = if all { "Show fewer".to_string() } else { format!("Show all {}", features.len()) };
            let id = ElementId::from(SharedString::from(format!("activity-more-{key}")));
            let button = ActionButton::new(id, label).ghost().small().surface(Surface::Card).on_click(cx.listener(
                move |this, _, _, cx| {
                    if !this.expanded.remove(&key) {
                        this.expanded.insert(key.clone());
                    }
                    cx.notify();
                },
            ));
            div().p(px(6.)).flex().justify_center().child(button)
        });
        card_default(div().flex().flex_col(), cx)
            .child(
                table_header(cx)
                    .px(px(14.))
                    .gap(px(8.))
                    .text_size(TextSize::Sm)
                    .font_weight(FontWeight::MEDIUM)
                    .child(div().flex_1().text_color(c.foreground).child(title.to_string()))
                    .child(div().text_color(c.muted_foreground).font_features(tabular_numbers()).child(number(count))),
            )
            .child(divider_on(Surface::Card, cx))
            .children(rows)
            .when_some(more, |card, more| card.child(divider_on(Surface::Card, cx)).child(more))
    }
}

/// The header row of a card that is a table, filled as a table's header is.
fn table_header(cx: &App) -> Div {
    div().min_h(px(40.)).flex().items_center().rounded_t(px(Radius::LG)).bg(colors(cx).card_accent)
}

impl Render for ActivityView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let period = store.usage_period();
        if period != self.period {
            self.period = period;
            self.pointed = None;
        }
        let Some(report) = store.usage.activity.value().cloned() else { return div() };
        let outdated: Vec<String> = store
            .servers
            .iter()
            .filter(|server| server.connected() && server.protocol_version < 40)
            .map(|server| server.name.clone())
            .collect();
        let body: Vec<AnyElement> = if report.empty {
            vec![super::note(
                "Nothing was done in this time. Servers keep what is done from the version that shows this on.",
                cx,
            )]
        } else {
            let mut sections = vec![self.tiles(&report, cx), self.over_time(&report, cx)];
            sections.push(self.hours(&report, window, cx));
            if !report.clients.is_empty() {
                sections.push(self.clients(&report, cx));
            }
            if !report.outcomes.is_empty() {
                sections.push(self.turns(&report, cx));
            }
            if !report.features.is_empty() || !report.agent_features.is_empty() {
                sections.push(self.features(&report, cx));
            }
            sections
        };
        div().flex().flex_col().gap(px(36.)).children(body).children(
            outdated
                .into_iter()
                .map(|name| caption(format!("Update {name} to see what was done on it."), c.muted_foreground)),
        )
    }
}

/// A section's title, with what it says under it and a control on its right, or `stacked` across
/// the row under it, where a narrow page has no room beside the title.
fn heading(title: &str, detail: Option<String>, trailing: Option<AnyElement>, stacked: bool, cx: &App) -> Div {
    let c = colors(cx);
    let words = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .text_size(TextSize::Md)
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .child(title.to_string()),
        )
        .when_some(detail, |words, detail| {
            words.child(div().text_size(TextSize::Xs).text_color(c.muted_foreground).child(detail))
        });
    if stacked {
        return div().flex().flex_col().gap(px(10.)).child(words.px(px(4.))).children(trailing);
    }
    div().px(px(4.)).flex().items_center().gap(px(12.)).child(words.flex_1().min_w_0()).children(trailing)
}

/// An outcome, an agent, a model, a project or a starter: its name, how long its turns took, and
/// its part of them.
fn line_row(line: &ActivityLine, tint: Option<Hsla>, cx: &App) -> AnyElement {
    let c = colors(cx);
    let agent = line.agent.map(|agent| logos::agent_color(agent, c));
    div()
        .px(px(14.))
        .py(px(11.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(TextSize::Xs)
        .font_features(tabular_numbers())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .when_some(agent.or(tint), |row, color| row.child(dot(color)))
                .child(middle_truncated(line.name.clone()).flex_shrink(1.).text_color(c.foreground))
                .when_some(line.server.clone(), |row, server| {
                    row.child(div().flex_shrink_0().whitespace_nowrap().text_color(c.muted_foreground).child(server))
                })
                .child(div().flex_1().min_w(px(8.)))
                .child(
                    div().flex_shrink_0().whitespace_nowrap().text_color(c.muted_foreground).child(line.detail.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(36.))
                        .text_right()
                        .text_color(c.foreground)
                        .child(number(line.turns)),
                ),
        )
        .child(share_bar(line.share, tint.or(agent).unwrap_or(c.muted_foreground), c))
        .into_any_element()
}

/// Numbers in cards, `columns` to a row, every card of a row as tall as its tallest.
fn stat_grid(stats: &[ActivityFact], columns: usize, value_size: TextSize, cx: &App) -> Div {
    let c = colors(cx);
    let cards = stats
        .iter()
        .map(|stat| {
            card_default(div(), cx)
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .font_features(tabular_numbers())
                .child(
                    div()
                        .text_size(TextSize::Xs)
                        .text_color(c.muted_foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(stat.label.clone()),
                )
                .child(
                    div()
                        .text_size(value_size)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c.foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(stat.value.clone()),
                )
                .child(
                    div().text_size(TextSize::Xxs).text_color(c.muted_stronger_foreground).child(stat.detail.clone()),
                )
        })
        .collect();
    grid(cards, columns, 10., true)
}

/// A feature: its name with its variant under it, and how often it was used.
fn feature_row(feature: &FeatureLine, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .min_h(px(40.))
        .px(px(14.))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(TextSize::Xs)
        .font_features(tabular_numbers())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .mr(px(8.))
                .py(px(8.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_color(c.foreground).line_clamp(2).child(feature.label.clone()))
                .when(!feature.detail.is_empty(), |words| {
                    words.child(
                        div()
                            .text_size(TextSize::Xxs)
                            .text_color(c.muted_foreground)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(feature.detail.clone()),
                    )
                }),
        )
        .child(share_bar(feature.share, c.chart_1, c).w(px(FEATURE_BAR_WIDTH)).flex_shrink_0())
        .child(div().flex_shrink_0().min_w(px(32.)).text_right().text_color(c.foreground).child(number(feature.count)))
        .into_any_element()
}

/// The rows with a divider between each two.
fn separated(rows: Vec<AnyElement>, cx: &App) -> Vec<AnyElement> {
    let last = rows.len().saturating_sub(1);
    rows.into_iter()
        .enumerate()
        .flat_map(|(index, row)| {
            let divider = (index < last).then(|| divider_on(Surface::Card, cx).into_any_element());
            std::iter::once(row).chain(divider)
        })
        .collect()
}

/// The cells in rows of `columns`, each as wide as the others, and as tall as its row's tallest
/// when `equal_heights`.
fn grid(cells: Vec<Div>, columns: usize, spacing: f32, equal_heights: bool) -> Div {
    let columns = columns.max(1);
    let mut cells = cells.into_iter().peekable();
    let mut rows = Vec::new();
    while cells.peek().is_some() {
        let row: Vec<Div> = cells.by_ref().take(columns).collect();
        let empty = columns - row.len();
        rows.push(
            div()
                .flex()
                .when(!equal_heights, |row| row.items_start())
                .gap(px(spacing))
                .children(
                    row.into_iter().map(|cell| div().flex_1().min_w_0().flex().flex_col().child(cell.flex_grow(1.))),
                )
                .children((0..empty).map(|_| div().flex_1().min_w_0())),
        );
    }
    div().flex().flex_col().gap(px(spacing)).children(rows)
}

/// A part of the whole as a thin bar across what it is given.
fn share_bar(share: f64, color: Hsla, c: &Colors) -> Div {
    let share = share.clamp(0., 1.) as f32;
    div()
        .w_full()
        .h(px(4.))
        .rounded_full()
        .bg(Surface::Card.layer(Layer::Control, c))
        .child(div().w(relative(share)).min_w(px(if share > 0. { 4. } else { 0. })).h_full().rounded_full().bg(color))
}

fn shown_sources(report: &ActivityReport, measure: Measure) -> impl Iterator<Item = &ActivitySource> {
    report.sources.iter().filter(move |source| measure.total(source) > 0.)
}

/// Each bucket's sources stacked from the first, the bucket the pointer is over in full and the
/// others faded.
fn paint_bars(
    bounds: Bounds<Pixels>,
    series: &[(Hsla, Vec<f64>)],
    top: f64,
    pointed: Option<usize>,
    dimmed: f32,
    window: &mut Window,
) {
    let count = series.first().map_or(0, |(_, values)| values.len());
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if count == 0 || top <= 0. || width <= 0. {
        return;
    }
    let band = width / count as f32;
    let bar = (band * BAR_FILL).max(1.);
    for index in 0..count {
        let alpha = pointed.map_or(1., |pointed| if pointed == index { 1. } else { dimmed });
        let x = bounds.origin.x + px(band * index as f32 + (band - bar) / 2.);
        let mut base = height;
        for (color, values) in series {
            let tall = height * (values[index] / top) as f32;
            if tall <= 0. {
                continue;
            }
            let segment = Bounds::new(point(x, bounds.origin.y + px(base - tall)), size(px(bar), px(tall)));
            window.paint_quad(fill(segment, color.opacity(alpha)).corner_radii(px(2.)));
            base -= tall;
        }
    }
}

fn chart_color(index: u8, c: &Colors) -> Hsla {
    match index {
        1 => c.chart_1,
        2 => c.chart_2,
        3 => c.chart_3,
        4 => c.chart_4,
        5 => c.chart_5,
        6 => c.chart_6,
        _ => c.muted_foreground,
    }
}

fn outcome_color(name: &str, c: &Colors) -> Hsla {
    match name {
        "Finished" => c.success,
        "Stopped" => c.muted_foreground,
        "Hit the usage limit" => c.warning,
        _ => c.destructive,
    }
}

fn level_fill(level: u8, c: &Colors) -> Hsla {
    match level {
        0 => Surface::Card.layer(Layer::Control, c),
        1 => c.at(c.chart_1, Opacity::ColorTintChart),
        2 => c.at(c.chart_1, Opacity::ColorTintChartStronger),
        3 => c.at(c.chart_1, Opacity::ColorTintChartStrongest),
        _ => c.chart_1,
    }
}

/// The Lucide icon of a symbol the core names by its name in `Symbol.swift`.
fn symbol_name(name: &str) -> &'static str {
    match name {
        "laptop" => "laptop",
        "smartphone" => "smartphone",
        "tablet" => "tablet",
        "monitor" => "monitor",
        "bot" => "bot",
        "sparkles" => "sparkles",
        _ => "monitor-smartphone",
    }
}

fn hour_label(hour: u8) -> String {
    match hour {
        0 => "12 am".into(),
        12 => "12 pm".into(),
        1..12 => format!("{hour} am"),
        _ => format!("{} pm", hour - 12),
    }
}

fn pointed_label(start: f64, period: UsagePeriod) -> String {
    if period == UsagePeriod::Day {
        format::date(start, "EEEj", "%a %-I %p")
    } else {
        format::date(start, "EEEMMMd", "%a, %b %-d")
    }
}

#[cfg(test)]
mod tests {
    use super::hour_label;

    #[test]
    fn hours_read_as_on_a_twelve_hour_clock() {
        let labels: Vec<String> = [0, 6, 11, 12, 18, 23].into_iter().map(hour_label).collect();
        assert_eq!(labels, ["12 am", "6 am", "11 am", "12 pm", "6 pm", "11 pm"]);
    }
}
