//! The panel beside the thread: its tabs in the window's top bar, and under them what the open
//! tab shows of the folder the thread works in.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::{AgentState, AgentView};
use motile_protocol::wire::{DiffScope, Side, ToolStatus};

use super::code_view::{CodeActions, CodeSnapshot, CodeView, counts_text};
use super::state::{
    CodeDocument, CodeMarks, CodeMedia, FileContent, FileNode, Loaded, PanelTab, PanelTarget, REMOVED, file_symbol,
};
use crate::media::zoom::ZoomableImage;
use crate::models::{AgentViewExt, duration, elapsed};
use crate::panel::line_comment::{self, LineCommentSheet};
use crate::panel::linear::LinearView;
use crate::panel::pull_request::PullRequestView;
use crate::panel::pull_request_list::PullRequestListView;
use crate::panel::ricrac::RicracView;
use crate::store::Store;
use crate::store::integrations::Integration;
use crate::theme::{self, Colors, ControlSize, Layer, Radius, colors};
use crate::ui::agent_avatar::{AgentAvatar, state_color};
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, ActionMenu, Highlight, KeyCaps, Spinner, TOOLBAR_WIDTH, Variant, edges, icons};

/// How far the tabs keep from the strip's ends.
const TAB_EDGE: f32 = 8.;
/// The room on each side of a tab that looks empty but is the tab's, so no click falls between
/// tabs.
const TAB_MARGIN: f32 = 1.;

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

/// A tab of a thread or a draft, by what its tabs are kept under.
type TabOf = (String, PanelTab);

pub struct RightPanelView {
    store: Entity<Store>,
    /// The view of the open pull request, Linear or Ricrac tab, which starts afresh when another
    /// tab or another thread's is shown, as the Mac's does.
    pull_request: Option<(TabOf, Entity<PullRequestView>)>,
    pull_request_list: Option<Entity<PullRequestListView>>,
    linear: Option<(TabOf, Entity<LinearView>)>,
    ricrac: Option<(TabOf, Entity<RicracView>)>,
    line_comment: Option<Entity<LineCommentSheet>>,
    /// Where the tabs start from the panel's left edge in place of the strip's own margin: past
    /// the window's buttons when the panel reaches them.
    pub tab_inset: Option<f32>,
    /// What the window's buttons take of the top bar's end, where the app draws them.
    pub trailing_room: f32,
    diff: Entity<CodeView>,
    /// The views of the tabs that show one file, by tab.
    files: HashMap<PanelTab, Entity<CodeView>>,
    /// The images and the videos of the tabs that show such a file, by tab, with the file.
    zooms: HashMap<PanelTab, (std::path::PathBuf, Entity<ZoomableImage>)>,
    #[cfg(target_os = "macos")]
    players: HashMap<PanelTab, (std::path::PathBuf, Entity<crate::media::video::VideoPlayer>)>,
    /// The videos the tabs handed to the system's player, by tab.
    #[cfg(not(target_os = "macos"))]
    played_elsewhere: HashMap<PanelTab, std::path::PathBuf>,
    asked: u64,
    /// What each tab last asked its server with, by tab.
    triggers: HashMap<String, Trigger>,
    strip: ScrollHandle,
    /// The tab the strip was last scrolled to.
    scrolled_to: Option<String>,
    hovered_tab: Option<String>,
    /// The file tab whose path was just copied.
    copied: Option<PanelTab>,
    /// The change tab whose one file is closed, until another tab is shown.
    change_closed: Option<PanelTab>,
    /// The marks of the pull request's diff, and what they were worked out from.
    marks: Option<(MarksMadeOf, CodeMarks)>,
    agents: ListState,
    listed_agents: Vec<String>,
    /// Whether the agents that ended are shown, once the user has said. Until then they are
    /// while none works.
    shows_ended: Option<bool>,
    /// The agent row under the pointer, which offers to stop its agent.
    hovered_agent: Option<String>,
    _ticks: Task<()>,
}

/// What a pull request diff's marks are worked out from: the document, the read of the page, and
/// the comments waiting for a review.
#[derive(PartialEq)]
struct MarksMadeOf {
    document: usize,
    reads: u64,
    pending: Vec<u64>,
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

/// The light of a row under the pointer, behind what the element `group` names shows.
fn light(group: impl Into<SharedString>, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    Highlight::new().radius(radius).inset(inset).selected(selected).render(group, cx)
}

impl RightPanelView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |this, _, cx| {
            this.pause_hidden_videos(cx);
            cx.notify()
        })
        .detach();
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
            pull_request: None,
            pull_request_list: None,
            linear: None,
            ricrac: None,
            line_comment: None,
            tab_inset: None,
            trailing_room: 0.,
            diff: cx.new(CodeView::new),
            files: HashMap::new(),
            zooms: HashMap::new(),
            #[cfg(target_os = "macos")]
            players: HashMap::new(),
            #[cfg(not(target_os = "macos"))]
            played_elsewhere: HashMap::new(),
            asked: 0,
            triggers: HashMap::new(),
            strip: ScrollHandle::new(),
            scrolled_to: None,
            hovered_tab: None,
            copied: None,
            change_closed: None,
            marks: None,
            agents: ListState::new(0, ListAlignment::Top, px(400.)),
            listed_agents: Vec::new(),
            shows_ended: None,
            hovered_agent: None,
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
        let leading = self.tab_inset.unwrap_or(TAB_EDGE);
        div()
            .absolute()
            .top_0()
            .left_0()
            .right(px(2. * TOOLBAR_WIDTH + 14. + self.trailing_room))
            .h(px(theme::TOP_BAR))
            .child(
                div()
                    .id("panel-tabs")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.strip)
                    .pl(px(leading - TAB_MARGIN))
                    .pr(px(TAB_EDGE - TAB_MARGIN))
                    .flex()
                    .items_center()
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
        let close_help = self.store.read(cx).shortcuts.help("Close", "rightPanel.close");
        div()
            .id(SharedString::from(key.clone()))
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .max_w(px(180. + 2. * TAB_MARGIN))
            .h(px(height))
            .pl(px(9. + TAB_MARGIN))
            .pr(px(close_margin + TAB_MARGIN))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                Highlight::new()
                    .selected(active)
                    .lit(hovering)
                    .small(true)
                    .inset(edges(0., TAB_MARGIN, 0., TAB_MARGIN))
                    .render(group, cx),
            )
            .child(div().relative().w(px(14.)).flex().justify_center().child(
                icons::symbol(tab.symbol(), 11.).text_color(if lit { c.foreground } else { c.muted_foreground }),
            ))
            .child(
                crate::ui::middle_truncated(tab.title())
                    .relative()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if lit { c.foreground } else { c.muted_foreground }),
            )
            .child(div().relative().size(px(close_size)).flex_shrink_0().when(lit, |close| {
                close.child(
                    ActionButton::icon(SharedString::from(format!("close-{key}")), "x", close_help)
                        .small()
                        .symbol_size(ControlSize::Small.small_symbol())
                        .row(Some(if active { Layer::ControlLit } else { Layer::Control }))
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
        super::pull_request::panel_message(text, failed, cx)
    }

    /// What the changes tab says of a folder that is no git repository, with the button that
    /// makes it one.
    fn no_repository(&self, project_id: &str, cx: &App) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(Some(project_id)).filter(|project| store.can_initialize_git(project)).cloned();
        let pending = project.as_ref().is_some_and(|project| store.initializing_git.contains(&project.id));
        div()
            .size_full()
            .p(px(24.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(c.muted_foreground)
                    .text_center()
                    .child("This folder isn't a git repository."),
            )
            .when_some(project, |message, project| {
                message.child(
                    ActionButton::new("initialize-git-panel", "Initialize git")
                        .symbol("git-branch")
                        .pending(pending)
                        .on_click(act(&self.store, move |store, _| store.initialize_git(&project))),
                )
            })
            .into_any_element()
    }

    fn loading(cx: &App) -> AnyElement {
        let c = colors(cx);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(c.muted_stronger_foreground)
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
            .child(div().px(px(12.)).py(px(7.)).bg(c.warning_tint).text_size(px(12.)).text_color(c.warning).child(text))
            .child(crate::ui::divider(cx))
    }

    /// The button that reads the tab again, which turns while `reading` is read.
    fn refresh_button(
        &self,
        id: &'static str,
        help: &'static str,
        reading: &PanelTab,
        cx: &mut Context<Self>,
    ) -> ActionButton {
        let this = cx.entity().downgrade();
        let pending = self.store.read(cx).right_panel.reading.contains(reading);
        ActionButton::icon(id, "rotate-cw", help).pending(pending).on_click(move |_, _, cx| {
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
        self.follow("linear", trigger.clone(), move |store| store.linear_read(&server_id), cx);
        let server_id = target.server_id.clone();
        self.follow("ricrac", trigger, move |store| store.ricrac_read(&server_id), cx);
        let store = self.store.read(cx);
        let pull_requests_unavailable = store.pull_requests_unavailable();
        let extended = store.pull_requests_extended();
        let (linear_on, ricrac_on) = (store.is_on(Integration::Linear), store.is_on(Integration::Ricrac));
        let linear_unavailable = store.linear_unavailable();
        let linear_connected = store.linear_connected();
        let linear_title = if linear_connected { "Linear" } else { "Connect Linear" };
        let ricrac_unavailable = store.ricrac_unavailable();
        let ricrac_connected = store.ricrac_connected();
        let ricrac_title = if ricrac_connected { "Ricrac" } else { "Connect Ricrac" };
        let working = store.agents.iter().filter(|agent| agent.working()).count();
        let row = |id: &'static str,
                   symbol: &'static str,
                   title: &'static str,
                   count: usize,
                   command: Option<&'static str>,
                   reason: Option<String>,
                   open: Box<dyn Fn(&mut Store)>| {
            let enabled = reason.is_none();
            let keys = command.and_then(|command| store.shortcuts.caps(command));
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
                .when(enabled, |row| row.child(light(group, Radius::MD, edges(0., 0., 0., 0.), false, cx)))
                .when(!enabled, |row| row.opacity(c.opacity(theme::Opacity::Disabled)))
                .child(
                    div()
                        .relative()
                        .w(px(18.))
                        .flex()
                        .justify_center()
                        .child(icons::symbol(symbol, 13.).text_color(c.foreground)),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .flex()
                        .text_size(px(13.))
                        .text_color(c.foreground)
                        .when(count == 0, |text| text.child(title))
                        // How many of its things work, in the working colour.
                        .when(count > 0, |text| {
                            text.child(format!("{title}\u{a0}"))
                                .child(div().text_color(c.muted_foreground).child("·"))
                                .child(div().text_color(c.process).child(format!("\u{a0}{count}")))
                        }),
                )
                .when_some(keys, |row, keys| {
                    row.child(div().relative().text_color(c.muted_foreground).child(KeyCaps::new(keys).joined(true)))
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
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.foreground).child("Open"))
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
                        0,
                        Some("rightPanel.files"),
                        None,
                        Box::new(|store| store.open_tab(PanelTab::Files)),
                    ))
                    .child(row(
                        "launch-diff",
                        "diff",
                        "Diff",
                        0,
                        Some("rightPanel.diff"),
                        (!target.repository).then(|| "Available in git repositories.".to_string()),
                        Box::new(|store| store.show_diff(None, None)),
                    ))
                    .child(row(
                        "launch-agents",
                        "bot",
                        "Agents",
                        working,
                        Some("rightPanel.agents"),
                        None,
                        Box::new(|store| store.open_tab(PanelTab::Agents)),
                    ))
                    .child(row(
                        "launch-pull-request",
                        "git-pull-request",
                        "Pull Request",
                        0,
                        Some("rightPanel.pullRequest"),
                        pull_requests_unavailable.clone(),
                        Box::new(|store| store.open_tab(PanelTab::PullRequest)),
                    ))
                    .when(extended, |rows| {
                        rows.child(row(
                            "launch-pull-requests",
                            "list",
                            "All Pull Requests",
                            0,
                            Some("rightPanel.pullRequests"),
                            pull_requests_unavailable,
                            Box::new(|store| store.open_tab(PanelTab::PullRequests)),
                        ))
                    })
                    .when(linear_on, |rows| {
                        rows.child(row(
                            "launch-linear",
                            "linear",
                            linear_title,
                            0,
                            Some("rightPanel.linear"),
                            linear_unavailable,
                            Box::new(|store| store.open_tab(PanelTab::Linear)),
                        ))
                    })
                    .when(ricrac_on, |rows| {
                        rows.child(row(
                            "launch-ricrac",
                            "ricrac",
                            ricrac_title,
                            0,
                            Some("rightPanel.ricrac"),
                            ricrac_unavailable,
                            Box::new(|store| store.open_tab(PanelTab::Ricrac)),
                        ))
                    }),
            )
            .into_any_element()
    }

    /// What the diff shows of the pull request besides its lines: which files were viewed and
    /// where its conversations and the comments waiting for a review are.
    fn marks(&mut self, document: &Arc<CodeDocument>, scope: &DiffScope, target: &PanelTarget, cx: &App) -> CodeMarks {
        let store = self.store.read(cx);
        let DiffScope::PullRequest { number } = scope else { return CodeMarks::default() };
        let Some(page) = store.pull_requests.shown_page(target).filter(|page| page.number == *number) else {
            return CodeMarks::default();
        };
        if !store.pull_requests_extended() {
            return CodeMarks::default();
        }
        let made_of = MarksMadeOf {
            document: Arc::as_ptr(document) as usize,
            reads: store.pull_requests.reads,
            pending: store.pull_requests.pending.get(number).into_iter().flatten().map(|comment| comment.id).collect(),
        };
        if let Some((made, marks)) = &self.marks
            && *made == made_of
        {
            return marks.clone();
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
        self.marks = Some((made_of, marks.clone()));
        marks
    }

    /// The changes in the thread's folder: what isn't committed, what the branch adds, or what
    /// one turn did.
    fn diff_surface(&mut self, target: &PanelTarget, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                        && store.pull_requests.shown_page(&load_target).map(|page| page.number) != Some(number)
                    {
                        store.load_pull_request(&load_target, number);
                    }
                    store.load_diff(&load_target, load_scope)
                },
                cx,
            );
        }
        let store = self.store.read(cx);
        let document = store.right_panel.diff.value().cloned();
        let all_closed =
            document.as_ref().is_some_and(|document| store.right_panel.collapsed.len() >= document.files.len());
        let counts = document.as_ref().filter(|document| !document.files.is_empty()).map(|document| {
            let (text, runs) = counts_text(document.added(), document.removed(), colors(cx));
            div().pl(px(4.)).text_size(px(11.5)).child(StyledText::new(text).with_runs(runs))
        });
        let viewed = match (&scope, store.pull_requests.shown_page(target)) {
            (DiffScope::PullRequest { number }, Some(page))
                if page.number == *number && store.pull_requests_extended() && !page.viewed.is_empty() =>
            {
                let done = page.viewed.iter().filter(|file| file.viewed).count();
                Some((format!("{done} of {} viewed", page.viewed.len()), page.can_review_lines))
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
                .children(counts)
                .when_some(viewed, |bar, (viewed, comments)| {
                    bar.child(
                        div()
                            .id("viewed-count")
                            .pl(px(6.))
                            .text_size(px(12.))
                            .text_color(colors(cx).muted_stronger_foreground)
                            .whitespace_nowrap()
                            .child(viewed)
                            .when(comments, |viewed| {
                                viewed.tooltip(crate::ui::tooltip("Click a line's number to comment on it"))
                            }),
                    )
                })
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
                .child(self.refresh_button("refresh-diff", "Read the changes again", &PanelTab::Diff, cx)),
            cx,
        );
        let body: AnyElement = if !target.repository {
            self.no_repository(&target.project_id, cx)
        } else if target.awaits_worktree {
            Self::message("Nothing has changed.", false, cx)
        } else {
            match &self.store.read(cx).right_panel.diff {
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
                    let document = document.clone();
                    let marks = self.marks(&document, &scope, target, cx);
                    let (viewed_target, viewed_marks) = (target.clone(), marks.clone());
                    let viewed_scope = scope.clone();
                    let actions = CodeActions::default()
                        .toggle(act_path(&self.store, |store, path| store.toggle_collapsed(path)))
                        .open(act_path(&self.store, |store, path| store.open_tab(PanelTab::File(path.to_string()))))
                        .viewed(act_path(&self.store, move |store, path| {
                            let DiffScope::PullRequest { number } = viewed_scope else { return };
                            let viewed = !viewed_marks.viewed.contains(path);
                            store.set_viewed(path, viewed, &viewed_target, number);
                        }))
                        .comment(act_line(&self.store, |store, line| store.comment_on_line(Some(line.into()))))
                        .media(load_media(&self.store, target), open_media(&self.store));
                    let code = self.diff.update(cx, |view, cx| {
                        let panel = &store.read(cx).right_panel;
                        let (collapsed, reveal, media) =
                            (panel.collapsed.clone(), panel.reveal.clone(), panel.media_files.clone());
                        view.element(
                            CodeSnapshot {
                                document: &document,
                                collapsed: &collapsed,
                                reveal: reveal.as_ref(),
                                marks: &marks,
                                media: &media,
                            },
                            actions,
                            window,
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
        // The pull request the menu offers: the one the scope shows, the thread's, or the one the
        // panel shows.
        let page = store.pull_requests.shown_page(target);
        let pull_request = match scope {
            DiffScope::PullRequest { number } => Some(*number),
            _ => target.pull_request.or(page.map(|page| page.number)),
        }
        .filter(|_| store.pull_requests_unavailable().is_none());
        let commits: Vec<(String, String)> = match (pull_request, page) {
            (Some(number), Some(page)) if page.number == number => page
                .activity
                .iter()
                .flat_map(|entry| entry.commits.iter())
                .filter(|commit| !commit.sha.is_empty())
                .map(|commit| (commit.sha.clone(), format!("{}  {}", commit.oid, commit.headline)))
                .collect(),
            _ => Vec::new(),
        };
        let first_commit = commits.first().map(|(sha, _)| DiffScope::Commit { sha: sha.clone() });
        let lists_commits = commits.len() > 1 || first_commit.as_ref() == Some(scope);
        let turns: Vec<(DiffScope, String)> = store
            .right_panel
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
                if lists_commits {
                    let mut its = Menu::new();
                    for (sha, label) in &commits {
                        let scope = DiffScope::Commit { sha: sha.clone() };
                        its = its.checked(chosen == scope, label.clone(), pick(scope));
                    }
                    menu = menu.submenu("Its Commits", its);
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
                .child(icons::symbol("folder", 11.).text_color(c.muted_foreground))
                .child(
                    div()
                        .pl(px(3.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.foreground)
                        .truncate()
                        .child(target.name.clone()),
                )
                .child(div().flex_1().min_w(px(4.)))
                .child(self.refresh_button("refresh-files", "Read the folder again", &PanelTab::Files, cx)),
            cx,
        );
        let store = self.store.read(cx);
        let body: AnyElement = if let Some(error) = &store.right_panel.files_error {
            Self::message(error.clone(), true, cx)
        } else if let Some(top) = store.right_panel.listings.get("") {
            if top.is_empty() {
                Self::message("This folder is empty.", false, cx)
            } else {
                let nodes = std::rc::Rc::new(store.right_panel.nodes());
                let store = self.store.clone();
                uniform_list("file-tree", nodes.len(), move |range, _, cx| {
                    range.map(|index| file_row(&nodes[index], &store, cx)).collect::<Vec<_>>()
                })
                .size_full()
                .pt(px(6.))
                .pb(px(super::END_ROOM))
                .into_any_element()
            }
        } else {
            Self::loading(cx)
        };
        Self::surface(bar, body)
    }

    /// The image of a tab that shows an image file, kept for as long as the tab shows it.
    fn zoomable(&mut self, tab: &PanelTab, file: &std::path::Path, cx: &mut Context<Self>) -> Entity<ZoomableImage> {
        if let Some((shown, image)) = self.zooms.get(tab)
            && shown == file
        {
            return image.clone();
        }
        let image = cx.new(|_| ZoomableImage::new(file, size(16., 16.), 0.));
        self.zooms.insert(tab.clone(), (file.to_path_buf(), image.clone()));
        image
    }

    /// A video file of the folder, which plays where it is.
    #[cfg(target_os = "macos")]
    fn video(&mut self, tab: &PanelTab, file: &std::path::Path, cx: &mut Context<Self>) -> AnyElement {
        let player = match self.players.get(tab) {
            Some((shown, player)) if shown == file => player.clone(),
            _ => {
                let player = cx.new(|cx| crate::media::video::VideoPlayer::new(file, cx));
                self.players.insert(tab.clone(), (file.to_path_buf(), player.clone()));
                player
            }
        };
        div().size_full().p(px(16.)).child(player).into_any_element()
    }

    #[cfg(not(target_os = "macos"))]
    fn video(&mut self, tab: &PanelTab, file: &std::path::Path, cx: &mut Context<Self>) -> AnyElement {
        if self.played_elsewhere.get(tab).is_none_or(|played| played != file) {
            self.played_elsewhere.insert(tab.clone(), file.to_path_buf());
            cx.open_with_system(file);
        }
        Self::message("This video opened in the app your system has for it.", false, cx)
    }

    /// The videos of tabs out of sight stop, as the Mac's do when they disappear.
    #[cfg(target_os = "macos")]
    fn pause_hidden_videos(&mut self, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let shown = store.right_panel.is_open.then(|| store.panel_tabs().active).flatten();
        let tabs = store.panel_tabs().tabs;
        self.players.retain(|tab, _| tabs.contains(tab));
        self.zooms.retain(|tab, _| tabs.contains(tab));
        for (tab, (_, player)) in &self.players {
            if Some(tab) != shown.as_ref() {
                player.update(cx, |player, _| player.pause());
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn pause_hidden_videos(&mut self, cx: &mut Context<Self>) {
        let tabs = self.store.read(cx).panel_tabs().tabs;
        self.played_elsewhere.retain(|tab, _| tabs.contains(tab));
        self.zooms.retain(|tab, _| tabs.contains(tab));
    }

    /// The code view of a tab that shows one file, kept for as long as the tab is.
    fn file_view(&mut self, tab: &PanelTab, cx: &mut Context<Self>) -> Entity<CodeView> {
        let open: Vec<PanelTab> = self.store.read(cx).panel_tabs().tabs;
        self.files.retain(|kept, _| open.contains(kept));
        self.files.entry(tab.clone()).or_insert_with(|| cx.new(CodeView::new)).clone()
    }

    /// One file of the folder the thread works in.
    fn file_surface(
        &mut self,
        target: &PanelTarget,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let tab = PanelTab::File(path.to_string());
        let copied = self.copied.as_ref() == Some(&tab);
        let (copy_path, copied_tab, this) = (path.to_string(), tab.clone(), cx.entity().downgrade());
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child({
                    let folder = folder.map(|folder| format!("{folder}/")).unwrap_or_default();
                    let muted = HighlightStyle { color: Some(c.muted_foreground), ..Default::default() };
                    let title = StyledText::new(format!("{folder}{name}")).with_highlights([(0..folder.len(), muted)]);
                    div()
                        .min_w_0()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis_start()
                        .child(title)
                })
                .child(div().flex_1().min_w(px(4.)))
                .child(
                    ActionButton::icon("copy-path", if copied { "check" } else { "copy" }, "Copy the path").on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()));
                            let copied_tab = copied_tab.clone();
                            let _ = this.update(cx, |this, cx| {
                                this.copied = Some(copied_tab.clone());
                                cx.notify();
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor().timer(Duration::from_millis(1200)).await;
                                    let _ = this.update(cx, |this, cx| {
                                        if this.copied.as_ref() == Some(&copied_tab) {
                                            this.copied = None;
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                            });
                        },
                    ),
                )
                .child(self.refresh_button("refresh-file", "Read the file again", &tab, cx)),
            cx,
        );
        let content = self.store.read(cx).right_panel.contents.get(&tab).cloned();
        let body: AnyElement = match content {
            None | Some(Loaded::Loading) => Self::loading(cx),
            Some(Loaded::Failed(message)) => Self::message(message, true, cx),
            Some(Loaded::Ready(FileContent::Text(document, truncated))) => {
                if document.files.first().is_some_and(|file| file.lines.is_empty()) {
                    Self::message("This file is empty.", false, cx)
                } else {
                    let view = self.file_view(&tab, cx);
                    let (collapsed, marks, media) = (Default::default(), CodeMarks::default(), HashMap::new());
                    let code = view.update(cx, |view, cx| {
                        view.element(
                            CodeSnapshot {
                                document: &document,
                                collapsed: &collapsed,
                                reveal: None,
                                marks: &marks,
                                media: &media,
                            },
                            CodeActions::default(),
                            window,
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
            Some(Loaded::Ready(FileContent::Image(file))) => {
                let image = self.zoomable(&tab, &file, cx);
                if self.store.read(cx).viewing.is_none() {
                    ZoomableImage::take_keys(Some(&image), cx);
                }
                div().size_full().child(image).into_any_element()
            }
            Some(Loaded::Ready(FileContent::Video(file))) => self.video(&tab, &file, cx),
            Some(Loaded::Ready(FileContent::Binary(size))) => Self::message(
                format!("This file can't be shown here. It is {}.", crate::composer::attachments::file_size(size)),
                false,
                cx,
            ),
        };
        Self::surface(bar, body)
    }

    /// What one turn changed in one file.
    fn change_surface(
        &mut self,
        target: &PanelTarget,
        turn: &str,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                        .text_color(c.foreground)
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
        let content = store.right_panel.contents.get(&tab).cloned();
        let body: AnyElement = match content {
            None | Some(Loaded::Loading) => Self::loading(cx),
            Some(Loaded::Failed(message)) => Self::message(message, true, cx),
            Some(Loaded::Ready(FileContent::Text(document, truncated))) if !document.files.is_empty() => {
                let view = self.file_view(&tab, cx);
                let closed = self.change_closed.as_ref() == Some(&tab);
                let collapsed = Rc::new(if closed { [path.to_string()].into() } else { HashSet::new() });
                let marks = CodeMarks::default();
                let (this, toggled) = (cx.entity().downgrade(), tab.clone());
                let actions = CodeActions::default()
                    .toggle(move |_, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.change_closed = if closed { None } else { Some(toggled.clone()) };
                            cx.notify();
                        });
                    })
                    .open(act_path(&self.store, |store, path| store.open_tab(PanelTab::File(path.to_string()))))
                    .media(load_media(&self.store, target), open_media(&self.store));
                let media = self.store.read(cx).right_panel.media_files.clone();
                let code = view.update(cx, |view, cx| {
                    view.element(
                        CodeSnapshot {
                            document: &document,
                            collapsed: &collapsed,
                            reveal: None,
                            marks: &marks,
                            media: &media,
                        },
                        actions,
                        window,
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

    /// The agents the thread's agent has started, the ones at work first and the ones that ended
    /// folded under them, and the thread whose agent started this one.
    fn agents_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let agents = store.agents.clone();
        let parent = store.parent_thread().map(|thread| (thread.id.clone(), thread.title.clone()));
        let shown_agent = store.right_panel.shown_agent.clone();
        if agents.is_empty() && parent.is_none() {
            return Self::message("The agents this thread starts show up here.", false, cx);
        }
        let said = if agents.is_empty() { StyledText::new("No agents") } else { summary(&agents, c) };
        let bar = Self::bar(
            div().flex().w_full().child(
                div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(c.muted_foreground).child(said),
            ),
            cx,
        );
        let (working, ended): (Vec<AgentView>, Vec<AgentView>) = agents.into_iter().partition(|agent| agent.working());
        let shown = self.shows_ended.unwrap_or(working.is_empty());
        let mut entries: Vec<AgentEntry> =
            parent.into_iter().map(|(id, title)| AgentEntry::Parent { id, title }).collect();
        entries.extend(working.into_iter().map(|agent| AgentEntry::Agent(Box::new(agent))));
        if !ended.is_empty() {
            let failed = ended.iter().filter(|agent| agent.status == ToolStatus::Failed).count();
            entries.push(AgentEntry::Ended { count: ended.len(), failed, shown });
            if shown {
                entries.extend(ended.into_iter().map(|agent| AgentEntry::Agent(Box::new(agent))));
            }
        }
        crate::ui::relist(&self.agents, &mut self.listed_agents, entries.iter().map(AgentEntry::key).collect());
        let (entries, store, view) = (Rc::new(entries), self.store.clone(), cx.entity().downgrade());
        let hovered = self.hovered_agent.clone();
        let list = list(self.agents.clone(), move |index, _, cx| {
            let Some(entry) = entries.get(index) else { return div().into_any_element() };
            match entry {
                AgentEntry::Parent { id, title } => parent_row(id, title, &store, cx).into_any_element(),
                AgentEntry::Ended { count, failed, shown } => {
                    let (view, shown) = (view.clone(), *shown);
                    ended_header(*count, *failed, shown, cx)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.shows_ended = Some(!shown);
                                cx.notify();
                            });
                        })
                        .into_any_element()
                }
                AgentEntry::Agent(agent) => {
                    let selected = shown_agent.as_ref() == Some(&agent.id);
                    let hovered = hovered.as_ref() == Some(&agent.id);
                    agent_row(agent, selected, hovered, &store, &view, cx).into_any_element()
                }
            }
        })
        .size_full()
        .px(px(8.))
        .pt(px(8. - super::ROW_MARGIN))
        .pb(px(super::END_ROOM - super::ROW_MARGIN));
        Self::surface(bar, list.into_any_element())
    }

    /// The issues of a Linear workspace the server is connected to, or one issue of it.
    fn linear_tab(
        &mut self,
        tab: PanelTab,
        workspace: String,
        issue: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(target) = self.store.read(cx).panel_target() else {
            return div().into_any_element();
        };
        let wanted = (target.key, tab);
        let view = match &self.linear {
            Some((shown, view)) if *shown == wanted => view.clone(),
            _ => {
                let store = self.store.clone();
                let view = cx.new(|cx| LinearView::new(store, workspace, issue, target.project_id, window, cx));
                self.linear = Some((wanted, view.clone()));
                view
            }
        };
        div().size_full().child(view).into_any_element()
    }

    /// The issues of a Ricrac workspace the server is connected to, or one issue of it.
    fn ricrac_tab(
        &mut self,
        tab: PanelTab,
        workspace: String,
        issue: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(target) = self.store.read(cx).panel_target() else {
            return div().into_any_element();
        };
        let wanted = (target.key, tab);
        let view = match &self.ricrac {
            Some((shown, view)) if *shown == wanted => view.clone(),
            _ => {
                let store = self.store.clone();
                let view = cx.new(|cx| RicracView::new(store, workspace, issue, target.project_id, window, cx));
                self.ricrac = Some((wanted, view.clone()));
                view
            }
        };
        div().size_full().child(view).into_any_element()
    }

    fn pull_request_tab(
        &mut self,
        tab: PanelTab,
        number: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(key) = self.store.read(cx).panel_target().map(|target| target.key) else {
            return div().into_any_element();
        };
        let wanted = (key, tab);
        let view = match &self.pull_request {
            Some((shown, view)) if *shown == wanted => view.clone(),
            _ => {
                let store = self.store.clone();
                let view = cx.new(|cx| PullRequestView::new(store, number, window, cx));
                self.pull_request = Some((wanted, view.clone()));
                view
            }
        };
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

    /// Lets go of what the tabs that aren't shown kept, as the Mac's views go once another tab
    /// is shown: their drafts, their editors and what was closed in them.
    fn prune_tabs(&mut self, cx: &App) {
        let store = self.store.read(cx);
        let active = store.panel_target().map(|target| target.key).zip(store.panel_tabs().active);
        let shown = |tab: &TabOf| Some(tab) == active.as_ref();
        if !self.pull_request.as_ref().is_some_and(|(tab, _)| shown(tab)) {
            self.pull_request = None;
        }
        if !self.linear.as_ref().is_some_and(|(tab, _)| shown(tab)) {
            self.linear = None;
        }
        if !self.ricrac.as_ref().is_some_and(|(tab, _)| shown(tab)) {
            self.ricrac = None;
        }
        let active = active.map(|(_, tab)| tab);
        if active != Some(PanelTab::PullRequests) {
            self.pull_request_list = None;
        }
        if self.change_closed.is_some() && self.change_closed != active {
            self.change_closed = None;
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
                    .child(icons::symbol("linear", 28.).text_color(c.foreground))
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
                                    .text_color(c.foreground)
                                    .child("Connect Linear"),
                            )
                            .child(div().text_size(px(12.5)).text_color(c.muted_foreground).text_center().child(
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
                        prompt.child(
                            div().w_full().text_size(px(12.5)).text_color(c.destructive).text_center().child(error),
                        )
                    }),
            )
            .into_any_element()
    }

    fn ricrac_surface(&mut self, target: &PanelTarget, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        if let Some(reason) = store.ricrac_unavailable() {
            return Self::message(reason, false, cx);
        }
        if !store.ricrac.connected(&target.server_id).is_empty() {
            return self.ricrac_tab(PanelTab::Ricrac, String::new(), None, window, cx);
        }
        let trigger = Trigger { target: target.clone(), scope: None, path: None, version: 0, asked: 0 };
        let read_id = target.server_id.clone();
        self.follow("ricrac", trigger, move |store| store.ricrac_read(&read_id), cx);
        let store = self.store.read(cx);
        let server_id = target.server_id.clone();
        let connecting = store.ricrac.connecting.as_deref() == Some(&target.server_id);
        let error = store.ricrac.error.clone();
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
                    .child(icons::symbol("ricrac", 28.).text_color(c.foreground))
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
                                    .text_color(c.foreground)
                                    .child("Connect Ricrac"),
                            )
                            .child(div().text_size(px(12.5)).text_color(c.muted_foreground).text_center().child(
                                "See your issues and hand them to your agents. The connection is kept on your server.",
                            )),
                    )
                    .child(
                        ActionButton::new("connect-ricrac", "Connect Ricrac")
                            .variant(Variant::Primary)
                            .pending(connecting)
                            .on_click(act(&self.store, move |store, _| store.ricrac_connect(&server_id))),
                    )
                    .when_some(error, |prompt, error| {
                        prompt.child(
                            div().w_full().text_size(px(12.5)).text_color(c.destructive).text_center().child(error),
                        )
                    }),
            )
            .into_any_element()
    }
}

/// A file or a folder of the files tab: a folder opens and closes, a file opens in a tab.
fn file_row(node: &FileNode, store: &Entity<Store>, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let group: SharedString = format!("node-{}", node.path).into();
    let (path, folder, copy) = (node.path.clone(), node.folder, node.path.clone());
    div()
        .id(SharedString::from(format!("file-{}", node.path)))
        .group(group.clone())
        .relative()
        .w_full()
        .h(px(26.))
        .pl(px(12. + node.depth as f32 * 14.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(light(group, Radius::SM, edges(0., 6., 0., 6.), false, cx))
        .child(div().relative().w(px(10.)).flex().justify_center().when(node.folder, |chevron| {
            chevron.child(
                icons::symbol(if node.open { "chevron-down" } else { "chevron-right" }, 8.)
                    .text_color(c.muted_stronger_foreground),
            )
        }))
        .child(
            div().relative().w(px(16.)).flex().justify_center().child(
                icons::symbol(if node.folder { "folder" } else { file_symbol(&node.path) }, 11.)
                    .text_color(if node.ignored { c.muted_strongest_foreground } else { c.muted_foreground }),
            ),
        )
        .child(
            crate::ui::middle_truncated(node.name.clone()).relative().text_size(px(12.5)).text_color(if node.ignored {
                c.muted_strongest_foreground
            } else {
                c.foreground
            }),
        )
        .on_click(act(store, move |store, _| {
            if folder {
                store.toggle_folder(&path);
            } else {
                store.open_tab(PanelTab::File(path.clone()));
            }
        }))
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            let path = copy.clone();
            Menu::new()
                .item("Copy Path", move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(path.clone())))
                .show(event.position, window, cx);
        })
}

/// Fetches the file of an image or a video a code view is to draw.
fn load_media(
    store: &Entity<Store>,
    target: &PanelTarget,
) -> impl Fn(&str, &CodeMedia, &mut Window, &mut App) + 'static {
    let (store, target) = (store.clone(), target.clone());
    move |path, media, _, cx| {
        store.update(cx, |store, cx| {
            store.load_media(path, media, &target);
            cx.notify();
        })
    }
}

/// Opens an image or a video a code view shows in the viewer.
fn open_media(store: &Entity<Store>) -> impl Fn(&str, &CodeMedia, &mut Window, &mut App) + 'static {
    let store = store.clone();
    move |path, media, _, cx| {
        store.update(cx, |store, cx| {
            store.view_diff_media(path, media);
            cx.notify();
        })
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

/// A row of the agents tab.
enum AgentEntry {
    /// The thread whose agent started this one.
    Parent {
        id: String,
        title: String,
    },
    Agent(Box<AgentView>),
    /// Over the agents that ended, which it folds.
    Ended {
        count: usize,
        failed: usize,
        shown: bool,
    },
}

impl AgentEntry {
    fn key(&self) -> String {
        match self {
            Self::Parent { id, title } => format!("parent:{id}:{title}"),
            Self::Ended { count, failed, shown } => format!("ended:{count}:{failed}:{shown}"),
            Self::Agent(agent) => format!(
                "agent:{}:{}:{:?}:{:?}:{:?}:{}:{:?}:{}:{}:{}",
                agent.id,
                agent.title,
                agent.status,
                agent.state,
                agent.agent,
                agent.label,
                agent.kind,
                agent.detail,
                agent.facts().unwrap_or_default(),
                agent.parent.is_some()
            ),
        }
    }
}

/// The thread whose agent started this one, which a click opens.
fn parent_row(id: &str, title: &str, store: &Entity<Store>, cx: &App) -> Div {
    let c = colors(cx);
    let group: SharedString = format!("agents-parent-{id}").into();
    let id = id.to_string();
    div().pb(px(4.)).child(
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .px(px(10.))
            .py(px(8. + super::ROW_MARGIN))
            .child(light(group, Radius::MD, edges(super::ROW_MARGIN, 0., super::ROW_MARGIN, 0.), false, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .child(
                        div()
                            .w(px(16.))
                            .flex()
                            .justify_center()
                            .child(icons::symbol("corner-left-up", 12.).text_color(c.muted_foreground)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(px(11.)).text_color(c.muted_stronger_foreground).child("Started by"))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c.foreground)
                                    .truncate()
                                    .child(title.to_string()),
                            ),
                    ),
            )
            .on_click(act(store, move |store, cx| store.open_started_thread(&id, cx))),
    )
}

/// Over the agents that ended: how many, and how many of them failed. A click folds them.
fn ended_header(count: usize, failed: usize, shown: bool, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let group: SharedString = "agents-ended".into();
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .mt(px(6.))
        .px(px(10.))
        .py(px(super::ROW_MARGIN))
        .child(light(group, Radius::MD, edges(super::ROW_MARGIN, 0., super::ROW_MARGIN, 0.), false, cx))
        .child(
            div()
                .relative()
                .h(px(ControlSize::Regular.height()))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(c.foreground).child("Previous agents"))
                .child(div().text_color(c.muted_stronger_foreground).child(count.to_string()))
                .when(failed > 0, |line| line.child(div().text_color(c.destructive).child(format!("{failed} failed"))))
                .child(div().flex_1().min_w(px(6.)))
                .child(
                    icons::symbol(if shown { "chevron-down" } else { "chevron-right" }, 10.)
                        .text_color(c.muted_stronger_foreground),
                ),
        )
}

/// An agent the thread's agent started: how it is doing, what it is doing or said last, and what
/// it runs on and has spent. A click opens its thread, or what it did in the thread's place. One
/// that works in a thread of its own can be stopped from here.
fn agent_row(
    agent: &AgentView,
    selected: bool,
    hovered: bool,
    store: &Entity<Store>,
    view: &WeakEntity<RightPanelView>,
    cx: &App,
) -> Stateful<Div> {
    let c = colors(cx);
    let group: SharedString = format!("agent-{}", agent.id).into();
    let stoppable = agent.working() && agent.thread_id.is_some();
    let open = {
        let (id, thread) = (agent.id.clone(), agent.thread_id.clone());
        move |store: &mut Store, cx: &mut Context<Store>| match &thread {
            Some(thread) => store.open_started_thread(thread, cx),
            None => store.show_agent(id.clone(), cx),
        }
    };
    let stop = {
        let thread = agent.thread_id.clone();
        move |store: &mut Store, cx: &mut Context<Store>| {
            if let Some(thread) = &thread {
                store.stop_thread_id(thread, cx);
            }
        }
    };
    let (hover_view, hover_id) = (view.clone(), agent.id.clone());
    let menu_store = store.clone();
    let (menu_open, menu_stop) = (open.clone(), stop.clone());
    let menu_title = if agent.thread_id.is_none() { "Show What It Did" } else { "Open Thread" };
    // The stop button is as far from the row's top as the title's line is.
    let stop_top = 8. - (ControlSize::Small.height() - 18.) / 2. + super::ROW_MARGIN;
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .px(px(10.))
        .py(px(8. + super::ROW_MARGIN))
        // An agent that another agent started stands in from it.
        .when(agent.parent.is_some(), |row| row.pl(px(28.)))
        .child(light(group, Radius::MD, edges(super::ROW_MARGIN, 0., super::ROW_MARGIN, 0.), selected, cx))
        .child(
            div()
                .relative()
                .flex()
                .items_start()
                .gap(px(10.))
                .child(AgentAvatar::new(agent.agent).state(agent.state))
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
                                        .text_color(c.foreground)
                                        .truncate()
                                        .child(agent.title.clone()),
                                )
                                .when_some(agent.kind.clone(), |line, kind| {
                                    line.child(
                                        div()
                                            .min_w_0()
                                            .text_size(px(11.))
                                            .text_color(c.muted_foreground)
                                            .truncate()
                                            .child(kind),
                                    )
                                })
                                .when(agent.state.flagged(), |line| {
                                    line.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(state_color(agent.state, c))
                                            .child(agent.label.clone()),
                                    )
                                })
                                .child(div().flex_1().min_w(px(6.)))
                                .child(
                                    div()
                                        .when(hovered && stoppable, |time| time.opacity(0.))
                                        .child(agent_time(agent, cx)),
                                ),
                        )
                        // Always one line, so that a row keeps its height as what its agent does changes.
                        .child(
                            div()
                                .h(px(16.))
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(if agent.status == ToolStatus::Failed {
                                    c.destructive
                                } else {
                                    c.muted_foreground
                                })
                                .truncate()
                                .child(agent.detail.clone()),
                        )
                        .when_some(agent.facts(), |text, facts| {
                            text.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(c.muted_stronger_foreground)
                                    .truncate()
                                    .child(facts),
                            )
                        }),
                ),
        )
        .when(hovered && stoppable, |row| {
            row.child(
                div().absolute().top(px(stop_top)).right(px(6.)).child(
                    ActionButton::icon(
                        SharedString::from(format!("stop-agent-{}", agent.id)),
                        "circle-stop",
                        format!("Stop {}", agent.title),
                    )
                    .small()
                    .on_click(act(store, stop.clone())),
                ),
            )
        })
        .on_hover(move |hovering, _, cx| {
            let id = hover_id.clone();
            let _ = hover_view.update(cx, |view, cx| {
                let hovered =
                    if *hovering { Some(id) } else { view.hovered_agent.clone().filter(|other| *other != id) };
                if view.hovered_agent != hovered {
                    view.hovered_agent = hovered;
                    cx.notify();
                }
            });
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            let mut menu = Menu::new().item(menu_title, choose(&menu_store, menu_open.clone()));
            if stoppable {
                menu = menu.item("Stop Agent", choose(&menu_store, menu_stop.clone()));
            }
            menu.show(event.position, window, cx);
        })
        .on_click(act(store, open))
}

/// How long the agent has worked so far, or worked in all.
pub fn agent_time(agent: &AgentView, cx: &App) -> AnyElement {
    let c = colors(cx);
    let time = |text: String, color: Hsla| {
        div()
            .flex_shrink_0()
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .font_features(crate::ui::tabular_numbers())
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    if agent.working() && agent.state != AgentState::Limited {
        return time(elapsed(agent.started_at), c.process);
    }
    match agent.duration_ms {
        Some(milliseconds) => time(duration(milliseconds), c.muted_stronger_foreground),
        None => div().into_any_element(),
    }
}

/// "1 waiting · 2 working · 1 done · 48K tokens", leaving out what there is none of, each count
/// in its state's colour.
fn summary(agents: &[AgentView], c: &Colors) -> StyledText {
    let count = |state: AgentState| agents.iter().filter(|agent| agent.state == state).count();
    let spent: Vec<u64> = agents.iter().filter_map(|agent| agent.tokens).collect();
    let total = (!spent.is_empty()).then(|| (crate::models::tokens(spent.iter().sum()), None));
    let parts = [
        (AgentState::Waiting, "waiting"),
        (AgentState::Working, "working"),
        (AgentState::Limited, "limited"),
        (AgentState::Done, "done"),
        (AgentState::Failed, "failed"),
        (AgentState::Stopped, "stopped"),
    ]
    .into_iter()
    .filter(|(state, _)| count(*state) > 0)
    .map(|(state, word)| (format!("{} {word}", count(state)), Some(state)))
    .chain(total);
    let (mut text, mut highlights) = (String::new(), Vec::new());
    let tint = |color| HighlightStyle { color: Some(color), ..Default::default() };
    for (index, (part, state)) in parts.enumerate() {
        if index > 0 {
            highlights.push((text.len()..text.len() + " · ".len(), tint(c.muted_stronger_foreground)));
            text.push_str(" · ");
        }
        if let Some(state) = state {
            highlights
                .push((text.len()..text.len() + part.len(), tint(crate::ui::agent_avatar::state_color(state, c))));
        }
        text.push_str(&part);
    }
    StyledText::new(text).with_highlights(highlights)
}

impl Render for RightPanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        self.prune_tabs(cx);
        ZoomableImage::take_keys(None, cx);
        let line_comment = line_comment::follow(&mut self.line_comment, &self.store, window, cx);
        let store = self.store.read(cx);
        let unavailable = store.panel_unavailable();
        let target = store.panel_target();
        let active = store.panel_tabs().active;
        let content: AnyElement = if let Some(reason) = unavailable {
            Self::message(reason, false, cx)
        } else if let Some(target) = target {
            match active {
                Some(PanelTab::Diff) => self.diff_surface(&target, window, cx),
                Some(PanelTab::Files) => self.files_surface(&target, cx),
                Some(PanelTab::File(path)) => self.file_surface(&target, &path, window, cx),
                Some(PanelTab::Change { turn, path }) => self.change_surface(&target, &turn, &path, window, cx),
                Some(PanelTab::Agents) => self.agents_surface(cx),
                Some(tab @ PanelTab::PullRequest) => self.pull_request_tab(tab, None, window, cx),
                Some(tab @ PanelTab::PullRequestNumber(number)) => self.pull_request_tab(tab, Some(number), window, cx),
                Some(PanelTab::PullRequests) => self.pull_request_list_tab(window, cx),
                Some(PanelTab::Linear) => self.linear_surface(&target, window, cx),
                Some(tab @ PanelTab::LinearIssue { .. }) => {
                    let PanelTab::LinearIssue { workspace, id, .. } = tab.clone() else { unreachable!() };
                    self.linear_tab(tab, workspace, Some(id), window, cx)
                }
                Some(PanelTab::Ricrac) => self.ricrac_surface(&target, window, cx),
                Some(tab @ PanelTab::RicracIssue { .. }) => {
                    let PanelTab::RicracIssue { workspace, id, .. } = tab.clone() else { unreachable!() };
                    self.ricrac_tab(tab, workspace, Some(id), window, cx)
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
