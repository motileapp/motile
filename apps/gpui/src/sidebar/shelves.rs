//! The lines of the working, the snoozed and the archived threads in the list, and the shelf of
//! the servers under it: a line that opens into a list, which the line above it makes taller or
//! shorter. The offer to undo archiving or snoozing stands above the servers.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Section;
use motile_core::link::State;

use super::list::{Layout, place};
use super::rows::row_light;
use super::updates::ServerUpdateStatus;
use super::{
    BUTTON_OUTSET, DONE_ROW_HEIGHT, ROW_GAP, ROW_PADDING, SHELF_LIST_TOP, SIDEBAR_EDGE, Shelf, Sidebar, row_margin,
};
use crate::models::{Server, ThreadInfo, section_label};
use crate::prefs::Prefs;
use crate::store::{Store, UndoNotice};
use crate::theme::{self, IconSize, Opacity, TextSize, colors};
use crate::ui::{Highlight, divider, ease_out, icons, tabular_numbers, tooltip};

/// How tall a shelf's list can get at least, when there is room, and before it is resized.
const SHELF_MIN_HEIGHT: f32 = 4. * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
const SHELF_DEFAULT_HEIGHT: f32 = 250.;
/// How long the offer to undo takes to come and go.
pub const UNDO_FADE: Duration = Duration::from_millis(150);
const UNDO_HEIGHT: f32 = 1. + DONE_ROW_HEIGHT + 8.;

impl Shelf {
    /// The parts of the list under the active threads, in the order they stand.
    pub const THREADS: [Shelf; 3] = [Shelf::Working, Shelf::Snoozed, Shelf::Archived];

    fn height_key(self) -> &'static str {
        match self {
            Shelf::Working => "sidebar.workingHeight",
            Shelf::Snoozed => "sidebar.snoozedHeight",
            Shelf::Archived => "sidebar.doneHeight",
            Shelf::Servers => "sidebar.serversHeight",
        }
    }

    fn expanded_key(self) -> &'static str {
        match self {
            Shelf::Working => "sidebar.workingExpanded",
            Shelf::Snoozed => "sidebar.snoozedExpanded",
            Shelf::Archived => "sidebar.doneExpanded",
            Shelf::Servers => "sidebar.serversExpanded",
        }
    }

    /// Whether the part is open. The working threads are, until the user closes them.
    pub fn expanded(self, prefs: &Prefs) -> bool {
        prefs.get(self.expanded_key()).unwrap_or(self == Shelf::Working)
    }

    pub fn open(self, prefs: &mut Prefs, open: bool) {
        prefs.set(self.expanded_key(), open);
    }

    /// The part of the list the threads of a section are in.
    pub fn of(section: Section) -> Option<Shelf> {
        match section {
            Section::Working => Some(Shelf::Working),
            Section::Snoozed => Some(Shelf::Snoozed),
            Section::Archived => Some(Shelf::Archived),
            Section::Active => None,
        }
    }

    fn section(self) -> Section {
        match self {
            Shelf::Working => Section::Working,
            Shelf::Snoozed => Section::Snoozed,
            Shelf::Archived | Shelf::Servers => Section::Archived,
        }
    }

    pub fn threads(self, store: &Store) -> &[ThreadInfo] {
        match self {
            Shelf::Working => store.working_threads(),
            Shelf::Snoozed => store.snoozed_threads(),
            Shelf::Archived => store.archived_threads(),
            Shelf::Servers => &[],
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Shelf::Working => "working-shelf",
            Shelf::Snoozed => "snoozed-shelf",
            Shelf::Archived => "archived-shelf",
            Shelf::Servers => "servers-shelf",
        }
    }

    fn resize_id(self) -> &'static str {
        match self {
            Shelf::Working => "working-resize",
            Shelf::Snoozed => "snoozed-resize",
            Shelf::Archived => "archived-resize",
            Shelf::Servers => "servers-resize",
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Shelf::Working => "working-list",
            Shelf::Snoozed => "snoozed-list",
            Shelf::Archived => "archived-list",
            Shelf::Servers => "servers-list",
        }
    }
}

/// The heights a shelf's list can have: from the least to as tall as its rows, up to `max`.
fn heights(count: usize, max: f32) -> (f32, f32) {
    let content = count as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
    let tallest = max.min(content);
    (SHELF_MIN_HEIGHT.min(tallest), tallest)
}

/// The chevron that opens and closes a line's list, at the line's end, a step quieter than its
/// title: muted only under the pointer. Open or closed, it ends where the rows end, in a box as
/// wide as when it is open, so that what comes before it stays put.
fn shelf_chevron(expanded: bool, lit: bool, cx: &App) -> Div {
    let c = colors(cx);
    let color = if lit { c.muted_foreground } else { c.muted_stronger_foreground };
    let shown = if expanded { "chevron-down" } else { "chevron-right" };
    div()
        .relative()
        .flex_shrink_0()
        .flex()
        .items_center()
        .child(div().invisible().child(icons::trimmed("chevron-down", IconSize::Xs)))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .flex()
                .items_center()
                .child(icons::trimmed(shown, IconSize::Xs).text_color(color)),
        )
}

/// What a line at the bottom of the sidebar starts with: a symbol, or a server's state, in a
/// column as wide as a slim row's project icon, so that every line's words start where its title does.
pub fn line_mark(mark: impl IntoElement) -> Div {
    div().w(px(IconSize::Md.side())).flex_shrink_0().flex().justify_center().items_center().child(mark)
}

/// A state as a dot in a ring of its colour's tint, the size of a symbol. Both are even, so that
/// the dot sits in the ring's middle on whole points.
pub fn state_ring(tint: Hsla, cx: &App) -> Div {
    let ring = colors(cx).at(tint, Opacity::ColorTintChartStronger);
    div()
        .size(px(12.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(ring)
        .child(div().size(px(6.)).rounded_full().bg(tint))
}

/// The name of a sidebar line: muted while it is closed and not lit.
fn shelf_title(title: &'static str, lit: bool, cx: &App) -> Div {
    div()
        .min_w_0()
        .text_size(TextSize::Xs)
        .font_weight(FontWeight::MEDIUM)
        .truncate()
        .text_color(shelf_tint(lit, cx))
        .child(title)
}

/// The colour of a sidebar line's name, and of a mark that goes with it.
fn shelf_tint(lit: bool, cx: &App) -> Hsla {
    let c = colors(cx);
    if lit { c.foreground } else { c.muted_foreground }
}

/// A server's error, under its name, in lines of this size.
const ERROR_SIZE: TextSize = TextSize::Xs;
const ERROR_LINE_HEIGHT: f32 = 15.;
const ERROR_LINES: usize = 3;

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

/// Why a server that isn't connected isn't, as its link last said.
fn server_error(server: &Server) -> Option<&str> {
    if server.state == State::Connected {
        return None;
    }
    server.error.as_deref().filter(|error| !error.is_empty())
}

/// How many lines a server's error takes under its name in `width`, at most `ERROR_LINES`.
fn error_lines(server: &Server, width: f32, cx: &App) -> usize {
    let Some(error) = server_error(server) else { return 0 };
    let font = crate::transcript::rows::font(theme::UI_FONT, FontWeight::NORMAL, false);
    let mut wrapper = cx.text_system().line_wrapper(font, ERROR_SIZE.into());
    let breaks = wrapper.wrap_line(&[LineFragment::text(error)], px(width), IndentAdjustment::NoIndent).count();
    (breaks + 1).min(ERROR_LINES)
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
    /// The offer to undo archiving or snoozing threads: a line above the shelves, which stays
    /// while the pointer is on it. It comes and goes as one piece.
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
                            .child(line_mark(icons::symbol("undo-2", IconSize::Sm).text_color(shelf_tint(lit, cx))))
                            .child(shelf_title("Undo", lit, cx))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(TextSize::Xs)
                                    .text_color(c.muted_stronger_foreground)
                                    .truncate()
                                    .child(notice.text.clone()),
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

    /// The working, the snoozed or the archived threads' line in the list, which opens and closes
    /// them. It is laid out as a row is: the gap around it is its own. Its symbol and its name
    /// light up only under the pointer, never because it is open; its count stays the quietest.
    pub(super) fn section_row(&self, shelf: Shelf, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let (count, expanded) = (shelf.threads(store).len(), shelf.expanded(&store.prefs));
        let key = shelf.group();
        let lit = self.hovering(key);
        let (title, _) = section_label(shelf.section());
        let color = if lit { c.foreground } else { c.muted_foreground };
        let margin = row_margin();
        let handle = self.store.clone();
        div()
            .id(key)
            .group(key)
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(key, false, false, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(ROW_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(TextSize::Xs)
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .text_color(color)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Xs)
                            .text_color(c.muted_strongest_foreground)
                            .font_features(tabular_numbers())
                            .child(count.to_string()),
                    )
                    .child(div().flex_1().min_w(px(4.)))
                    .child(shelf_chevron(expanded, lit, cx)),
            )
            .on_hover(self.row_hover(key.into(), None, cx))
            .on_click(move |_, _, cx| {
                handle.update(cx, |store, cx| {
                    let open = !shelf.expanded(&store.prefs);
                    shelf.open(&mut store.prefs, open);
                    cx.notify();
                })
            })
            .into_any_element()
    }

    /// A shelf's line, which opens and closes its list, with the chevron at its end.
    fn shelf_header(
        &self,
        shelf: Shelf,
        expanded: bool,
        header: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = shelf.group();
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
                    .child(header)
                    .child(shelf_chevron(expanded, lit, cx)),
            )
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    let open = !shelf.expanded(&store.prefs);
                    shelf.open(&mut store.prefs, open);
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
                .id(shelf.resize_id())
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
        let element = self.list(shelf).element(shelf.list_id(), layout, rows, fill);
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

    /// The servers of the account: one line for them all that opens into a line for each, or the
    /// one server's line.
    pub(super) fn servers_shelf(&mut self, max: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let servers: Vec<Server> = self.store.read(cx).live_servers().cloned().collect();
        let window_width = f32::from(window.viewport_size().width);
        let room = crate::main_view::shown_sidebar_width(self.store.read(cx), window_width)
            - 2. * SIDEBAR_EDGE
            - IconSize::Md.side()
            - 7.;
        let height = |server: &Server| DONE_ROW_HEIGHT + error_lines(server, room, cx) as f32 * ERROR_LINE_HEIGHT;
        if let [server] = servers.as_slice() {
            return div().flex().flex_col().flex_shrink_0().child(divider(cx)).child(
                div()
                    .h(px(height(server)))
                    .my(px(3.))
                    .pl(px(SIDEBAR_EDGE))
                    .pr(px(SIDEBAR_EDGE - BUTTON_OUTSET))
                    .flex()
                    .items_center()
                    .child(self.server_line(server, cx)),
            );
        }
        let expanded = Shelf::Servers.expanded(&self.store.read(cx).prefs);
        let group = Shelf::Servers.group();
        let header = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(line_mark(state_ring(worst_tint(&servers, cx), cx)))
            .child(shelf_title("Servers", expanded || self.hovering(group), cx))
            .child(div().flex_1().min_w(px(4.)))
            .child(
                div()
                    .text_size(TextSize::Xs)
                    .text_color(c.muted_stronger_foreground)
                    .font_features(tabular_numbers())
                    .whitespace_nowrap()
                    .child(reach(&servers)),
            )
            .into_any_element();
        self.list_mut(Shelf::Servers).set_ids(servers.iter().map(|server| server.id.clone()).collect());
        let heights = servers.iter().map(|server| height(server) + ROW_GAP).collect();
        let layout = Layout::new(heights, SHELF_LIST_TOP, 4. - ROW_GAP / 2.);
        let mut rows = Vec::new();
        if expanded {
            let margin = row_margin();
            let visible = self.list_mut(Shelf::Servers).visible(&layout, window);
            for index in visible {
                let line = div()
                    .size_full()
                    .pt(px(margin.top))
                    .pb(px(margin.bottom))
                    .pl(px(margin.left + ROW_PADDING))
                    .pr(px(margin.right + ROW_PADDING - BUTTON_OUTSET))
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
        let line = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(line_mark(state_ring(tint, cx)))
            .child(
                div()
                    .min_w_0()
                    .text_size(TextSize::Xs)
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(server.name.clone()),
            )
            .child(div().flex_1().min_w(px(4.)))
            .child(
                ServerUpdateStatus::new(&self.store, server).otherwise(
                    div()
                        .text_size(TextSize::Xs)
                        .text_color(c.muted_stronger_foreground)
                        .font_features(tabular_numbers())
                        .whitespace_nowrap()
                        .child(detail.clone()),
                ),
            );
        let error = server_error(server).map(|error| {
            div()
                .pl(px(IconSize::Md.side() + 7.))
                .text_size(ERROR_SIZE)
                .line_height(px(ERROR_LINE_HEIGHT))
                .text_color(c.muted_stronger_foreground)
                .line_clamp(ERROR_LINES)
                .child(error.to_string())
        });
        div()
            .id(SharedString::from(format!("server-{}", server.id)))
            .w_full()
            .flex()
            .flex_col()
            .child(line)
            .children(error)
            .tooltip(tooltip(server.error.clone().unwrap_or(detail)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{SHELF_MIN_HEIGHT, capitalized, heights};

    #[test]
    fn a_shelf_is_as_tall_as_its_rows_up_to_its_room() {
        assert_eq!(heights(2, 400.), (72., 72.));
        assert_eq!(heights(20, 400.), (SHELF_MIN_HEIGHT, 400.));
        assert_eq!(heights(20, 100.), (100., 100.));
    }

    #[test]
    fn a_path_reads_capitalised() {
        assert_eq!(capitalized("relay"), "Relay");
        assert_eq!(capitalized(""), "");
    }
}
