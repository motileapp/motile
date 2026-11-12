//! What the user and the agents did on the servers counted, the Activity tab of the usage route:
//! the headline numbers, the work over time by where it came from, the clients it came from, the
//! times it was done at, how the turns went and the features used.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::activity::{duration, number};
use serde::{Deserialize, Serialize};

use super::spending::{CAPTION, CHART_HEIGHT, X_GUTTER, Y_GUTTER, axis_label, axis_ticks, caption, dot, ticks};
use crate::format;
use crate::models::ago;
use crate::store::Store;
use crate::store::usage::{
    ActivityClient, ActivityFact, ActivityHeatmap, ActivityLine, ActivityReport, ActivitySource, FeatureLine,
    HeatmapShape, UsagePeriod,
};
use crate::theme::{Colors, ControlSize, IconSize, Layer, Opacity, Radius, Surface, TextSize, colors};
use crate::ui::control::dotted;
use crate::ui::menu::Anchor;
use crate::ui::{
    Chip, FadingScroll, Segmented, card_default, divider_on, icons, logos, middle_truncated, tabular_numbers,
    tabular_text_width, text_width,
};

/// Narrower than this, the tiles go two to a row and the turns' cards under each other.
const BESIDE_WIDTH: f32 = 640.;
/// How much of its bucket's width a bar takes.
const BAR_FILL: f32 = 0.7;
const BADGE: f32 = 32.;
const FEATURE_BAR_WIDTH: f32 = 56.;
const HEATMAP_PADDING: f32 = 16.;
const HEATMAP_GAP: f32 = 3.;
const HEATMAP_CELL_HEIGHT: f32 = 18.;
const MEASURE_KEY: &str = "activity.measure";
const BREAKDOWN_KEY: &str = "activity.breakdown";
const WHO_KEY: &str = "activity.who";
const HOURS_KEY: &str = "activity.hours";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Measure {
    #[default]
    Messages,
    Turns,
    Engaged,
    Time,
}

impl Measure {
    const ALL: [Measure; 4] = [Measure::Messages, Measure::Turns, Measure::Engaged, Measure::Time];

    fn label(self) -> &'static str {
        match self {
            Measure::Messages => "Messages",
            Measure::Turns => "Turns",
            Measure::Engaged => "Your time",
            Measure::Time => "Agent time",
        }
    }

    fn total(self, source: &ActivitySource) -> f64 {
        match self {
            Measure::Messages => f64::from(source.messages),
            Measure::Turns => f64::from(source.turns),
            Measure::Engaged => source.engaged_seconds,
            Measure::Time => source.seconds,
        }
    }

    fn at(self, source: &ActivitySource, index: usize) -> f64 {
        match self {
            Measure::Messages => source.message_points.get(index).map_or(0., |value| f64::from(*value)),
            Measure::Turns => source.turn_points.get(index).map_or(0., |value| f64::from(*value)),
            Measure::Engaged => source.engaged_points.get(index).copied().unwrap_or(0.),
            Measure::Time => source.second_points.get(index).copied().unwrap_or(0.),
        }
    }

    fn percent(self, source: &ActivitySource) -> u32 {
        match self {
            Measure::Messages => source.message_percent,
            Measure::Turns => source.turn_percent,
            Measure::Engaged => source.engaged_percent,
            Measure::Time => source.second_percent,
        }
    }

    fn amount(self, value: f64) -> String {
        match self {
            Measure::Messages | Measure::Turns => number(value.max(0.).round() as u32),
            Measure::Engaged | Measure::Time => duration(value),
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

/// What the hours of the week are coloured by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HourMeasure {
    #[default]
    Time,
    Messages,
}

impl HourMeasure {
    const ALL: [HourMeasure; 2] = [HourMeasure::Time, HourMeasure::Messages];

    fn label(self) -> &'static str {
        match self {
            HourMeasure::Time => "Time spent",
            HourMeasure::Messages => "Messages sent",
        }
    }
}

/// The most a long list shows before it scrolls: half the window.
fn list_height(window: &Window) -> f32 {
    (f32::from(window.viewport_size().height) / 2.).max(240.)
}

pub struct ActivityView {
    store: Entity<Store>,
    /// The bucket the pointer is over.
    pointed: Option<usize>,
    /// Where the plot was drawn, to tell which bucket the pointer is over.
    plot: Anchor,
    /// The heatmap's cell the pointer was over or clicked last, with the heatmap's shape then.
    chosen: Option<(usize, HeatmapShape)>,
    /// How wide the content is, which lays it out.
    pub width: f32,
    period: UsagePeriod,
}

impl ActivityView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let period = store.read(cx).usage_period();
        Self { store, pointed: None, plot: Anchor::default(), chosen: None, width: 0., period }
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
            .child(heading("Over time", None, Some(picker.into_any_element()), true, cx))
            .child(self.legend(report, measure, cx))
            .child(self.chart(report, measure, cx))
            .into_any_element()
    }

    /// Each source with its total and its share of it, or what it did in the bucket the pointer is over, whose date
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
                    let shown = index.map_or_else(
                        || format!("{} ({}%)", measure.amount(measure.total(source)), measure.percent(source)),
                        |index| measure.amount(measure.at(source, index)),
                    );
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .whitespace_nowrap()
                        .child(dot(chart_color(source.color, c)))
                        .child(icons::symbol(symbol_name(&source.symbol), IconSize::Xs).text_color(c.muted_foreground))
                        .child(div().text_color(c.foreground).child(source.label.clone()))
                        .child(div().text_color(c.muted_foreground).font_features(tabular_numbers()).child(shown))
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
        let durations = match measure {
            Measure::Messages | Measure::Turns => None,
            Measure::Engaged => Some(&report.engaged_ticks),
            Measure::Time => Some(&report.second_ticks),
        };
        let (step, top) = match durations.filter(|ticks| ticks.len() > 1) {
            Some(lines) => (lines[1], lines[lines.len() - 1]),
            None => ticks(max),
        };
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

    fn clients(&self, report: &ActivityReport, window: &Window, cx: &App) -> AnyElement {
        let rows = separated(report.clients.iter().map(|client| self.client_row(client, cx)).collect(), cx);
        let list = FadingScroll::new("activity-clients", list_height(window)).fill(colors(cx).card).children(rows);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Clients", Some("Where you worked from".into()), None, false, cx))
            .child(card_default(div().flex().flex_col(), cx).child(list))
            .into_any_element()
    }

    /// A client: its device, what it is, how it reaches the servers, and under them its part of your
    /// time or of the messages, as wide as every client's so that the bars line up.
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
        let summary = client.summary.clone();
        let seen = if client.active {
            Some("Active now".to_string())
        } else {
            client.last_seen.map(|seen| {
                let ago = ago(seen);
                if ago == "now" { "Seen just now".to_string() } else { format!("Seen {ago} ago") }
            })
        };
        let main = div().flex().items_center().gap(px(12.)).child(badge).child(about);
        let numbers = div()
            .flex()
            .justify_between()
            .gap(px(8.))
            .text_size(TextSize::Xxs)
            .whitespace_nowrap()
            .font_features(tabular_numbers())
            .child(div().text_color(c.foreground).child(summary))
            .when_some(seen, |numbers, seen| numbers.child(div().text_color(c.muted_stronger_foreground).child(seen)));
        div()
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
            .into_any_element()
    }

    fn hours(&self, report: &ActivityReport, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let measure: HourMeasure = self.pref(HOURS_KEY, cx);
        let picker =
            self.picker(HOURS_KEY, HourMeasure::ALL.to_vec(), measure, HourMeasure::label).fills(!self.beside());
        let busiest = match measure {
            HourMeasure::Time => report.busiest_time.clone(),
            HourMeasure::Messages => report.busiest_messages.clone(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("When you work", busiest, Some(picker.into_any_element()), true, cx))
            .child(card_default(div().p(px(HEATMAP_PADDING)), cx).child(self.heatmap(
                &report.heatmap,
                measure,
                window,
                cx,
            )))
            .into_any_element()
    }

    /// When the work was done, each slice as busy as its colour is strong. The slice the pointer was
    /// over or clicked last says what was done in it, the slice of now to start with. The gaps
    /// between the slices are only drawn: each one's hit area is an even share of its cell's.
    fn heatmap(
        &self,
        heatmap: &ActivityHeatmap,
        measure: HourMeasure,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let of = heatmap.shape();
        let shown = self.shown(heatmap);
        let inner = self.width - 2. * HEATMAP_PADDING - 2.;
        let labels = heatmap
            .rows
            .iter()
            .map(|row| text_width(row, TextSize::Xxs, FontWeight::NORMAL, window))
            .fold(0., f32::max)
            .ceil();
        let beside_labels = if heatmap.rows.is_empty() { 0. } else { labels + 8. };
        let width = (inner - beside_labels).max(0.);
        let pitch = (width + HEATMAP_GAP) / heatmap.columns.max(1) as f32;
        let cell_width = (pitch - HEATMAP_GAP).max(1.);
        let label_line = (TextSize::Xxs.points() * crate::theme::LINE_HEIGHT).ceil();
        let height = cell_width.clamp(4., HEATMAP_CELL_HEIGHT).max(label_line);
        let corner = Radius::XS.min(cell_width.min(height) / 4.);
        let label = |text: &str| {
            div().text_size(TextSize::Xxs).text_color(c.muted_foreground).whitespace_nowrap().child(text.to_string())
        };
        let row_labels = div()
            .flex_shrink_0()
            .w(px(labels))
            .flex()
            .flex_col()
            .gap(px(HEATMAP_GAP))
            .children(heatmap.rows.iter().map(|row| label(row).h(px(height)).flex().items_center()));
        let rows = (0..heatmap.row_count()).map(|row| {
            div().flex().children((0..heatmap.columns).map(|column| {
                let index = row * heatmap.columns + column;
                let slot = div().flex_shrink_0().w(px(pitch)).h(px(height + HEATMAP_GAP));
                let Some(cell) = heatmap.cells.get(index).and_then(Option::as_ref) else {
                    return slot.into_any_element();
                };
                let level = match measure {
                    HourMeasure::Time => cell.time_level,
                    HourMeasure::Messages => cell.message_level,
                };
                let shape = div()
                    .absolute()
                    .top(px(HEATMAP_GAP / 2.))
                    .left(px(HEATMAP_GAP / 2.))
                    .w(px(cell_width))
                    .h(px(height))
                    .rounded(px(corner))
                    .bg(level_fill(level, c))
                    .when(shown == index, |shape| shape.border_1().border_color(c.foreground));
                slot.relative()
                    .id(("activity-cell", index))
                    .child(shape)
                    .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                        if *inside {
                            this.choose(index, of, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose(index, of, cx)))
                    .into_any_element()
            }))
        });
        let cells = div().w(px(width + HEATMAP_GAP)).m(px(-HEATMAP_GAP / 2.)).flex().flex_col().children(rows);
        let axis = div().relative().w(px(width)).child(label(" ").invisible()).children(
            heatmap.axis.iter().map(|axis| label(&axis.label).absolute().top_0().left(px(axis.column as f32 * pitch))),
        );
        let grid = div()
            .flex()
            .items_start()
            .gap(px(8.))
            .when(!heatmap.rows.is_empty(), |grid| grid.child(row_labels))
            .child(div().flex_shrink_0().w(px(width)).flex().flex_col().gap(px(6.)).child(cells).child(axis));
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(grid)
            .child(self.heatmap_caption(heatmap, shown, inner, window, c))
            .into_any_element()
    }

    /// The cell chosen in this heatmap's shape, or the cell of now.
    fn shown(&self, heatmap: &ActivityHeatmap) -> usize {
        self.chosen.filter(|(_, shape)| *shape == heatmap.shape()).map_or(heatmap.latest, |(index, _)| index)
    }

    fn choose(&mut self, index: usize, shape: HeatmapShape, cx: &mut Context<Self>) {
        if self.chosen != Some((index, shape)) {
            self.chosen = Some((index, shape));
            cx.notify();
        }
    }

    /// What the chosen cell holds and the key to the colours: beside each other, or the key under it
    /// where they don't fit.
    fn heatmap_caption(
        &self,
        heatmap: &ActivityHeatmap,
        shown: usize,
        room: f32,
        window: &mut Window,
        c: &Colors,
    ) -> Div {
        let said = heatmap.cells.get(shown).and_then(Option::as_ref).map_or_else(String::new, |cell| {
            let messages =
                if cell.messages == 1 { "1 message".into() } else { format!("{} messages", number(cell.messages)) };
            let turns = if cell.turns == 1 { "1 turn".into() } else { format!("{} turns", number(cell.turns)) };
            if cell.time.is_empty() {
                format!("{} · {messages} · {turns}", cell.label)
            } else {
                format!("{} · {} · {messages} · {turns}", cell.label, cell.time)
            }
        });
        let words = |text: &str, window: &mut Window| text_width(text, TextSize::Xxs, FontWeight::NORMAL, window);
        let key_width = words("Less", window) + words("More", window) + 2. * 3. + 5. * 10. + 6. * HEATMAP_GAP;
        let fits = tabular_text_width(&said, TextSize::Xxs, FontWeight::NORMAL, window) + 8. + key_width <= room;
        let said =
            div().min_w_0().overflow_hidden().text_ellipsis().text_color(c.muted_foreground).child(dotted(said, c));
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

    fn turns(&self, report: &ActivityReport, window: &Window, cx: &App) -> AnyElement {
        let outcomes = self.outcomes(report, cx);
        let breakdown = self.breakdown(report, window, cx);
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
                    .min_h(px(breakdown_header_height()))
                    .px(px(14.))
                    .text_size(TextSize::Sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground)
                    .child("How they ended"),
            )
            .child(divider_on(Surface::Card, cx))
            .children(rows)
    }

    fn breakdown(&self, report: &ActivityReport, window: &Window, cx: &App) -> Div {
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
            .child(div().p(px(BREAKDOWN_PADDING)).flex().items_center().child(picker))
            .child(divider_on(Surface::Card, cx))
            .child(FadingScroll::new("activity-breakdown", list_height(window)).fill(colors(cx).card).children(rows))
    }

    fn features(&self, report: &ActivityReport, window: &Window, cx: &App) -> AnyElement {
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
                let column = |groups: Vec<&crate::store::usage::FeatureGroup>| {
                    div().flex_1().min_w_0().flex().flex_col().gap(px(12.)).children(groups.into_iter().map(|group| {
                        self.feature_card(
                            &format!("you-{}", group.name),
                            &group.name,
                            group.count,
                            &group.features,
                            window,
                            cx,
                        )
                    }))
                };
                match self.beside() {
                    true => div()
                        .flex()
                        .items_start()
                        .gap(px(12.))
                        .child(column(report.features.iter().filter(|group| group.column == 0).collect()))
                        .child(column(report.features.iter().filter(|group| group.column == 1).collect()))
                        .into_any_element(),
                    false => column(report.features.iter().collect()).into_any_element(),
                }
            }
            Who::Agents => {
                let count = report.agent_features.iter().map(|feature| feature.count).sum();
                self.feature_card("agents", "Motile tools", count, &report.agent_features, window, cx)
                    .into_any_element()
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
    /// often it was used, scrolling past half the window.
    fn feature_card(
        &self,
        key: &str,
        title: &str,
        count: u32,
        features: &[FeatureLine],
        window: &Window,
        cx: &App,
    ) -> Div {
        let c = colors(cx);
        let rows = separated(features.iter().map(|feature| feature_row(feature, cx)).collect(), cx);
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
            .child(
                FadingScroll::new(SharedString::from(format!("activity-features-{key}")), list_height(window))
                    .fill(c.card)
                    .children(rows),
            )
    }
}

const BREAKDOWN_PADDING: f32 = 4.;

/// The breakdown's picker with its padding, which the outcomes' header beside it matches.
fn breakdown_header_height() -> f32 {
    ControlSize::Regular.height() + 2. * (Segmented::INSET + Segmented::BORDER + BREAKDOWN_PADDING)
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
                sections.push(self.clients(&report, window, cx));
            }
            if !report.outcomes.is_empty() {
                sections.push(self.turns(&report, window, cx));
            }
            if !report.features.is_empty() || !report.agent_features.is_empty() {
                sections.push(self.features(&report, window, cx));
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
    let overhang = (ControlSize::Regular.height() - ControlSize::Small.height()) / 2.;
    div()
        .px(px(4.))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(words.flex_1().min_w_0())
        .children(trailing.map(|trailing| div().my(px(-overhang)).child(trailing)))
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
        7 => c.pending,
        8 => c.at(c.pending, Opacity::ColorTintChartStronger),
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
        _ => "monitor-smartphone",
    }
}

fn pointed_label(start: f64, period: UsagePeriod) -> String {
    if period == UsagePeriod::Day {
        format::date(start, "EEEj", "%a %H:00")
    } else {
        format::date(start, "EEEMMMd", "%a, %b %-d")
    }
}
