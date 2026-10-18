//! The drafts, then every active thread on every server in one list, with the ones marked done
//! and the servers on shelves at the bottom, and the way to the settings and the usage under
//! them: the Mac app's `SidebarView.swift`. While it searches, it is one flat list of what was
//! found.

mod card;
mod drag;
mod footer;
mod list;
mod rows;
mod search;
mod shelves;
mod updates;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub use self::footer::SidebarFooter;
use self::list::{Layout, RecycledList, place};
use self::rows::{DRAFTS_DIVIDER, EMPTY_LINE_HEIGHT, THREAD_ROW_HEIGHT};
use self::search::RESULT_ROW_HEIGHT;
pub use self::updates::{AppUpdateRow, ServerUpdateStatus};
use crate::models::ThreadInfo;
use crate::store::{ListedDraft, SearchResult, Store, UndoNotice};
use crate::theme::{self, ControlSize, Layer, colors};
use crate::ui::menu::Menu;
use crate::ui::modal::{Answer, Confirmation, confirmation, modal, presented};
use crate::ui::{ActionButton, InputField, InputVariant, divider, edges};

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
const ROW_GAP: f32 = 2.;
/// How far the rows' highlights stay from the sidebar's edges.
pub const ROW_INSET: f32 = 10.;
/// How far everything the sidebar shows stays from its sides, a row's content as well as a line's.
const SIDEBAR_EDGE: f32 = ROW_INSET + 8.;
const DONE_ROW_HEIGHT: f32 = 30.;
/// How far a row's lines stand from its highlight's sides, top and bottom.
const SIDE_PADDING: f32 = 8.;
const TOP_PADDING: f32 = 5.;
const BOTTOM_PADDING: f32 = 7.;
const FIRST_LINE_HEIGHT: f32 = 22.;
const TITLE_HEIGHT: f32 = 16.;
/// The search field's line above the list.
const HEADER_HEIGHT: f32 = 2. + 28. + 6.;

fn row_margin() -> Edges<f32> {
    edges(ROW_GAP / 2., ROW_INSET, ROW_GAP / 2., ROW_INSET)
}

/// A button on a row's first line is as far from the row's side as from its top.
fn button_inset() -> f32 {
    TOP_PADDING + (FIRST_LINE_HEIGHT - ControlSize::Small.height()) / 2.
}

/// The light a row's buttons lie on while the pointer is over them: the row's.
fn row_layer(selected: bool) -> Option<Layer> {
    Some(if selected { Layer::RowSelected } else { Layer::Row })
}

/// Keeps a press on a control in a row from picking the row up.
fn stop_press(control: impl IntoElement) -> AnyElement {
    div()
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(control)
        .into_any_element()
}

/// The shelves at the bottom of the sidebar.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Shelf {
    Done,
    Servers,
}

/// A row of the list: the drafts as one, an active thread, what the search found, the line that
/// says there is nothing, and the offer to start a thread. Threads and results are by their
/// place in the store's list and in the search's.
#[derive(Clone, Copy)]
enum Item {
    Drafts,
    Active(usize),
    Result(usize),
    Empty,
    NewThread,
}

/// What the lists are made of, worked out again only once the store changed, so that a frame that
/// only redraws, as the times in the rows go on or the pointer moves, builds just the rows on
/// screen.
struct Plan {
    searching: bool,
    results: Vec<SearchResult>,
    drafts: Vec<ListedDraft>,
    items: Vec<Item>,
    /// Where the rows of the list above the shelves are, which of them can move, and the thread
    /// of each, for a row being moved.
    layout: Layout,
    movable: Vec<bool>,
    row_threads: Vec<Option<String>>,
    /// Where each thread the jump keys go through is among them, while their keys show.
    jumps: HashMap<String, usize>,
    done_layout: Layout,
    /// Some thread works, so the times in the rows go on every second.
    running: bool,
}

/// A thread being renamed or deleted, with what its alert needs.
#[derive(Clone)]
enum Asking {
    Rename { thread_id: String, title: Entity<InputState> },
    Delete { thread_id: String, title: String, in_worktree: bool },
}

pub struct Sidebar {
    store: Entity<Store>,
    search: Entity<InputState>,
    footer: Entity<SidebarFooter>,
    asking: Option<Asking>,
    /// The row under the pointer, which shows its buttons in place of its status.
    hovered: Option<String>,
    /// Where each row was drawn last, by its key, for the card beside it.
    row_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    /// The search's result the arrow keys are on, and the results it was on.
    highlighted: usize,
    result_ids: Vec<String>,
    list: RecycledList,
    done_list: RecycledList,
    servers_list: RecycledList,
    /// What the lists are made of, until the store changes.
    plan: Option<Rc<Plan>>,
    stale: bool,
    press: Option<drag::Press>,
    lift: Option<drag::Lift>,
    /// The shelf whose line is being pulled, from where to where.
    pulling: Option<(Shelf, f32, f32)>,
    /// How many rows each shelf has and the most room it may take, as last drawn.
    shelf_room: HashMap<Shelf, (usize, f32)>,
    peek: card::Peek,
    /// The offer to undo as last seen, and the one going away.
    undo_seen: Option<UndoNotice>,
    undo_leaving: Option<UndoNotice>,
    undo_serial: usize,
    revealed_seen: Option<String>,
    _ticks: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let footer = cx.new(|cx| SidebarFooter::new(store.clone(), cx));
        let keys = {
            let this = cx.entity().downgrade();
            let field = search.clone();
            cx.intercept_keystrokes(move |event, window, cx| {
                let keystroke = &event.keystroke;
                let step = match keystroke.key.as_str() {
                    "up" => -1,
                    "down" => 1,
                    _ => return,
                };
                if keystroke.modifiers.modified() || !field.read(cx).focus_handle(cx).is_focused(window) {
                    return;
                }
                let steered = this.update(cx, |this, cx| this.steer(step, cx)).unwrap_or(false);
                if steered {
                    cx.stop_propagation();
                }
            })
        };
        let subscriptions = vec![
            cx.observe_in(&store, window, |this, store, window, cx| {
                this.stale = true;
                this.follow_store(store, window, cx);
                cx.notify();
            }),
            cx.subscribe_in(&search, window, |this, search, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = search.read(cx).value().to_string();
                    this.store.update(cx, |store, cx| {
                        store.set_thread_search(value);
                        cx.notify();
                    });
                }
                InputEvent::PressEnter { .. } => this.open_highlighted(window, cx),
                _ => {}
            }),
            keys,
        ];
        // The times in the rows go on: every second while a thread works, else every half minute.
        let ticks = cx.spawn(async move |this, cx| {
            let mut waited = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                waited += 1;
                let Ok(running) = this.read_with(cx, |this, _| this.plan.as_ref().is_some_and(|plan| plan.running))
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
            footer,
            asking: None,
            hovered: None,
            row_bounds: Rc::default(),
            highlighted: 0,
            result_ids: Vec::new(),
            list: RecycledList::new(),
            done_list: RecycledList::new(),
            servers_list: RecycledList::new(),
            plan: None,
            stale: true,
            press: None,
            lift: None,
            pulling: None,
            shelf_room: HashMap::new(),
            peek: card::Peek::default(),
            undo_seen: None,
            undo_leaving: None,
            undo_serial: 0,
            revealed_seen: None,
            _ticks: ticks,
            _subscriptions: subscriptions,
        }
    }

    /// What follows the store: the search the store ended, Done opening on the thread it
    /// reveals, and the card going when the window does.
    fn follow_store(&mut self, store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) {
        let (search, revealed, active, hidden) = {
            let store = store.read(cx);
            (
                store.sidebar.search.clone(),
                store.sidebar.revealed_thread_id.clone(),
                store.app_active,
                store.prefs.bool("sidebar.hidden"),
            )
        };
        if search.is_empty() && !self.search.read(cx).value().is_empty() {
            self.search.update(cx, |input, cx| input.set_value("", window, cx));
        }
        if revealed != self.revealed_seen {
            self.revealed_seen = revealed.clone();
            let done =
                revealed.as_ref().is_some_and(|id| store.read(cx).done_threads().iter().any(|thread| &thread.id == id));
            if done {
                store.update(cx, |store, _| store.prefs.set(Shelf::Done.expanded_key(), true));
            }
        }
        if !active || hidden {
            self.peek.hide();
        }
    }

    /// Keeps where a row is drawn, for the card that comes up beside it.
    fn track_bounds(&self, key: &str) -> AnyElement {
        let (bounds, key) = (self.row_bounds.clone(), key.to_string());
        canvas(
            move |drawn, _, _| {
                bounds.borrow_mut().insert(key.clone(), drawn);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// Follows the pointer onto a row and off it: the row shows its buttons, and a thread's row
    /// brings up its card.
    fn row_hover(
        &self,
        key: String,
        thread_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hovered = Some(key.clone());
            } else if this.hovered.as_ref() == Some(&key) {
                this.hovered = None;
            }
            match (&thread_id, *hovered) {
                (Some(thread_id), true) => {
                    let bounds = this.row_bounds.borrow().get(&key).copied();
                    this.point(bounds.map(|bounds| (thread_id.clone(), bounds)), cx);
                }
                (Some(thread_id), false) if this.peek.points_at(thread_id) => this.point(None, cx),
                (None, true) => this.point(None, cx),
                _ => {}
            }
            cx.notify();
        })
    }

    fn hovering(&self, key: &str) -> bool {
        self.hovered.as_deref() == Some(key)
    }

    /// Forgets where the pointer was, for rows that are no longer where they were.
    fn forget_pointer(&mut self, cx: &mut Context<Self>) {
        self.hovered = None;
        self.peek.hide();
        cx.notify();
    }

    fn begin_rename(&mut self, thread_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self.store.read(cx).threads.get(&thread_id).map(|thread| thread.title.clone()) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title).placeholder("Title"));
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::PressEnter { .. } => this.finish_rename(cx),
            InputEvent::Change => cx.notify(),
            _ => {}
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
        let Some(Asking::Rename { title, .. }) = &self.asking else { return };
        if title.read(cx).value().trim().is_empty() {
            return;
        }
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

    /// What a thread's menu offers: marking it done or undone and renaming it, then deleting it.
    /// A removed server's thread has none.
    fn thread_menu(&mut self, thread_id: String, cx: &mut Context<Self>) -> Option<Menu> {
        self.peek.hide();
        let store = self.store.read(cx);
        let thread = store.threads.get(&thread_id).filter(|thread| !store.on_removed_server(thread))?;
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
        let menu = menu
            .item("Rename", move |window, cx| {
                let _ = rename.update(cx, |this, cx| this.begin_rename(rename_id.clone(), window, cx));
            })
            .separator()
            .danger_item("Delete", move |_, cx| {
                let _ = delete.update(cx, |this, cx| this.begin_delete(delete_id.clone(), cx));
            });
        Some(menu)
    }

    /// The search, with the buttons that add a project and start a thread beside it.
    fn header(&self, cx: &mut Context<Self>) -> Div {
        let shortcuts = &self.store.read(cx).shortcuts;
        let help = format!(
            "{}. {} starts one in this project",
            shortcuts.help("New thread", "chat.new"),
            shortcuts.shift_click()
        );
        let (add_project, new_thread) = (self.store.clone(), self.store.clone());
        div()
            .px(px(10.))
            .pt(px(2.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(InputField::new(&self.search).icon("search").variant(InputVariant::Filled).clearable(true)),
            )
            .child(
                div()
                    .flex()
                    .child(ActionButton::icon("add-project", "folder-plus", "Add a project").on_click(
                        move |_, _, cx| {
                            add_project.update(cx, |store, cx| {
                                store.add_project();
                                cx.notify();
                            })
                        },
                    ))
                    .child(ActionButton::icon("new-thread", "square-pen", help).on_click(move |event, _, cx| {
                        let shift = event.modifiers().shift;
                        new_thread.update(cx, |store, cx| {
                            if shift {
                                let project = store.composer_project().map(|project| project.id);
                                store.start_new_thread(project, cx);
                            } else {
                                store.new_thread(cx);
                            }
                            cx.notify();
                        })
                    })),
            )
    }

    fn item_height(item: Item, drafts: usize) -> f32 {
        match item {
            Item::Drafts => drafts as f32 * (THREAD_ROW_HEIGHT + ROW_GAP) + DRAFTS_DIVIDER,
            Item::Active(_) => THREAD_ROW_HEIGHT + ROW_GAP,
            Item::Result(_) => RESULT_ROW_HEIGHT + ROW_GAP,
            Item::Empty => EMPTY_LINE_HEIGHT + 2. * (6. + ROW_GAP / 2.),
            Item::NewThread => DONE_ROW_HEIGHT + ROW_GAP,
        }
    }

    /// What the lists are made of now: as last worked out, unless the store changed since.
    fn plan(&mut self, cx: &mut Context<Self>) -> Rc<Plan> {
        if let Some(plan) = self.plan.clone().filter(|_| !self.stale) {
            return plan;
        }
        self.stale = false;
        let plan = Rc::new(self.make_plan(cx));
        let ids = plan.items.iter().map(|item| self.item_id(*item, &plan.results, cx)).collect();
        if plan.searching {
            self.keep_highlight(plan.results.iter().map(SearchResult::id).collect());
        } else {
            self.result_ids.clear();
            self.highlighted = 0;
        }
        if self.list.set_ids(ids) {
            self.cancel_lift();
            self.forget_pointer(cx);
        }
        let done_ids = self.store.read(cx).done_threads().iter().map(|thread| thread.id.clone()).collect();
        if self.done_list.set_ids(done_ids) {
            self.forget_pointer(cx);
        }
        self.plan = Some(plan.clone());
        plan
    }

    fn make_plan(&self, cx: &App) -> Plan {
        let store = self.store.read(cx);
        let searching = store.is_searching_threads();
        let results = if searching { store.search_results() } else { Vec::new() };
        let drafts = if searching { Vec::new() } else { store.listed_drafts() };
        let active = store.active_threads();
        let mut items: Vec<Item> = Vec::new();
        if searching {
            if results.is_empty() {
                items.push(Item::Empty);
            }
            items.extend((0..results.len()).map(Item::Result));
        } else {
            if !drafts.is_empty() {
                items.push(Item::Drafts);
            }
            items.extend((0..active.len()).map(Item::Active));
            if active.is_empty() {
                items.push(Item::Empty);
            }
            if store.offers_new_thread(&drafts) {
                items.push(Item::NewThread);
            }
        }
        let heights = items.iter().map(|item| Self::item_height(*item, drafts.len())).collect();
        let movable = items
            .iter()
            .map(|item| match item {
                Item::Active(index) => active.len() > 1 && store.moves(&active[*index]),
                _ => false,
            })
            .collect();
        let row_threads = items
            .iter()
            .map(|item| match item {
                Item::Active(index) => Some(active[*index].id.clone()),
                _ => None,
            })
            .collect();
        let mut jumps = HashMap::new();
        if store.shows_jump_hints {
            let threads: Vec<&ThreadInfo> = if searching {
                results.iter().filter_map(SearchResult::thread).collect()
            } else {
                active.iter().collect()
            };
            for (at, thread) in threads.into_iter().enumerate() {
                jumps.entry(thread.id.clone()).or_insert(at);
            }
        }
        let done = store.done_threads();
        Plan {
            searching,
            layout: Layout::new(heights, 4. - ROW_GAP / 2., 4. - ROW_GAP / 2.),
            done_layout: Layout::new(vec![DONE_ROW_HEIGHT + ROW_GAP; done.len()], 0., 4. - ROW_GAP / 2.),
            running: active.iter().chain(done).any(ThreadInfo::busy),
            results,
            drafts,
            items,
            movable,
            row_threads,
            jumps,
        }
    }

    fn item_id(&self, item: Item, results: &[SearchResult], cx: &App) -> String {
        match item {
            Item::Drafts => "drafts".into(),
            Item::Active(index) => {
                self.store.read(cx).active_threads().get(index).map(|thread| thread.id.clone()).unwrap_or_default()
            }
            Item::Result(index) => results[index].id(),
            Item::Empty => "empty".into(),
            Item::NewThread => "new-thread".into(),
        }
    }

    /// The list above the shelves: the drafts and the active threads, or what the search found.
    fn threads(&mut self, plan: &Plan, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let layout = &plan.layout;
        let target = if plan.searching {
            plan.results.get(self.highlighted).map(SearchResult::id)
        } else {
            self.store.read(cx).sidebar.revealed_thread_id.clone()
        };
        self.list.follow(target.as_deref(), false, layout, window);
        let jump_hint = |id: &str, store: &Store| plan.jumps.get(id).and_then(|index| store.jump_hint(*index));
        let mut rows = Vec::new();
        for index in self.list.visible(layout, window) {
            let Some(shift) = self.active_shift(index) else { continue };
            let element = match plan.items[index] {
                Item::Drafts => self.draft_rows(&plan.drafts, cx),
                Item::Active(at) => {
                    let Some(thread) = self.store.read(cx).active_threads().get(at).cloned() else { continue };
                    let jump = jump_hint(&thread.id, self.store.read(cx));
                    self.thread_row(&thread, index, plan.movable[index], jump, cx)
                }
                Item::Result(at) => {
                    let result = &plan.results[at];
                    let jump = result.thread().and_then(|thread| jump_hint(&thread.id, self.store.read(cx)));
                    self.result_row(result, at, at == self.highlighted, jump, cx)
                }
                Item::Empty => self.empty_line(cx),
                Item::NewThread => self.new_thread_row(cx),
            };
            rows.push(place(layout, index, shift, element));
        }
        let element = self.list.element("threads", layout, rows, colors(cx).background);
        div()
            .flex_1()
            .min_h_0()
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                this.list.stop_scrolling();
                this.forget_pointer(cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.peek.hide();
                    cx.notify();
                }),
            )
            .child(element)
            .into_any_element()
    }

    /// The offer to undo, as it comes and goes.
    fn undo(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let current = self.store.read(cx).undo.clone();
        if current != self.undo_seen {
            self.undo_serial += 1;
            if self.hovering("undo-row") {
                self.hovered = None;
            }
            if current.is_none() {
                self.undo_leaving = self.undo_seen.take();
                let serial = self.undo_serial;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(shelves::UNDO_FADE).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.undo_serial == serial {
                            this.undo_leaving = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            } else {
                self.undo_leaving = None;
            }
            self.undo_seen = current.clone();
        }
        let serial = self.undo_serial;
        if let Some(notice) = current {
            return Some(self.undo_row(&notice, true, serial, cx));
        }
        let leaving = self.undo_leaving.clone()?;
        Some(self.undo_row(&leaving, false, serial, cx))
    }

    fn asking_alert(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (asking, leaving) = presented("asking", self.asking.clone(), window, cx)?;
        let asking = &asking;
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
                let (rename, submit) = (this.clone(), this.clone());
                let blank = title.read(cx).value().trim().is_empty();
                let content = div()
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Rename Thread"))
                    .child(InputField::new(title))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(ActionButton::new("rename-cancel", "Cancel").on_click({
                                let cancel = cancel.clone();
                                move |_, window, cx| cancel(window, cx)
                            }))
                            .child(ActionButton::new("rename", "Rename").primary().disabled(blank).on_click(
                                move |_, _, cx| {
                                    let _ = rename.update(cx, |this, cx| this.finish_rename(cx));
                                },
                            )),
                    );
                modal("rename", content, cancel)
                    .width(400.)
                    .leaving(leaving)
                    .on_return(move |_, cx| {
                        let _ = submit.update(cx, |this, cx| this.finish_rename(cx));
                    })
                    .into_any_element()
            }
            Asking::Delete { thread_id, title, in_worktree } => {
                let (store, thread_id) = (self.store.clone(), thread_id.clone());
                let message = if *in_worktree {
                    "Its worktree and the uncommitted changes there are deleted too. Its branch stays."
                } else {
                    "The files the agent changed stay as they are."
                };
                let asked = Confirmation::new(format!("Delete “{title}”?")).message(message).answer(
                    Answer::new("Delete", move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.delete(&thread_id);
                            cx.notify();
                        })
                    })
                    .danger(),
                );
                confirmation("delete", asked, cancel, cx).leaving(leaving).into_any_element()
            }
        })
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let room = f32::from(crate::window_frame::content_size(window).height) - theme::TOP_BAR - HEADER_HEIGHT;
        let plan = self.plan(cx);
        let has_servers = self.store.read(cx).has_live_servers();
        let shows_done = !plan.done_layout.tops.is_empty() && !plan.searching;
        let header = self.header(cx);
        let threads = self.threads(&plan, window, cx);
        let undo = self.undo(cx);
        let done = shows_done.then(|| self.done_shelf(&plan.done_layout, room * 0.5, window, cx));
        let servers = has_servers.then(|| self.servers_shelf(room * 0.3, window, cx));
        let pulling = self.pulling.is_some();
        div()
            .id("sidebar")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .pt(px(theme::TOP_BAR))
            .when(pulling, |sidebar| sidebar.cursor(CursorStyle::ResizeUpDown))
            .child(header)
            .child(threads)
            .children(undo)
            .children(done)
            .children(servers)
            .when(shows_done || has_servers, |sidebar| sidebar.child(divider(cx)))
            .child(self.footer.clone())
            .children(self.follow_pointer(cx))
            .children(self.card_layer(cx))
            .children(self.lift_layer(window, cx))
            .children(self.asking_alert(window, cx))
    }
}
