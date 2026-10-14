//! The shelves at the bottom of the sidebar: a line that opens into a list, which the line above
//! it makes taller or shorter. The threads marked done are on one and the servers on another, and
//! the offer to undo marking threads done stands above them.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::link::State;

use super::list::{Layout, place};
use super::updates::ServerUpdateStatus;
use super::{DONE_ROW_HEIGHT, ROW_GAP, SIDEBAR_EDGE, Shelf, Sidebar, row_margin};
use crate::models::Server;
use crate::store::UndoNotice;
use crate::theme::{self, colors};
use crate::ui::{Highlight, divider, ease_out, icons, tabular_numbers, tooltip};

/// How tall a shelf's list can get at least, when there is room, and before it is resized.
const SHELF_MIN_HEIGHT: f32 = 4. * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
const SHELF_DEFAULT_HEIGHT: f32 = 250.;
/// How long the offer to undo takes to come and go.
pub const UNDO_FADE: Duration = Duration::from_millis(150);
const UNDO_HEIGHT: f32 = 1. + DONE_ROW_HEIGHT + 8.;

impl Shelf {
    fn height_key(self) -> &'static str {
        match self {
            Shelf::Done => "sidebar.doneHeight",
            Shelf::Servers => "sidebar.serversHeight",
        }
    }

    pub fn expanded_key(self) -> &'static str {
        match self {
            Shelf::Done => "sidebar.doneExpanded",
            Shelf::Servers => "sidebar.serversExpanded",
        }
    }
}

/// The heights a shelf's list can have: from the least to as tall as its rows, up to `max`.
fn heights(count: usize, max: f32) -> (f32, f32) {
    let content = count as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
    let tallest = max.min(content);
    (SHELF_MIN_HEIGHT.min(tallest), tallest)
}

/// What a sidebar line starts with, against its edge, in a column that lines up the words after
/// it, a step quieter than its title: muted while it is open or lit.
fn shelf_mark(lit: bool, mark: impl Fn(Hsla) -> AnyElement, cx: &App) -> Div {
    let c = colors(cx);
    let color = if lit { c.muted_foreground } else { c.muted_stronger_foreground };
    div().w(px(9.)).flex_shrink_0().flex().items_center().text_color(color).child(mark(color))
}

/// The name of a sidebar line: muted while it is closed and not lit.
fn shelf_title(title: &'static str, lit: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .min_w_0()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .truncate()
        .text_color(if lit { c.foreground } else { c.muted_foreground })
        .child(title)
}

fn state_dot(tint: Hsla) -> Div {
    div().size(px(7.)).flex_shrink_0().rounded_full().bg(tint)
}

/// How the client reaches a server, or why it doesn't.
fn server_detail(server: &Server) -> String {
    match server.state {
        State::Connected => {
            let path = server.path.as_deref().map(capitalized).unwrap_or_else(|| "Connected".into());
            match server.rtt_ms {
                Some(rtt) => format!("{path} · {rtt} ms"),
                None => path,
            }
        }
        State::Connecting => "Connecting".into(),
        State::Disconnected => "Offline".into(),
        State::Refused => "Refused".into(),
    }
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

/// How the connected servers are reached, "Mixed" if not all alike, and their slowest round trip.
fn reach(servers: &[Server]) -> String {
    let connected: Vec<&Server> = servers.iter().filter(|server| server.connected()).collect();
    let mut paths: Vec<&str> = connected.iter().filter_map(|server| server.path.as_deref()).collect();
    paths.sort();
    paths.dedup();
    let path = match paths.as_slice() {
        [] => None,
        [path] => Some(capitalized(path)),
        _ => Some("Mixed".to_string()),
    };
    let slowest = connected.iter().filter_map(|server| server.rtt_ms).max().map(|rtt| format!("{rtt} ms"));
    [path, slowest].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// The worst state among the servers: one out of reach, then one connecting.
fn worst_tint(servers: &[Server], cx: &App) -> Hsla {
    let c = colors(cx);
    if servers.iter().any(|server| matches!(server.state, State::Disconnected | State::Refused)) {
        return c.destructive;
    }
    if servers.iter().any(|server| server.state == State::Connecting) {
        return c.pending;
    }
    c.success
}

impl Sidebar {
    /// The offer to undo marking threads done: a line above the done threads, which stays while
    /// the pointer is on it. It comes and goes as one piece.
    pub(super) fn undo_row(
        &self,
        notice: &UndoNotice,
        appearing: bool,
        serial: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let group = "undo-row";
        let lit = self.hovering(group);
        let store = self.store.clone();
        let hold = self.store.clone();
        let hovered = self.row_hover(group.into(), None, cx);
        let row = div()
            .id(("undo", serial))
            .w_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(divider(cx))
            .child(
                div()
                    .id(("undo-line", serial))
                    .group(group)
                    .relative()
                    .h(px(DONE_ROW_HEIGHT + 8.))
                    .flex_shrink_0()
                    .child(Highlight::new().radius(0.).render(group, cx))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .px(px(SIDEBAR_EDGE))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(shelf_mark(
                                lit,
                                |color| {
                                    div()
                                        .ml(px(-2.))
                                        .child(icons::trimmed("undo-2", 10.).text_color(color))
                                        .into_any_element()
                                },
                                cx,
                            ))
                            .child(shelf_title("Undo", lit, cx))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .text_color(c.muted_stronger_foreground)
                                    .child(icons::symbol("check", 10.))
                                    .child(div().min_w_0().text_size(px(11.)).truncate().child(notice.text.clone())),
                            ),
                    )
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.perform_undo(cx);
                            cx.notify();
                        })
                    })
                    .on_hover(move |held, window, cx| {
                        hovered(held, window, cx);
                        hold.update(cx, |store, cx| store.hold_undo(*held, cx));
                    }),
            );
        row.with_animation(("undo-fade", serial), Animation::new(UNDO_FADE).with_easing(ease_out), move |row, delta| {
            let shown = if appearing { delta } else { 1. - delta };
            row.h(px(UNDO_HEIGHT * shown)).opacity(shown)
        })
        .into_any_element()
    }

    /// A shelf's line, which opens and closes its list.
    fn shelf_header(
        &self,
        shelf: Shelf,
        expanded: bool,
        header: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = match shelf {
            Shelf::Done => "done-shelf",
            Shelf::Servers => "servers-shelf",
        };
        let store = self.store.clone();
        let lit = self.hovering(group);
        div()
            .id(group)
            .group(group)
            .relative()
            .w_full()
            .flex_shrink_0()
            .on_hover(self.row_hover(group.into(), None, cx))
            .pt(px(3.))
            .pb(px(if expanded { 3. - ROW_GAP / 2. } else { 3. }))
            .child(Highlight::new().radius(0.).render(group, cx))
            .child(
                div()
                    .relative()
                    .h(px(DONE_ROW_HEIGHT))
                    .px(px(SIDEBAR_EDGE))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(shelf_mark(
                        expanded || lit,
                        |color| {
                            icons::trimmed("chevron-right", 10.)
                                .text_color(color)
                                .rotate(if expanded { 90. } else { 0. })
                                .into_any_element()
                        },
                        cx,
                    ))
                    .child(header),
            )
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    let open = !store.prefs.bool(shelf.expanded_key());
                    store.prefs.set(shelf.expanded_key(), open);
                    cx.notify();
                })
            })
            .into_any_element()
    }

    /// The line above an open shelf. Dragging it makes the list taller or shorter; the height is
    /// saved when the drag ends.
    fn resize_handle(&self, shelf: Shelf, cx: &mut Context<Self>) -> Div {
        div().relative().flex_shrink_0().child(divider(cx)).child(
            div()
                .id(match shelf {
                    Shelf::Done => "done-resize",
                    Shelf::Servers => "servers-resize",
                })
                .absolute()
                .left_0()
                .right_0()
                .top(px(-(theme::RESIZE_GRAB - 1.) / 2.))
                .h(px(theme::RESIZE_GRAB))
                .cursor(CursorStyle::ResizeUpDown)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        let y = f32::from(event.position.y);
                        this.pulling = Some((shelf, y, y));
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        )
    }

    /// How tall the shelf's list is: as it was left, pulled as far as the line is being dragged,
    /// within what it can be.
    fn shelf_list_height(&self, shelf: Shelf, count: usize, max: f32, cx: &App) -> f32 {
        let (lowest, tallest) = heights(count, max);
        let resting = self.store.read(cx).prefs.f32_or(shelf.height_key(), SHELF_DEFAULT_HEIGHT).clamp(lowest, tallest);
        let pulled = self.pulling.filter(|(pulling, ..)| *pulling == shelf).map_or(0., |(_, start, now)| start - now);
        (resting + pulled).clamp(lowest, tallest)
    }

    /// Saves the height the shelf was pulled to.
    pub(super) fn finish_pulling(&mut self, cx: &mut Context<Self>) {
        let Some((shelf, ..)) = self.pulling else { return };
        let (count, max) = self.shelf_room.get(&shelf).copied().unwrap_or((0, 0.));
        let height = self.shelf_list_height(shelf, count, max, cx);
        self.pulling = None;
        self.store.update(cx, |store, cx| {
            store.prefs.set(shelf.height_key(), height);
            cx.notify();
        });
        cx.notify();
    }

    /// A shelf: its line above, its header, and its list while it is open.
    /// `room` is how many rows it has and the most room its list may take.
    #[allow(clippy::too_many_arguments)]
    fn shelf(
        &mut self,
        shelf: Shelf,
        layout: &Layout,
        max: f32,
        expanded: bool,
        header: AnyElement,
        rows: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Div {
        let count = layout.tops.len();
        self.shelf_room.insert(shelf, (count, max));
        let (lowest, tallest) = heights(count, max);
        let list_height = self.shelf_list_height(shelf, count, max, cx);
        let fill = colors(cx).background;
        let list = match shelf {
            Shelf::Done => &self.done_list,
            Shelf::Servers => &self.servers_list,
        };
        let id = match shelf {
            Shelf::Done => "done-list",
            Shelf::Servers => "servers-list",
        };
        let element = list.element(id, layout, rows, fill);
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(if expanded && lowest < tallest {
                self.resize_handle(shelf, cx).into_any_element()
            } else {
                divider(cx).into_any_element()
            })
            .child(self.shelf_header(shelf, expanded, header, cx))
            .when(expanded, |shelf| {
                shelf.child(
                    div()
                        .h(px(list_height + ROW_GAP / 2.))
                        .flex_shrink_0()
                        .on_scroll_wheel(cx.listener(|this, _, _, cx| this.forget_pointer(cx)))
                        .child(element),
                )
            })
    }

    /// The threads marked done, on a shelf that opens into their list.
    pub(super) fn done_shelf(&mut self, layout: &Layout, max: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let count = layout.tops.len();
        let (expanded, revealed) = {
            let store = self.store.read(cx);
            (store.prefs.bool(Shelf::Done.expanded_key()), store.sidebar.revealed_thread_id.clone())
        };
        let group = "done-shelf";
        let header = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(shelf_title("Done", expanded || self.hovering(group), cx))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(c.muted_stronger_foreground)
                    .font_features(tabular_numbers())
                    .child(count.to_string()),
            )
            .into_any_element();
        let mut rows = Vec::new();
        if expanded {
            self.done_list.follow(revealed.as_deref(), true, layout, window);
            for index in self.done_list.visible(layout, window) {
                let Some(thread) = self.store.read(cx).done_threads().get(index).cloned() else { continue };
                rows.push(place(layout, index, 0., self.done_row(&thread, cx)));
            }
        }
        self.shelf(Shelf::Done, layout, max, expanded, header, rows, cx)
    }

    /// The servers: one line for them all that opens into a line for each, or the one server's
    /// line.
    pub(super) fn servers_shelf(&mut self, max: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let servers = self.store.read(cx).servers.clone();
        if let [server] = servers.as_slice() {
            return div().flex().flex_col().flex_shrink_0().child(divider(cx)).child(
                div()
                    .h(px(DONE_ROW_HEIGHT))
                    .my(px(3.))
                    .px(px(SIDEBAR_EDGE))
                    .flex()
                    .items_center()
                    .child(self.server_line(server, cx)),
            );
        }
        let expanded = self.store.read(cx).prefs.bool(Shelf::Servers.expanded_key());
        let group = "servers-shelf";
        let header = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(state_dot(worst_tint(&servers, cx)))
            .child(shelf_title("Servers", expanded || self.hovering(group), cx))
            .child(div().flex_1().min_w(px(4.)))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(c.muted_stronger_foreground)
                    .font_features(tabular_numbers())
                    .whitespace_nowrap()
                    .child(reach(&servers)),
            )
            .into_any_element();
        self.servers_list.set_ids(servers.iter().map(|server| server.id.clone()).collect());
        let layout = Layout::new(vec![DONE_ROW_HEIGHT + ROW_GAP; servers.len()], 0., 4. - ROW_GAP / 2.);
        let mut rows = Vec::new();
        if expanded {
            let margin = row_margin();
            for index in self.servers_list.visible(&layout, window) {
                let line = div()
                    .size_full()
                    .pt(px(margin.top))
                    .pb(px(margin.bottom))
                    .px(px(margin.left + 8.))
                    .flex()
                    .items_center()
                    .child(self.server_line(&servers[index], cx));
                rows.push(place(&layout, index, 0., line));
            }
        }
        self.shelf(Shelf::Servers, &layout, max, expanded, header, rows, cx)
    }

    /// A server and how the client reaches it, or the update it needs.
    fn server_line(&self, server: &Server, cx: &App) -> AnyElement {
        let c = colors(cx);
        let detail = server_detail(server);
        let tint = server.state_tint(c).unwrap_or(c.success);
        div()
            .id(SharedString::from(format!("server-{}", server.id)))
            .w_full()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().w(px(9.)).flex_shrink_0().flex().items_center().child(state_dot(tint)))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(server.name.clone()),
            )
            .child(div().flex_1().min_w(px(4.)))
            .child(
                ServerUpdateStatus::new(&self.store, server).otherwise(
                    div()
                        .text_size(px(11.))
                        .text_color(c.muted_stronger_foreground)
                        .font_features(tabular_numbers())
                        .whitespace_nowrap()
                        .child(detail.clone()),
                ),
            )
            .tooltip(tooltip(server.error.clone().unwrap_or(detail)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{SHELF_MIN_HEIGHT, capitalized, heights};

    #[test]
    fn a_shelf_is_as_tall_as_its_rows_up_to_its_room() {
        assert_eq!(heights(2, 400.), (68., 68.));
        assert_eq!(heights(20, 400.), (SHELF_MIN_HEIGHT, 400.));
        assert_eq!(heights(20, 100.), (100., 100.));
    }

    #[test]
    fn a_path_reads_capitalised() {
        assert_eq!(capitalized("relay"), "Relay");
        assert_eq!(capitalized(""), "");
    }
}
