//! The panel beside the thread: its tabs in the window's top bar, and under them what the open
//! tab shows of the folder the thread works in.

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentView;
use motile_protocol::wire::{DiffScope, ToolStatus};

use super::code_view::{CodeSnapshot, CodeView, counts_text};
use super::state::{FileContent, Loaded, PanelTab, PanelTarget, file_symbol};
use crate::models::{AgentViewExt, duration, elapsed};
use crate::store::Store;
use crate::theme::{self, colors};
use crate::transcript::view::TranscriptView;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{IconButton, TOOLBAR_WIDTH, edges, highlight, icons, spinner};

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
    /// How far the tabs start from the panel's left edge: past the window's buttons when the
    /// panel reaches them.
    pub tab_inset: f32,
    diff: Entity<CodeView>,
    files: HashMap<String, Entity<CodeView>>,
    agent: Entity<TranscriptView>,
    asked: u64,
    /// What each tab last asked its server with, by tab.
    triggers: HashMap<String, Trigger>,
    add_menu: Anchor,
    scope_menu: Anchor,
    hovered_tab: Option<String>,
    copied: bool,
    _ticks: Task<()>,
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
            tab_inset: 0.,
            diff: cx.new(CodeView::new),
            files: HashMap::new(),
            agent,
            asked: 0,
            triggers: HashMap::new(),
            add_menu: Anchor::default(),
            scope_menu: Anchor::default(),
            hovered_tab: None,
            copied: false,
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

    fn tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let tabs = self.store.read(cx).panel_tabs();
        let repository = self.store.read(cx).panel_target().is_some_and(|target| target.repository);
        let anchor = self.add_menu.clone();
        let store = self.store.clone();
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
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .children(tabs.tabs.iter().map(|tab| {
                        let active = tabs.active.as_ref() == Some(tab);
                        let key = tab.id();
                        let hovering = self.hovered_tab.as_ref() == Some(&key);
                        let group: SharedString = format!("tab-{key}").into();
                        let (activate, close, menu) = (self.store.clone(), self.store.clone(), self.store.clone());
                        let (activate_tab, close_tab, menu_tab) = (tab.clone(), tab.clone(), tab.clone());
                        let path = match tab {
                            PanelTab::File(path) => Some(path.clone()),
                            _ => None,
                        };
                        let hover_key = key.clone();
                        div()
                            .id(SharedString::from(key.clone()))
                            .group(group.clone())
                            .relative()
                            .flex_shrink_0()
                            .max_w(px(180.))
                            .h(px(28.))
                            .pl(px(9.))
                            .pr(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(highlight(group, 7., edges(0., 0., 0., 0.), active, cx))
                            .child(div().relative().w(px(14.)).flex().justify_center().child(
                                icons::symbol(tab.symbol(), 11.).text_color(if active { c.text } else { c.secondary }),
                            ))
                            .child(
                                div()
                                    .relative()
                                    .min_w_0()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if active { c.text } else { c.secondary })
                                    .truncate()
                                    .child(tab.title()),
                            )
                            .child(
                                div().relative().when(!(hovering || active), |close| close.invisible()).child(
                                    IconButton::new(SharedString::from(format!("close-{key}")), "xmark")
                                        .help("Close (⌘W)")
                                        .size(16.)
                                        .symbol_size(8.)
                                        .radius(4.)
                                        .color(c.secondary)
                                        .on_click(move |_, _, cx| {
                                            let tab = close_tab.clone();
                                            close.update(cx, |store, cx| {
                                                store.close_tab(&tab);
                                                cx.notify();
                                            })
                                        }),
                                ),
                            )
                            .tooltip(crate::ui::tooltip(path.clone().unwrap_or(tab.title())))
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.hovered_tab = Some(hover_key.clone());
                                } else if this.hovered_tab.as_ref() == Some(&hover_key) {
                                    this.hovered_tab = None;
                                }
                                cx.notify();
                            }))
                            .on_click(move |_, _, cx| {
                                let tab = activate_tab.clone();
                                activate.update(cx, |store, cx| {
                                    store.activate_tab(tab);
                                    cx.notify();
                                })
                            })
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                let (close, others, all) = (menu.clone(), menu.clone(), menu.clone());
                                let (close_tab, other_tab) = (menu_tab.clone(), menu_tab.clone());
                                let copy = path.clone();
                                Menu::new()
                                    .item("Close", move |_, cx| {
                                        close.update(cx, |store, cx| {
                                            store.close_tab(&close_tab);
                                            cx.notify();
                                        })
                                    })
                                    .item("Close Others", move |_, cx| {
                                        others.update(cx, |store, cx| {
                                            store.close_other_tabs(&other_tab);
                                            cx.notify();
                                        })
                                    })
                                    .item("Close All", move |_, cx| {
                                        all.update(cx, |store, cx| {
                                            store.close_all_tabs();
                                            cx.notify();
                                        })
                                    })
                                    .when_some(copy, |menu, path| {
                                        menu.separator().item("Copy Path", move |_, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                                        })
                                    })
                                    .show(event.position, window, cx);
                            })
                    }))
                    .when(!tabs.tabs.is_empty(), |strip| {
                        strip.child(
                            div().relative().child(anchor.track()).child(
                                IconButton::new("add-tab", "plus")
                                    .help("Open a tab")
                                    .size(28.)
                                    .symbol_size(12.)
                                    .radius(7.)
                                    .color(c.secondary)
                                    .on_click(move |_, window, cx| {
                                        let (files, diff, agents) = (store.clone(), store.clone(), store.clone());
                                        Menu::new()
                                            .item("Files", move |_, cx| {
                                                files.update(cx, |store, cx| {
                                                    store.open_tab(PanelTab::Files);
                                                    cx.notify();
                                                })
                                            })
                                            .item_if(repository, "Diff", move |_, cx| {
                                                diff.update(cx, |store, cx| {
                                                    store.show_diff(None, None);
                                                    cx.notify();
                                                })
                                            })
                                            .item("Agents", move |_, cx| {
                                                agents.update(cx, |store, cx| {
                                                    store.open_tab(PanelTab::Agents);
                                                    cx.notify();
                                                })
                                            })
                                            .show(anchor.below(), window, cx);
                                    }),
                            ),
                        )
                    }),
            )
    }

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
        div().size_full().flex().items_center().justify_center().child(spinner(14., cx)).into_any_element()
    }

    /// The strip under the tabs with what the open tab is about and its buttons.
    fn bar(content: impl IntoElement, cx: &App) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(div().h(px(36.)).pl(px(12.)).pr(px(6.)).flex().items_center().gap(px(4.)).child(content))
            .child(crate::ui::divider(cx))
    }

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

    fn refresh_button(id: &'static str, help: &'static str, cx: &mut Context<Self>) -> IconButton {
        let c = colors(cx);
        let this = cx.entity().downgrade();
        IconButton::new(id, "arrow.clockwise").help(help).color(c.secondary).on_click(move |_, _, cx| {
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        })
    }

    fn launcher(&self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let row = |id: &'static str,
                   symbol: &'static str,
                   title: &'static str,
                   keys: &'static str,
                   reason: Option<&'static str>,
                   tab: Option<PanelTab>,
                   store: Entity<Store>| {
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
                .when(enabled, |row| row.child(highlight(group, 7., edges(0., 0., 0., 0.), false, cx)))
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
                .child(
                    div()
                        .relative()
                        .h(px(20.))
                        .px(px(6.))
                        .rounded(px(5.))
                        .bg(c.hover)
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.secondary)
                        .child(keys),
                )
                .when_some(reason, |row, reason| row.tooltip(crate::ui::tooltip(reason)))
                .when(enabled, |row| {
                    row.on_click(move |_, _, cx| {
                        let tab = tab.clone();
                        store.update(cx, |store, cx| {
                            match tab {
                                Some(tab) => store.open_tab(tab),
                                None => store.show_diff(None, None),
                            }
                            cx.notify();
                        })
                    })
                })
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child("Open a tab"))
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
                        "⇧⌘E",
                        None,
                        Some(PanelTab::Files),
                        self.store.clone(),
                    ))
                    .child(row(
                        "launch-diff",
                        "plusminus",
                        "Diff",
                        "⌘D",
                        (!target.repository).then_some("Available in git repositories."),
                        None,
                        self.store.clone(),
                    ))
                    .child(row(
                        "launch-agents",
                        "person.2",
                        "Agents",
                        "⇧⌘A",
                        None,
                        Some(PanelTab::Agents),
                        self.store.clone(),
                    )),
            )
            .into_any_element()
    }

    fn diff_surface(&mut self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let scope = store.diff_scope(target);
        let trigger = Trigger {
            target: target.clone(),
            scope: Some(scope.clone()),
            path: None,
            version: store.workspace_version,
            asked: self.asked,
        };
        if target.repository {
            let (load_target, load_scope) = (target.clone(), scope.clone());
            self.follow("diff", trigger, move |store| store.load_diff(&load_target, load_scope), cx);
        }
        let store = self.store.read(cx);
        let document = store.side_panel.diff.value().cloned();
        let turns = store.side_panel.turns.clone();
        let all_closed =
            document.as_ref().is_some_and(|document| store.side_panel.collapsed.len() >= document.files.len());
        let title = match &scope {
            DiffScope::Uncommitted => "Uncommitted".to_string(),
            DiffScope::Branch => "Branch".to_string(),
            DiffScope::Turn { item_id } => match turns.iter().find(|turn| &turn.id == item_id) {
                Some(turn) if Some(&turn.id) != turns.last().map(|last| &last.id) => {
                    format!("Turn at {}", clock(turn.at))
                }
                _ => "Latest turn".to_string(),
            },
        };
        let anchor = self.scope_menu.clone();
        let menu_store = self.store.clone();
        let chosen = scope.clone();
        let menu_turns = turns.clone();
        let scope_menu = div()
            .id("scope")
            .group("scope")
            .relative()
            .ml(px(-8.))
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(5.))
            .child(highlight("scope", 7., edges(0., 0., 0., 0.), false, cx))
            .child(anchor.track())
            .child(div().relative().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(c.text).child(title))
            .child(div().relative().child(icons::symbol("chevron.down", 8.).text_color(c.tertiary)))
            .on_click(move |_, window, cx| {
                let choose = |scope: DiffScope| {
                    let store = menu_store.clone();
                    move |_: &mut Window, cx: &mut App| {
                        store.update(cx, |store, cx| {
                            store.choose_diff_scope(scope.clone());
                            cx.notify();
                        })
                    }
                };
                let mut menu = Menu::new()
                    .checked(chosen == DiffScope::Uncommitted, "Uncommitted changes", choose(DiffScope::Uncommitted))
                    .checked(chosen == DiffScope::Branch, "Branch changes", choose(DiffScope::Branch));
                if !menu_turns.is_empty() {
                    menu = menu.separator();
                    let latest = menu_turns.last().map(|turn| turn.id.clone());
                    for turn in menu_turns.iter().rev() {
                        let name = if Some(&turn.id) == latest.as_ref() {
                            "Latest turn".to_string()
                        } else {
                            format!("Turn at {}", clock(turn.at))
                        };
                        let files =
                            if turn.files == 1 { "1 file".to_string() } else { format!("{} files", turn.files) };
                        let scope = DiffScope::Turn { item_id: turn.id.clone() };
                        menu = menu.checked(chosen == scope, format!("{name} · {files}"), choose(scope.clone()));
                    }
                }
                menu.show(anchor.below(), window, cx);
            });
        let counts = document.as_ref().filter(|document| !document.files.is_empty()).map(|document| {
            let (text, runs) = counts_text(document.added(), document.removed(), colors(cx));
            div().pl(px(4.)).text_size(px(11.5)).child(StyledText::new(text).with_runs(runs))
        });
        let store_handle = self.store.clone();
        let several = document.as_ref().is_some_and(|document| document.files.len() > 1);
        let bar = Self::bar(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(scope_menu)
                .children(counts)
                .child(div().flex_1())
                .when(several, |bar| {
                    bar.child(
                        IconButton::new(
                            "collapse-all",
                            if all_closed { "rectangle.expand.vertical" } else { "rectangle.compress.vertical" },
                        )
                        .help(if all_closed { "Open every file" } else { "Close every file" })
                        .color(c.secondary)
                        .on_click(move |_, _, cx| {
                            store_handle.update(cx, |store, cx| {
                                store.set_all_collapsed(!all_closed);
                                cx.notify();
                            })
                        }),
                    )
                })
                .child(Self::refresh_button("refresh-diff", "Read the changes again", cx)),
            cx,
        );
        let body: AnyElement = if !target.repository {
            Self::message("This folder isn't a git repository.", false, cx)
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
                    let (toggle, open) = (self.store.clone(), self.store.clone());
                    let store = self.store.clone();
                    let code = self.diff.update(cx, |view, cx| {
                        let state = store.read(cx);
                        let Loaded::Ready(document) = &state.side_panel.diff else { return div().into_any_element() };
                        let snapshot = CodeSnapshot {
                            document,
                            collapsed: &state.side_panel.collapsed,
                            reveal: state.side_panel.reveal.as_ref(),
                        };
                        let document = snapshot.document.clone();
                        let collapsed = snapshot.collapsed.clone();
                        let reveal = snapshot.reveal.cloned();
                        view.element(
                            CodeSnapshot { document: &document, collapsed: &collapsed, reveal: reveal.as_ref() },
                            move |path, _, cx| {
                                toggle.update(cx, |store, cx| {
                                    store.toggle_collapsed(path);
                                    cx.notify();
                                })
                            },
                            move |path, _, cx| {
                                open.update(cx, |store, cx| {
                                    store.open_tab(PanelTab::File(path.to_string()));
                                    cx.notify();
                                })
                            },
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
        div().size_full().flex().flex_col().child(bar).child(div().flex_1().min_h_0().child(body)).into_any_element()
    }

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
                .child(div().flex_1())
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
                        let (open, menu) = (self.store.clone(), node.path.clone());
                        let (path, folder) = (node.path.clone(), node.folder);
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
                            .child(highlight(group, 6., edges(0., 6., 0., 6.), false, cx))
                            .when(node.ignored, |row| row.opacity(0.5))
                            .child(div().relative().w(px(10.)).flex().justify_center().when(node.folder, |chevron| {
                                chevron.child(
                                    icons::symbol(if node.open { "chevron.down" } else { "chevron.right" }, 8.)
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
                            .on_click(move |_, _, cx| {
                                let path = path.clone();
                                open.update(cx, |store, cx| {
                                    if folder {
                                        store.toggle_folder(&path);
                                    } else {
                                        store.open_tab(PanelTab::File(path));
                                    }
                                    cx.notify();
                                })
                            })
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                let path = menu.clone();
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
        div().size_full().flex().flex_col().child(bar).child(div().flex_1().min_h_0().child(body)).into_any_element()
    }

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
                .child(div().flex_1())
                .child(
                    IconButton::new("copy-path", if copied { "checkmark" } else { "doc.on.doc" })
                        .help("Copy the path")
                        .color(c.secondary)
                        .on_click(move |_, _, cx| {
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
                        }),
                )
                .child(Self::refresh_button("refresh-file", "Read the file again", cx)),
            cx,
        );
        let content = self.store.read(cx).side_panel.contents.get(path).cloned();
        let body: AnyElement = match content {
            None | Some(Loaded::Loading) => Self::loading(cx),
            Some(Loaded::Failed(message)) => Self::message(message, true, cx),
            Some(Loaded::Ready(FileContent::Text(document, truncated))) => {
                if document.files.first().is_some_and(|file| file.lines.is_empty()) {
                    Self::message("This file is empty.", false, cx)
                } else {
                    let view = match self.files.get(path) {
                        Some(view) => view.clone(),
                        None => {
                            let view = cx.new(CodeView::new);
                            self.files.insert(path.to_string(), view.clone());
                            view
                        }
                    };
                    let collapsed = Default::default();
                    let code = view.update(cx, |view, cx| {
                        view.element(
                            CodeSnapshot { document: &document, collapsed: &collapsed, reveal: None },
                            |_, _, _| {},
                            |_, _, _| {},
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
        div().size_full().flex().flex_col().child(bar).child(div().flex_1().min_h_0().child(body)).into_any_element()
    }

    fn agents_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let agents = store.agents.clone();
        if let Some(agent) =
            agents.iter().find(|agent| Some(&agent.id) == store.side_panel.shown_agent.as_ref()).cloned()
        {
            let back = self.store.clone();
            let bar = Self::bar(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(4.))
                    .child(div().ml(px(-6.)).child(
                        IconButton::new("all-agents", "chevron.left").help("All agents").color(c.secondary).on_click(
                            move |_, _, cx| {
                                back.update(cx, |store, cx| {
                                    store.show_agents(cx);
                                    cx.notify();
                                })
                            },
                        ),
                    ))
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
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(bar)
                .child(div().flex_1().min_h_0().child(self.agent.clone()))
                .into_any_element();
        }
        if agents.is_empty() {
            return Self::message("The agents this thread starts show up here.", false, cx);
        }
        let summary = summary(&agents);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(Self::bar(
                div().flex().w_full().child(
                    div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(c.secondary).child(summary),
                ),
                cx,
            ))
            .child(
                div()
                    .id("agents")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(agents.iter().map(|agent| {
                        let group: SharedString = format!("agent-{}", agent.id).into();
                        let (store, id) = (self.store.clone(), agent.id.clone());
                        div()
                            .id(group.clone())
                            .group(group.clone())
                            .relative()
                            .px(px(10.))
                            .py(px(8.))
                            .when(agent.parent.is_some(), |row| row.pl(px(28.)))
                            .child(highlight(group, 7., edges(0., 0., 0., 0.), false, cx))
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
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                store.update(cx, |store, cx| {
                                    store.show_agent(id, cx);
                                    cx.notify();
                                })
                            })
                    })),
            )
            .into_any_element()
    }
}

fn status_icon(status: ToolStatus, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    match status {
        ToolStatus::Running => icons::symbol("circle.dashed", 11.).text_color(c.working),
        ToolStatus::Succeeded => icons::symbol("checkmark", 11.).text_color(c.secondary),
        ToolStatus::Failed => icons::symbol("xmark", 11.).text_color(c.danger),
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

/// "14:03", or with the day when it isn't today.
fn clock(at: f64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_opt(at as i64, 0).single() else { return String::new() };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M").to_string()
    } else {
        time.format("%-d %b %Y, %H:%M").to_string()
    }
}

impl Render for SidePanelView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
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
                Some(PanelTab::Agents) => self.agents_surface(cx),
                None => self.launcher(&target, cx),
            }
        } else {
            div().into_any_element()
        };
        div()
            .size_full()
            .relative()
            .bg(c.background)
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
