//! The drafts, then every active thread on every server in one list, with the ones marked done on
//! a shelf at the bottom, and the servers and the account under them.

use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::main_view::ROW_INSET;
use crate::models::{ThreadInfo, ago, elapsed};
use crate::store::{ListedDraft, Selection, Store, UndoNotice};
use crate::theme::{self, colors};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{IconButton, edges, highlight, icons, logos, pill_button, spinner};

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
const ROW_GAP: f32 = 2.;
const DONE_ROW_HEIGHT: f32 = 30.;
const DONE_DEFAULT_HEIGHT: f32 = 250.;

fn row_margin() -> Edges<f32> {
    edges(ROW_GAP / 2., ROW_INSET, ROW_GAP / 2., ROW_INSET)
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
    _ticks: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
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
                    this.read_with(cx, |this, cx| this.store.read(cx).threads.values().any(|thread| thread.running))
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
            _ticks: ticks,
            _subscriptions: subscriptions,
        }
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
        let subscription_input = input.clone();
        cx.subscribe_in(&subscription_input, window, |this, _, event: &InputEvent, _, cx| {
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
        let menu = Menu::new();
        let id = thread_id.clone();
        let menu = if done {
            menu.item("Mark Undone", move |_, cx| {
                handle.update(cx, |store, cx| {
                    store.set_done(std::slice::from_ref(&id), false, false, cx);
                    cx.notify();
                })
            })
        } else {
            menu.item_if(!busy, "Mark Done", move |_, cx| {
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
        .item("Delete…", move |_, cx| {
            let _ = delete.update(cx, |this, cx| this.begin_delete(delete_id.clone(), cx));
        })
    }

    fn search_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let has_text = !self.query(cx).is_empty();
        let search = self.search.clone();
        div()
            .mx(px(10.))
            .mt(px(2.))
            .mb(px(6.))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(c.hover)
            .child(icons::symbol("magnifyingglass", 12.).text_color(c.tertiary))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.search).appearance(false).px_0().py_0().h(px(28.)).text_size(px(13.))),
            )
            .when(has_text, |field| {
                field.child(
                    div().mr(px((28. - 18.) / 2. - 8.)).child(
                        IconButton::new("clear-search", "xmark.circle.fill")
                            .help("Clear")
                            .size(18.)
                            .symbol_size(12.)
                            .color(c.tertiary)
                            .on_click(move |_, window, cx| {
                                search.update(cx, |search, cx| search.set_value("", window, cx));
                            }),
                    ),
                )
            })
    }

    /// An active thread: its project and what it is doing on the first line, its title on the
    /// second, its project's branch, its server and its agent on the third.
    fn thread_row(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(Some(&thread.project_id)).map(|project| project.seen(thread));
        let project_name = project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| crate::models::last_component(&thread.cwd));
        let selected = store.selection == Selection::Thread(thread.id.clone());
        let server = (store.servers.len() > 1).then(|| store.server(Some(&thread.server_id)).cloned()).flatten();
        let group: SharedString = format!("thread-{}", thread.id).into();
        let id = thread.id.clone();
        let (select, menu_id, done_id) = (self.store.clone(), thread.id.clone(), thread.id.clone());
        let mark_done = self.store.clone();
        let margin = row_margin();
        let busy = thread.busy();
        let key = format!("thread-{id}");
        let hovering = self.hovered.as_ref() == Some(&key);
        div()
            .id(SharedString::from(format!("thread-row-{id}")))
            .on_hover(self.track_hover(key, cx))
            .group(group.clone())
            .relative()
            .w_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(highlight(group.clone(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .px(px(8.))
                    .pt(px(3.))
                    .pb(px(7.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.label_secondary)
                            .child(logos::project_icon(project.as_ref(), 14., cx))
                            .child(
                                div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).truncate().child(project_name),
                            )
                            .child(div().flex_1().min_w(px(6.)))
                            .when(!(hovering && !busy), |line| line.child(thread_status(thread, cx)))
                            .when(hovering && !busy, |line| {
                                line.child(div().mr(px(3. - 8.)).child(mark_done_button(
                                    SharedString::from(format!("mark-done-{done_id}")),
                                    cx,
                                    move |_, _, cx| {
                                        mark_done.update(cx, |store, cx| {
                                            store.set_done(std::slice::from_ref(&done_id), true, true, cx);
                                            cx.notify();
                                        })
                                    },
                                )))
                            }),
                    )
                    .child(
                        div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).truncate().child(thread.title.clone()),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.label_tertiary)
                            .when_some(project.as_ref().and_then(|project| project.branch.clone()), |line, branch| {
                                line.child(div().text_size(px(11.)).truncate().child(branch))
                            })
                            .child(div().flex_1().min_w(px(6.)))
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
        let server = (store.servers.len() > 1)
            .then(|| project.as_ref().and_then(|project| store.server(Some(&project.server_id)).cloned()))
            .flatten();
        let group: SharedString = format!("draft-{}", listed.draft.id).into();
        let (select, discard, menu_discard) = (self.store.clone(), self.store.clone(), self.store.clone());
        let (id, draft, menu_draft) = (listed.draft.id.clone(), listed.draft.clone(), listed.draft.clone());
        let margin = row_margin();
        let key = format!("draft-{id}");
        let hovering = self.hovered.as_ref() == Some(&key);
        div()
            .id(SharedString::from(format!("draft-row-{id}")))
            .on_hover(self.track_hover(key, cx))
            .group(group.clone())
            .relative()
            .w_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(highlight(group.clone(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .px(px(8.))
                    .pt(px(3.))
                    .pb(px(7.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(c.label_secondary)
                            .child(logos::project_icon(project.as_ref(), 14., cx))
                            .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).truncate().child(
                                project.as_ref().map(|project| project.name.clone()).unwrap_or("No project".into()),
                            ))
                            .when_some(server, |line, server| line.child(logos::server_label(&server, 11., cx)))
                            .child(div().flex_1().min_w(px(6.)))
                            .when(!hovering, |line| {
                                line.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.))
                                        .text_color(c.secondary)
                                        .child(icons::symbol("square.and.pencil", 11.))
                                        .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).child("Draft")),
                                )
                            })
                            .when(hovering, |line| {
                                line.child(
                                    div().mr(px(3. - 8.)).child(
                                        IconButton::new(SharedString::from(format!("discard-{id}")), "xmark")
                                            .help("Discard draft")
                                            .size(22.)
                                            .symbol_size(12.)
                                            .color(c.secondary)
                                            .on_click(move |_, _, cx| {
                                                discard.update(cx, |store, cx| {
                                                    store.discard(&draft, cx);
                                                    cx.notify();
                                                })
                                            }),
                                    ),
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
                    .item("Discard Draft", move |_, cx| {
                        discard.update(cx, |store, cx| {
                            store.discard(&draft, cx);
                            cx.notify();
                        })
                    })
                    .show(event.position, window, cx);
            })
            .into_any_element()
    }

    /// The offer to undo marking threads done: a line above the done threads.
    fn undo_row(&self, notice: &UndoNotice, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.clone();
        div().flex().flex_col().child(crate::ui::divider(cx)).child(
            div()
                .id("undo")
                .h(px(DONE_ROW_HEIGHT + 8.))
                .px(px(18.))
                .flex()
                .items_center()
                .gap(px(7.))
                .text_color(c.label_secondary)
                .hover(|row| row.bg(c.hover))
                .child(div().w(px(14.)).flex().justify_center().child(icons::symbol("arrow.uturn.backward", 10.)))
                .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).child("Undo"))
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(icons::symbol("checkmark", 9.))
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
        &self,
        threads: &[&ThreadInfo],
        max_height: f32,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = colors(cx);
        let content_height = threads.len() as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
        let tallest = max_height.min(content_height);
        let min_height = 4. * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
        let heights = (min_height.min(tallest), tallest);
        let resting = self.store.read(cx).prefs.f32_or("sidebar.doneHeight", DONE_DEFAULT_HEIGHT);
        let pulled = self.pulling.map_or(0., |(start, now)| start - now);
        let list_height = (resting.clamp(heights.0, heights.1) + pulled).clamp(heights.0, heights.1);
        let toggle = self.store.clone();
        let searching = !self.query(cx).is_empty();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(if expanded && heights.0 < heights.1 {
                div()
                    .relative()
                    .child(crate::ui::divider(cx))
                    .child(
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
                    .into_any_element()
            } else {
                crate::ui::divider(cx).into_any_element()
            })
            .child(
                div()
                    .id("done-shelf")
                    .h(px(DONE_ROW_HEIGHT))
                    .mt(px(4.))
                    .mb(px(if expanded { 4. - ROW_GAP / 2. } else { 4. }))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_color(c.label_secondary)
                    .hover(|row| row.bg(c.hover))
                    .child(
                        div()
                            .w(px(14.))
                            .flex()
                            .justify_center()
                            .child(icons::symbol("chevron.right", 10.).rotate(if expanded { 90. } else { 0. })),
                    )
                    .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).child("Done"))
                    .child(div().flex_1())
                    .child(div().text_size(px(11.)).text_color(c.label_tertiary).child(threads.len().to_string()))
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
                shelf.child(div().id("done-list").h(px(list_height + ROW_GAP / 2.)).overflow_y_scroll().child(
                    div().pb(px(4. - ROW_GAP / 2.)).children(threads.iter().map(|thread| self.done_row(thread, cx))),
                ))
            })
    }

    /// A thread that is done: one quiet line.
    fn done_row(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.selection == Selection::Thread(thread.id.clone());
        let project = store.project(Some(&thread.project_id)).cloned();
        let group: SharedString = format!("done-{}", thread.id).into();
        let (select, undone, menu_id) = (self.store.clone(), self.store.clone(), thread.id.clone());
        let (id, undone_id) = (thread.id.clone(), thread.id.clone());
        let margin = row_margin();
        let key = format!("done-{id}");
        let hovering = self.hovered.as_ref() == Some(&key);
        div()
            .id(SharedString::from(format!("done-row-{id}")))
            .on_hover(self.track_hover(key, cx))
            .group(group.clone())
            .relative()
            .h(px(DONE_ROW_HEIGHT + ROW_GAP))
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(highlight(group.clone(), 8., margin, selected, cx))
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(logos::project_icon(project.as_ref(), 14., cx))
                    .child(
                        div().text_size(px(13.)).text_color(c.label_secondary).truncate().child(thread.title.clone()),
                    )
                    .child(div().flex_1().min_w(px(6.)))
                    .when(!hovering, |line| {
                        line.child(
                            div()
                                .text_size(px(11.))
                                .text_color(c.label_tertiary)
                                .child(ago(thread.done_at.unwrap_or(thread.updated_at))),
                        )
                    })
                    .when(hovering, |line| {
                        line.child(
                            div().mr(px((DONE_ROW_HEIGHT - 22.) / 2. - 8.)).child(
                                IconButton::new(SharedString::from(format!("undone-{id}")), "arrow.uturn.backward")
                                    .help("Mark undone")
                                    .size(22.)
                                    .symbol_size(12.)
                                    .color(c.label_secondary)
                                    .on_click(move |_, _, cx| {
                                        undone.update(cx, |store, cx| {
                                            store.set_done(std::slice::from_ref(&undone_id), false, false, cx);
                                            cx.notify();
                                        })
                                    }),
                            ),
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

    /// The servers and how the app reaches them, and the account.
    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let email = store.account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let anchor = self.account_menu.clone();
        let menu_store = self.store.clone();
        div()
            .px(px(18.))
            .pt(px(10.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(store.servers.iter().map(|server| {
                let detail = server_detail(server);
                div()
                    .id(SharedString::from(format!("server-{}", server.id)))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(div().size(px(7.)).rounded_full().bg(server_color(server, cx)))
                    .child(
                        div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).truncate().child(server.name.clone()),
                    )
                    .child(div().flex_1().min_w(px(4.)))
                    .child(server_update_status(&self.store, server, cx, |cx| {
                        div()
                            .text_size(px(11.))
                            .text_color(colors(cx).label_tertiary)
                            .child(detail.clone())
                            .into_any_element()
                    }))
                    .tooltip(crate::ui::tooltip(server.error.clone().unwrap_or(server_detail(server))))
            }))
            .child(
                div()
                    .id("account")
                    .group("account")
                    .relative()
                    .mx(px(-8. - 10.))
                    .mt(px(-4.))
                    .mb(px(-8.))
                    .pt(px(4.))
                    .px(px(10.))
                    .pb(px(8.))
                    .child(highlight("account", 8., edges(4., 10., 8., 10.), false, cx))
                    .child(
                        div()
                            .relative()
                            .h(px(30.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .text_color(c.label_secondary)
                            .child(anchor.track())
                            .child(icons::symbol("person.crop.circle", 14.))
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
                let field = crate::ui::text_field(title, cx);
                alert(
                    "rename",
                    "Rename thread",
                    None,
                    Some(field.into_any_element()),
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
                    "The thread and its transcript are removed from its server, and so is its worktree with what isn't committed there. Its branch stays."
                } else {
                    "The thread and its transcript are removed from its server. Files the agent changed stay as they are."
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
        let max_done_height = (f32::from(window.viewport_size().height) - theme::TOP_BAR - 36.) * 0.6;
        let done_refs: Vec<&ThreadInfo> = done.iter().collect();
        let pulling = self.pulling.is_some();

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
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            let Some((start, now)) = this.pulling.take() else { return };
                            let store = this.store.read(cx);
                            let done = store.done_threads().len();
                            let content = done as f32 * (DONE_ROW_HEIGHT + ROW_GAP) + 4.;
                            let tallest =
                                ((f32::from(window.viewport_size().height) - theme::TOP_BAR - 36.) * 0.6).min(content);
                            let lowest = (4. * (DONE_ROW_HEIGHT + ROW_GAP) + 4.).min(tallest);
                            let resting =
                                store.prefs.f32_or("sidebar.doneHeight", DONE_DEFAULT_HEIGHT).clamp(lowest, tallest);
                            let height = (resting + start - now).clamp(lowest, tallest);
                            this.store.update(cx, |store, cx| {
                                store.prefs.set("sidebar.doneHeight", height);
                                cx.notify();
                            });
                            cx.notify();
                        }),
                    )
            })
            .child(self.search_field(cx))
            .child(
                div().id("threads").flex_1().min_h_0().overflow_y_scroll().child(
                    div()
                        .py(px(4. - ROW_GAP / 2.))
                        .when(!drafts.is_empty(), |list| {
                            list.children(drafts.iter().map(|listed| self.draft_row(listed, cx))).child(
                                div()
                                    .mx(px(row_margin().left + 8.))
                                    .my(px(4. + ROW_GAP / 2.))
                                    .child(crate::ui::divider(cx)),
                            )
                        })
                        .children(active.iter().map(|thread| self.thread_row(thread, cx)))
                        .when(active.is_empty(), |list| {
                            list.child(
                                div()
                                    .px(px(row_margin().left + 8.))
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

fn mark_done_button(
    id: impl Into<ElementId>,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let c = colors(cx);
    let id: ElementId = id.into();
    let group: SharedString = format!("mark-done-{id:?}").into();
    div()
        .id(id)
        .group(group.clone())
        .relative()
        .h(px(22.))
        .px(px(5.))
        .flex()
        .items_center()
        .gap(px(3.))
        .text_color(c.label_secondary)
        .hover(|button| button.text_color(c.text))
        .child(highlight(group, 6., edges(0., 0., 0., 0.), false, cx))
        .child(div().relative().flex().items_center().child(icons::symbol("checkmark", 11.)))
        .child(div().relative().text_size(px(11.)).font_weight(FontWeight::MEDIUM).child("Mark Done"))
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx)
        })
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
    let symbol = |name: &str| icons::symbol(name, 11.).into_any_element();
    if thread.needs_approval {
        return label("Approval".into(), c.warning, symbol("questionmark.circle"));
    }
    if thread.running {
        return div()
            .flex()
            .items_center()
            .gap(px(6.))
            .when(thread.agents > 0, |status| {
                let help = if thread.agents == 1 {
                    "1 agent is working".to_string()
                } else {
                    format!("{} agents are working", thread.agents)
                };
                status.child(
                    div()
                        .id(SharedString::from(format!("agents-{}", thread.id)))
                        .tooltip(crate::ui::tooltip(help))
                        .child(label(thread.agents.to_string(), c.working, symbol("person.2"))),
                )
            })
            .child(label(elapsed(thread.updated_at), c.working, symbol("circle.dashed")))
            .into_any_element();
    }
    if thread.monitoring {
        return label("Monitoring".into(), c.text, symbol("eye"));
    }
    if thread.unread {
        return label("Unread".into(), c.unread, div().size(px(6.)).rounded_full().bg(c.unread).into_any_element());
    }
    div().text_size(px(11.)).text_color(c.label_tertiary).child(ago(thread.updated_at)).into_any_element()
}

fn server_color(server: &crate::models::Server, cx: &App) -> Hsla {
    use motile_core::link::State;
    let c = colors(cx);
    match server.state {
        State::Connected => c.success,
        State::Connecting => c.warning,
        State::Disconnected | State::Refused => c.danger,
    }
}

fn server_detail(server: &crate::models::Server) -> String {
    use motile_core::link::State;
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

/// What stands at the end of a server's line: the offer to update it, the update as it goes, or
/// `otherwise`.
pub fn server_update_status(
    store: &Entity<Store>,
    server: &crate::models::Server,
    cx: &App,
    otherwise: impl Fn(&App) -> AnyElement,
) -> AnyElement {
    let c = colors(cx);
    let state = store.read(cx);
    if let Some(update) = state.server_updates.get(&server.id) {
        let progress = if update.restarting {
            "Restarting…".to_string()
        } else if let Some(fraction) = update.fraction {
            format!("Updating {}%", (fraction * 100.) as u32)
        } else {
            "Updating…".to_string()
        };
        return div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().text_size(px(11.)).text_color(c.label_secondary).child(progress))
            .child(spinner(12., cx))
            .into_any_element();
    }
    if state.is_outdated(server) {
        let help = format!(
            "Install version {} on {}. It restarts, and no agent may be working.",
            state.updater.latest.clone().unwrap_or_default(),
            server.name
        );
        let (handle, server) = (store.clone(), server.clone());
        return div()
            .id(SharedString::from(format!("update-{}", server.id)))
            .tooltip(crate::ui::tooltip(help))
            .child(pill_button(
                SharedString::from(format!("update-button-{}", server.id)),
                "Update",
                cx,
                move |_, _, cx| {
                    handle.update(cx, |store, cx| {
                        store.update_server(&server);
                        cx.notify();
                    })
                },
            ))
            .into_any_element();
    }
    otherwise(cx)
}
