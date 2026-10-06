//! What the agents spent on the account's servers, the Usage page of the settings: a line for
//! each agent over time, and what each model, project and kind of token took.

use chrono::{Local, TimeZone};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::link::State;
use motile_protocol::wire::Agent;
use serde::{Deserialize, Serialize};

use crate::models::agent_name;
use crate::store::Store;
use crate::store::settings::{UsageLine, UsagePeriod, UsageReport, UsageSeries, UsageState};
use crate::theme::{Colors, Surface, colors};
use crate::ui::menu::Anchor;
use crate::ui::{Segmented, Spinner, card_default, divider};

const CHART_HEIGHT: f32 = 190.;
/// Room for the labels of the y axis on the right and of the x axis below.
const Y_GUTTER: f32 = 48.;
const X_GUTTER: f32 = 18.;
/// How far the first and the last point stand from the plot's sides.
const PLOT_PADDING: f32 = 16.;
const BAR_WIDTH: f32 = 72.;
const CAPTION: f32 = 10.;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Measure {
    #[default]
    Cost,
    Tokens,
}

impl Measure {
    const ALL: [Measure; 2] = [Measure::Cost, Measure::Tokens];

    fn label(self) -> &'static str {
        if self == Measure::Cost { "Cost" } else { "Tokens" }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Breakdown {
    #[default]
    Models,
    Projects,
    Kinds,
}

impl Breakdown {
    const ALL: [Breakdown; 3] = [Breakdown::Models, Breakdown::Projects, Breakdown::Kinds];

    fn label(self) -> &'static str {
        match self {
            Breakdown::Models => "Models",
            Breakdown::Projects => "Projects",
            Breakdown::Kinds => "Tokens",
        }
    }
}

pub struct UsageView {
    store: Entity<Store>,
    period: UsagePeriod,
    measure: Measure,
    breakdown: Breakdown,
    /// The point of the series the pointer is over.
    pointed: Option<usize>,
    /// Where the plot was drawn, to tell which point the pointer is over.
    plot: Anchor,
    _subscription: Subscription,
}

impl UsageView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let (period, measure, breakdown) = {
            let prefs = &store.read(cx).prefs;
            (
                prefs.get("usage.period").unwrap_or_default(),
                prefs.get("usage.measure").unwrap_or_default(),
                prefs.get("usage.breakdown").unwrap_or_default(),
            )
        };
        store.update(cx, |store, _| store.load_usage(period));
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, period, measure, breakdown, pointed: None, plot: Anchor::default(), _subscription: subscription }
    }

    fn set_period(&mut self, period: UsagePeriod, cx: &mut Context<Self>) {
        self.period = period;
        self.pointed = None;
        self.store.update(cx, |store, cx| {
            store.prefs.set("usage.period", period);
            store.load_usage(period);
            cx.notify();
        });
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

    fn amount(&self, value: f64) -> String {
        if self.measure == Measure::Cost { cost(value) } else { count(value.round() as u64) }
    }

    fn value(&self, series: &UsageSeries, index: usize) -> f64 {
        match self.measure {
            Measure::Cost => series.cost_points.get(index).copied().unwrap_or(0.),
            Measure::Tokens => series.token_points.get(index).copied().unwrap_or(0) as f64,
        }
    }

    fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let periods = Segmented::new(
            "usage-period",
            UsagePeriod::ALL.iter().map(|period| SharedString::from(period.label())).collect(),
            UsagePeriod::ALL.iter().position(|period| *period == self.period).unwrap_or(0),
        )
        .on_select(listener(cx, |this, index, cx| this.set_period(UsagePeriod::ALL[index], cx)));
        let measures = Segmented::new(
            "usage-measure",
            Measure::ALL.iter().map(|measure| SharedString::from(measure.label())).collect(),
            Measure::ALL.iter().position(|measure| *measure == self.measure).unwrap_or(0),
        )
        .on_select(listener(cx, |this, index, cx| {
            this.measure = Measure::ALL[index];
            this.store.update(cx, |store, _| store.prefs.set("usage.measure", this.measure));
            cx.notify();
        }));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(periods)
            .child(div().flex_1().min_w(px(12.)))
            .child(measures)
    }

    fn note(&self, text: &str, cx: &App) -> AnyElement {
        let c = colors(cx);
        div()
            .w_full()
            .min_h(px(240.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(c.secondary)
            .text_center()
            .child(text.to_string())
            .into_any_element()
    }

    fn summary(&self, report: &UsageReport, cx: &App) -> AnyElement {
        let c = colors(cx);
        let cost_shown = self.measure == Measure::Cost;
        let total = if cost_shown { cost(report.cost_usd) } else { count(report.tokens) };
        let written = if cost_shown { cost(report.writing_cost_usd) } else { count(report.writing_tokens) };
        div()
            .flex()
            .items_start()
            .gap(px(16.))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child(total))
                    .child(caption(
                        if cost_shown { "What the API would have charged" } else { "Tokens in and out" },
                        c.secondary,
                    ))
                    .when(report.writing_tokens > 0, |summary| {
                        let of = if cost_shown { "it" } else { "them" };
                        summary.child(caption(
                            format!("{written} of {of} for titles, branch names, commit messages and pull requests"),
                            c.secondary,
                        ))
                    })
                    .when(cost_shown && report.unpriced_tokens > 0, |summary| {
                        let unpriced = count(report.unpriced_tokens);
                        summary.child(caption(
                            format!("Without {unpriced} tokens of models whose price isn't known"),
                            c.tertiary,
                        ))
                    }),
            )
            .child(div().flex_1().min_w(px(12.)))
            .when(report.cache_savings_usd > 0., |summary| {
                summary.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(c.text)
                                .child(cost(report.cache_savings_usd)),
                        )
                        .child(caption("Saved by the cache", c.secondary)),
                )
            })
            .into_any_element()
    }

    /// The agents with what each spent: in all, or at the time the pointer is over.
    fn legend(&self, report: &UsageReport, cx: &App) -> AnyElement {
        let c = colors(cx);
        let index = self.pointed.filter(|index| *index < report.starts.len());
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .text_size(px(12.))
            .children(report.agents.iter().map(|series| {
                let total = if self.measure == Measure::Cost { series.cost_usd } else { series.tokens as f64 };
                let shown = index.map(|index| self.value(series, index)).unwrap_or(total);
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(dot(series_color(series.agent, c)))
                    .child(div().text_color(c.text).child(agent_name(series.agent)))
                    .child(div().text_color(c.secondary).child(self.amount(shown)))
            }))
            .child(div().flex_1().min_w(px(8.)))
            .when_some(index, |legend, index| {
                legend.child(div().text_color(c.secondary).child(pointed_label(report.starts[index], self.period)))
            })
            .into_any_element()
    }

    fn chart(&self, report: &UsageReport, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let count = report.starts.len();
        let series: Vec<(Hsla, Vec<f64>)> = report
            .agents
            .iter()
            .map(|series| (series_color(series.agent, c), (0..count).map(|index| self.value(series, index)).collect()))
            .collect();
        let max = series.iter().flat_map(|(_, points)| points.iter().copied()).fold(0., f64::max);
        let (step, top) = ticks(max);
        let tick_count = (top / step).round() as usize;
        let label_step = (count as f32 / 5.).ceil().max(1.) as usize;
        let fraction = |index: usize| if count > 1 { index as f32 / (count - 1) as f32 } else { 0. };
        let plot = self.plot.clone();
        let pointed = self.pointed.filter(|index| *index < count);
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
                                .text_color(c.secondary)
                                .whitespace_nowrap()
                                .child(self.amount(value)),
                        )
                    }))
                    .child(
                        div().absolute().left_0().right_0().top_0().bottom_0().child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| paint_series(bounds, &series, top, window),
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
                                        .bg(c.border_secondary),
                                )
                            })
                            .child(div().absolute().left_0().right_0().top_full().h(px(X_GUTTER)).children(
                                (0..count).step_by(label_step).map(|index| {
                                    div()
                                        .absolute()
                                        .left(relative(fraction(index)))
                                        .ml(px(-30.))
                                        .w(px(60.))
                                        .pt(px(4.))
                                        .flex()
                                        .justify_center()
                                        .text_size(px(CAPTION))
                                        .text_color(c.secondary)
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

    fn lines(&self, report: &UsageReport, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let shown: Vec<UsageLine> = match self.breakdown {
            Breakdown::Models => report.models.clone(),
            Breakdown::Projects => report.projects.clone(),
            Breakdown::Kinds => report.kind_lines(),
        };
        let picker = Segmented::new(
            "usage-breakdown",
            Breakdown::ALL.iter().map(|breakdown| SharedString::from(breakdown.label())).collect(),
            Breakdown::ALL.iter().position(|breakdown| *breakdown == self.breakdown).unwrap_or(0),
        )
        .on_select(listener(cx, |this, index, cx| {
            this.breakdown = Breakdown::ALL[index];
            this.store.update(cx, |store, _| store.prefs.set("usage.breakdown", this.breakdown));
            cx.notify();
        }));
        let last = shown.len().saturating_sub(1);
        let mut rows = Vec::new();
        for (index, line) in shown.iter().enumerate() {
            rows.push(self.line_row(line, c));
            if index < last {
                rows.push(divider(cx).into_any_element());
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

    /// A model, a project or a kind of token: its name, its part of the whole as a bar, its
    /// tokens and what they cost.
    fn line_row(&self, line: &UsageLine, c: &Colors) -> AnyElement {
        let cost_shown = self.measure == Measure::Cost;
        let share = line.share.clamp(0., 1.) as f32;
        div()
            .px(px(12.))
            .min_h(px(36.))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(12.))
            .when_some(line.agent, |row, agent| row.child(dot(series_color(agent, c))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(c.text)
                    .child(line.name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(BAR_WIDTH))
                    .h(px(4.))
                    .rounded_full()
                    .bg(Surface::Secondary.next().color(c))
                    .child(div().w(px(BAR_WIDTH * share)).h_full().rounded_full().bg(c.secondary)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(64.))
                    .text_right()
                    .text_color(if cost_shown { c.secondary } else { c.text })
                    .child(count(line.tokens)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(72.))
                    .text_right()
                    .text_color(if cost_shown && line.cost_usd.is_some() { c.text } else { c.secondary })
                    .child(line.cost_usd.map(cost).unwrap_or_else(|| "No price".into())),
            )
            .into_any_element()
    }
}

impl Render for UsageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let state = self.store.read(cx).settings.usage.clone();
        let outdated: Vec<String> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| server.state == State::Connected && server.protocol_version < 10)
            .map(|server| server.name.clone())
            .collect();
        let body: Vec<AnyElement> = match &state {
            UsageState::Loading => vec![
                div()
                    .w_full()
                    .min_h(px(240.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c.secondary)
                    .child(Spinner::regular().render(cx))
                    .into_any_element(),
            ],
            UsageState::Failed(message) => vec![self.note(message, cx)],
            UsageState::Ready(report) if report.tokens == 0 => vec![self.note(
                "Nothing was spent in this time. Your servers keep what their agents spend from the version that shows this on.",
                cx,
            )],
            UsageState::Ready(report) => vec![
                self.summary(report, cx),
                div().flex().flex_col().gap(px(10.)).child(self.legend(report, cx)).child(self.chart(report, cx)).into_any_element(),
                self.lines(report, cx),
            ],
        };
        div().flex().flex_col().gap(px(18.)).child(self.controls(cx)).children(body).children(
            outdated
                .into_iter()
                .map(|name| caption(format!("Update {name} to see what its agents spend."), c.secondary)),
        )
    }
}

fn series_color(agent: Agent, c: &Colors) -> Hsla {
    if agent == Agent::Claude { c.claude_series } else { c.codex_series }
}

fn dot(color: Hsla) -> Div {
    div().size(px(8.)).rounded_full().bg(color).flex_shrink_0()
}

fn caption(text: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(px(CAPTION)).text_color(color).child(text.into())
}

/// The y axis: a round step with about three of them under the highest point, and the top line.
fn ticks(max: f64) -> (f64, f64) {
    if max <= 0. {
        return (1., 3.);
    }
    let raw = max / 3.;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1., 2., 2.5, 5., 10.]
        .into_iter()
        .map(|unit| unit * magnitude)
        .find(|step| *step >= raw)
        .unwrap_or(magnitude * 10.);
    (step, (max / step).ceil() * step)
}

/// Each agent's series as a line through its points and a wash under it.
fn paint_series(bounds: Bounds<Pixels>, series: &[(Hsla, Vec<f64>)], top: f64, window: &mut Window) {
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
            window.paint_path(path, color.opacity(0.12));
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
    local_time(start, if period == UsagePeriod::Day { "%-I %p" } else { "%b %-d" })
}

fn pointed_label(start: f64, period: UsagePeriod) -> String {
    local_time(start, if period == UsagePeriod::Day { "%a %-I %p" } else { "%a, %b %-d" })
}

fn local_time(start: f64, pattern: &str) -> String {
    Local.timestamp_opt(start as i64, 0).single().map(|time| time.format(pattern).to_string()).unwrap_or_default()
}

/// Dollars and cents, with a comma every three digits; less than a cent is said so.
pub fn cost(value: f64) -> String {
    if value > 0. && value < 0.01 {
        return "<$0.01".into();
    }
    let cents = (value * 100.).round() as u64;
    let (dollars, cents) = (cents / 100, cents % 100);
    let digits = dollars.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{cents:02}")
}

/// A count in up to three figures with K, M, B or T after it.
pub fn count(value: u64) -> String {
    let units = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")];
    let value = value as f64;
    let Some((size, unit)) = units.into_iter().find(|(size, _)| value >= *size) else {
        return format!("{value}");
    };
    let shown = value / size;
    let decimals = if shown >= 100. {
        0
    } else if shown >= 10. {
        1
    } else {
        2
    };
    let text = format!("{shown:.decimals$}");
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
    format!("{text}{unit}")
}

/// A handler for a choice that runs on the view, as `cx.listener` does for an event.
fn listener(
    cx: &mut Context<UsageView>,
    run: impl Fn(&mut UsageView, usize, &mut Context<UsageView>) + 'static,
) -> impl Fn(usize, &mut Window, &mut App) + 'static {
    let view = cx.weak_entity();
    move |index, _, cx| {
        let _ = view.update(cx, |this, cx| run(this, index, cx));
    }
}
