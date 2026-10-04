//! The strips against the composer's top and bottom, in the composer's fill and border: rounded
//! on their outer corners and open where they meet the composer. They stand in from the
//! composer's sides by its corner radius, so they meet its straight edge.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::Branch;

use crate::composer::RADIUS;
use crate::models::{Project, Server};
use crate::store::Store;
use crate::theme::colors;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{edges, highlight, icons, logos};

const HEIGHT: f32 = 32.;
const STRIP_RADIUS: f32 = 14.;

fn strip(top: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .mx(px(RADIUS))
        .h(px(HEIGHT))
        .flex()
        .items_center()
        .bg(c.composer)
        .border_color(c.strong_border)
        .border_l_1()
        .border_r_1()
        .when(top, |strip| strip.border_t_1().rounded_t(px(STRIP_RADIUS)).mb(px(-1.)))
        .when(!top, |strip| strip.border_b_1().rounded_b(px(STRIP_RADIUS)).mt(px(-1.)))
}

/// The agent is watching something it left running: the strip says so, with the button that
/// stops it.
pub fn monitoring_strip(store: &Entity<Store>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let store = store.clone();
    let margin = edges(4., 4., 4., 8.);
    strip(true, cx)
        .child(div().ml(px(14.)).mr(px(8.)).size(px(6.)).rounded_full().bg(c.text))
        .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child("Monitoring"))
        .child(div().flex_1().min_w(px(8.)))
        .child(
            div()
                .id("stop-monitoring")
                .group("stop-monitoring")
                .relative()
                .pt(px(margin.top))
                .pb(px(margin.bottom))
                .pl(px(margin.left))
                .pr(px(margin.right))
                .child(highlight("stop-monitoring", 7., margin, false, cx))
                .child(
                    div()
                        .relative()
                        .h(px(24.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.secondary)
                        .child("Stop"),
                )
                .tooltip(crate::ui::tooltip("Stop monitoring (⌘.)"))
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.stop_thread();
                        cx.notify();
                    })
                }),
        )
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

fn branch_label(branch: String, opens: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .min_w_0()
        .h(px(24.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(c.secondary)
        .child(div().flex_shrink_0().child(icons::symbol("arrow.triangle.branch", 11.)))
        .child(div().min_w_0().text_size(px(12.)).truncate().child(branch))
        .when(opens, |label| {
            label.child(div().flex_shrink_0().child(icons::symbol("chevron.down", 9.).text_color(c.tertiary)))
        })
}

/// Where the composer's thread works: the server, the folder or a worktree of its own, and the
/// branch checked out there, which opens the picker to switch. A thread that starts in a new
/// worktree picks the branch it starts from there instead.
pub fn context_strip(
    store: &Entity<Store>,
    project: &Project,
    server: Option<&Server>,
    workspace_menu: &Anchor,
    branch_anchor: &Anchor,
    picker: Option<Entity<BranchPicker>>,
    cx: &App,
) -> impl IntoElement {
    let state = store.read(cx);
    let margin = edges(4., 4., 4., 8.);
    let starts_in_worktree = state.draft_uses_worktree();
    let branch = if starts_in_worktree { state.draft_base() } else { project.branch.clone() };
    let has_thread = state.selected_thread().is_some();
    let can_switch = state.can_switch_branches(project);
    let can_use_worktrees = state.can_use_worktrees(project);

    let workspace: Option<AnyElement> =
        if let Some(worktree) = &project.worktree {
            Some(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(strip_divider(cx).mx(px(10.)))
                    .child(div().id("worktree").min_w_0().tooltip(crate::ui::tooltip(worktree.path.clone())).child(
                        part("Worktree".into(), icons::symbol("folder.badge.gearshape", 11.).into_any_element(), cx),
                    ))
                    .into_any_element(),
            )
        } else if has_thread && project.branch.is_some() {
            Some(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(strip_divider(cx).mx(px(10.)))
                    .child(
                        div()
                            .id("checkout")
                            .min_w_0()
                            .tooltip(crate::ui::tooltip("The thread works in the project's folder"))
                            .child(part("Local checkout".into(), icons::symbol("folder", 11.).into_any_element(), cx)),
                    )
                    .into_any_element(),
            )
        } else if !has_thread && can_use_worktrees {
            let in_worktree = starts_in_worktree;
            let c = colors(cx);
            let workspace_margin = edges(4., 3., 4., 3.);
            let anchor = workspace_menu.clone();
            let choose = store.clone();
            Some(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(strip_divider(cx).ml(px(10.)))
                    .child(
                        div()
                            .id("workspace")
                            .group("workspace")
                            .relative()
                            .min_w_0()
                            .pt(px(workspace_margin.top))
                            .pb(px(workspace_margin.bottom))
                            .pl(px(workspace_margin.left))
                            .pr(px(workspace_margin.right))
                            .child(highlight("workspace", 7., workspace_margin, false, cx))
                            .child(
                                div()
                                    .relative()
                                    .min_w_0()
                                    .h(px(24.))
                                    .px(px(9.))
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .text_color(c.secondary)
                                    .child(workspace_menu.track())
                                    .child(div().flex_shrink_0().child(icons::symbol(
                                        if in_worktree { "folder.badge.plus" } else { "folder" },
                                        11.,
                                    )))
                                    .child(div().min_w_0().text_size(px(12.)).truncate().child(if in_worktree {
                                        "New worktree"
                                    } else {
                                        "Current checkout"
                                    }))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .child(icons::symbol("chevron.down", 9.).text_color(c.tertiary)),
                                    ),
                            )
                            .tooltip(crate::ui::tooltip(if in_worktree {
                                "The thread works in a folder and on a branch of its own"
                            } else {
                                "The thread works in the project's folder"
                            }))
                            .on_click(move |_, window, cx| {
                                let (current, new) = (choose.clone(), choose.clone());
                                Menu::new()
                                    .item_if(false, "Workspace", |_, _| {})
                                    .checked(!in_worktree, "Current checkout", move |_, cx| {
                                        current.update(cx, |store, cx| {
                                            store.set_draft_worktree(false, cx);
                                            cx.notify();
                                        })
                                    })
                                    .checked(in_worktree, "New worktree", move |_, cx| {
                                        new.update(cx, |store, cx| {
                                            store.set_draft_worktree(true, cx);
                                            cx.notify();
                                        })
                                    })
                                    .show(anchor.below(), window, cx);
                            }),
                    )
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
                .pt(px(margin.top))
                .pb(px(margin.bottom))
                .pl(px(margin.left))
                .pr(px(margin.right))
                .tooltip(crate::ui::tooltip("The branch of this thread's worktree"))
                .child(branch_label(branch, false, cx))
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
                .child(
                    div()
                        .id("branch")
                        .group("branch")
                        .relative()
                        .min_w_0()
                        .pt(px(margin.top))
                        .pb(px(margin.bottom))
                        .pl(px(margin.left))
                        .pr(px(margin.right))
                        .child(highlight("branch", 7., margin, false, cx))
                        .child(
                            div()
                                .relative()
                                .min_w_0()
                                .child(branch_anchor.track())
                                .child(branch_label(title, true, cx)),
                        )
                        .tooltip(crate::ui::tooltip(help))
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
                                .position(branch_anchor.popover_origin(300.))
                                .snap_to_window_with_margin(px(8.))
                                .child(picker),
                        )
                        .with_priority(1),
                    )
                })
                .into_any_element();
        }
        let help = if server.is_some_and(|server| server.known) {
            format!(
                "Update {} to switch branches from here",
                server.map(|server| server.name.clone()).unwrap_or_default()
            )
        } else {
            "The branch checked out there".to_string()
        };
        div()
            .id("branch")
            .min_w_0()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .pl(px(margin.left))
            .pr(px(margin.right))
            .tooltip(crate::ui::tooltip(help))
            .child(branch_label(branch, false, cx))
            .into_any_element()
    });

    strip(false, cx)
        .when_some(server, |strip, server| {
            strip
                .child(
                    div()
                        .id("strip-server")
                        .flex_shrink_0()
                        .pl(px(14.))
                        .tooltip(crate::ui::tooltip(format!("On {}", server.name)))
                        .child(part(server.name.clone(), icons::symbol("server.rack", 11.).into_any_element(), cx)),
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
        .children(workspace)
        .child(div().flex_1().min_w(px(8.)))
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
}

impl Render for BranchPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let choices = self.choices(cx);
        let base = self.base(cx);
        let working = self.working(cx);
        let listed = self.store.read(cx).listed_branches.clone();
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
                .children(choices.iter().enumerate().map(|(index, choice)| {
                    let highlighted = index == self.highlighted;
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
                        .when(highlighted, |row| row.bg(c.selected))
                        .when(working, |row| row.opacity(0.5))
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.highlighted = index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)));
                    match choice {
                        Choice::Branch(branch) => {
                            let chosen = base.as_ref().map_or(branch.current, |base| base == &branch.name);
                            let tag = if branch.default {
                                Some("default")
                            } else if branch.remote {
                                Some("remote")
                            } else {
                                None
                            };
                            row.child(
                                div().w(px(14.)).flex().justify_center().child(
                                    icons::symbol(if chosen { "checkmark" } else { "arrow.triangle.branch" }, 11.)
                                        .text_color(if chosen { c.text } else { c.tertiary }),
                                ),
                            )
                            .child(div().min_w_0().truncate().child(branch.name.clone()))
                            .child(div().flex_1().min_w(px(8.)))
                            .when_some(tag, |row, tag| {
                                row.child(div().text_size(px(11.)).text_color(c.tertiary).child(tag))
                            })
                        }
                        Choice::Create(name) => row
                            .child(
                                div()
                                    .w(px(14.))
                                    .flex()
                                    .justify_center()
                                    .child(icons::symbol("plus", 11.).text_color(c.tertiary)),
                            )
                            .child(div().min_w_0().truncate().child(format!("Create branch “{name}”"))),
                    }
                }))
                .into_any_element(),
        };
        div()
            .id("branch-picker")
            .w(px(300.))
            .bg(c.popover)
            .rounded(px(12.))
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
                    .child(icons::symbol("magnifyingglass", 12.).text_color(c.tertiary))
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
