//! The strips against the composer's top and bottom, on the composer's surface: rounded on their
//! outer corners and open where they meet the composer. They stand in from the composer's sides
//! by its corner radius, so they meet its straight edge.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::Waiting;
use motile_protocol::wire::Branch;

use crate::composer::RADIUS;
use crate::composer::questions::Questions;
use crate::models::{Project, Server, ThreadInfo, elapsed};
use crate::store::Store;
use crate::theme::{self, ControlSize, Surface, colors};
use crate::ui::control::{ControlIcon, ControlLabel};
use crate::ui::menu::{Anchor, MenuIcon};
use crate::ui::{ActionButton, ActionMenu, Variant, edges, even, icons, logos};

pub const HEIGHT: f32 = 32.;
const STRIP_RADIUS: f32 = 14.;
/// How far the top strip's open edge goes under the composer, which is drawn after it and
/// covers its outline there. The bottom strip starts under the composer's outline instead.
const OVERLAP: f32 = 1.;
/// The room around a control in a strip, which is the control's to click.
pub fn margin() -> Edges<f32> {
    edges(4., 4., 4., 8.)
}

/// A strip of `height`, or as tall as what is in it with none.
fn strip(top: bool, height: Option<f32>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .mx(px(RADIUS))
        .when_some(height, |strip, height| strip.h(px(height)))
        .flex()
        .items_center()
        .bg(c.composer)
        .border_color(c.border)
        .border_l_1()
        .border_r_1()
        .when(top, |strip| strip.border_t_1().rounded_t(px(STRIP_RADIUS)).mb(px(-OVERLAP)))
        .when(!top, |strip| strip.border_b_1().rounded_b(px(STRIP_RADIUS)))
}

/// The agent is watching something it left running: the strip says for how long, with the
/// button that stops it.
pub fn monitoring_strip(store: &Entity<Store>, thread: &ThreadInfo, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let store = store.clone();
    let since = thread.turn_ended_at.unwrap_or(thread.updated_at);
    strip(true, Some(HEIGHT), cx)
        .child(div().ml(px(14.)).mr(px(8.)).size(px(6.)).rounded_full().bg(c.text).flex_shrink_0())
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text)
                .child(format!("Monitoring for {}", elapsed(since))),
        )
        .child(div().flex_1().min_w(px(8.)))
        .child(
            ActionButton::new("stop-monitoring", "Stop")
                .help("Stop monitoring (⌘.)")
                .ghost()
                .small()
                .surface(Surface::Composer)
                .margin(margin())
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.stop_thread();
                        cx.notify();
                    })
                }),
        )
}

/// What a waiting strip is about, and which of several it is.
pub fn waiting_title(title: String, symbol: &'static str, place: Option<String>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(icons::symbol(symbol, 13.))
        .child(div().min_w_0().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).truncate().child(title))
        .when_some(place, |title, place| {
            title.child(div().flex_shrink_0().text_size(px(12.)).text_color(c.secondary).child(place))
        })
}

/// Four lines of a command, after which it scrolls.
const TARGET_HEIGHT: f32 = 66.;

/// The tool call the agent waits with, above the composer: it is allowed or refused, and one
/// that asks questions is answered. The calls behind it wait their turn.
pub fn waiting_strip(
    store: &Entity<Store>,
    approval: &Waiting,
    count: usize,
    questions: Option<Entity<Questions>>,
    cx: &App,
) -> impl IntoElement {
    let content = match questions {
        Some(questions) => questions.into_any_element(),
        None => waiting_call(store, approval, count, cx).into_any_element(),
    };
    strip(true, None, cx).child(div().w_full().min_w_0().p(px(12.)).text_size(px(12.5)).child(content))
}

fn waiting_call(store: &Entity<Store>, approval: &Waiting, count: usize, cx: &App) -> Div {
    let c = colors(cx);
    let (refuse, allow) = (store.clone(), store.clone());
    let (refuse_id, allow_id) = (approval.id.clone(), approval.id.clone());
    let place = (count > 1).then(|| format!("1 of {count}"));
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    waiting_title(approval.title.clone(), icons::tool_symbol(approval.icon), place, cx)
                        .text_color(c.warning),
                )
                .child(div().flex_1().min_w(px(8.)))
                .child(ActionButton::new("refuse", approval.refuse).small().surface(Surface::Composer).on_click(
                    move |_, _, cx| {
                        refuse.update(cx, |store, cx| {
                            store.answer(&refuse_id, false, Default::default());
                            cx.notify();
                        })
                    },
                ))
                .child(
                    ActionButton::new("allow", approval.allow)
                        .variant(Variant::Warning)
                        .small()
                        .surface(Surface::Composer)
                        .on_click(move |_, _, cx| {
                            allow.update(cx, |store, cx| {
                                store.answer(&allow_id, true, Default::default());
                                cx.notify();
                            })
                        }),
                ),
        )
        .when(!approval.target.is_empty(), |call| {
            call.child(
                div()
                    .id("target")
                    .max_h(px(TARGET_HEIGHT))
                    .overflow_y_scroll()
                    .font_family(theme::MONO_FONT)
                    .text_size(px(12.))
                    .child(approval.target.clone()),
            )
        })
}

fn strip_divider(cx: &App) -> Div {
    div().w(px(1.)).h(px(14.)).flex_shrink_0().bg(colors(cx).border)
}

fn part(title: String, icon: AnyElement, cx: &App) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(colors(cx).secondary)
        .child(div().flex_shrink_0().child(icon))
        .child(div().min_w_0().text_size(px(12.)).truncate().child(title))
}

/// A branch that can't be switched from here, set as the button that switches one is.
fn branch_label(branch: String, cx: &App) -> Div {
    let label = ControlLabel {
        title: Some(branch.into()),
        icon: Some(ControlIcon::Symbol("git-branch".into())),
        size: ControlSize::Small,
        chevron: false,
        pending: false,
        pending_title: None,
        fills: false,
        symbol_size: None,
    };
    let m = margin();
    div()
        .min_w_0()
        .pt(px(m.top))
        .pl(px(m.left))
        .pb(px(m.bottom))
        .pr(px(m.right))
        .text_color(colors(cx).secondary)
        .child(label.render(cx))
}

/// Where the composer's thread works: the server, the folder or a worktree of its own, and the
/// branch checked out there, which opens the picker to switch. A thread that starts in a new
/// worktree picks the branch it starts from there instead.
pub fn context_strip(
    store: &Entity<Store>,
    project: &Project,
    server: Option<&Server>,
    branch_anchor: &Anchor,
    picker: Option<Entity<BranchPicker>>,
    cx: &App,
) -> impl IntoElement {
    let state = store.read(cx);
    let starts_in_worktree = state.draft_uses_worktree();
    let branch = if starts_in_worktree { state.draft_base() } else { project.branch.clone() };
    let has_thread = state.selected_thread().is_some();
    let can_switch = state.can_switch_branches(project);
    let can_use_worktrees = state.can_use_worktrees(project);
    let has_workspace =
        project.worktree.is_some() || (has_thread && project.branch.is_some()) || (!has_thread && can_use_worktrees);

    let working = |id: &'static str, title: &'static str, symbol: &'static str, help: String, cx: &App| {
        div()
            .id(id)
            .min_w_0()
            .px(px(13.))
            .tooltip(crate::ui::tooltip(help))
            .child(part(title.into(), icons::symbol(symbol, 11.).into_any_element(), cx))
            .into_any_element()
    };
    let workspace: Option<AnyElement> = if let Some(worktree) = &project.worktree {
        Some(working("worktree", "Worktree", "folder-git-2", worktree.path.clone(), cx))
    } else if has_thread && project.branch.is_some() {
        Some(working("checkout", "Local checkout", "folder", "The thread works in the project's folder".into(), cx))
    } else if !has_thread && can_use_worktrees {
        let in_worktree = starts_in_worktree;
        let choose = store.clone();
        let (title, symbol, help) = if in_worktree {
            ("New worktree", "folder-git-2", "The thread works in a folder and on a branch of its own")
        } else {
            ("Current checkout", "folder", "The thread works in the project's folder")
        };
        Some(
            ActionMenu::new("workspace", title, move |_, _| {
                let (current, new) = (choose.clone(), choose.clone());
                crate::ui::menu::Menu::new()
                    .note("Workspace")
                    .choice(!in_worktree, "Current checkout", Some(MenuIcon::Symbol("folder")), None, move |_, cx| {
                        current.update(cx, |store, cx| {
                            store.set_draft_worktree(false, cx);
                            cx.notify();
                        })
                    })
                    .choice(in_worktree, "New worktree", Some(MenuIcon::Symbol("folder-git-2")), None, move |_, cx| {
                        new.update(cx, |store, cx| {
                            store.set_draft_worktree(true, cx);
                            cx.notify();
                        })
                    })
            })
            .button(|button| button.symbol(symbol).help(help).small().surface(Surface::Composer).margin(even(4.)))
            .into_any_element(),
        )
    } else {
        None
    };

    let branch_part: Option<AnyElement> = branch.map(|branch| {
        if project.worktree.is_some() {
            return div()
                .id("branch")
                .min_w_0()
                .tooltip(crate::ui::tooltip("The branch of this thread's worktree"))
                .child(branch_label(branch, cx))
                .into_any_element();
        }
        if can_switch {
            let (open, project) = (store.clone(), project.clone());
            let title = if starts_in_worktree { format!("From {branch}") } else { branch.clone() };
            let help = if starts_in_worktree {
                "The branch the worktree's branch starts from".to_string()
            } else {
                format!("Switch the branch of {}", project.name)
            };
            return div()
                .relative()
                .min_w_0()
                .child(branch_anchor.track())
                .child(
                    ActionButton::new("branch", title)
                        .symbol("git-branch")
                        .help(help)
                        .ghost()
                        .small()
                        .opens(true)
                        .surface(Surface::Composer)
                        .margin(margin())
                        .on_click(move |_, _, cx| {
                            open.update(cx, |store, cx| {
                                if store.shows_branches {
                                    store.shows_branches = false;
                                } else {
                                    store.show_branches(&project);
                                }
                                cx.notify();
                            })
                        }),
                )
                .when_some(picker.clone(), |button, picker| {
                    button.child(
                        deferred(
                            anchored()
                                .position(branch_anchor.popover_origin(PICKER_WIDTH))
                                .snap_to_window_with_margin(px(8.))
                                .child(picker),
                        )
                        .with_priority(1),
                    )
                })
                .into_any_element();
        }
        let help = match server {
            Some(server) if server.known => format!("Update {} to switch branches from here", server.name),
            _ => "The branch checked out there".to_string(),
        };
        div()
            .id("branch")
            .min_w_0()
            .tooltip(crate::ui::tooltip(help))
            .child(branch_label(branch, cx))
            .into_any_element()
    });
    let divides = has_workspace && branch_part.is_some();

    strip(false, Some(HEIGHT), cx)
        .when_some(server, |strip, server| {
            strip
                .child(
                    div()
                        .id("strip-server")
                        .flex_shrink_0()
                        .pl(px(14.))
                        .tooltip(crate::ui::tooltip(format!("On {}", server.name)))
                        .child(part(server.name.clone(), icons::symbol("server", 11.).into_any_element(), cx)),
                )
                .child(strip_divider(cx).mx(px(10.)))
        })
        .child(
            div()
                .id("strip-project")
                .when(server.is_none(), |part| part.pl(px(14.)))
                .flex_shrink_0()
                .tooltip(crate::ui::tooltip(project.path.clone()))
                .child(part(project.name.clone(), logos::project_icon(Some(project), 13., cx), cx)),
        )
        .child(div().flex_1().min_w(px(8.)))
        .children(workspace)
        .when(divides, |strip| strip.child(strip_divider(cx)))
        .children(branch_part)
}

#[derive(Clone)]
enum Choice {
    Branch(Branch),
    Create(String),
}

/// The branches of the project's repository, to switch to one or make a new one. What is typed
/// narrows the list, and becomes the name of a branch to make when it matches none. When the
/// open draft starts in a new worktree it picks the branch that starts from instead, and
/// switches nothing.
pub struct BranchPicker {
    store: Entity<Store>,
    query: Entity<InputState>,
    problem: Option<String>,
    highlighted: usize,
    switching: bool,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

const PICKER_WIDTH: f32 = 300.;
const ROW_HEIGHT: f32 = 30.;
const LIST_PADDING: f32 = 8.;
const MAX_LIST_HEIGHT: f32 = 300.;

impl BranchPicker {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let base = store.read(cx).draft_uses_worktree();
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder(if base {
                "Start from a branch…"
            } else {
                "Switch or create a branch…"
            })
        });
        query.update(cx, |query, cx| query.focus(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&query, window, |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.highlighted = 0;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.choose(this.highlighted, cx),
                _ => {}
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self {
            store,
            query,
            problem: None,
            highlighted: 0,
            switching: false,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn base(&self, cx: &App) -> Option<String> {
        let store = self.store.read(cx);
        if store.draft_uses_worktree() { store.draft_base() } else { None }
    }

    fn project(&self, cx: &App) -> Option<Project> {
        self.store.read(cx).composer_project()
    }

    fn working(&self, cx: &App) -> bool {
        self.base(cx).is_none() && self.project(cx).is_some_and(|project| self.store.read(cx).is_working_in(&project))
    }

    fn choices(&self, cx: &App) -> Vec<Choice> {
        let Ok(branches) = &self.store.read(cx).listed_branches else { return Vec::new() };
        let needle = self.query.read(cx).value().trim().to_string();
        let lower = needle.to_lowercase();
        let mut choices: Vec<Choice> = branches
            .iter()
            .filter(|branch| needle.is_empty() || branch.name.to_lowercase().contains(&lower))
            .cloned()
            .map(Choice::Branch)
            .collect();
        if self.base(cx).is_none() && !needle.is_empty() && !branches.iter().any(|branch| branch.name == needle) {
            choices.push(Choice::Create(needle));
        }
        choices
    }

    fn close(&self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.shows_branches = false;
            store.composer_focus += 1;
            cx.notify();
        });
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        let choices = self.choices(cx);
        if self.switching || self.working(cx) || index >= choices.len() {
            return;
        }
        let (name, create) = match &choices[index] {
            Choice::Branch(branch) => (branch.name.clone(), false),
            Choice::Create(name) => (name.clone(), true),
        };
        if self.base(cx).is_some() {
            self.store.update(cx, |store, _| store.set_draft_base(name));
            return self.close(cx);
        }
        let current = matches!(&choices[index], Choice::Branch(branch) if branch.current);
        if !create && current {
            return self.close(cx);
        }
        let Some(project) = self.project(cx) else { return };
        self.switching = true;
        self.problem = None;
        let this = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.switch_branch(&project, name, create, move |store, failure, cx| {
                let Some(failure) = failure else {
                    store.shows_branches = false;
                    store.composer_focus += 1;
                    return;
                };
                let _ = this.update(cx, |this, cx| {
                    this.switching = false;
                    this.problem = Some(failure);
                    cx.notify();
                });
            });
        });
    }

    fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.choices(cx).len();
        if count == 0 {
            return;
        }
        self.highlighted = (self.highlighted as isize + step).clamp(0, count as isize - 1) as usize;
        self.scroll.scroll_to_item(self.highlighted);
        cx.notify();
    }

    fn row(
        &self,
        index: usize,
        choice: &Choice,
        base: Option<&String>,
        working: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = colors(cx);
        let lit = Surface::Popover.next().color(c);
        let row = div()
            .id(("branch-choice", index))
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(8.))
            .text_size(px(12.5))
            .text_color(c.text)
            .when(index == self.highlighted, |row| row.bg(lit))
            .when(working, |row| row.opacity(0.5))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.highlighted = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)));
        let mark = |symbol: &'static str, color: Hsla| {
            div().w(px(14.)).flex_shrink_0().flex().justify_center().child(icons::symbol(symbol, 11.).text_color(color))
        };
        match choice {
            Choice::Branch(branch) => {
                let chosen = base.map_or(branch.current, |base| base == &branch.name);
                let tag = if branch.default {
                    Some("default")
                } else if branch.remote {
                    Some("remote")
                } else {
                    None
                };
                row.child(mark(if chosen { "check" } else { "git-branch" }, if chosen { c.text } else { c.tertiary }))
                    .child(div().min_w_0().truncate().child(branch.name.clone()))
                    .child(div().flex_1().min_w(px(8.)))
                    .when_some(tag, |row, tag| row.child(div().text_size(px(11.)).text_color(c.tertiary).child(tag)))
            }
            Choice::Create(name) => row
                .child(mark("plus", c.tertiary))
                .child(div().min_w_0().truncate().child(format!("Create branch “{name}”")))
                .child(div().flex_1().min_w(px(8.))),
        }
    }
}

impl Render for BranchPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let choices = self.choices(cx);
        let base = self.base(cx);
        let working = self.working(cx);
        let listed = self.store.read(cx).listed_branches.clone();
        // The height of all the branches, also while fewer match: a popover that shrinks leaves
        // the button it opened from.
        let list_height = MAX_LIST_HEIGHT
            .min(listed.as_ref().map_or(0, |branches| branches.len()) as f32 * ROW_HEIGHT + 2. * LIST_PADDING);
        let note = if working {
            Some("An agent is working in this project. Switch when it has finished.".to_string())
        } else {
            self.problem.clone()
        };
        let list: AnyElement = match &listed {
            Err(error) => {
                div().p(px(16.)).text_size(px(12.5)).text_color(c.danger).child(error.clone()).into_any_element()
            }
            Ok(_) if choices.is_empty() => div()
                .p(px(16.))
                .text_size(px(12.5))
                .text_color(c.secondary)
                .child("No branch matches.")
                .into_any_element(),
            Ok(_) => div()
                .id("branches")
                .h(px(list_height))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .p(px(LIST_PADDING))
                .children(
                    choices
                        .iter()
                        .enumerate()
                        .map(|(index, choice)| self.row(index, choice, base.as_ref(), working, cx)),
                )
                .into_any_element(),
        };
        div()
            .id("branch-picker")
            .w(px(PICKER_WIDTH))
            .bg(c.popover)
            .rounded(px(theme::Radius::SHEET))
            .border_1()
            .border_color(c.border)
            .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.18), 8., 24.))
            .occlude()
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| match event.keystroke.key.as_str() {
                "down" => this.move_highlight(1, cx),
                "up" => this.move_highlight(-1, cx),
                "escape" => this.close(cx),
                _ => {}
            }))
            .child(
                div()
                    .h(px(38.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol("search", 12.).text_color(c.tertiary))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.query).appearance(false).px_0().py_0().text_size(px(13.))),
                    ),
            )
            .child(crate::ui::divider(cx))
            .child(list)
            .when_some(note, |picker, note| {
                picker.child(crate::ui::divider(cx)).child(
                    div()
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(11.5))
                        .text_color(if working { c.secondary } else { c.danger })
                        .child(note),
                )
            })
    }
}
