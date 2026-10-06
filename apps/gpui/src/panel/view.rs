//! The panel beside the thread: its tabs in the window's top bar, and under them what the open
//! tab shows of the folder the thread works in.

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentView;
use motile_protocol::wire::{DiffScope, PullRequestEdit, Side, ToolStatus};

use super::code_view::{CodeActions, CodeSnapshot, CodeView, counts_text};
use super::state::{CodeDocument, CodeMarks, FileContent, Loaded, PanelTab, PanelTarget, REMOVED, file_symbol};
use crate::models::{AgentViewExt, duration, elapsed};
use crate::panel::line_comment::{self, LineCommentSheet};
use crate::panel::linear::LinearView;
use crate::panel::pull_request::PullRequestView;
use crate::panel::pull_request_list::PullRequestListView;
use crate::store::Store;
use crate::theme::{self, ControlSize, Radius, Surface, colors};
use crate::transcript::view::TranscriptView;
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, ActionMenu, Spinner, TOOLBAR_WIDTH, Variant, edges, icons};

/// What makes a tab ask its server again: another folder or scope, a turn that ended there, or
/// the button that asks.
#[derive(Clone, PartialEq, Debug)]
struct Trigger {
    target: PanelTarget,
    scope: Option<DiffScope>,
    path: Option<String>,
    version: u64,
    asked: u64,
}

pub struct SidePanelView {
    store: Entity<Store>,
    /// The views of the pull request tabs, by tab, and of the list and the Linear tabs.
    pull_requests: HashMap<PanelTab, Entity<PullRequestView>>,
    pull_request_list: Option<Entity<PullRequestListView>>,
    linear_tabs: HashMap<PanelTab, Entity<LinearView>>,
    line_comment: Option<Entity<LineCommentSheet>>,
    /// How far the tabs start from the panel's left edge: past the window's buttons when the
    /// panel reaches them.
    pub tab_inset: f32,
    diff: Entity<CodeView>,
    /// The views of the tabs that show one file, by tab.
    files: HashMap<PanelTab, Entity<CodeView>>,
    agent: Entity<TranscriptView>,
    asked: u64,
    /// What each tab last asked its server with, by tab.
    triggers: HashMap<String, Trigger>,
    strip: ScrollHandle,
    /// The tab the strip was last scrolled to.
    scrolled_to: Option<String>,
    hovered_tab: Option<String>,
    copied: bool,
    /// A change tab's one file is closed.
    change_closed: bool,
    _ticks: Task<()>,
}

/// Runs something on the store from a click.
fn act(
    store: &Entity<Store>,
    run: impl Fn(&mut Store, &mut Context<Store>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let store = store.clone();
    move |_, _, cx| {
        store.update(cx, |store, cx| {
            run(store, cx);
            cx.notify();
        })
    }
}

/// Runs something on the store from a menu item.
fn choose(
    store: &Entity<Store>,
    run: impl Fn(&mut Store, &mut Context<Store>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let store = store.clone();
    move |_, cx| {
        store.update(cx, |store, cx| {
            run(store, cx);
            cx.notify();
        })
    }
}

/// The light under what the pointer is over and under what is selected, a layer further up the
/// page than the element `group` names.
fn light(group: impl Into<SharedString>, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    let c = colors(cx);
    let lit = Surface::Background.next().color(c);
    let chosen = Surface::Background.further().color(c);
    div()
        .absolute()
        .top(px(inset.top))
        .left(px(inset.left))
        .right(px(inset.right))
        .bottom(px(inset.bottom))
        .rounded(px(radius))
        .when(selected, |light| light.bg(chosen))
        .when(!selected, |light| light.group_hover(group, move |light| light.bg(lit)))
}

impl SidePanelView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let model = store.read(cx).agent_transcript.clone();
        let agent = cx.new(|cx| TranscriptView::new(store.clone(), model, true, cx));
        // The times of the agents that work go on.
        let ticks = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(working) =
                    this.read_with(cx, |this, cx| this.store.read(cx).agents.iter().any(|agent| agent.working()))
                else {
                    break;
                };
                if working && this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            store,
            pull_requests: HashMap::new(),
            pull_request_list: None,
            linear_tabs: HashMap::new(),
            line_comment: None,
            tab_inset: 0.,
            diff: cx.new(CodeView::new),
            files: HashMap::new(),
            agent,
            asked: 0,
            triggers: HashMap::new(),
            strip: ScrollHandle::new(),
            scrolled_to: None,
            hovered_tab: None,
            copied: false,
            change_closed: false,
            _ticks: ticks,
        }
    }

    /// Asks the server for what the tab shows when what it shows may have changed.
    fn follow(&mut self, key: &str, trigger: Trigger, load: impl FnOnce(&mut Store) + 'static, cx: &mut Context<Self>) {
        if self.triggers.get(key) == Some(&trigger) {
            return;
        }
        self.triggers.insert(key.to_string(), trigger);
        let store = self.store.clone();
        cx.defer(move |cx| {
            store.update(cx, |store, cx| {
                load(store);
                cx.notify();
            });
        });
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asked += 1;
        cx.notify();
    }

    fn tab_strip(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.read(cx);
        let tabs = store.panel_tabs();
        let available = store.panel_unavailable().is_none();
        let blank = tabs.is_blank();
        if let Some(active) = tabs.active.as_ref().map(PanelTab::id)
            && self.scrolled_to.as_ref() != Some(&active)
            && let Some(index) = tabs.tabs.iter().position(|tab| tab.id() == active)
        {
            self.scrolled_to = Some(active);
            self.strip.scroll_to_item(index);
        }
        let chips: Vec<AnyElement> =
            tabs.tabs.iter().map(|tab| self.tab_chip(tab, tabs.active.as_ref() == Some(tab), cx)).collect();
        div()
            .absolute()
            .top_0()
            .left(px(self.tab_inset))
            .right(px(2. * TOOLBAR_WIDTH + 14.))
            .h(px(theme::TOP_BAR))
            .child(
                div()
                    .id("panel-tabs")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.strip)
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .when(!blank, |strip| strip.children(chips))
                    .when(available && !blank, |strip| {
                        strip.child(
                            ActionButton::icon("add-tab", "plus", "New tab")
                                .on_click(act(&self.store, |store, _| store.open_blank_tab())),
                        )
                    }),
            )
            .into_any_element()
    }

    fn tab_chip(&self, tab: &PanelTab, active: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let height = ControlSize::Regular.height();
        let close_size = ControlSize::Small.height();
        // The close button is as far from the tab's side as from its top and bottom.
        let close_margin = (height - close_size) / 2.;
        let key = tab.id();
        let hovering = self.hovered_tab.as_ref() == Some(&key);
        let lit = hovering || active;
        let group: SharedString = format!("tab-{key}").into();
        let path = tab.path().map(str::to_string);
        let (hover_key, menu_tab, close_tab, activate_tab) = (key.clone(), tab.clone(), tab.clone(), tab.clone());
        let menu_store = self.store.clone();
        div()
            .id(SharedString::from(key.clone()))
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .max_w(px(180.))
            .h(px(height))
            .pl(px(9.))
            .pr(px(close_margin))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(light(group, Radius::CONTROL, edges(0., 0., 0., 0.), active, cx))
            .child(
                div()
                    .relative()
                    .w(px(14.))
                    .flex()
                    .justify_center()
                    .child(icons::symbol(tab.symbol(), 11.).text_color(if lit { c.text } else { c.secondary })),
            )
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if lit { c.text } else { c.secondary })
                    .truncate()
                    .child(tab.title()),
            )
            .child(div().relative().size(px(close_size)).flex_shrink_0().when(lit, |close| {
                close.child(
                    ActionButton::icon(SharedString::from(format!("close-{key}")), "x", "Close (⌘W)")
                        .small()
                        .symbol_size(ControlSize::Small.small_symbol())
                        .surface(if active { Surface::Background.further() } else { Surface::Background.next() })
                        .on_click(act(&self.store, move |store, _| store.close_tab(&close_tab))),
                )
            }))
            .tooltip(crate::ui::tooltip(path.clone().unwrap_or_else(|| tab.title())))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hovered_tab = Some(hover_key.clone());
                } else if this.hovered_tab.as_ref() == Some(&hover_key) {
                    this.hovered_tab = None;
                }
                cx.notify();
            }))
            .on_click(act(&self.store, move |store, _| store.activate_tab(activate_tab.clone())))
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let (close_tab, other_tab) = (menu_tab.clone(), menu_tab.clone());
                let copy = path.clone();
                Menu::new()
                    .item("Close", choose(&menu_store, move |store, _| store.close_tab(&close_tab)))
                    .item("Close Others", choose(&menu_store, move |store, _| store.close_other_tabs(&other_tab)))
                    .item("Close All", choose(&menu_store, |store, _| store.close_all_tabs()))
                    .when_some(copy, |menu, path| {
                        menu.separator().item("Copy Path", move |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                        })
                    })
                    .show(event.position, window, cx);
            })
            .into_any_element()
    }

    /// What a tab says in the middle of the panel when it has nothing to show.
    fn message(text: impl Into<SharedString>, failed: bool, cx: &App) -> AnyElement {
        let c = colors(cx);
        div()
            .size_full()
            .p(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(if failed { c.danger } else { c.secondary })
            .text_center()
            .child(text.into())
            .into_any_element()
    }

    fn loading(cx: &App) -> AnyElement {
        let c = colors(cx);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(c.tertiary)
            .child(Spinner::new(ControlSize::Large.symbol()).render(cx))
            .into_any_element()
    }

    /// The strip under the tabs with what the open tab is about and its buttons.
    fn bar(content: impl IntoElement, cx: &App) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(div().h(px(36.)).pl(px(12.)).pr(px(4.)).flex().items_center().gap(px(4.)).child(content))
            .child(crate::ui::divider(cx))
    }

    /// Said over what a tab shows when that is only a part of it.
    fn note(text: &'static str, cx: &App) -> Div {
        let c = colors(cx);
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.))
                    .py(px(7.))
                    .bg(c.warning_background)
                    .text_size(px(12.))
                    .text_color(c.warning)
                    .child(text),
            )
            .child(crate::ui::divider(cx))
    }

    fn refresh_button(id: &'static str, help: &'static str, cx: &mut Context<Self>) -> ActionButton {
        let this = cx.entity().downgrade();
        ActionButton::icon(id, "rotate-cw", help).on_click(move |_, _, cx| {
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        })
    }

    fn surface(bar: Div, body: AnyElement) -> AnyElement {
        div().size_full().flex().flex_col().child(bar).child(div().flex_1().min_h_0().child(body)).into_any_element()
    }

    /// What a blank tab shows: the tabs there are to open.
    fn launcher(&mut self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let trigger = Trigger { target: target.clone(), scope: None, path: None, version: 0, asked: 0 };
        let server_id = target.server_id.clone();
        self.follow("linear", trigger, move |store| store.linear_read(&server_id), cx);
        let store = self.store.read(cx);
        let pull_requests_unavailable = store.pull_requests_unavailable();
        let extended = store.pull_requests_extended();
        let linear_unavailable = store.linear_unavailable();
        let linear_title =
            if store.linear.connected(&target.server_id).is_empty() { "Connect Linear" } else { "Linear" };
        let row = |id: &'static str,
                   symbol: &'static str,
                   title: &'static str,
                   keys: Option<&'static str>,
                   reason: Option<String>,
                   open: Box<dyn Fn(&mut Store)>| {
            let enabled = reason.is_none();
            let group: SharedString = format!("launch-{id}").into();
            div()
                .id(id)
                .group(group.clone())
                .relative()
                .w_full()
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .when(enabled, |row| row.child(light(group, Radius::CONTROL, edges(0., 0., 0., 0.), false, cx)))
                .when(!enabled, |row| row.opacity(0.45))
                .child(
                    div()
                        .relative()
                        .w(px(18.))
                        .flex()
                        .justify_center()
                        .child(icons::symbol(symbol, 13.).text_color(c.text)),
                )
                .child(div().relative().flex_1().text_size(px(13.)).text_color(c.text).child(title))
                .when_some(keys, |row, keys| {
                    row.child(
                        div()
                            .relative()
                            .h(px(20.))
                            .px(px(6.))
                            .rounded(px(5.))
                            .bg(c.background_tertiary)
                            .flex()
                            .items_center()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.secondary)
                            .child(keys),
                    )
                })
                .when_some(reason, |row, reason| row.tooltip(crate::ui::tooltip(reason)))
                .when(enabled, |row| row.on_click(act(&self.store, move |store, _| open(store))))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child("Open"))
            .child(
                div()
                    .w(px(250.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(row(
                        "launch-files",
                        "folder",
                        "Files",
                        Some("⇧⌘E"),
                        None,
                        Box::new(|store| store.open_tab(PanelTab::Files)),
                    ))
                    .child(row(
                        "launch-diff",
                        "diff",
                        "Diff",
                        Some("⌘D"),
                        (!target.repository).then(|| "Available in git repositories.".to_string()),
                        Box::new(|store| store.show_diff(None, None)),
                    ))
                    .child(row(
                        "launch-agents",
                        "users",
                        "Agents",
                        Some("⇧⌘A"),
                        None,
                        Box::new(|store| store.open_tab(PanelTab::Agents)),
                    ))
                    .child(row(
                        "launch-pull-request",
                        "git-pull-request",
                        "Pull Request",
                        Some("⇧⌘R"),
                        pull_requests_unavailable.clone(),
                        Box::new(|store| store.open_tab(PanelTab::PullRequest)),
                    ))
                    .when(extended, |rows| {
                        rows.child(row(
                            "launch-pull-requests",
                            "list",
                            "All Pull Requests",
                            Some("⌥⇧⌘R"),
                            pull_requests_unavailable,
                            Box::new(|store| store.open_tab(PanelTab::PullRequests)),
                        ))
                    })
                    .child(row(
                        "launch-linear",
                        "linear",
                        linear_title,
                        None,
                        linear_unavailable,
                        Box::new(|store| store.open_tab(PanelTab::Linear)),
                    )),
            )
            .into_any_element()
    }

    /// What the diff shows of the pull request besides its lines: which files were viewed and
    /// where its conversations and the comments waiting for a review are.
    fn marks(&self, document: &CodeDocument, scope: &DiffScope, cx: &App) -> CodeMarks {
        let store = self.store.read(cx);
        let DiffScope::PullRequest { number } = scope else { return CodeMarks::default() };
        let Some(page) = store.pull_requests.page.value().filter(|page| page.number == *number) else {
            return CodeMarks::default();
        };
        if !store.pull_requests_extended() {
            return CodeMarks::default();
        }
        let mut marks = CodeMarks {
            viewable: true,
            viewed: page.viewed.iter().filter(|file| file.viewed).map(|file| file.path.clone()).collect(),
            commentable: page.can_review_lines,
            marked: HashMap::new(),
        };
        let pending = store.pull_requests.pending.get(number).cloned().unwrap_or_default();
        let places = page
            .threads
            .iter()
            .filter_map(|thread| thread.line.map(|line| (thread.path.clone(), line, thread.side)))
            .chain(pending.into_iter().map(|comment| (comment.path, comment.line, comment.side)));
        for (path, line, side) in places {
            let Some(file) = document.files.iter().find(|file| file.path == path) else { continue };
            let index = (0..file.lines.len()).find(|&index| {
                let removed = file.kind(index) == REMOVED;
                match side {
                    Side::Left => removed && file.old.get(index).copied() == Some(line),
                    Side::Right => !removed && file.new.get(index).copied() == Some(line),
                }
            });
            if let Some(index) = index {
                marks.marked.entry(path).or_default().insert(index);
            }
        }
        marks
    }

    /// The changes in the thread's folder: what isn't committed, what the branch adds, or what
    /// one turn did.
    fn diff_surface(&mut self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.read(cx);
        let scope = store.diff_scope(target);
        let trigger = Trigger {
            target: target.clone(),
            scope: Some(scope.clone()),
            path: None,
            version: store.workspace_version,
            asked: self.asked,
        };
        if target.repository && !target.awaits_worktree {
            let (load_target, load_scope) = (target.clone(), scope.clone());
            self.follow(
                "diff",
                trigger,
                move |store| {
                    if let DiffScope::PullRequest { number } = load_scope
                        && store.pull_requests_extended()
                        && store.pull_requests.page.value().map(|page| page.number) != Some(number)
                    {
                        store.load_pull_request(&load_target, number);
                    }
                    store.load_diff(&load_target, load_scope)
                },
                cx,
            );
        }
        let store = self.store.read(cx);
        let document = store.side_panel.diff.value().cloned();
        let all_closed =
            document.as_ref().is_some_and(|document| store.side_panel.collapsed.len() >= document.files.len());
        let counts = document.as_ref().filter(|document| !document.files.is_empty()).map(|document| {
            let (text, runs) = counts_text(document.added(), document.removed(), colors(cx));
            div().pl(px(4.)).text_size(px(11.5)).child(StyledText::new(text).with_runs(runs))
        });
        let viewed = match (&scope, store.pull_requests.page.value()) {
            (DiffScope::PullRequest { number }, Some(page)) if page.number == *number && !page.viewed.is_empty() => {
                let done = page.viewed.iter().filter(|file| file.viewed).count();
                Some(format!("{done} of {} viewed", page.viewed.len()))
            }
            _ => None,
        };
        let several = document.as_ref().is_some_and(|document| document.files.len() > 1);
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(div().ml(px(-8.)).child(self.scope_menu(target, &scope, cx)))
                .when_some(viewed, |bar, viewed| {
                    bar.child(
                        div().text_size(px(11.5)).text_color(colors(cx).tertiary).whitespace_nowrap().child(viewed),
                    )
                })
                .children(counts)
                .child(div().flex_1().min_w(px(4.)))
                .when(several, |bar| {
                    bar.child(
                        ActionButton::icon(
                            "collapse-all",
                            if all_closed { "unfold-vertical" } else { "fold-vertical" },
                            if all_closed { "Open every file" } else { "Close every file" },
                        )
                        .on_click(act(&self.store, move |store, _| store.set_all_collapsed(!all_closed))),
                    )
                })
                .child(Self::refresh_button("refresh-diff", "Read the changes again", cx)),
            cx,
        );
        let body: AnyElement = if !target.repository {
            Self::message("This folder isn't a git repository.", false, cx)
        } else if target.awaits_worktree {
            Self::message("Nothing has changed.", false, cx)
        } else {
            match &self.store.read(cx).side_panel.diff {
                Loaded::Loading => Self::loading(cx),
                Loaded::Failed(message) => Self::message(message.clone(), true, cx),
                Loaded::Ready(document) if document.files.is_empty() => Self::message(
                    if scope == DiffScope::Uncommitted { "Everything is committed." } else { "Nothing has changed." },
                    false,
                    cx,
                ),
                Loaded::Ready(document) => {
                    let truncated = document.truncated;
                    let store = self.store.clone();
                    let marks = self.marks(document, &scope, cx);
                    let (viewed_target, viewed_marks) = (target.clone(), marks.clone());
                    let viewed_scope = scope.clone();
                    let actions = CodeActions::default()
                        .toggle(act_path(&self.store, |store, path| store.toggle_collapsed(path)))
                        .open(act_path(&self.store, |store, path| store.open_tab(PanelTab::File(path.to_string()))))
                        .viewed(act_path(&self.store, move |store, path| {
                            let DiffScope::PullRequest { number } = viewed_scope else { return };
                            let viewed = !viewed_marks.viewed.contains(path);
                            let edit = PullRequestEdit::Viewed { path: path.to_string(), viewed };
                            store.pull_request_edit(edit, None, &viewed_target, number);
                        }))
                        .comment(act_line(&self.store, |store, line| store.comment_on_line(Some(line.into()))));
                    let code = self.diff.update(cx, |view, cx| {
                        let state = store.read(cx);
                        let Loaded::Ready(document) = &state.side_panel.diff else { return div().into_any_element() };
                        let (document, collapsed, reveal) =
                            (document.clone(), state.side_panel.collapsed.clone(), state.side_panel.reveal.clone());
                        view.element(
                            CodeSnapshot {
                                document: &document,
                                collapsed: &collapsed,
                                reveal: reveal.as_ref(),
                                marks: &marks,
                            },
                            actions,
                            cx,
                        )
                    });
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .when(truncated, |body| {
                            body.child(Self::note(
                                "These changes are too long to show in full. This is their start.",
                                cx,
                            ))
                        })
                        .child(div().flex_1().min_h_0().child(code))
                        .into_any_element()
                }
            }
        };
        Self::surface(bar, body)
    }

    /// The menu that picks which changes the diff shows.
    fn scope_menu(&self, target: &PanelTarget, scope: &DiffScope, cx: &mut Context<Self>) -> ActionMenu {
        let store = self.store.read(cx);
        let title = match scope {
            DiffScope::Uncommitted => "Uncommitted".to_string(),
            DiffScope::Branch => "Branch".to_string(),
            DiffScope::PullRequest { number } => format!("PR #{number}"),
            DiffScope::Commit { sha } => format!("Commit {}", sha.chars().take(7).collect::<String>()),
            DiffScope::Turn { item_id } => store.turn_name(item_id),
        };
        // The pull request the menu offers: the thread's, or the one the scope shows.
        let pull_request = match scope {
            DiffScope::PullRequest { number } => Some(*number),
            _ => target.pull_request,
        }
        .filter(|_| store.pull_requests_unavailable().is_none());
        let commits: Vec<(String, String)> = match (pull_request, store.pull_requests.page.value()) {
            (Some(number), Some(page)) if page.number == number => page
                .activity
                .iter()
                .flat_map(|entry| entry.commits.iter())
                .map(|commit| (commit.sha.clone(), commit.headline.clone()))
                .collect(),
            _ => Vec::new(),
        };
        let turns: Vec<(DiffScope, String)> = store
            .side_panel
            .turns
            .iter()
            .rev()
            .map(|turn| {
                let files = if turn.files == 1 { "1 file".to_string() } else { format!("{} files", turn.files) };
                (DiffScope::Turn { item_id: turn.id.clone() }, format!("{} · {files}", store.turn_name(&turn.id)))
            })
            .collect();
        let chosen = scope.clone();
        let handle = self.store.clone();
        ActionMenu::new("scope", title, move |_, _| {
            let pick = |scope: DiffScope| choose(&handle, move |store, _| store.choose_diff_scope(scope.clone()));
            let mut menu = Menu::new()
                .checked(chosen == DiffScope::Uncommitted, "Uncommitted changes", pick(DiffScope::Uncommitted))
                .checked(chosen == DiffScope::Branch, "Branch changes", pick(DiffScope::Branch));
            if let Some(number) = pull_request {
                let scope = DiffScope::PullRequest { number };
                menu = menu.checked(chosen == scope, format!("Pull request #{number}"), pick(scope));
                for (sha, headline) in &commits {
                    let scope = DiffScope::Commit { sha: sha.clone() };
                    menu = menu.checked(chosen == scope, headline.clone(), pick(scope));
                }
            }
            if !turns.is_empty() {
                menu = menu.separator();
                for (scope, label) in &turns {
                    menu = menu.checked(&chosen == scope, label.clone(), pick(scope.clone()));
                }
            }
            menu
        })
    }

    /// The folder the thread works in: its files under their folders, to open one in a tab.
    fn files_surface(&mut self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let trigger = Trigger {
            target: target.clone(),
            scope: None,
            path: None,
            version: store.workspace_version,
            asked: self.asked,
        };
        let load_target = target.clone();
        self.follow("files", trigger, move |store| store.load_files(&load_target), cx);
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(icons::symbol("folder", 11.).text_color(c.secondary))
                .child(
                    div()
                        .pl(px(3.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .truncate()
                        .child(target.name.clone()),
                )
                .child(div().flex_1().min_w(px(4.)))
                .child(Self::refresh_button("refresh-files", "Read the folder again", cx)),
            cx,
        );
        let store = self.store.read(cx);
        let body: AnyElement = if let Some(error) = &store.side_panel.files_error {
            Self::message(error.clone(), true, cx)
        } else if let Some(top) = store.side_panel.listings.get("") {
            if top.is_empty() {
                Self::message("This folder is empty.", false, cx)
            } else {
                let nodes = store.side_panel.nodes();
                div()
                    .id("file-tree")
                    .size_full()
                    .overflow_y_scroll()
                    .py(px(6.))
                    .children(nodes.into_iter().map(|node| {
                        let group: SharedString = format!("node-{}", node.path).into();
                        let (path, folder, copy) = (node.path.clone(), node.folder, node.path.clone());
                        div()
                            .id(SharedString::from(format!("file-{}", node.path)))
                            .group(group.clone())
                            .relative()
                            .h(px(26.))
                            .pl(px(12. + node.depth as f32 * 14.))
                            .pr(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(light(group, Radius::SMALL, edges(0., 6., 0., 6.), false, cx))
                            .when(node.ignored, |row| row.opacity(0.5))
                            .child(div().relative().w(px(10.)).flex().justify_center().when(node.folder, |chevron| {
                                chevron.child(
                                    icons::symbol(if node.open { "chevron-down" } else { "chevron-right" }, 8.)
                                        .text_color(c.tertiary),
                                )
                            }))
                            .child(
                                div().relative().w(px(16.)).flex().justify_center().child(
                                    icons::symbol(if node.folder { "folder" } else { file_symbol(&node.path) }, 11.)
                                        .text_color(c.secondary),
                                ),
                            )
                            .child(
                                div()
                                    .relative()
                                    .min_w_0()
                                    .text_size(px(12.5))
                                    .text_color(c.text)
                                    .truncate()
                                    .child(node.name.clone()),
                            )
                            .on_click(act(&self.store, move |store, _| {
                                if folder {
                                    store.toggle_folder(&path);
                                } else {
                                    store.open_tab(PanelTab::File(path.clone()));
                                }
                            }))
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                let path = copy.clone();
                                Menu::new()
                                    .item("Copy Path", move |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                                    })
                                    .show(event.position, window, cx);
                            })
                    }))
                    .into_any_element()
            }
        } else {
            Self::loading(cx)
        };
        Self::surface(bar, body)
    }

    /// The code view of a tab that shows one file, kept for as long as the tab is.
    fn file_view(&mut self, tab: &PanelTab, cx: &mut Context<Self>) -> Entity<CodeView> {
        let open: Vec<PanelTab> = self.store.read(cx).panel_tabs().tabs;
        self.files.retain(|kept, _| open.contains(kept));
        self.files.entry(tab.clone()).or_insert_with(|| cx.new(CodeView::new)).clone()
    }

    /// One file of the folder the thread works in.
    fn file_surface(&mut self, target: &PanelTarget, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let trigger = Trigger {
            target: target.clone(),
            scope: None,
            path: Some(path.to_string()),
            version: store.workspace_version,
            asked: self.asked,
        };
        let (load_target, load_path) = (target.clone(), path.to_string());
        self.follow(&format!("file:{path}"), trigger, move |store| store.load_file(&load_path, &load_target), cx);
        let folder = std::path::Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .filter(|folder| !folder.is_empty());
        let name = crate::models::last_component(path);
        let copied = self.copied;
        let (copy_path, this) = (path.to_string(), cx.entity().downgrade());
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .when_some(folder, |title, folder| {
                            title.child(div().text_color(c.secondary).truncate().child(format!("{folder}/")))
                        })
                        .child(div().text_color(c.text).flex_shrink_0().child(name)),
                )
                .child(div().flex_1().min_w(px(4.)))
                .child(
                    ActionButton::icon("copy-path", if copied { "check" } else { "copy" }, "Copy the path").on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()));
                            let _ = this.update(cx, |this, cx| {
                                this.copied = true;
                                cx.notify();
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor().timer(Duration::from_millis(1200)).await;
                                    let _ = this.update(cx, |this, cx| {
                                        this.copied = false;
                                        cx.notify();
                                    });
                                })
                                .detach();
                            });
                        },
                    ),
                )
                .child(Self::refresh_button("refresh-file", "Read the file again", cx)),
            cx,
        );
        let tab = PanelTab::File(path.to_string());
        let content = self.store.read(cx).side_panel.contents.get(&tab).cloned();
        let body: AnyElement = match content {
            None | Some(Loaded::Loading) => Self::loading(cx),
            Some(Loaded::Failed(message)) => Self::message(message, true, cx),
            Some(Loaded::Ready(FileContent::Text(document, truncated))) => {
                if document.files.first().is_some_and(|file| file.lines.is_empty()) {
                    Self::message("This file is empty.", false, cx)
                } else {
                    let view = self.file_view(&tab, cx);
                    let (collapsed, marks) = (Default::default(), CodeMarks::default());
                    let code = view.update(cx, |view, cx| {
                        view.element(
                            CodeSnapshot { document: &document, collapsed: &collapsed, reveal: None, marks: &marks },
                            CodeActions::default(),
                            cx,
                        )
                    });
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .when(truncated, |body| {
                            body.child(Self::note("This file is too long to show in full. This is its start.", cx))
                        })
                        .child(div().flex_1().min_h_0().child(code))
                        .into_any_element()
                }
            }
            Some(Loaded::Ready(FileContent::Image(file))) => div()
                .size_full()
                .p(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .child(img(std::path::PathBuf::from(file)).max_w_full().max_h_full().object_fit(ObjectFit::ScaleDown))
                .into_any_element(),
            Some(Loaded::Ready(FileContent::Binary(size))) => Self::message(
                format!(
                    "This file isn't text, so it isn't shown. It is {}.",
                    crate::composer::attachments::file_size(size)
                ),
                false,
                cx,
            ),
        };
        Self::surface(bar, body)
    }

    /// What one turn changed in one file.
    fn change_surface(&mut self, target: &PanelTarget, turn: &str, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let tab = PanelTab::Change { turn: turn.to_string(), path: path.to_string() };
        let trigger = Trigger { target: target.clone(), scope: None, path: Some(tab.id()), version: 0, asked: 0 };
        let (load_target, load_turn, load_path) = (target.clone(), turn.to_string(), path.to_string());
        self.follow(&tab.id(), trigger, move |store| store.load_change(&load_turn, &load_path, &load_target), cx);
        let store = self.store.read(cx);
        let (turn_scope, reveal) = (DiffScope::Turn { item_id: turn.to_string() }, path.to_string());
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .truncate()
                        .child(store.turn_name(turn)),
                )
                .child(div().flex_1().min_w(px(4.)))
                .child(
                    ActionButton::icon("turn-diff", "diff", "Show everything this turn changed")
                        .on_click(act(&self.store, move |store, _| {
                            store.show_diff(Some(turn_scope.clone()), Some(reveal.clone()))
                        })),
                ),
            cx,
        );
        let content = store.side_panel.contents.get(&tab).cloned();
        let body: AnyElement = match content {
            None | Some(Loaded::Loading) => Self::loading(cx),
            Some(Loaded::Failed(message)) => Self::message(message, true, cx),
            Some(Loaded::Ready(FileContent::Text(document, truncated))) if !document.files.is_empty() => {
                let view = self.file_view(&tab, cx);
                let collapsed = if self.change_closed { [path.to_string()].into() } else { Default::default() };
                let marks = CodeMarks::default();
                let this = cx.entity().downgrade();
                let actions = CodeActions::default()
                    .toggle(move |_, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.change_closed = !this.change_closed;
                            cx.notify();
                        });
                    })
                    .open(act_path(&self.store, |store, path| store.open_tab(PanelTab::File(path.to_string()))));
                let code = view.update(cx, |view, cx| {
                    view.element(
                        CodeSnapshot { document: &document, collapsed: &collapsed, reveal: None, marks: &marks },
                        actions,
                        cx,
                    )
                });
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .when(truncated, |body| {
                        body.child(Self::note(
                            "The turn's changes are too long to show in full, so this file's may be cut short.",
                            cx,
                        ))
                    })
                    .child(div().flex_1().min_h_0().child(code))
                    .into_any_element()
            }
            Some(Loaded::Ready(_)) => Self::message("The turn's changes to this file can't be shown.", false, cx),
        };
        Self::surface(bar, body)
    }

    /// The agents the thread's agent has started, and what one of them did once it is opened.
    fn agents_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let agents = store.agents.clone();
        if let Some(agent) =
            agents.iter().find(|agent| Some(&agent.id) == store.side_panel.shown_agent.as_ref()).cloned()
        {
            let bar = Self::bar(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(4.))
                    .child(
                        div().ml(px(-8.)).child(
                            ActionButton::icon("all-agents", "chevron-left", "All agents")
                                .on_click(act(&self.store, |store, cx| store.show_agents(cx))),
                        ),
                    )
                    .child(div().w(px(16.)).flex().justify_center().child(status_icon(agent.status, cx)))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.text)
                            .truncate()
                            .child(agent.title.clone()),
                    )
                    .child(div().flex_1().min_w(px(6.)))
                    .child(div().pr(px(6.)).child(agent_time(&agent, cx))),
                cx,
            );
            return Self::surface(bar, div().size_full().child(self.agent.clone()).into_any_element());
        }
        if agents.is_empty() {
            return Self::message("The agents this thread starts show up here.", false, cx);
        }
        let bar = Self::bar(
            div().flex().w_full().child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.secondary)
                    .child(summary(&agents)),
            ),
            cx,
        );
        let list = div().id("agents").size_full().overflow_y_scroll().p(px(8.)).flex().flex_col().gap(px(2.)).children(
            agents.iter().map(|agent| {
                let group: SharedString = format!("agent-{}", agent.id).into();
                let id = agent.id.clone();
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .relative()
                    .px(px(10.))
                    .py(px(8.))
                    // An agent that another agent started stands in from it.
                    .when(agent.parent.is_some(), |row| row.pl(px(28.)))
                    .child(light(group, Radius::CONTROL, edges(0., 0., 0., 0.), false, cx))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_start()
                            .gap(px(9.))
                            .child(
                                div()
                                    .w(px(16.))
                                    .h(px(18.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(status_icon(agent.status, cx)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .text_size(px(13.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(c.text)
                                                    .truncate()
                                                    .child(agent.title.clone()),
                                            )
                                            .when_some(agent.kind.clone(), |line, kind| {
                                                line.child(
                                                    div()
                                                        .min_w_0()
                                                        .text_size(px(11.))
                                                        .text_color(c.secondary)
                                                        .truncate()
                                                        .child(kind),
                                                )
                                            })
                                            .child(div().flex_1().min_w(px(6.)))
                                            .child(agent_time(agent, cx)),
                                    )
                                    .when(!agent.detail.is_empty(), |text| {
                                        text.child(
                                            div()
                                                .text_size(px(12.))
                                                .line_height(px(16.))
                                                .text_color(c.secondary)
                                                .line_clamp(2)
                                                .child(agent.detail.clone()),
                                        )
                                    })
                                    .when_some(agent.usage(), |text, usage| {
                                        text.child(div().text_size(px(11.)).text_color(c.tertiary).child(usage))
                                    }),
                            ),
                    )
                    .on_click(act(&self.store, move |store, cx| store.show_agent(id.clone(), cx)))
            }),
        );
        Self::surface(bar, list.into_any_element())
    }

    /// The issues of a Linear workspace the server is connected to, or how to connect one.
    fn linear_tab(
        &mut self,
        tab: PanelTab,
        workspace: String,
        issue: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let store = self.store.clone();
        let view = self
            .linear_tabs
            .entry(tab)
            .or_insert_with(|| cx.new(|cx| LinearView::new(store, workspace, issue, window, cx)))
            .clone();
        div().size_full().child(view).into_any_element()
    }

    fn pull_request_tab(
        &mut self,
        tab: PanelTab,
        number: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let store = self.store.clone();
        let view = self
            .pull_requests
            .entry(tab)
            .or_insert_with(|| cx.new(|cx| PullRequestView::new(store, number, window, cx)))
            .clone();
        div().size_full().child(view).into_any_element()
    }

    fn pull_request_list_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.clone();
        let view = self
            .pull_request_list
            .get_or_insert_with(|| cx.new(|cx| PullRequestListView::new(store, window, cx)))
            .clone();
        div().size_full().child(view).into_any_element()
    }

    /// Lets go of the views of tabs that are closed.
    fn prune_tabs(&mut self, cx: &App) {
        let tabs = self.store.read(cx).panel_tabs().tabs;
        self.pull_requests.retain(|tab, _| tabs.contains(tab));
        self.linear_tabs.retain(|tab, _| tabs.contains(tab));
        if !tabs.contains(&PanelTab::PullRequests) {
            self.pull_request_list = None;
        }
    }

    fn linear_surface(&mut self, target: &PanelTarget, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        if let Some(reason) = store.linear_unavailable() {
            return Self::message(reason, false, cx);
        }
        if !store.linear.connected(&target.server_id).is_empty() {
            return self.linear_tab(PanelTab::Linear, String::new(), None, window, cx);
        }
        let trigger = Trigger { target: target.clone(), scope: None, path: None, version: 0, asked: 0 };
        let read_id = target.server_id.clone();
        self.follow("linear", trigger, move |store| store.linear_read(&read_id), cx);
        let store = self.store.read(cx);
        let server_id = target.server_id.clone();
        let connecting = store.linear.connecting.as_deref() == Some(&target.server_id);
        let error = store.linear.error.clone();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .child(
                div()
                    .max_w(px(320.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(16.))
                    .child(icons::symbol("linear", 28.).text_color(c.text))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c.text)
                                    .child("Connect Linear"),
                            )
                            .child(div().text_size(px(12.5)).text_color(c.secondary).text_center().child(
                                "See your issues and hand them to your agents. The connection is kept on your server.",
                            )),
                    )
                    .child(
                        ActionButton::new("connect-linear", "Connect Linear")
                            .variant(Variant::Primary)
                            .pending(connecting)
                            .on_click(act(&self.store, move |store, _| store.linear_connect(&server_id))),
                    )
                    .when_some(error, |prompt, error| {
                        prompt.child(div().text_size(px(12.5)).text_color(c.danger).text_center().child(error))
                    }),
            )
            .into_any_element()
    }
}

/// Runs something on the store with the line a code view's click was on.
fn act_line(
    store: &Entity<Store>,
    run: impl Fn(&mut Store, crate::panel::code_view::CommentedLine) + 'static,
) -> impl Fn(crate::panel::code_view::CommentedLine, &mut Window, &mut App) + 'static {
    let store = store.clone();
    move |line, _, cx| {
        store.update(cx, |store, cx| {
            run(store, line);
            cx.notify();
        })
    }
}

/// Runs something on the store with the path a code view's click was on.
fn act_path(
    store: &Entity<Store>,
    run: impl Fn(&mut Store, &str) + 'static,
) -> impl Fn(&str, &mut Window, &mut App) + 'static {
    let store = store.clone();
    move |path, _, cx| {
        store.update(cx, |store, cx| {
            run(store, path);
            cx.notify();
        })
    }
}

fn status_icon(status: ToolStatus, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    match status {
        ToolStatus::Running => icons::symbol("circle-dashed", 11.).text_color(c.working),
        ToolStatus::Succeeded => icons::symbol("check", 11.).text_color(c.secondary),
        ToolStatus::Failed => icons::symbol("x", 11.).text_color(c.danger),
    }
}

/// How long the agent has worked so far, or worked in all.
fn agent_time(agent: &AgentView, cx: &App) -> AnyElement {
    let c = colors(cx);
    let time = |text: String, color: Hsla| {
        div()
            .flex_shrink_0()
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    if agent.working() {
        return time(elapsed(agent.started_at), c.working);
    }
    match agent.duration_ms {
        Some(milliseconds) => time(duration(milliseconds), c.tertiary),
        None => div().into_any_element(),
    }
}

/// "2 working · 1 done · 1 failed", leaving out what there is none of.
fn summary(agents: &[AgentView]) -> String {
    let count = |status: ToolStatus| agents.iter().filter(|agent| agent.status == status).count();
    [
        (count(ToolStatus::Running), "working"),
        (count(ToolStatus::Succeeded), "done"),
        (count(ToolStatus::Failed), "failed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect::<Vec<_>>()
    .join(" · ")
}

impl Render for SidePanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        self.prune_tabs(cx);
        let line_comment = line_comment::follow(&mut self.line_comment, &self.store, window, cx);
        let store = self.store.read(cx);
        let unavailable = store.panel_unavailable();
        let target = store.panel_target();
        let active = store.panel_tabs().active;
        let content: AnyElement = if let Some(reason) = unavailable {
            Self::message(reason, false, cx)
        } else if let Some(target) = target {
            match active {
                Some(PanelTab::Diff) => self.diff_surface(&target, cx),
                Some(PanelTab::Files) => self.files_surface(&target, cx),
                Some(PanelTab::File(path)) => self.file_surface(&target, &path, cx),
                Some(PanelTab::Change { turn, path }) => self.change_surface(&target, &turn, &path, cx),
                Some(PanelTab::Agents) => self.agents_surface(cx),
                Some(tab @ PanelTab::PullRequest) => self.pull_request_tab(tab, None, window, cx),
                Some(tab @ PanelTab::PullRequestNumber(number)) => self.pull_request_tab(tab, Some(number), window, cx),
                Some(PanelTab::PullRequests) => self.pull_request_list_tab(window, cx),
                Some(PanelTab::Linear) => self.linear_surface(&target, window, cx),
                Some(tab @ PanelTab::LinearIssue { .. }) => {
                    let PanelTab::LinearIssue { workspace, id, .. } = tab.clone() else { unreachable!() };
                    self.linear_tab(tab, workspace, Some(id), window, cx)
                }
                Some(PanelTab::Blank(_)) | None => self.launcher(&target, cx),
            }
        } else {
            div().into_any_element()
        };
        div()
            .size_full()
            .relative()
            .bg(c.background)
            .children(line_comment)
            .child(
                div()
                    .size_full()
                    .pt(px(theme::TOP_BAR))
                    .flex()
                    .flex_col()
                    .child(crate::ui::divider(cx))
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .child(self.tab_strip(cx))
    }
}
