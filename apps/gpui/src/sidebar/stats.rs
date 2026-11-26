//! What a server's machine does, in the dropdown its line in the sidebar opens: how long it has
//! been up, then its CPU, memory and traffic over the last minutes as bars and how full its disk
//! is, scrolling where the window is too short. The Mac app's `ServerStatsCard.swift`.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::server_stats::{StatsKind, StatsRow};

use super::updates::{ServerUpdateStatus, update_note};
use super::{Shelf, Sidebar};
use crate::models::Server;
use crate::store::Store;
use crate::store::server_stats::ShownStats;
use crate::theme::{ControlSize, IconSize, Surface, TextSize, colors};
use crate::ui::dropdown::{backdrop, drop_from, over_content};
use crate::ui::{FadingScroll, Spinner, divider_on, dropdown_card, icons, limit_pill, tabular_numbers, tone_color};

const WIDTH: f32 = 300.;
const EDGE: f32 = 14.;
const HEADER_TOP: f32 = 8.;
const HEADER_BOTTOM: f32 = 10.;
/// The update's button stands as far from the header's side as from its top and bottom.
const BUTTON_SIDE: f32 = 12.;
/// Between the line over the rows and the first.
const ROWS_TOP: f32 = 10.;
/// Between one part's chart and the next part's name: clearly more than a name and its chart.
const ROW_GAP: f32 = 20.;
const SYMBOL_GAP: f32 = 5.;
/// Between a part's name and its chart, so that they read as one.
const CHART_GAP: f32 = 4.;
/// A meter is a thin line, so it stands further from its name than a chart's bars do.
const METER_GAP: f32 = CHART_GAP + 4.;
const BARS_HEIGHT: f32 = 12.;
const BAR_GAP: f32 = 1.;

/// The key a server's line keeps where it was drawn by, for the dropdown that hangs from it.
pub(super) fn line_key(server_id: &str) -> String {
    format!("server-stats-{server_id}")
}

/// Opens the dropdown of the server with that short name, with the servers' list open if it holds
/// the line, as the dev driver's `stats` asks.
pub fn show_stats(store: &mut Store, short_name: &str) -> bool {
    let Some(id) = store.live_servers().find(|server| server.short_name == short_name).map(|server| server.id.clone())
    else {
        return false;
    };
    if store.live_servers().count() > 1 {
        Shelf::Servers.open(&mut store.prefs, true);
    }
    store.open_stats(&id);
    true
}

impl Sidebar {
    /// Gives the dropdown the keys while it is open, and what had them back once it closes.
    pub(super) fn follow_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.store.read(cx).server_stats.open_server().map(str::to_string);
        if open == self.stats_shown {
            return;
        }
        let opened = self.stats_shown.is_none();
        self.stats_shown = open.clone();
        if open.is_none() {
            if let Some(returns) = self.stats_returns.take() {
                window.focus(&returns, cx);
            }
            return;
        }
        if opened {
            self.stats_returns = window.focused(cx);
            self.peek.hide();
            crate::ui::overlay_opened(cx);
        }
        window.focus(&self.stats_focus, cx);
    }

    /// The open dropdown, hung from its server's line as it was last drawn. Esc or a press
    /// elsewhere closes it.
    pub(super) fn stats_dropdown(&self, window: &Window, cx: &App) -> Option<Deferred> {
        let store = self.store.read(cx);
        let server = store.server(store.server_stats.open_server())?.clone();
        let line = self.row_bounds.borrow().get(&line_key(&server.id)).copied()?;
        let close = {
            let store = self.store.clone();
            move |_: &mut Window, cx: &mut App| {
                store.update(cx, |store, cx| {
                    store.close_stats();
                    cx.notify();
                })
            }
        };
        let escape = close.clone();
        let card = stats_card(&self.store, &server, crate::ui::dropdown::room(line, window), cx)
            .id("server-stats")
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let layer = backdrop("server-stats-layer", close, window)
            .track_focus(&self.stats_focus)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                cx.stop_propagation();
                escape(window, cx)
            })
            .child(drop_from(line, false, window).child(card));
        Some(over_content(layer, window))
    }
}

fn stats_card(store: &Entity<Store>, server: &Server, room: Pixels, cx: &App) -> Div {
    let c = colors(cx);
    let rows = div()
        .px(px(EDGE))
        .pt(px(ROWS_TOP))
        .pb(px(EDGE))
        .child(stats_rows(store.read(cx).server_stats.of(&server.id), cx));
    dropdown_card(div(), cx)
        .w(px(WIDTH))
        .max_h(room)
        .flex()
        .flex_col()
        .child(
            header(store, server, cx)
                .flex_shrink_0()
                .pl(px(EDGE))
                .pr(px(BUTTON_SIDE))
                .pt(px(HEADER_TOP))
                .pb(px(HEADER_BOTTOM)),
        )
        .child(divider_on(Surface::Popover, cx))
        .child(FadingScroll::new("server-stats-rows", f32::from(room)).shrinks().fill(c.popover).child(rows))
}

/// The server's name over its uptime, the update's button at their end and, while the update
/// goes, how far it is under them.
fn header(store: &Entity<Store>, server: &Server, cx: &App) -> Div {
    let c = colors(cx);
    let state = store.read(cx);
    // The line keeps its room while the stats come, so that nothing under it moves.
    let uptime = state.server_stats.of(&server.id).uptime().unwrap_or(" ").to_string();
    let note = state.server_update(server).map(update_note).map(|(text, spins)| {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(TextSize::Xs)
            .text_color(c.muted_foreground)
            .font_features(tabular_numbers())
            .when(spins, |note| note.child(Spinner::new(IconSize::Xs).render(cx)))
            .child(text)
    });
    let names = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .whitespace_nowrap()
        .child(
            div()
                .truncate()
                .text_size(TextSize::Sm)
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .child(server.name.clone()),
        )
        .child(
            div()
                .truncate()
                .text_size(TextSize::Xs)
                .text_color(c.muted_foreground)
                .font_features(tabular_numbers())
                .child(uptime),
        );
    let button =
        ServerUpdateStatus::new(store, server).button_only().size(ControlSize::Regular).surface(Surface::Popover);
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div().min_h(px(ControlSize::Regular.height())).flex().items_center().gap(px(8.)).child(names).child(button),
        )
        .children(note)
}

/// The machine's parts, each a row of its name, how much it is now and its chart, or why they
/// can't be shown.
fn stats_rows(shown: &ShownStats, cx: &App) -> AnyElement {
    let c = colors(cx);
    match shown {
        ShownStats::Loading => div()
            .w_full()
            .py(px(24.))
            .flex()
            .justify_center()
            .text_color(c.muted_foreground)
            .child(Spinner::new(IconSize::Md).render(cx))
            .into_any_element(),
        ShownStats::Failed(error) => {
            div().text_size(TextSize::Xs).text_color(c.muted_foreground).child(error.clone()).into_any_element()
        }
        ShownStats::Shown(stats) => div()
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            .children(stats.rows.iter().map(|row| stats_row(row, cx)))
            .into_any_element(),
    }
}

/// A part of the machine: its symbol, its name and amounts, how much it is now, and under them the
/// last minutes as bars or how full it is.
fn stats_row(row: &StatsRow, cx: &App) -> Div {
    let c = colors(cx);
    let muted = |text: &str| div().text_color(c.muted_foreground).child(text.to_string());
    let separator = || div().text_color(c.muted_stronger_foreground).child(" · ");
    let label = div()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .child(muted(&row.label))
        .when_some(row.detail.as_deref(), |label, detail| label.child(separator()).child(muted(detail)));
    let value = div()
        .flex_shrink_0()
        .flex()
        .font_weight(FontWeight::MEDIUM)
        .when_some(row.used.as_deref(), |value, used| value.child(muted(used)).child(separator()))
        .child(div().text_color(c.foreground).child(row.value.clone()));
    let line = div()
        .flex()
        .items_center()
        .text_size(TextSize::Sm)
        .font_features(tabular_numbers())
        .whitespace_nowrap()
        .child(
            div()
                .flex_shrink_0()
                .mr(px(SYMBOL_GAP))
                .flex()
                .child(icons::symbol(symbol(row.kind), IconSize::Sm).text_color(c.muted_foreground)),
        )
        .child(label)
        .child(div().flex_1().min_w(px(8.)))
        .child(value);
    let chart = match row.fill {
        Some(fill) => {
            limit_pill(fill.height as f64 * 100., fill.tone, cx).mt(px(METER_GAP - CHART_GAP)).into_any_element()
        }
        None => stats_bars(row, cx).into_any_element(),
    };
    div().flex().flex_col().gap(px(CHART_GAP)).child(line).child(chart)
}

fn symbol(kind: StatsKind) -> &'static str {
    match kind {
        StatsKind::Cpu => "cpu",
        StatsKind::Memory => "memory-stick",
        StatsKind::Download => "arrow-down",
        StatsKind::Upload => "arrow-up",
        StatsKind::Disk => "hard-drive",
    }
}

/// A bar for each sample, standing on a line of dots: one under every place a bar can stand.
fn stats_bars(row: &StatsRow, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let baseline = c.muted_strongest_foreground;
    let bars: Vec<(f32, Hsla)> = row.bars.iter().map(|bar| (bar.height, tone_color(bar.tone, c))).collect();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if bars.is_empty() {
                return;
            }
            let pitch = f32::from(bounds.size.width) / bars.len() as f32;
            let width = (pitch - BAR_GAP).max(1.);
            for (index, (height, tone)) in bars.iter().enumerate() {
                let height = if *height > 0. { (height * BARS_HEIGHT).round().max(1.) } else { 0. };
                let (height, color) = if height > 0. { (height, *tone) } else { (1., baseline) };
                let origin = point(bounds.left() + px(index as f32 * pitch), bounds.bottom() - px(height));
                window.paint_quad(fill(Bounds::new(origin, size(px(width), px(height))), color));
            }
        },
    )
    .w_full()
    .h(px(BARS_HEIGHT))
}
