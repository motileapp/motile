//! The sidebar's rows: an active thread on three lines, a draft laid out like one, a working,
//! snoozed or archived thread on one quiet line, a draft being sent on the working shelf, the offer to start a thread, the line that says
//! there is nothing, and what a thread is up to.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Section;
use motile_protocol::wire::PullRequest;

use super::{
    BOTTOM_PADDING, FIRST_LINE_HEIGHT, ROW_GAP, ROW_PADDING, SIDEBAR_EDGE, Sidebar, TITLE_HEIGHT, TOP_PADDING,
    button_inset, row_layer, row_margin, snooze_menu, stop_press,
};
use crate::models::{
    Project, ThreadInfo, ago, ask_label, elapsed, interruption_detail, interruption_label, last_component, left, now,
    shortened,
};
use crate::store::{ListedDraft, Selection, stage_label};
use crate::theme::{ControlSize, IconSize, Opacity, Radius, Surface, TextSize, colors};
use crate::ui::logos::{self, PullRequestLabel};
use crate::ui::menu::Menu;
use crate::ui::{
    ActionButton, ActionMenu, Highlight, KeyCaps, Spinner, dashed_divider, icons, sharing, tabular_numbers, tooltip,
};

/// How tall the line under the drafts or an open section is, with the room around it.
pub const LIST_DIVIDER: f32 = 1. + 2. * (4. + ROW_GAP / 2.);
pub const EMPTY_LINE_HEIGHT: f32 = 15.;
/// The room between the parts of a row's status, and of its last line.
const STATUS_GAP: f32 = 9.;

/// A draft's status while its first message is on its way.
fn sending_status(cx: &App) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(3.))
        .child(Spinner::new(IconSize::Sm).render(cx))
        .child(div().text_size(TextSize::Xs).font_weight(FontWeight::MEDIUM).child("Sending"))
        .into_any_element()
}

/// The light under a row: the row layer when the pointer is over it, the selected one when it is
/// selected, drawn inside the row's margin.
pub fn row_light(group: &str, selected: bool, lit: bool, cx: &App) -> Div {
    Highlight::new()
        .radius(Radius::MD)
        .inset(row_margin())
        .selected(selected)
        .lit(lit)
        .opaque(true)
        .render(group.to_string(), cx)
}

/// Where a thread works, on the last line of its row.
fn checkout_label(symbol: &'static str, text: String) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(3.))
        .child(icons::symbol(symbol, IconSize::Sm))
        .child(crate::ui::middle_truncated(text).text_size(TextSize::Xs))
}

impl Sidebar {
    /// The branch the thread works on, or its folder outside git.
    fn where_label(&self, project: Option<&Project>, server_id: &str, folder: &str, cx: &App) -> Option<Div> {
        if let Some(project) = project
            && let Some(branch) = &project.branch
        {
            return Some(checkout_label(project.checkout_symbol(), branch.clone()));
        }
        let home = self.store.read(cx).server(Some(server_id)).map(|server| server.home.clone()).unwrap_or_default();
        Some(checkout_label("folder", shortened(folder, &home)))
    }

    /// The server something is on, when there is more than one.
    fn server_label(&self, server_id: &str, cx: &App) -> Option<AnyElement> {
        let store = self.store.read(cx);
        if store.servers.len() < 2 {
            return None;
        }
        let server = store.server(Some(server_id))?;
        Some(div().min_w_0().child(logos::server_label(server, TextSize::Xs, false, cx)).into_any_element())
    }

    /// A thread's pull request in its row. A click opens the thread and then the pull request's
    /// tab, as View PR does.
    pub(super) fn pull_request_label(&self, thread_id: &str, pull_request: &PullRequest, colored: bool) -> AnyElement {
        let store = self.store.clone();
        let (id, url) = (thread_id.to_string(), pull_request.url.clone());
        stop_press(
            PullRequestLabel::new(SharedString::from(format!("pull-request-{thread_id}")), pull_request)
                .colored(colored)
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.select(Selection::Thread(id.clone()), cx);
                        store.show_pull_request(&url, cx);
                        cx.notify();
                    })
                }),
        )
    }

    /// What a thread's row shows inside its light: its project and what it is doing, its title,
    /// and where it works, its pull request, its server and its agent.
    pub(super) fn thread_row_content(
        &self,
        thread: &ThreadInfo,
        selected: bool,
        hovering: bool,
        jump: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(Some(&thread.project_id)).map(|project| project.seen(thread));
        let project_name =
            project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| last_component(&thread.cwd));
        let read_only = store.on_removed_server(thread);
        let pull_request = project.as_ref().and_then(|project| project.pull_request(thread)).filter(|_| !read_only);
        let status = if let Some(jump) = jump {
            div()
                .mr(px(button_inset() - ROW_PADDING))
                .text_color(c.foreground)
                .child(KeyCaps::new([jump]))
                .into_any_element()
        } else if hovering && (!thread.unfinished() || store.snoozes(thread)) && !read_only {
            div().mr(px(button_inset() - ROW_PADDING)).child(self.row_buttons(thread, selected, cx)).into_any_element()
        } else {
            thread_status(thread, read_only, cx)
        };
        div()
            .relative()
            .size_full()
            .px(px(ROW_PADDING))
            .pt(px(TOP_PADDING))
            .pb(px(BOTTOM_PADDING))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(FIRST_LINE_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(c.muted_foreground)
                    .child(logos::project_icon(project.as_ref(), IconSize::Sm, cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(TextSize::Xs)
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(project_name),
                    )
                    .child(div().flex_1().min_w(px(6.)))
                    .child(div().flex_shrink_0().child(status)),
            )
            .child(
                div()
                    .h(px(TITLE_HEIGHT))
                    .mt(px(1.))
                    .mb(px(5.))
                    .text_size(TextSize::Sm)
                    .line_height(px(TITLE_HEIGHT))
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .text_color(if selected || hovering { c.foreground } else { c.muted_foreground })
                    .child(thread.title.clone()),
            )
            .child(
                div()
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(c.muted_stronger_foreground)
                    .children(self.where_label(project.as_ref(), &thread.server_id, &thread.cwd, cx))
                    .child(div().flex_1().min_w(px(6.)))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(STATUS_GAP))
                            .when_some(pull_request, |line, pull_request| {
                                line.child(self.pull_request_label(&thread.id, &pull_request, true))
                            })
                            .children(self.server_label(&thread.server_id, cx))
                            .child(logos::agent_icon(thread.agent, IconSize::Sm, cx)),
                    ),
            )
    }

    /// What a thread's row offers under the pointer: a snooze, and archiving it once it has finished.
    fn row_buttons(&self, thread: &ThreadInfo, selected: bool, cx: &App) -> Div {
        const GAP: f32 = 2.;
        let id = thread.id.clone();
        let snoozes = self.store.read(cx).snoozes(thread);
        let count = usize::from(snoozes) + usize::from(!thread.unfinished());
        let snooze = snoozes.then(|| {
            let (store, thread_id) = (self.store.clone(), id.clone());
            let menu =
                ActionMenu::icon(SharedString::from(format!("snooze-{id}")), "alarm-clock", "Snooze", move |_, _| {
                    snooze_menu(&store, &thread_id)
                })
                .button(|button| button.small().symbol_size(ControlSize::Small.small_symbol()).row(row_layer(selected)))
                .margin(sharing(GAP, 0, count));
            stop_press(menu)
        });
        let archive = (!thread.unfinished()).then(|| {
            let (store, thread_id) = (self.store.clone(), id.clone());
            let button = ActionButton::new(SharedString::from(format!("archive-{id}")), "Archive")
                .symbol("check")
                .symbol_size(ControlSize::Small.small_symbol())
                .ghost()
                .small()
                .row(row_layer(selected))
                .margin(sharing(GAP, count - 1, count))
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.set_archived(std::slice::from_ref(&thread_id), true, true, cx);
                        cx.notify();
                    })
                });
            stop_press(button)
        });
        div().flex().items_center().children(snooze).children(archive)
    }

    /// An active thread. `movable` rows are picked up by a press the list follows; the others
    /// take a click.
    pub(super) fn thread_row(
        &self,
        thread: &ThreadInfo,
        index: usize,
        movable: bool,
        jump: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.store.read(cx).selection == Selection::Thread(thread.id.clone());
        let key = format!("thread-{}", thread.id);
        let hovering = self.hovering(&key);
        let content = self.thread_row_content(thread, selected, hovering, jump, cx);
        let margin = row_margin();
        let (id, menu_id) = (thread.id.clone(), thread.id.clone());
        let row = div()
            .id(SharedString::from(key.clone()))
            .group(SharedString::from(key.clone()))
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(self.track_bounds(&key))
            .child(row_light(&key, selected, false, cx))
            .child(content)
            .on_hover(self.row_hover(key, Some(thread.id.clone()), cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let Some(menu) = this.thread_menu(menu_id.clone(), cx) else { return };
                    menu.show(event.position, window, cx);
                }),
            );
        if movable {
            let pressed = thread.clone();
            return row
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.press(index, pressed.clone(), event.position, window, cx);
                    }),
                )
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.drag_released(cx)))
                .into_any_element();
        }
        let store = self.store.clone();
        row.on_click(move |_, _, cx| {
            store.update(cx, |store, cx| {
                store.select(Selection::Thread(id.clone()), cx);
                cx.notify();
            })
        })
        .into_any_element()
    }

    /// The threads that haven't been sent yet, above the others, with a line under them.
    pub(super) fn draft_rows(&self, drafts: &[ListedDraft], cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = drafts.iter().map(|listed| self.draft_row(listed, cx)).collect();
        div().flex().flex_col().children(rows).child(list_divider(cx)).into_any_element()
    }

    /// A draft, laid out like a thread's row: its project and "Draft", or "Sending" while its
    /// first message is on its way, on the first line, what was written in it on the second,
    /// where it will work, its server and its agent on the third.
    fn draft_row(&self, listed: &ListedDraft, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(listed.draft.project_id.as_deref()).cloned();
        let selected = store.selection == Selection::Draft(listed.draft.id.clone());
        let agent = store.agent_of(&listed.draft);
        let key = format!("draft-{}", listed.draft.id);
        let hovering = self.hovering(&key);
        let id = listed.draft.id.clone();
        let status = if listed.sending {
            sending_status(cx)
        } else if hovering {
            let (store, draft) = (self.store.clone(), listed.draft.clone());
            let button = ActionButton::icon(SharedString::from(format!("discard-{id}")), "x", "Discard draft")
                .small()
                .row(row_layer(selected))
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.discard(&draft, cx);
                        cx.notify();
                    })
                });
            div().mr(px(button_inset() - ROW_PADDING)).child(button).into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(3.))
                .child(icons::symbol("file", IconSize::Sm))
                .child(div().text_size(TextSize::Xs).font_weight(FontWeight::MEDIUM).child("Draft"))
                .into_any_element()
        };
        let group = SharedString::from(key.clone());
        let margin = row_margin();
        let (select, menu_store, menu_draft, sending) =
            (self.store.clone(), self.store.clone(), listed.draft.clone(), listed.sending);
        div()
            .id(SharedString::from(key.clone()))
            .group(group.clone())
            .relative()
            .h(px(THREAD_ROW_HEIGHT + ROW_GAP))
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(&key, selected, false, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(ROW_PADDING))
                    .pt(px(TOP_PADDING))
                    .pb(px(BOTTOM_PADDING))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(FIRST_LINE_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.muted_foreground)
                            .child(logos::project_icon(project.as_ref(), IconSize::Sm, cx))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(TextSize::Xs)
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(
                                        project
                                            .as_ref()
                                            .map_or("No project".to_string(), |project| project.name.clone()),
                                    ),
                            )
                            .child(div().flex_1().min_w(px(6.)))
                            .child(div().flex_shrink_0().child(status)),
                    )
                    .child(
                        div()
                            .h(px(TITLE_HEIGHT))
                            .mt(px(1.))
                            .mb(px(5.))
                            .text_size(TextSize::Sm)
                            .line_height(px(TITLE_HEIGHT))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .text_color(if selected || hovering { c.foreground } else { c.muted_foreground })
                            .child(listed.preview.clone()),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.muted_stronger_foreground)
                            .when_some(project.clone(), |line, project| {
                                line.children(self.where_label(Some(&project), &project.server_id, &project.path, cx))
                            })
                            .child(div().flex_1().min_w(px(6.)))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(STATUS_GAP))
                                    .when_some(project.as_ref(), |line, project| {
                                        line.children(self.server_label(&project.server_id, cx))
                                    })
                                    .when_some(agent, |line, agent| {
                                        line.child(logos::agent_icon(agent, IconSize::Sm, cx))
                                    }),
                            ),
                    ),
            )
            .on_hover(self.row_hover(key, None, cx))
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Draft(id.clone()), cx);
                    cx.notify();
                })
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if sending {
                    return;
                }
                let (store, draft) = (menu_store.clone(), menu_draft.clone());
                Menu::new()
                    .danger_item("Discard Draft", move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.discard(&draft, cx);
                            cx.notify();
                        })
                    })
                    .show(event.position, window, cx);
            })
            .into_any_element()
    }

    /// A thread that is working, snoozed or archived: one quiet line, with its pull request and
    /// what it does, how soon it wakes or when it was archived. Under the pointer a snoozed or
    /// archived one's time gives way to the button that brings it back, which lies on the row's light.
    pub(super) fn slim_row(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.selection == Selection::Thread(thread.id.clone());
        let project = store.project(Some(&thread.project_id)).cloned();
        let pull_request = project.as_ref().and_then(|project| project.pull_request(thread));
        let key = format!("slim-{}", thread.id);
        let hovering = self.hovering(&key);
        let removed = store.on_removed_server(thread);
        let (select, menu_id, id) = (self.store.clone(), thread.id.clone(), thread.id.clone());
        let margin = row_margin();
        let layer = row_layer(selected).unwrap_or(crate::theme::Layer::Row);
        let back = self.bring_back_button(thread, layer);
        let trailing = match (thread.section, thread.snoozed_until) {
            (Section::Working, _) => thread_status(thread, removed, cx),
            (Section::Snoozed, Some(until)) => div()
                .id(SharedString::from(format!("wakes-{}", thread.id)))
                .flex()
                .items_center()
                .gap(px(3.))
                .child(icons::symbol("alarm-clock", IconSize::Sm))
                .child(div().font_features(tabular_numbers()).child(left(until)))
                .tooltip(tooltip(format!("Wakes {}", crate::format::stamp(until))))
                .into_any_element(),
            _ => div().child(ago(thread.archived_at.unwrap_or(thread.updated_at))).into_any_element(),
        };
        div()
            .id(SharedString::from(key.clone()))
            .group(SharedString::from(key.clone()))
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(self.track_bounds(&key))
            .child(row_light(&key, selected, false, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(ROW_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(logos::project_icon(project.as_ref(), IconSize::Md, cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(TextSize::Sm)
                            .text_color(if selected || hovering { c.foreground } else { c.muted_foreground })
                            .truncate()
                            .child(thread.title.clone()),
                    )
                    .child(div().flex_1().min_w(px(6.)))
                    .when_some(pull_request, |line, pull_request| {
                        line.child(self.pull_request_label(&thread.id, &pull_request, false))
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextSize::Xs)
                            .text_color(c.muted_stronger_foreground)
                            .when(hovering && back.is_some(), |time| time.opacity(0.))
                            .child(trailing),
                    )
                    .when_some(back.filter(|_| hovering && !removed), |line, back| {
                        line.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right(px(ControlSize::INSET_MARGIN))
                                .flex()
                                .items_center()
                                .pl(px(6.))
                                .bg(Surface::Background.layer(layer, c))
                                .child(stop_press(back)),
                        )
                    }),
            )
            .on_hover(self.row_hover(key, Some(thread.id.clone()), cx))
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Thread(id.clone()), cx);
                    cx.notify();
                })
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let Some(menu) = this.thread_menu(menu_id.clone(), cx) else { return };
                    menu.show(event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// A draft whose first message is on its way, on the working shelf until its thread takes its
    /// row: one quiet line, like the thread's.
    pub(super) fn starting_row(&self, listed: &ListedDraft, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.selection == Selection::Draft(listed.draft.id.clone());
        let project = store.project(listed.draft.project_id.as_deref()).cloned();
        let key = format!("slim-{}", listed.draft.id);
        let hovering = self.hovering(&key);
        let (select, id) = (self.store.clone(), listed.draft.id.clone());
        let margin = row_margin();
        div()
            .id(SharedString::from(key.clone()))
            .group(SharedString::from(key.clone()))
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(&key, selected, false, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(ROW_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(logos::project_icon(project.as_ref(), IconSize::Md, cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(TextSize::Sm)
                            .text_color(if selected || hovering { c.foreground } else { c.muted_foreground })
                            .truncate()
                            .child(listed.preview.clone()),
                    )
                    .child(div().flex_1().min_w(px(6.)))
                    .child(div().flex_shrink_0().text_color(c.muted_foreground).child(sending_status(cx))),
            )
            .on_hover(self.row_hover(key, None, cx))
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Draft(id.clone()), cx);
                    cx.notify();
                })
            })
            .into_any_element()
    }

    /// What brings a snoozed or archived thread back: waking it, or unarchiving it. A working
    /// one has none.
    fn bring_back_button(&self, thread: &ThreadInfo, layer: crate::theme::Layer) -> Option<ActionButton> {
        let (store, id) = (self.store.clone(), thread.id.clone());
        let (symbol, help) = match thread.section {
            Section::Snoozed => ("alarm-clock-off", "Wake"),
            Section::Archived => ("undo-2", "Unarchive"),
            Section::Active | Section::Working => return None,
        };
        let section = thread.section;
        let button = ActionButton::icon(SharedString::from(format!("back-{id}")), symbol, help)
            .size(ControlSize::Inset)
            .row(Some(layer))
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    if section == Section::Snoozed {
                        store.wake(std::slice::from_ref(&id));
                    } else {
                        store.set_archived(std::slice::from_ref(&id), false, false, cx);
                    }
                    cx.notify();
                })
            });
        Some(button)
    }

    /// Starts a thread, where the sidebar has no draft and no active thread. Laid out like a slim
    /// row, and muted until it lights up.
    pub(super) fn new_thread_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let key = "new-thread-row";
        let margin = row_margin();
        let store = self.store.clone();
        let light = Highlight::new().radius(Radius::MD).inset(margin).lit(self.hovering(key)).opaque(true);
        div()
            .id(key)
            .group(key)
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(light.render(key, cx))
            .child(
                light.faded(
                    div()
                        .relative()
                        .size_full()
                        .px(px(ROW_PADDING))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .child(
                            div()
                                .w(px(IconSize::Md.side()))
                                .flex()
                                .justify_center()
                                .child(icons::symbol("plus", IconSize::Md)),
                        )
                        .child(div().min_w_0().text_size(TextSize::Sm).truncate().child("Create a thread")),
                    cx,
                ),
            )
            .on_hover(self.row_hover(key.into(), None, cx))
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.new_thread(cx);
                    cx.notify();
                })
            })
            .into_any_element()
    }

    /// The line where the list has nothing: no thread yet, none active, or nothing found; or under
    /// what was found while the servers still search.
    pub(super) fn empty_line(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let line = div()
            .h(px(EMPTY_LINE_HEIGHT))
            .mx(px(SIDEBAR_EDGE))
            .my(px(6. + ROW_GAP / 2.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(TextSize::Xs)
            .text_color(c.muted_stronger_foreground);
        if !store.is_searching_threads() {
            let text = if store.all_threads().next().is_none() { "No threads yet" } else { "No active threads" };
            return line.child(text).into_any_element();
        }
        if store.sidebar.message_search.searching {
            return line.child(Spinner::new(IconSize::Sm).render(cx)).child("Searching messages").into_any_element();
        }
        line.child("No threads found").into_any_element()
    }
}

/// How tall an active thread's row is, without the gap around it.
pub const THREAD_ROW_HEIGHT: f32 = TOP_PADDING + FIRST_LINE_HEIGHT + 1. + TITLE_HEIGHT + 5. + 16. + BOTTOM_PADDING;

/// What a thread is up to, or how long ago it last was. A thread of an unlinked server is only
/// ever read-only.
pub fn thread_status(thread: &ThreadInfo, read_only: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    let label = |text: String, color: Hsla, icon: AnyElement| {
        div().flex().items_center().gap(px(3.)).text_color(color).child(icon).child(
            div()
                .text_size(TextSize::Xs)
                .font_weight(FontWeight::MEDIUM)
                .font_features(tabular_numbers())
                .whitespace_nowrap()
                .child(text),
        )
    };
    let symbol = |name: &'static str| icons::symbol(name, IconSize::Sm).into_any_element();
    if read_only {
        return label("Read-only".into(), c.warning, symbol("unlink"))
            .id(SharedString::from(format!("read-only-{}", thread.id)))
            .tooltip(tooltip("Its server is unlinked, so this thread can only be read or continued on another server."))
            .into_any_element();
    }
    if let Some(ask) = thread.waits_for() {
        let (word, name) = ask_label(ask);
        let status = label(word.into(), c.warning, symbol(name));
        // An agent it started waits: the tooltip says which.
        let waiting = thread.asking().is_none().then(|| thread.children.as_ref()?.waiting_text.clone()).flatten();
        let Some(waiting) = waiting else { return status.into_any_element() };
        return status
            .id(SharedString::from(format!("waiting-{}", thread.id)))
            .tooltip(tooltip(waiting))
            .into_any_element();
    }
    if thread.waits_on_agents() {
        let agents = thread.children.as_ref().map_or(0, |children| children.running);
        let help = thread.children.as_ref().and_then(|children| children.running_text.clone()).unwrap_or_default();
        return div()
            .id(SharedString::from(format!("waits-on-agents-{}", thread.id)))
            .flex()
            .items_center()
            .gap(px(STATUS_GAP))
            .child(label(agents.to_string(), c.pending, symbol("bot")))
            .child(label(elapsed(thread.monitoring_since()), c.process, symbol("circle-dot-dashed")))
            .tooltip(tooltip(help))
            .into_any_element();
    }
    if thread.working() {
        let agents = thread.agents.max(thread.children.as_ref().map_or(0, |children| children.running));
        return div()
            .flex()
            .items_center()
            .gap(px(STATUS_GAP))
            .when(agents > 0, |status| {
                let help =
                    if agents == 1 { "1 agent is working".to_string() } else { format!("{agents} agents are working") };
                status.child(
                    label(agents.to_string(), c.pending, symbol("bot"))
                        .id(SharedString::from(format!("agents-{}", thread.id)))
                        .tooltip(tooltip(help)),
                )
            })
            .child(label(elapsed(thread.updated_at), c.process, symbol("circle-dashed")))
            .into_any_element();
    }
    if let Some(stage) = thread.git_stage {
        return label(stage_label(stage).into(), c.process, Spinner::new(IconSize::Sm).render(cx).into_any_element())
            .into_any_element();
    }
    if thread.monitoring {
        return label(elapsed(thread.monitoring_since()), c.foreground, symbol("eye")).into_any_element();
    }
    if let Some(interruption) = thread.interruption {
        let (word, name) = interruption_label(interruption);
        return label(word.into(), c.warning, symbol(name))
            .id(SharedString::from(format!("interruption-{}", thread.id)))
            .tooltip(tooltip(interruption_detail(interruption, now())))
            .into_any_element();
    }
    if thread.woke {
        return label("Woke".into(), c.warning, symbol("alarm-clock")).into_any_element();
    }
    if thread.unread {
        return label("Finished".into(), c.success, symbol("flag")).into_any_element();
    }
    div()
        .text_size(TextSize::Xs)
        .text_color(c.muted_stronger_foreground)
        .whitespace_nowrap()
        .child(ago(thread.updated_at))
        .into_any_element()
}

/// Whether an archived or found thread is shown quieter: its project's icon at the disabled opacity.
pub fn quieted(element: AnyElement, quiet: bool, cx: &App) -> AnyElement {
    if !quiet {
        return element;
    }
    div().opacity(colors(cx).opacity(Opacity::Disabled)).child(element).into_any_element()
}

/// The line that sets a group of the list's rows off from the rows under it: the drafts, or an
/// open section.
pub fn list_divider(cx: &App) -> AnyElement {
    div().mx(px(SIDEBAR_EDGE)).my(px(4. + ROW_GAP / 2.)).child(dashed_divider(cx)).into_any_element()
}
