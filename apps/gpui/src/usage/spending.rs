//! What the agents spent on the servers counted, the Cost and Tokens tabs of the usage route:
//! the total and each agent's part beside a line for each agent over time, then what each model,
//! project, server, account and kind of token took.

use chrono::{Datelike, Local, TimeZone};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::{Deserialize, Serialize};

use crate::format::{self, compact as count, percent, usd as cost};
use crate::models::agent_name;
use crate::store::Store;
use crate::store::usage::{UsageLine, UsagePeriod, UsageReport, UsageSeries, UsageTab};
use crate::theme::{Colors, Layer, Opacity, Surface, colors};
use crate::ui::menu::Anchor;
use crate::ui::{Segmented, card_default, divider_on, logos, tabular_numbers, text_width};

const CHART_HEIGHT: f32 = 190.;
/// Room for the labels of the y axis on the right and of the x axis below.
const Y_GUTTER: f32 = 48.;
const X_GUTTER: f32 = 18.;
/// How far the first and the last point stand from the plot's sides.
const PLOT_PADDING: f32 = 16.;
const BAR_WIDTH: f32 = 72.;
const DOT: f32 = 8.;
const CAPTION: f32 = 10.;
/// Narrower than this, the chart goes under the totals.
const BESIDE_WIDTH: f32 = 640.;
/// Narrower than this, a row's bar goes under its name and numbers.
const STACK_WIDTH: f32 = 520.;
const SUMMARY_WIDTH: f32 = 240.;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Breakdown {
    #[default]
    Models,
    Projects,
    Servers,
    Accounts,
    Kinds,
}

impl Breakdown {
    const ALL: [Breakdown; 5] =
        [Breakdown::Models, Breakdown::Projects, Breakdown::Servers, Breakdown::Accounts, Breakdown::Kinds];

    fn label(self) -> &'static str {
        match self {
            Breakdown::Models => "Models",
            Breakdown::Projects => "Projects",
            Breakdown::Servers => "Servers",
            Breakdown::Accounts => "Accounts",
            Breakdown::Kinds => "Tokens",
        }
    }
}

pub struct SpendingView {
    store: Entity<Store>,
    /// The point of the series the pointer is over.
    pointed: Option<usize>,
    /// Where the plot was drawn, to tell which point the pointer is over.
    plot: Anchor,
    /// How wide the content is, which lays it out.
    pub width: f32,
    period: UsagePeriod,
}

impl SpendingView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let period = store.read(cx).usage_period();
        Self { store, pointed: None, plot: Anchor::default(), width: 0., period }
    }

    fn cost_shown(&self, cx: &App) -> bool {
        self.store.read(cx).usage_tab() == UsageTab::Cost
    }

    fn point_at(&mut self, position: Point<Pixels>, count: usize, cx: &mut Context<Self>) {
        let bounds = self.plot.bounds();
        let inner = f32::from(bounds.size.width) - 2. * PLOT_PADDING;
        let pointed = if !bounds.contains(&position) || count == 0 || inner <= 0. {
            None
        } else {
            let fraction = ((f32::from(position.x - bounds.origin.x) - PLOT_PADDING) / inner).clamp(0., 1.);
            Some((fraction * (count - 1) as f32).round() as usize)
        };
        if self.pointed != pointed {
            self.pointed = pointed;
            cx.notify();
        }
    }

    fn summary(&self, report: &UsageReport, cost_shown: bool, cx: &App) -> Div {
        let c = colors(cx);
        let total = if cost_shown { cost(report.cost_usd) } else { count(report.tokens) };
        let written = if cost_shown { cost(report.writing_cost_usd) } else { count(report.writing_tokens) };
        let numbers = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_features(tabular_numbers())
                    .text_color(c.foreground)
                    .child(total),
            )
            .child(caption(
                if cost_shown { "What the API would have charged" } else { "Tokens in and out" },
                c.muted_foreground,
            ))
            .when(report.writing_tokens > 0, |summary| {
                let of = if cost_shown { "it" } else { "them" };
                summary.child(caption(
                    format!("{written} of {of} for titles, branch names, commit messages and pull requests"),
                    c.muted_stronger_foreground,
                ))
            })
            .when(cost_shown && report.unpriced_tokens > 0, |summary| {
                let unpriced = count(report.unpriced_tokens);
                summary.child(caption(
                    format!("Without {unpriced} tokens of models whose price isn’t known"),
                    c.muted_stronger_foreground,
                ))
            });
        let agents = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(report.agents.iter().map(|series| agent_line(report, series, cost_shown, cx)))
            .when(report.cache_savings_usd > 0., |lines| {
                lines.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .child(div().flex_1().text_color(c.muted_foreground).child("Saved by the cache"))
                        .child(
                            div()
                                .text_color(c.foreground)
                                .font_features(tabular_numbers())
                                .child(cost(report.cache_savings_usd)),
                        ),
                )
            });
        div().flex().flex_col().gap(px(14.)).child(numbers).child(agents)
    }

    /// The agents with what each spent: in all, or at the time the pointer is over.
    fn legend(&self, report: &UsageReport, cost_shown: bool, cx: &App) -> AnyElement {
        let c = colors(cx);
        let index = self.pointed.filter(|index| *index < report.starts.len());
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .text_size(px(12.))
            .children(report.agents.iter().map(|series| {
                let total = if cost_shown { series.cost_usd } else { series.tokens as f64 };
                let shown = index.map(|index| value(series, index, cost_shown)).unwrap_or(total);
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(dot(logos::agent_color(series.agent, c)))
                    .child(div().text_color(c.foreground).child(agent_name(series.agent)))
                    .child(
                        div()
                            .text_color(c.muted_foreground)
                            .font_features(tabular_numbers())
                            .child(amount(shown, cost_shown)),
                    )
            }))
            .child(div().flex_1().min_w(px(8.)))
            .when_some(index, |legend, index| {
                legend
                    .child(div().text_color(c.muted_foreground).child(pointed_label(report.starts[index], self.period)))
            })
            .into_any_element()
    }

    fn chart(&self, report: &UsageReport, cost_shown: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let count = report.starts.len();
        let series: Vec<(Hsla, Vec<f64>)> = report
            .agents
            .iter()
            .map(|series| {
                let points = (0..count).map(|index| value(series, index, cost_shown)).collect();
                (logos::agent_color(series.agent, c), points)
            })
            .collect();
        let max = series.iter().flat_map(|(_, points)| points.iter().copied()).fold(0., f64::max);
        let (step, top) = ticks(max);
        let tick_count = (top / step).round() as usize;
        let labelled = axis_ticks(&report.starts, self.period);
        let fraction = |index: usize| if count > 1 { index as f32 / (count - 1) as f32 } else { 0. };
        let plot = self.plot.clone();
        let pointed = self.pointed.filter(|index| *index < count);
        let wash = c.opacity(Opacity::ColorTintChart);
        div()
            .w_full()
            .h(px(CHART_HEIGHT))
            .relative()
            .child(
                div()
                    .id("usage-plot")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right(px(Y_GUTTER))
                    .bottom(px(X_GUTTER))
                    .child(plot.track())
                    .children((0..=tick_count).map(|tick| {
                        let value = tick as f64 * step;
                        let from_top = relative(1. - (value / top) as f32);
                        div().absolute().left_0().right_0().top(from_top).h(px(1.)).bg(c.border).child(
                            div()
                                .absolute()
                                .left_full()
                                .top(px(-7.))
                                .w(px(Y_GUTTER))
                                .pl(px(6.))
                                .text_size(px(CAPTION))
                                .text_color(c.muted_foreground)
                                .whitespace_nowrap()
                                .child(amount(value, cost_shown)),
                        )
                    }))
                    .child(
                        div().absolute().left_0().right_0().top_0().bottom_0().child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| paint_series(bounds, &series, top, wash, window),
                            )
                            .size_full(),
                        ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(PLOT_PADDING))
                            .right(px(PLOT_PADDING))
                            .top_0()
                            .bottom_0()
                            .when_some(pointed, |track, index| {
                                track.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(relative(fraction(index)))
                                        .w(px(1.))
                                        .bg(c.border),
                                )
                            })
                            .child(div().absolute().left_0().right_0().top_full().h(px(X_GUTTER)).children(
                                labelled.into_iter().map(|index| {
                                    div()
                                        .absolute()
                                        .left(relative(fraction(index)))
                                        .ml(px(-30.))
                                        .w(px(60.))
                                        .pt(px(4.))
                                        .flex()
                                        .justify_center()
                                        .text_size(px(CAPTION))
                                        .text_color(c.muted_foreground)
                                        .whitespace_nowrap()
                                        .child(axis_label(report.starts[index], self.period))
                                }),
                            )),
                    )
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

    fn lines(&self, report: &UsageReport, cost_shown: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let offered: Vec<Breakdown> = Breakdown::ALL
            .into_iter()
            .filter(|breakdown| *breakdown != Breakdown::Accounts || !report.accounts.is_empty())
            .collect();
        let chosen = self.store.read(cx).prefs.get::<Breakdown>("usage.breakdown").unwrap_or_default();
        let current = if offered.contains(&chosen) { chosen } else { Breakdown::Models };
        let shown: Vec<UsageLine> = match current {
            Breakdown::Models => report.models.clone(),
            Breakdown::Projects => report.projects.clone(),
            Breakdown::Servers => report.servers.clone(),
            Breakdown::Accounts => report.accounts.clone(),
            Breakdown::Kinds => report.kind_lines(),
        };
        let stacked = self.width < STACK_WIDTH;
        let store = self.store.clone();
        let choices = offered.clone();
        let picker = Segmented::new(
            "usage-breakdown",
            offered.iter().map(|breakdown| SharedString::from(breakdown.label())).collect(),
            offered.iter().position(|breakdown| *breakdown == current).unwrap_or(0),
        )
        .fills(stacked)
        .on_select(move |index, _, cx| {
            store.update(cx, |store, cx| {
                store.prefs.set("usage.breakdown", choices[index]);
                cx.notify();
            })
        });
        let last = shown.len().saturating_sub(1);
        let mut rows = Vec::new();
        for (index, line) in shown.iter().enumerate() {
            rows.push(self.line_row(line, cost_shown, stacked, window, cx));
            if index < last {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(picker)
            .child(card_default(div().flex().flex_col(), cx).children(rows))
            .into_any_element()
    }

    /// A model, a project, a server, an account or a kind of token: its name, its part of the
    /// whole as a bar, its tokens and what they cost. Stacked, the bar runs under the rest across
    /// the row; a row too narrow for the whole name gives up the bar.
    fn line_row(&self, line: &UsageLine, cost_shown: bool, stacked: bool, window: &mut Window, cx: &App) -> AnyElement {
        let c = colors(cx);
        let numbers = 64. + 72.;
        let dot_room = if line.agent.is_some() { DOT + 10. } else { 0. };
        let mut name_width = text_width(&line.name, 12., FontWeight::NORMAL, window);
        if let Some(server) = &line.server {
            name_width += 10. + text_width(server, 12., FontWeight::NORMAL, window);
        }
        let room = self.width - 24. - dot_room - numbers - 3. * 10. - 8.;
        let bar_shown = !stacked && name_width + BAR_WIDTH + 10. <= room;
        let main = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .when_some(line.agent, |row, agent| row.child(dot(logos::agent_color(agent, c))))
            .child(crate::ui::middle_truncated(line.name.clone()).min_w_0().flex_shrink(1.).text_color(c.foreground))
            .when_some(line.server.clone(), |row, server| {
                row.child(crate::ui::middle_truncated(server).min_w_0().flex_shrink(1.).text_color(c.muted_foreground))
            })
            .child(div().flex_1().min_w(px(8.)))
            .when(bar_shown, |row| row.child(bar(line.share, Some(BAR_WIDTH), c)))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(64.))
                    .text_right()
                    .text_color(if cost_shown { c.muted_foreground } else { c.foreground })
                    .child(count(line.tokens)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(72.))
                    .text_right()
                    .text_color(if cost_shown && line.cost_usd.is_some() { c.foreground } else { c.muted_foreground })
                    .child(line.cost_usd.map(cost).unwrap_or_else(|| "No price".into())),
            );
        let row = div().px(px(12.)).text_size(px(12.)).font_features(tabular_numbers()).flex().flex_col();
        let row = if stacked {
            row.py(px(11.)).gap(px(8.)).child(main).child(div().pl(px(dot_room)).child(bar(line.share, None, c)))
        } else {
            row.min_h(px(36.)).justify_center().child(main)
        };
        row.into_any_element()
    }
}

impl Render for SpendingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let period = store.usage_period();
        if period != self.period {
            self.period = period;
            self.pointed = None;
        }
        let Some(report) = store.usage.spending.value().cloned() else { return div() };
        let cost_shown = self.cost_shown(cx);
        let outdated: Vec<String> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| server.connected() && server.protocol_version < 10)
            .map(|server| server.name.clone())
            .collect();
        let body: Vec<AnyElement> = if report.tokens == 0 {
            vec![super::note(
                "Nothing was spent in this time. Your servers keep what their agents spend from the version that shows this on.",
                cx,
            )]
        } else {
            let summary = self.summary(&report, cost_shown, cx);
            let chart = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(self.legend(&report, cost_shown, cx))
                .child(self.chart(&report, cost_shown, cx));
            let top = if self.width >= BESIDE_WIDTH {
                div().flex().items_start().gap(px(28.)).child(summary.w(px(SUMMARY_WIDTH)).flex_shrink_0()).child(chart)
            } else {
                div().flex().flex_col().gap(px(24.)).child(summary).child(chart)
            };
            vec![top.into_any_element(), self.lines(&report, cost_shown, window, cx)]
        };
        div().flex().flex_col().gap(px(24.)).children(body).children(
            outdated
                .into_iter()
                .map(|name| caption(format!("Update {name} to see what its agents spend."), c.muted_foreground)),
        )
    }
}

/// An agent's part: what it spent, and its share of the whole with the other measure.
fn agent_line(report: &UsageReport, series: &UsageSeries, cost_shown: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    let whole = if cost_shown { report.cost_usd } else { report.tokens as f64 };
    let part = if cost_shown { series.cost_usd } else { series.tokens as f64 };
    let share = if whole > 0. { part / whole } else { 0. };
    let other = if cost_shown { format!("{} tokens", count(series.tokens)) } else { cost(series.cost_usd) };
    let of = if cost_shown { "the cost" } else { "the tokens" };
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .text_size(px(12.))
                .child(dot(logos::agent_color(series.agent, c)))
                .child(logos::agent_icon(series.agent, 14., cx))
                .child(div().flex_1().text_color(c.foreground).child(agent_name(series.agent)))
                .child(div().text_color(c.foreground).font_features(tabular_numbers()).child(amount(part, cost_shown))),
        )
        .child(
            div()
                .pl(px(15.))
                .text_size(px(11.))
                .text_color(c.muted_stronger_foreground)
                .font_features(tabular_numbers())
                .child(format!("{} of {of} · {other}", percent(share))),
        )
        .into_any_element()
}

fn value(series: &UsageSeries, index: usize, cost_shown: bool) -> f64 {
    if cost_shown {
        series.cost_points.get(index).copied().unwrap_or(0.)
    } else {
        series.token_points.get(index).copied().unwrap_or(0) as f64
    }
}

fn amount(value: f64, cost_shown: bool) -> String {
    if cost_shown { cost(value) } else { count(value.round() as u64) }
}

/// The line's share of the whole, `width` wide or as wide as it is given.
fn bar(share: f64, width: Option<f32>, c: &Colors) -> Div {
    let share = share.clamp(0., 1.) as f32;
    div()
        .flex_shrink_0()
        .when_some(width, |bar, width| bar.w(px(width)))
        .when(width.is_none(), |bar| bar.w_full())
        .h(px(4.))
        .rounded_full()
        .bg(Surface::Card.layer(Layer::Control, c))
        .child(div().w(relative(share)).h_full().rounded_full().bg(c.muted_foreground))
}

fn dot(color: Hsla) -> Div {
    div().size(px(DOT)).rounded_full().bg(color).flex_shrink_0()
}

fn caption(text: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(px(CAPTION)).text_color(color).child(text.into())
}

/// The y axis: a round step with about four of them under the highest point, and the top line.
fn ticks(max: f64) -> (f64, f64) {
    if max <= 0. {
        return (1., 4.);
    }
    let raw = max / 4.;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1., 2., 2.5, 5., 10.]
        .into_iter()
        .map(|unit| unit * magnitude)
        .find(|step| *step >= raw)
        .unwrap_or(magnitude * 10.);
    (step, (max / step).ceil() * step)
}

/// Each agent's series as a line through its points and a wash under it.
fn paint_series(bounds: Bounds<Pixels>, series: &[(Hsla, Vec<f64>)], top: f64, wash: f32, window: &mut Window) {
    let width = f32::from(bounds.size.width) - 2. * PLOT_PADDING;
    let height = f32::from(bounds.size.height);
    let baseline = bounds.origin.y + px(height);
    for (color, values) in series {
        let count = values.len();
        if count == 0 || width <= 0. {
            continue;
        }
        let points: Vec<Point<Pixels>> = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let fraction = if count > 1 { index as f32 / (count - 1) as f32 } else { 0.5 };
                let x = bounds.origin.x + px(PLOT_PADDING + fraction * width);
                let y = bounds.origin.y + px(height * (1. - (*value / top) as f32));
                point(x, y)
            })
            .collect();
        let tangents = monotone_tangents(&points);
        let mut area = PathBuilder::fill();
        area.move_to(point(points[0].x, baseline));
        area.line_to(points[0]);
        let mut line = PathBuilder::stroke(px(2.));
        line.move_to(points[0]);
        for index in 0..count.saturating_sub(1) {
            let (from, to) = (points[index], points[index + 1]);
            let third = (to.x - from.x) / 3.;
            let control_a = point(from.x + third, from.y + third * tangents[index]);
            let control_b = point(to.x - third, to.y - third * tangents[index + 1]);
            line.cubic_bezier_to(to, control_a, control_b);
            area.cubic_bezier_to(to, control_a, control_b);
        }
        area.line_to(point(points[count - 1].x, baseline));
        area.close();
        if let Ok(path) = area.build() {
            window.paint_path(path, color.opacity(wash));
        }
        if let Ok(path) = line.build() {
            window.paint_path(path, *color);
        }
    }
}

/// The slope at each point of a curve through them that never overshoots them.
fn monotone_tangents(points: &[Point<Pixels>]) -> Vec<f32> {
    let count = points.len();
    if count < 2 {
        return vec![0.; count];
    }
    let slopes: Vec<f32> = points
        .windows(2)
        .map(|pair| {
            let run = f32::from(pair[1].x - pair[0].x);
            if run.abs() < f32::EPSILON { 0. } else { f32::from(pair[1].y - pair[0].y) / run }
        })
        .collect();
    let mut tangents = vec![0.; count];
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        let (before, after) = (slopes[index - 1], slopes[index]);
        tangents[index] = if before * after <= 0. { 0. } else { (before + after) / 2. };
    }
    for index in 0..count - 1 {
        let slope = slopes[index];
        if slope == 0. {
            tangents[index] = 0.;
            tangents[index + 1] = 0.;
            continue;
        }
        let (a, b) = (tangents[index] / slope, tangents[index + 1] / slope);
        let size = a * a + b * b;
        if size > 9. {
            let scale = 3. / size.sqrt();
            tangents[index] = scale * a * slope;
            tangents[index + 1] = scale * b * slope;
        }
    }
    tangents
}

fn axis_label(start: f64, period: UsagePeriod) -> String {
    if period == UsagePeriod::Day { format::date(start, "j", "%-I %p") } else { format::date(start, "MMMd", "%b %-d") }
}

fn pointed_label(start: f64, period: UsagePeriod) -> String {
    if period == UsagePeriod::Day {
        format::date(start, "EEEj", "%a %-I %p")
    } else {
        format::date(start, "EEEMMMd", "%a, %b %-d")
    }
}

/// The points the time axis is labelled at, where Swift Charts puts its aligned marks for the
/// period: every six hours or every other day from the first point, or every week or every other
/// week from the start of the first point's week.
fn axis_ticks(starts: &[f64], period: UsagePeriod) -> Vec<usize> {
    let Some(first) = starts.first() else { return Vec::new() };
    let (every, lead) = match period {
        UsagePeriod::Day => (6, 0),
        UsagePeriod::Week => (2, 0),
        UsagePeriod::Month => (7, days_into_week(*first)),
        UsagePeriod::Quarter => (14, days_into_week(*first)),
    };
    (0..starts.len()).filter(|index| (index + lead) % every == 0).collect()
}

/// How many days the day that starts at `start` is past the start of its week.
fn days_into_week(start: f64) -> usize {
    // Midday, so that a day a clock change starts an hour early is still that day.
    let Some(day) = Local.timestamp_opt(start as i64 + 12 * 3600, 0).single() else { return 0 };
    ((day.weekday().number_from_sunday() + 7 - format::first_weekday()) % 7) as usize
}

#[cfg(test)]
mod tests {
    use super::{UsagePeriod, axis_ticks, ticks};

    #[test]
    fn the_axis_aims_for_four_round_steps() {
        assert_eq!(ticks(10.), (2.5, 10.));
        assert_eq!(ticks(0.), (1., 4.));
        assert_eq!(ticks(7.3), (2., 8.));
    }

    #[test]
    fn hours_and_days_are_labelled_from_the_first_point() {
        let hours: Vec<f64> = (0..24).map(|hour| f64::from(hour) * 3600.).collect();
        assert_eq!(axis_ticks(&hours, UsagePeriod::Day), vec![0, 6, 12, 18]);
        let days: Vec<f64> = (0..7).map(|day| f64::from(day) * 86400.).collect();
        assert_eq!(axis_ticks(&days, UsagePeriod::Week), vec![0, 2, 4, 6]);
        let month: Vec<f64> = (0..30).map(|day| f64::from(day) * 86400.).collect();
        let weeks = axis_ticks(&month, UsagePeriod::Month);
        assert!(weeks.len() >= 4 && weeks.windows(2).all(|pair| pair[1] - pair[0] == 7));
    }
}
