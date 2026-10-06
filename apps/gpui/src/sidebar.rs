//! The drafts, then every active thread on every server in one list, with the ones marked done on
//! a shelf at the bottom, and the servers and the account under them: the Mac app's
//! `SidebarView.swift` and `UpdateViews.swift`.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::link::State;
use motile_protocol::wire::PullRequest;

use crate::main_view::ROW_INSET;
use crate::models::{Project, Server, ThreadInfo, ago, elapsed, last_component};
use crate::store::{ListedDraft, Selection, Store, UndoNotice, stage_label};
use crate::theme::{self, ControlSize, Surface, colors};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::logos::{self, PullRequestLabel};
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{ActionButton, InputField, InputVariant, Spinner, divider, edges, icons, tooltip};
use crate::updater::UpdateState;

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
const ROW_GAP: f32 = 2.;
const DONE_ROW_HEIGHT: f32 = 30.;
const DONE_DEFAULT_HEIGHT: f32 = 250.;
const DONE_MIN_HEIGHT: f32 = 4. * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
/// How far a row's lines stand from its highlight's sides and top.
const SIDE_PADDING: f32 = 8.;
const TOP_PADDING: f32 = 5.;
const FIRST_LINE_HEIGHT: f32 = 22.;
/// What the sidebar has above its list: the window's top bar and the search field.
const ABOVE_LIST: f32 = theme::TOP_BAR + 36.;

fn row_margin() -> Edges<f32> {
    edges(ROW_GAP / 2., ROW_INSET, ROW_GAP / 2., ROW_INSET)
}

/// How far past the row's side padding a button on its first line reaches, so that it is as far
/// from the row's side as from its top.
fn button_outset() -> f32 {
    TOP_PADDING + (FIRST_LINE_HEIGHT - ControlSize::Small.height()) / 2. - SIDE_PADDING
}

/// The layer a row's buttons lie on: the row's light.
fn row_surface(selected: bool) -> Surface {
    if selected { Surface::Background.further() } else { Surface::Background.next() }
}

/// The light under a row: the next layer when the pointer is over it, the one further when it
/// is selected. It is drawn `inset` from the edges of the element `group` names.
fn row_light(group: SharedString, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .absolute()
        .top(px(inset.top))
        .left(px(inset.left))
        .right(px(inset.right))
        .bottom(px(inset.bottom))
        .rounded(px(radius))
        .when(selected, |light| light.bg(Surface::Background.further().color(c)))
        .when(!selected, |light| light.group_hover(group, move |light| light.bg(Surface::Background.next().color(c))))
}

/// A thread's pull request: its own, or the one of the worktree it works in.
fn pull_request_of(project: &Project, thread: &ThreadInfo) -> Option<PullRequest> {
    if let Some(pull_request) = &thread.pull_request {
        return Some(pull_request.clone());
    }
    project.worktrees.iter().find(|worktree| worktree.path == thread.cwd)?.git.as_ref()?.pull_request.clone()
}

/// The server something is on, when there is more than one.
fn other_server(store: &Store, server_id: &str) -> Option<Server> {
    if store.servers.len() < 2 {
        return None;
    }
    store.server(Some(server_id)).cloned()
}

/// A thread being renamed or deleted, with what its alert needs.
enum Asking {
    Rename { thread_id: String, title: Entity<InputState> },
    Delete { thread_id: String, title: String, in_worktree: bool },
}

pub struct Sidebar {
    store: Entity<Store>,
    search: Entity<InputState>,
    asking: Option<Asking>,
    account_menu: Anchor,
    /// How far the done shelf's line is being pulled up, while it is.
    pulling: Option<(f32, f32)>,
    /// The row under the pointer, which shows its buttons in place of its status.
    hovered: Option<String>,
    done_scroll: ScrollHandle,
    /// The thread the done list scrolls to once it is drawn.
    reveal_done: Option<String>,
    seen_settled: Option<String>,
    _ticks: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            cx.observe(&store, |this, store, cx| {
                this.settled(store, cx);
                cx.notify();
            }),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        // The times in the rows go on: every second while a thread works, else every half minute.
        let ticks = cx.spawn(async move |this, cx| {
            let mut waited = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                waited += 1;
                let Ok(running) =
                    this.read_with(cx, |this, cx| this.store.read(cx).threads.values().any(|thread| thread.busy()))
                else {
                    break;
                };
                if running || waited >= 30 {
                    waited = 0;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            store,
            search,
            asking: None,
            account_menu: Anchor::default(),
            pulling: None,
            hovered: None,
            done_scroll: ScrollHandle::new(),
            reveal_done: None,
            seen_settled: None,
            _ticks: ticks,
            _subscriptions: subscriptions,
        }
    }

    /// A merge has marked a thread done: the done list opens on it.
    fn settled(&mut self, store: Entity<Store>, cx: &mut Context<Self>) {
        let settled = store.read(cx).sidebar.settled_thread_id.clone();
        if settled.is_none() || settled == self.seen_settled {
            return;
        }
        self.seen_settled = settled.clone();
        self.reveal_done = settled;
        store.update(cx, |store, _| store.prefs.set("sidebar.doneExpanded", true));
    }

    fn track_hover(&self, key: String, cx: &mut Context<Self>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hovered = Some(key.clone());
            } else if this.hovered.as_ref() == Some(&key) {
                this.hovered = None;
            }
            cx.notify();
        })
    }

    fn hovering(&self, key: &str) -> bool {
        self.hovered.as_deref() == Some(key)
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    /// Whether the thread's title or project matches what is being searched for.
    fn matches(&self, store: &Store, thread: &ThreadInfo, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        let project =
            store.project(Some(&thread.project_id)).map(|project| project.name.to_lowercase()).unwrap_or_default();
        thread.title.to_lowercase().contains(&query) || project.contains(&query)
    }

    fn begin_rename(&mut self, thread_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self.store.read(cx).threads.get(&thread_id).map(|thread| thread.title.clone()) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title).placeholder("Title"));
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.finish_rename(cx);
            }
        })
        .detach();
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.asking = Some(Asking::Rename { thread_id, title: input });
        cx.notify();
    }

    fn finish_rename(&mut self, cx: &mut Context<Self>) {
        let Some(Asking::Rename { thread_id, title }) = self.asking.take() else { return };
        let title = title.read(cx).value().to_string();
        self.store.update(cx, |store, cx| {
            store.rename(&thread_id, &title);
            cx.notify();
        });
        cx.notify();
    }

    fn begin_delete(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let Some(thread) = store.threads.get(&thread_id) else { return };
        let in_worktree =
            store.project(Some(&thread.project_id)).is_some_and(|project| project.seen(thread).worktree.is_some());
        self.asking = Some(Asking::Delete { thread_id, title: thread.title.clone(), in_worktree });
        cx.notify();
    }

    fn thread_menu(&self, thread_id: String, cx: &mut Context<Self>) -> Menu {
        let store = self.store.read(cx);
        let Some(thread) = store.threads.get(&thread_id) else { return Menu::new() };
        let (done, busy) = (thread.is_done(), thread.busy());
        let this = cx.entity().downgrade();
        let handle = self.store.clone();
        let id = thread_id.clone();
        let menu = if done {
            Menu::new().item("Mark Undone", move |_, cx| {
                handle.update(cx, |store, cx| {
                    store.set_done(std::slice::from_ref(&id), false, false, cx);
                    cx.notify();
                })
            })
        } else {
            Menu::new().item_if(!busy, "Mark Done", move |_, cx| {
                handle.update(cx, |store, cx| {
                    store.set_done(std::slice::from_ref(&id), true, true, cx);
                    cx.notify();
                })
            })
        };
        let (rename, delete) = (this.clone(), this);
        let (rename_id, delete_id) = (thread_id.clone(), thread_id);
        menu.item("Rename…", move |window, cx| {
            let _ = rename.update(cx, |this, cx| this.begin_rename(rename_id.clone(), window, cx));
        })
        .separator()
        .danger_item("Delete…", move |_, cx| {
            let _ = delete.update(cx, |this, cx| this.begin_delete(delete_id.clone(), cx));
        })
    }

    fn search_field(&self) -> impl IntoElement {
        div()
            .px(px(10.))
            .pt(px(2.))
            .pb(px(6.))
            .child(InputField::new(&self.search).icon("search").variant(InputVariant::Filled).clearable(true))
    }

    /// Opens the thread and then its pull request's tab, as View PR does.
    fn pull_request_label(&self, thread_id: &str, pull_request: &PullRequest, colored: bool) -> PullRequestLabel {
        let store = self.store.clone();
        let (id, number) = (thread_id.to_string(), pull_request.number);
        PullRequestLabel::new(SharedString::from(format!("pull-request-{thread_id}")), pull_request)
            .colored(colored)
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.select(Selection::Thread(id.clone()), cx);
                    store.open_pull_request_tab(number, cx);
                    cx.notify();
                })
            })
    }

    /// An active thread: its project and what it is doing on the first line, its title on the
    /// second, its project's branch, its pull request, its server and its agent on the third.
    fn thread_row(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(Some(&thread.project_id)).map(|project| project.seen(thread));
        let project_name =
            project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| last_component(&thread.cwd));
        let selected = store.selection == Selection::Thread(thread.id.clone());
        let server = other_server(store, &thread.server_id);
        let pull_request = project.as_ref().and_then(|project| pull_request_of(project, thread));
        let branch = project.as_ref().and_then(|project| project.branch.clone());
        let key = format!("thread-{}", thread.id);
        let shows_button = self.hovering(&key) && !thread.busy();
        let (id, menu_id, done_id) = (thread.id.clone(), thread.id.clone(), thread.id.clone());
        let (select, mark_done) = (self.store.clone(), self.store.clone());
        let margin = row_margin();
        div()
            .id(SharedString::from(format!("thread-row-{id}")))
            .on_hover(self.track_hover(key.clone(), cx))
            .group(SharedString::from(key.clone()))
            .relative()
            .w_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(key.into(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .px(px(SIDE_PADDING))
                    .pt(px(TOP_PADDING))
                    .pb(px(7.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(FIRST_LINE_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.secondary)
                            .child(logos::project_icon(project.as_ref(), 14., cx))
                            .child(
                                div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).truncate().child(project_name),
                            )
                            .child(div().flex_1().min_w(px(6.)))
                            .when(shows_button, |line| {
                                line.child(
                                    div().mr(px(button_outset())).child(
                                        ActionButton::new(
                                            SharedString::from(format!("mark-done-{done_id}")),
                                            "Mark Done",
                                        )
                                        .symbol("check")
                                        .ghost()
                                        .small()
                                        .surface(row_surface(selected))
                                        .on_click(move |_, _, cx| {
                                            mark_done.update(cx, |store, cx| {
                                                store.set_done(std::slice::from_ref(&done_id), true, true, cx);
                                                cx.notify();
                                            })
                                        }),
                                    ),
                                )
                            })
                            .when(!shows_button, |line| line.child(thread_status(thread, cx))),
                    )
                    .child(
                        div()
                            .pt(px(1.))
                            .pb(px(5.))
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(thread.title.clone()),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.tertiary)
                            .when_some(branch, |line, branch| {
                                line.child(div().text_size(px(11.)).truncate().child(branch))
                            })
                            .child(div().flex_1().min_w(px(6.)))
                            .when_some(pull_request, |line, pull_request| {
                                line.child(self.pull_request_label(&thread.id, &pull_request, true))
                            })
                            .when_some(server, |line, server| line.child(logos::server_label(&server, 11., cx)))
                            .child(logos::agent_icon(thread.agent, 12., cx)),
                    ),
            )
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Thread(id.clone()), cx);
                    cx.notify();
                })
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.thread_menu(menu_id.clone(), cx).show(event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// A draft: its project on the first line, what was written in it on the second.
    fn draft_row(&self, listed: &ListedDraft, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(listed.draft.project_id.as_deref()).cloned();
        let selected = store.selection == Selection::Draft(listed.draft.id.clone());
        let server = project.as_ref().and_then(|project| other_server(store, &project.server_id));
        let key = format!("draft-{}", listed.draft.id);
        let hovering = self.hovering(&key);
        let (select, discard, menu_discard) = (self.store.clone(), self.store.clone(), self.store.clone());
        let (id, draft, menu_draft) = (listed.draft.id.clone(), listed.draft.clone(), listed.draft.clone());
        let margin = row_margin();
        div()
            .id(SharedString::from(format!("draft-row-{id}")))
            .on_hover(self.track_hover(key.clone(), cx))
            .group(SharedString::from(key.clone()))
            .relative()
            .w_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(key.into(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .px(px(SIDE_PADDING))
                    .pt(px(TOP_PADDING))
                    .pb(px(7.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .h(px(FIRST_LINE_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.secondary)
                            .child(logos::project_icon(project.as_ref(), 14., cx))
                            .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).truncate().child(
                                project.as_ref().map(|project| project.name.clone()).unwrap_or("No project".into()),
                            ))
                            .when_some(server, |line, server| line.child(logos::server_label(&server, 11., cx)))
                            .child(div().flex_1().min_w(px(6.)))
                            .when(hovering, |line| {
                                line.child(
                                    div().mr(px(button_outset())).child(
                                        ActionButton::icon(
                                            SharedString::from(format!("discard-{id}")),
                                            "x",
                                            "Discard draft",
                                        )
                                        .small()
                                        .surface(row_surface(selected))
                                        .on_click(move |_, _, cx| {
                                            discard.update(cx, |store, cx| {
                                                store.discard(&draft, cx);
                                                cx.notify();
                                            })
                                        }),
                                    ),
                                )
                            })
                            .when(!hovering, |line| {
                                line.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.))
                                        .child(icons::symbol("file", 11.))
                                        .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).child("Draft")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(listed.preview.clone()),
                    ),
            )
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Draft(id.clone()), cx);
                    cx.notify();
                })
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let discard = menu_discard.clone();
                let draft = menu_draft.clone();
                Menu::new()
                    .danger_item("Discard Draft", move |_, cx| {
                        discard.update(cx, |store, cx| {
                            store.discard(&draft, cx);
                            cx.notify();
                        })
                    })
                    .show(event.position, window, cx);
            })
            .into_any_element()
    }

    /// A line of the shelf at the bottom: in the secondary colour until the pointer is over it.
    fn faded_line(&self, id: &'static str, cx: &App) -> Stateful<Div> {
        let c = colors(cx);
        div()
            .id(id)
            .px(px(18.))
            .flex()
            .items_center()
            .gap(px(7.))
            .text_color(c.secondary)
            .hover(move |line| line.bg(Surface::Background.next().color(c)).text_color(c.text))
    }

    /// The offer to undo marking threads done: a line above the done threads.
    fn undo_row(&self, notice: &UndoNotice, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.clone();
        div().flex().flex_col().flex_shrink_0().child(divider(cx)).child(
            self.faded_line("undo", cx)
                .h(px(DONE_ROW_HEIGHT + 8.))
                .child(div().w(px(14.)).flex().justify_center().child(icons::symbol("undo-2", 10.)))
                .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).child("Undo"))
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(icons::symbol("check", 9.))
                        .child(div().text_size(px(11.)).truncate().child(notice.text.clone())),
                )
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.perform_undo(cx);
                        cx.notify();
                    })
                }),
        )
    }

    /// The threads marked done, at the bottom of the sidebar: a line that opens into their list.
    fn done_shelf(
        &mut self,
        threads: &[&ThreadInfo],
        max_height: f32,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = colors(cx);
        let content_height = threads.len() as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
        let tallest = max_height.min(content_height);
        let heights = (DONE_MIN_HEIGHT.min(tallest), tallest);
        let resting = self.store.read(cx).prefs.f32_or("sidebar.doneHeight", DONE_DEFAULT_HEIGHT);
        let pulled = self.pulling.map_or(0., |(start, now)| start - now);
        let list_height = (resting.clamp(heights.0, heights.1) + pulled).clamp(heights.0, heights.1);
        let toggle = self.store.clone();
        let searching = !self.query(cx).is_empty();
        if expanded
            && let Some(reveal) = self.reveal_done.take()
            && let Some(index) = threads.iter().position(|thread| thread.id == reveal)
        {
            self.done_scroll.scroll_to_item(index);
        }
        let rows: Vec<AnyElement> = threads.iter().map(|thread| self.done_row(thread, cx)).collect();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(if expanded && heights.0 < heights.1 {
                self.resize_handle(cx).into_any_element()
            } else {
                divider(cx).into_any_element()
            })
            .child(
                self.faded_line("done-shelf", cx)
                    .h(px(DONE_ROW_HEIGHT + 4. + if expanded { 4. - ROW_GAP / 2. } else { 4. }))
                    .pt(px(4.))
                    .pb(px(if expanded { 4. - ROW_GAP / 2. } else { 4. }))
                    .child(
                        div()
                            .w(px(14.))
                            .flex()
                            .justify_center()
                            .child(icons::symbol("chevron-right", 10.).rotate(if expanded { 90. } else { 0. })),
                    )
                    .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).child("Done"))
                    .child(div().flex_1())
                    .child(div().text_size(px(11.)).text_color(c.tertiary).child(threads.len().to_string()))
                    .on_click(move |_, _, cx| {
                        if searching {
                            return;
                        }
                        toggle.update(cx, |store, cx| {
                            let expanded = !store.prefs.bool("sidebar.doneExpanded");
                            store.prefs.set("sidebar.doneExpanded", expanded);
                            cx.notify();
                        })
                    }),
            )
            .when(expanded, |shelf| {
                shelf.child(
                    div()
                        .id("done-list")
                        .h(px(list_height + ROW_GAP / 2.))
                        .overflow_y_scroll()
                        .track_scroll(&self.done_scroll)
                        .child(div().pb(px(4. - ROW_GAP / 2.)).children(rows)),
                )
            })
            .child(divider(cx))
    }

    /// The line above the done threads. Dragging it makes their list taller or shorter; the
    /// height is saved when the drag ends.
    fn resize_handle(&self, cx: &mut Context<Self>) -> Div {
        div().relative().child(divider(cx)).child(
            div()
                .id("done-resize")
                .absolute()
                .left_0()
                .right_0()
                .top(px(-(theme::RESIZE_GRAB - 1.) / 2.))
                .h(px(theme::RESIZE_GRAB))
                .cursor(CursorStyle::ResizeUpDown)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        let y = f32::from(event.position.y);
                        this.pulling = Some((y, y));
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        )
    }

    fn finish_pulling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((start, now)) = self.pulling.take() else { return };
        let store = self.store.read(cx);
        let content = store.done_threads().len() as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
        let tallest = ((f32::from(window.viewport_size().height) - ABOVE_LIST) * 0.6).min(content);
        let lowest = DONE_MIN_HEIGHT.min(tallest);
        let resting = store.prefs.f32_or("sidebar.doneHeight", DONE_DEFAULT_HEIGHT).clamp(lowest, tallest);
        let height = (resting + start - now).clamp(lowest, tallest);
        self.store.update(cx, |store, cx| {
            store.prefs.set("sidebar.doneHeight", height);
            cx.notify();
        });
        cx.notify();
    }

    /// A thread that is done: one quiet line, with its pull request.
    fn done_row(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.selection == Selection::Thread(thread.id.clone());
        let project = store.project(Some(&thread.project_id)).cloned();
        let pull_request = project.as_ref().and_then(|project| pull_request_of(project, thread));
        let key = format!("done-{}", thread.id);
        let hovering = self.hovering(&key);
        let (select, undone, menu_id) = (self.store.clone(), self.store.clone(), thread.id.clone());
        let (id, undone_id) = (thread.id.clone(), thread.id.clone());
        let margin = row_margin();
        div()
            .id(SharedString::from(format!("done-row-{id}")))
            .on_hover(self.track_hover(key.clone(), cx))
            .group(SharedString::from(key.clone()))
            .relative()
            .h(px(DONE_ROW_HEIGHT + ROW_GAP))
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(row_light(key.into(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(SIDE_PADDING))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(logos::project_icon(project.as_ref(), 14., cx))
                    .child(div().text_size(px(13.)).text_color(c.secondary).truncate().child(thread.title.clone()))
                    .child(div().flex_1().min_w(px(6.)))
                    .when_some(pull_request, |line, pull_request| {
                        line.child(self.pull_request_label(&thread.id, &pull_request, false))
                    })
                    .when(hovering, |line| {
                        line.child(
                            div().mr(px((DONE_ROW_HEIGHT - ControlSize::Small.height()) / 2. - SIDE_PADDING)).child(
                                ActionButton::icon(SharedString::from(format!("undone-{id}")), "undo-2", "Mark undone")
                                    .small()
                                    .surface(row_surface(selected))
                                    .on_click(move |_, _, cx| {
                                        undone.update(cx, |store, cx| {
                                            store.set_done(std::slice::from_ref(&undone_id), false, false, cx);
                                            cx.notify();
                                        })
                                    }),
                            ),
                        )
                    })
                    .when(!hovering, |line| {
                        line.child(
                            div()
                                .text_size(px(11.))
                                .text_color(c.tertiary)
                                .child(ago(thread.done_at.unwrap_or(thread.updated_at))),
                        )
                    }),
            )
            .on_click(move |_, _, cx| {
                select.update(cx, |store, cx| {
                    store.select(Selection::Thread(id.clone()), cx);
                    cx.notify();
                })
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.thread_menu(menu_id.clone(), cx).show(event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// The servers and how the client reaches them, and the account.
    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let email = store.account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let anchor = self.account_menu.clone();
        let menu_store = self.store.clone();
        let account_margin = edges(4., 10., 8., 10.);
        div()
            .px(px(18.))
            .pt(px(10.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(self.app_update_row(cx))
            .children(store.servers.iter().map(|server| self.server_line(server, cx)))
            .child(
                div()
                    .id("account")
                    .group("account")
                    .relative()
                    .mx(px(-SIDE_PADDING - account_margin.left))
                    .mt(px(-account_margin.top))
                    .mb(px(-account_margin.bottom))
                    .pt(px(account_margin.top))
                    .pb(px(account_margin.bottom))
                    .px(px(account_margin.left))
                    .text_color(c.secondary)
                    .hover(|row| row.text_color(c.text))
                    .child(row_light("account".into(), 8., account_margin, false, cx))
                    .child(
                        div()
                            .relative()
                            .h(px(30.))
                            .px(px(SIDE_PADDING))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(anchor.track())
                            .child(icons::symbol("circle-user", 14.))
                            .child(div().text_size(px(12.)).truncate().child(email)),
                    )
                    .on_click(move |_, window, cx| {
                        let (add_project, add_server, sign_out) =
                            (menu_store.clone(), menu_store.clone(), menu_store.clone());
                        Menu::new()
                            .item("Settings…", |window, cx| window.dispatch_action(Box::new(crate::OpenSettings), cx))
                            .item("Add a Project…", move |_, cx| {
                                add_project.update(cx, |store, cx| {
                                    store.add_project();
                                    cx.notify();
                                })
                            })
                            .item("Add a Server…", move |_, cx| {
                                add_server.update(cx, |store, cx| {
                                    store.shows_add_server = true;
                                    cx.notify();
                                })
                            })
                            .separator()
                            .item("Sign Out", move |_, cx| {
                                sign_out.update(cx, |store, cx| {
                                    store.sign_out(cx);
                                    cx.notify();
                                })
                            })
                            .show(anchor.below(), window, cx);
                    }),
            )
    }

    /// A server and how the client reaches it.
    fn server_line(&self, server: &Server, cx: &App) -> impl IntoElement {
        let c = colors(cx);
        let detail = server_detail(server);
        let color = match server.state {
            State::Connected => c.success,
            State::Connecting => c.warning,
            State::Disconnected | State::Refused => c.danger,
        };
        div()
            .id(SharedString::from(format!("server-{}", server.id)))
            .h(px(20.))
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().size(px(7.)).rounded_full().flex_shrink_0().bg(color))
            .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).truncate().child(server.name.clone()))
            .child(div().flex_1().min_w(px(4.)))
            .child(server_update_status(&self.store, server, cx, move |cx| {
                div().text_size(px(11.)).text_color(colors(cx).tertiary).child(detail.clone()).into_any_element()
            }))
            .tooltip(tooltip(server.error.clone().unwrap_or(server_detail(server))))
    }

    /// A new version of the client, where the sidebar ends: the offer, the download as it goes,
    /// and the restart that finishes it.
    fn app_update_row(&self, cx: &App) -> Option<AnyElement> {
        let c = colors(cx);
        let updater = &self.store.read(cx).updater;
        let line = |text: String, symbol: &'static str, trailing: Option<AnyElement>| {
            div()
                .min_h(px(ControlSize::Small.height()))
                .flex()
                .items_center()
                .gap(px(7.))
                .child(icons::symbol(symbol, 13.).text_color(c.secondary))
                .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).truncate().child(text))
                .child(div().flex_1().min_w(px(4.)))
                .children(trailing)
        };
        let spinner = || {
            Some(
                div()
                    .text_color(c.secondary)
                    .child(Spinner::new(ControlSize::Small.symbol()).render(cx))
                    .into_any_element(),
            )
        };
        let button = |id: &'static str, title: &'static str, run: fn(&mut Store, &mut Context<Store>)| {
            let store = self.store.clone();
            Some(
                ActionButton::new(id, title)
                    .small()
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            run(store, cx);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            )
        };
        let row = match &updater.state {
            UpdateState::Idle => return None,
            UpdateState::Checking => line("Checking for updates".into(), "refresh-cw", spinner()),
            UpdateState::UpToDate => {
                line(format!("Motile {} is the newest version", updater.current), "circle-check", None)
            }
            UpdateState::Available(version) => line(
                format!("Motile {version} is available"),
                "circle-arrow-down",
                button("app-update", "Download", |store, cx| store.updater.download(cx)),
            ),
            UpdateState::Failed(message) => div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(line(
                    "The update didn’t work".into(),
                    "triangle-alert",
                    button("app-retry", "Try Again", |store, cx| store.updater.check(true, cx)),
                ))
                .child(div().text_size(px(11.)).text_color(c.secondary).child(message.clone())),
        };
        Some(row.into_any_element())
    }

    fn asking_alert(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let asking = self.asking.as_ref()?;
        let this = cx.entity().downgrade();
        let cancel = {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |this, cx| {
                    this.asking = None;
                    cx.notify();
                });
            }
        };
        Some(match asking {
            Asking::Rename { title, .. } => {
                let rename = this.clone();
                alert(
                    "rename",
                    "Rename thread",
                    None,
                    Some(InputField::new(title).into_any_element()),
                    vec![
                        AlertButton::new("Cancel", cancel),
                        AlertButton::new("Rename", move |_, cx| {
                            let _ = rename.update(cx, |this, cx| this.finish_rename(cx));
                        })
                        .prominent(),
                    ],
                    cx,
                )
                .into_any_element()
            }
            Asking::Delete { thread_id, title, in_worktree } => {
                let (store, thread_id, delete) = (self.store.clone(), thread_id.clone(), this.clone());
                let message = if *in_worktree {
                    "Its worktree and the uncommitted changes there are deleted too. Its branch stays."
                } else {
                    "The files the agent changed stay as they are."
                };
                alert(
                    "delete",
                    format!("Delete “{title}”?"),
                    Some(message.into()),
                    None,
                    vec![
                        AlertButton::new("Cancel", cancel.clone()),
                        AlertButton::new("Delete", move |_, cx| {
                            store.update(cx, |store, cx| {
                                store.delete(&thread_id);
                                cx.notify();
                            });
                            let _ = delete.update(cx, |this, cx| {
                                this.asking = None;
                                cx.notify();
                            });
                        })
                        .destructive(),
                    ],
                    cx,
                )
                .into_any_element()
            }
        })
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let query = self.query(cx);
        let store = self.store.read(cx);
        let active: Vec<ThreadInfo> =
            store.active_threads().into_iter().filter(|thread| self.matches(store, thread, &query)).cloned().collect();
        let done: Vec<ThreadInfo> =
            store.done_threads().into_iter().filter(|thread| self.matches(store, thread, &query)).cloned().collect();
        let drafts: Vec<ListedDraft> = store
            .listed_drafts()
            .into_iter()
            .filter(|listed| {
                if query.is_empty() {
                    return true;
                }
                let query = query.to_lowercase();
                let project = store
                    .project(listed.draft.project_id.as_deref())
                    .map(|project| project.name.to_lowercase())
                    .unwrap_or_default();
                listed.preview.to_lowercase().contains(&query) || project.contains(&query)
            })
            .collect();
        let undo = store.undo.clone();
        let expanded = !query.is_empty() || store.prefs.bool("sidebar.doneExpanded");
        let empty_text = if !query.is_empty() {
            "No threads found"
        } else if done.is_empty() {
            "No threads yet"
        } else {
            "No active threads"
        };
        let max_done_height = (f32::from(window.viewport_size().height) - ABOVE_LIST) * 0.6;
        let done_refs: Vec<&ThreadInfo> = done.iter().collect();
        let pulling = self.pulling.is_some();
        let margin = row_margin();

        div()
            .id("sidebar")
            .size_full()
            .flex()
            .flex_col()
            .pt(px(theme::TOP_BAR))
            .when(pulling, |sidebar| {
                sidebar
                    .cursor(CursorStyle::ResizeUpDown)
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        let Some((start, _)) = this.pulling else { return };
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.pulling = None;
                        } else {
                            this.pulling = Some((start, f32::from(event.position.y)));
                        }
                        cx.notify();
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.finish_pulling(window, cx)))
            })
            .child(self.search_field())
            .child(
                div().id("threads").flex_1().min_h_0().overflow_y_scroll().child(
                    div()
                        .py(px(4. - ROW_GAP / 2.))
                        .when(!drafts.is_empty(), |list| {
                            list.children(drafts.iter().map(|listed| self.draft_row(listed, cx))).child(
                                div().mx(px(margin.left + SIDE_PADDING)).my(px(4. + ROW_GAP / 2.)).child(divider(cx)),
                            )
                        })
                        .children(active.iter().map(|thread| self.thread_row(thread, cx)))
                        .when(active.is_empty(), |list| {
                            list.child(
                                div()
                                    .px(px(margin.left + SIDE_PADDING))
                                    .py(px(6. + ROW_GAP / 2.))
                                    .text_size(px(12.))
                                    .text_color(c.tertiary)
                                    .child(empty_text),
                            )
                        }),
                ),
            )
            .when_some(undo, |sidebar, undo| sidebar.child(self.undo_row(&undo, cx)))
            .when(!done_refs.is_empty(), |sidebar| {
                sidebar.child(self.done_shelf(&done_refs, max_done_height, expanded, cx))
            })
            .child(self.footer(cx))
            .children(self.asking_alert(cx))
    }
}

/// What a thread is up to, or how long ago it last was.
fn thread_status(thread: &ThreadInfo, cx: &App) -> AnyElement {
    let c = colors(cx);
    let label = |text: String, color: Hsla, icon: AnyElement| {
        div()
            .flex()
            .items_center()
            .gap(px(3.))
            .text_color(color)
            .child(icon)
            .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).child(text))
            .into_any_element()
    };
    let symbol = |name: &'static str| icons::symbol(name, 11.).into_any_element();
    if thread.needs_approval {
        return label("Approval".into(), c.warning, symbol("circle-question-mark"));
    }
    if thread.running {
        return div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(thread.agents > 0, |status| {
                let help = if thread.agents == 1 {
                    "1 agent is working".to_string()
                } else {
                    format!("{} agents are working", thread.agents)
                };
                status.child(
                    div().id(SharedString::from(format!("agents-{}", thread.id))).tooltip(tooltip(help)).child(label(
                        thread.agents.to_string(),
                        c.working,
                        symbol("users"),
                    )),
                )
            })
            .child(label(elapsed(thread.updated_at), c.working, symbol("circle-dashed")))
            .into_any_element();
    }
    if let Some(stage) = thread.git_stage {
        return label(stage_label(stage).into(), c.working, Spinner::new(11.).render(cx).into_any_element());
    }
    if thread.monitoring {
        let since = thread.turn_ended_at.unwrap_or(thread.updated_at);
        return label(elapsed(since), c.text, symbol("eye"));
    }
    if thread.unread {
        return label("Unread".into(), c.unread, div().size(px(6.)).rounded_full().bg(c.unread).into_any_element());
    }
    div().text_size(px(11.)).text_color(c.tertiary).child(ago(thread.updated_at)).into_any_element()
}

/// What stands at the end of a server's line: the offer to update it, the update as it goes, or
/// `otherwise`. Settings draws its servers with it too.
pub fn server_update_status(
    store: &Entity<Store>,
    server: &Server,
    cx: &App,
    otherwise: impl Fn(&App) -> AnyElement,
) -> AnyElement {
    let c = colors(cx);
    let state = store.read(cx);
    if let Some(update) = state.server_updates.get(&server.id) {
        let progress = if update.restarting {
            "Restarting".to_string()
        } else if let Some(fraction) = update.fraction {
            format!("Updating {}%", (fraction * 100.) as u32)
        } else {
            "Updating".to_string()
        };
        return div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_color(c.secondary)
            .child(div().text_size(px(11.)).child(progress))
            .child(Spinner::new(ControlSize::Small.symbol()).render(cx))
            .into_any_element();
    }
    if !state.is_outdated(server) {
        return otherwise(cx);
    }
    let help = format!(
        "Install version {} on {}. It restarts, and no agent may be working.",
        state.updater.latest.clone().unwrap_or_default(),
        server.name
    );
    let (handle, server) = (store.clone(), server.clone());
    ActionButton::new(SharedString::from(format!("update-{}", server.id)), "Update")
        .small()
        .help(help)
        .on_click(move |_, _, cx| {
            handle.update(cx, |store, cx| {
                store.update_server(&server);
                cx.notify();
            })
        })
        .into_any_element()
}

fn server_detail(server: &Server) -> String {
    match server.state {
        State::Connected => {
            let path = server.path.clone().unwrap_or_else(|| "connected".into());
            match server.rtt_ms {
                Some(rtt) => format!("{path} · {rtt} ms"),
                None => path,
            }
        }
        State::Connecting => "connecting…".into(),
        State::Disconnected => "offline".into(),
        State::Refused => "refused".into(),
    }
}
