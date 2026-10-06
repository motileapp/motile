//! The git button in the top bar of a thread, its menu, the commit sheet, and what git said.

use std::collections::HashSet;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Change, ChangedFile, GitAction, PullRequest};

use crate::models::Project;
use crate::store::{GitNotice, PendingGit, Store, stage_label};
use crate::theme::{Colors, ControlSize, Radius, Surface, colors};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::sheet::sheet;
use crate::ui::{ActionButton, Variant, icons};

/// What leaves the room under the button that the composer's menus leave over theirs.
const MENU_GAP: f32 = 16.;

/// The symbol of a git action.
pub fn git_symbol(action: Option<GitAction>) -> &'static str {
    match action {
        Some(GitAction::Pull) => "cloud-download",
        Some(GitAction::Push | GitAction::CommitPush | GitAction::CommitPushPr) => "cloud-upload",
        Some(GitAction::CreatePr) => "git-pull-request-create",
        Some(GitAction::Commit) | None => "git-commit-horizontal",
    }
}

/// The symbol of what became of a pull request.
pub fn pull_request_symbol(pull_request: &PullRequest) -> &'static str {
    if pull_request.merged {
        return "git-merge";
    }
    if pull_request.closed {
        return "git-pull-request-closed";
    }
    if pull_request.draft { "git-pull-request-draft" } else { "git-pull-request" }
}

/// The colour of what became of a pull request.
pub fn pull_request_color(pull_request: &PullRequest, c: &Colors) -> Hsla {
    if pull_request.merged {
        return c.merged;
    }
    if pull_request.closed {
        return c.danger;
    }
    if pull_request.draft { c.secondary } else { c.success }
}

/// Its left half does the one thing the repository calls for, at once: commit, push, open a pull
/// request, pull. Its right half opens the menu of all of them, where an item that can't run now
/// says why. While an action runs, the button says which stage it is at.
pub fn git_button(store: &Entity<Store>, anchor: &Anchor, cx: &App) -> Option<AnyElement> {
    let c = colors(cx);
    let state = store.read(cx);
    let project = state.git_project()?;
    let control = project.git_control.clone()?;
    let stage = state.git_stage(&project);
    let quick = control.quick.clone();
    let pull_request = project.git.as_ref().and_then(|git| git.pull_request.clone());
    let symbol = match (&quick.url, &pull_request) {
        (Some(_), Some(pull_request)) => pull_request_symbol(pull_request),
        (Some(_), None) => "git-pull-request",
        (None, _) => git_symbol(quick.action),
    };
    let title = match &quick.state {
        Some(state) => format!("{state} {}", quick.label),
        None => quick.label.clone(),
    };
    let label = stage.map(|stage| stage_label(stage).to_string()).unwrap_or(title.clone());
    let help = quick.hint.clone().or_else(|| pull_request.as_ref().map(|pr| pr.title.clone())).unwrap_or(title);
    // A pull request is in the colour of its state, and what has nothing to do is quiet.
    let tint = match (stage, &quick.url, &pull_request) {
        (Some(_), _, _) => c.text,
        (None, Some(_), Some(pull_request)) => pull_request_color(pull_request, c),
        (None, None, _) if quick.action.is_none() => c.tertiary,
        _ => c.text,
    };
    let quick_store = store.clone();
    let quick_project = project.clone();
    let menu_store = store.clone();
    let menu_anchor = anchor.clone();
    Some(
        div()
            .mx(px(6.))
            .flex_shrink_0()
            .relative()
            .flex()
            .items_center()
            .rounded(px(Radius::CONTROL))
            .overflow_hidden()
            .child(anchor.track())
            .child(
                ActionButton::new("git-quick", label)
                    .symbol(symbol)
                    .help(help)
                    .variant(Variant::Ghost)
                    .pending(stage.is_some())
                    .joined(true, true)
                    .tint(Some(tint))
                    .on_click(move |_, _, cx| {
                        quick_store.update(cx, |store, cx| {
                            store.run_quick_git(&quick_project, cx);
                            cx.notify();
                        });
                    }),
            )
            .child(div().w(px(1.)).h(px(ControlSize::Regular.height())).bg(c.border_secondary))
            .child(
                ActionButton::chevron("git-menu", "Commit, push or open a pull request").joined(true, true).on_click(
                    move |_, window, cx| {
                        git_menu(&menu_store, &project, cx).show_right_aligned(
                            menu_anchor.below_right(MENU_GAP),
                            window,
                            cx,
                        );
                    },
                ),
            )
            .child(div().absolute().inset_0().rounded(px(Radius::CONTROL)).border_1().border_color(c.border_secondary))
            .into_any_element(),
    )
}

/// Every action, greyed with the reason when it can't run now, and the warning under them.
fn git_menu(store: &Entity<Store>, project: &Project, cx: &App) -> Menu {
    let Some(control) = project.git_control.clone() else { return Menu::new() };
    let running = store.read(cx).git_stage(project).is_some();
    let mut menu = Menu::new();
    for item in control.menu {
        let store = store.clone();
        let project = project.clone();
        let enabled = item.reason.is_none() && !running;
        let symbol = git_symbol(Some(item.action));
        let reason = item.reason.clone().map(SharedString::from);
        menu = menu.icon_item(item.label.clone(), MenuIcon::Symbol(symbol), enabled, reason, move |window, cx| {
            store.update(cx, |store, cx| {
                store.choose_git(&item, &project, cx);
                cx.notify();
            });
            window.refresh();
        });
    }
    menu.when_some(control.warning, |menu, warning| menu.separator().note(warning))
}

/// Asks where an action that would push from the default branch should happen.
pub fn confirm_alert(store: &Entity<Store>, pending: &PendingGit, cx: &App) -> impl IntoElement {
    let on = |new_branch: bool| {
        let store = store.clone();
        let pending = pending.clone();
        move |_: &mut Window, cx: &mut App| {
            store.update(cx, |store, cx| {
                store.pending_git = None;
                store.confirm_git(pending.clone(), new_branch, cx);
                cx.notify();
            })
        }
    };
    let cancel = store.clone();
    alert(
        "git-confirm",
        pending.confirm.title.clone(),
        Some(pending.confirm.description.clone().into()),
        None,
        vec![
            AlertButton::new(pending.confirm.proceed.clone(), on(false)).prominent(),
            AlertButton::new(pending.confirm.branch_off.clone(), on(true)),
            AlertButton::new("Cancel", move |_, cx| {
                cancel.update(cx, |store, cx| {
                    store.pending_git = None;
                    cx.notify();
                })
            }),
        ],
        cx,
    )
}

const NOTICE_RADIUS: f32 = Radius::SHEET;
const NOTICE_PADDING: f32 = 14.;
/// The close button is this far from the top and the right.
const CLOSE_MARGIN: f32 = 8.;
const TITLE_HEIGHT: f32 = 16.;

/// What the last git action did, or what git refused, under the button. What follows is one click
/// away: the push after a commit, the pull request after a push.
pub fn git_notice(store: &Entity<Store>, notice: &GitNotice, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let close_size = ControlSize::Small.height();
    let close = store.clone();
    let next = store.clone();
    let view = store.clone();
    let url = notice.url.clone();
    let next_label = notice.next_label();
    let description = notice.description.clone().map(|description| {
        if !notice.failed {
            return div().text_size(px(12.5)).text_color(c.secondary).line_clamp(2).child(description);
        }
        // The end is where a hook says what it found.
        let lines: Vec<&str> = description.lines().collect();
        let shown = lines[lines.len().saturating_sub(8)..].join("\n");
        div().text_size(px(11.5)).font_family(crate::theme::MONO_FONT).text_color(c.secondary).child(shown)
    });
    div()
        .relative()
        .max_w(px(320.))
        .py(px(NOTICE_PADDING - 1.))
        .pl(px(NOTICE_PADDING - 1.))
        .pr(px(CLOSE_MARGIN + close_size + 4. - 1.))
        .bg(c.popover)
        .rounded(px(NOTICE_RADIUS))
        .border_1()
        .border_color(c.border_secondary)
        .shadow(vec![
            BoxShadow {
                color: hsla(0., 0., 0., 0.05),
                offset: point(px(0.), px(1.)),
                blur_radius: px(1.5),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.1),
                offset: point(px(0.), px(8.)),
                blur_radius: px(20.),
                spread_radius: px(0.),
                inset: false,
            },
        ])
        .flex()
        .items_start()
        .gap(px(10.))
        .child(
            div().h(px(TITLE_HEIGHT)).flex_shrink_0().flex().items_center().child(
                icons::symbol(if notice.failed { "circle-alert" } else { "circle-check" }, 15.)
                    .text_color(if notice.failed { c.danger } else { c.success }),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(4.))
                .child(
                    div()
                        .min_h(px(TITLE_HEIGHT))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .child(notice.title.clone()),
                )
                .children(description)
                .when(url.is_some() || next_label.is_some(), |column| {
                    column.child(
                        div()
                            .pt(px(6.))
                            .flex()
                            .gap(px(8.))
                            .when_some(url, |row, url| {
                                let variant = if next_label.is_none() { Variant::Primary } else { Variant::Secondary };
                                row.child(
                                    ActionButton::new("notice-pr", "View PR")
                                        .variant(variant)
                                        .surface(Surface::Popover)
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |store, cx| {
                                                store.show_pull_request(&url, cx);
                                                cx.notify();
                                            })
                                        }),
                                )
                            })
                            .when_some(next_label, |row, label| {
                                row.child(
                                    ActionButton::new("notice-next", label)
                                        .primary()
                                        .surface(Surface::Popover)
                                        .on_click(move |_, _, cx| {
                                            next.update(cx, |store, cx| {
                                                store.run_next_git(cx);
                                                cx.notify();
                                            })
                                        }),
                                )
                            }),
                    )
                }),
        )
        .child(div().absolute().top(px(CLOSE_MARGIN - 1.)).right(px(CLOSE_MARGIN - 1.)).child(
            ActionButton::icon("notice-close", "x", "Close").small().surface(Surface::Popover).on_click(
                move |_, _, cx| {
                    close.update(cx, |store, cx| {
                        store.dismiss_git_notice();
                        cx.notify();
                    })
                },
            ),
        ))
}

const ROW_HEIGHT: f32 = 24.;

/// The sheet behind the menu's Commit: the files to commit, which can be left out one by one, and
/// a message that is written for the user when they leave it empty.
pub struct CommitSheet {
    store: Entity<Store>,
    project: Project,
    files: Vec<ChangedFile>,
    message: Entity<TextareaState>,
    excluded: HashSet<String>,
    editing: bool,
    focus: FocusHandle,
}

impl CommitSheet {
    pub fn new(store: Entity<Store>, project: Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = store.read(cx).git_files.clone();
        let message = cx.new(|cx| TextareaState::new(window, cx).placeholder("Leave empty to have one written"));
        let focus = cx.focus_handle();
        message.update(cx, |message, cx| message.focus(window, cx));
        Self { store, project, files, message, excluded: HashSet::new(), editing: false, focus }
    }

    fn included(&self) -> Vec<String> {
        self.files.iter().map(|file| file.path.clone()).filter(|path| !self.excluded.contains(path)).collect()
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.committing_project = None;
            cx.notify();
        });
    }

    fn commit(&mut self, on_new_branch: bool, cx: &mut Context<Self>) {
        let included = self.included();
        if included.is_empty() {
            return;
        }
        let paths = if self.excluded.is_empty() { Vec::new() } else { included };
        let written = self.message.read(cx).value().trim().to_string();
        let project = self.project.clone();
        self.store.update(cx, |store, cx| {
            store.committing_project = None;
            store.run_git(GitAction::Commit, &project, Some(written), paths, on_new_branch, cx);
            cx.notify();
        });
    }

    fn file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let total = self.files.len();
        let included = total - self.excluded.len();
        let height = (total as f32 * ROW_HEIGHT).min(192.) + 8.;
        let mut rows = Vec::new();
        for (index, file) in self.files.iter().enumerate() {
            rows.push(self.row(index, file, cx));
        }
        let link = |id: &'static str, title: &'static str| {
            ActionButton::new(id, title).variant(Variant::Link).small().surface(Surface::Popover)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child("Files"))
                    .when(!self.excluded.is_empty() && !self.editing, |header| {
                        header.child(div().text_color(c.secondary).child(format!("{included} of {total}")))
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .my(px(-4.))
                            .mr(px(-ControlSize::Small.padding()))
                            .flex()
                            .when(self.editing, |links| {
                                let label = if self.excluded.is_empty() { "Select None" } else { "Select All" };
                                links.child(link("commit-select", label).on_click(cx.listener(|this, _, _, cx| {
                                    this.excluded = if this.excluded.is_empty() {
                                        this.files.iter().map(|file| file.path.clone()).collect()
                                    } else {
                                        HashSet::new()
                                    };
                                    cx.notify();
                                })))
                            })
                            .child(link("commit-edit", if self.editing { "Done" } else { "Edit" }).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.editing = !this.editing;
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .id("commit-files")
                    .h(px(height))
                    .overflow_y_scroll()
                    .px(px(10.))
                    .py(px(4.))
                    .bg(Surface::Popover.next().color(c))
                    .rounded(px(Radius::CONTROL))
                    .border_1()
                    .border_color(c.border_secondary)
                    .children(rows),
            )
    }

    fn row(&self, index: usize, file: &ChangedFile, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let excluded = self.excluded.contains(&file.path);
        let path = file.path.clone();
        div()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .when(self.editing, |row| {
                row.child(checkbox(("commit-file", index), !excluded, cx).on_click(cx.listener(
                    move |this, _, _, cx| {
                        if !this.excluded.remove(&path) {
                            this.excluded.insert(path.clone());
                        }
                        cx.notify();
                    },
                )))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .text_color(if excluded { c.tertiary } else { c.text })
                    .child(file.path.clone()),
            )
            .child(div().ml(px(8.)).flex_shrink_0().map(|end| {
                if excluded {
                    end.text_color(c.tertiary).child("Excluded")
                } else if file.added + file.removed > 0 {
                    end.child(line_counts(file.added, file.removed, cx))
                } else {
                    let change = match file.change {
                        Change::Added => "new",
                        Change::Modified => "modified",
                        Change::Deleted => "deleted",
                        Change::Renamed => "renamed",
                    };
                    end.text_color(c.tertiary).child(change)
                }
            }))
    }
}

/// The lines added and removed, as git counts them.
fn line_counts(added: u32, removed: u32, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex()
        .gap(px(4.))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .when(added > 0, |counts| counts.child(div().text_color(c.success).child(format!("+{added}"))))
        .when(removed > 0, |counts| counts.child(div().text_color(c.danger).child(format!("−{removed}"))))
}

/// A checkbox: a small rounded square, filled with the primary colour when on.
fn checkbox(id: impl Into<ElementId>, checked: bool, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    div()
        .id(id.into())
        .size(px(14.))
        .flex_shrink_0()
        .rounded(px(3.5))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |square| square.bg(c.primary).child(icons::symbol("check", 9.).text_color(white())))
        .when(!checked, |square| {
            square.bg(Surface::Popover.next().color(c)).border_1().border_color(c.border_secondary)
        })
}

impl Render for CommitSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let git = self.project.git.clone();
        let nothing = self.included().is_empty();
        let button = |id: &'static str, title: &'static str| ActionButton::new(id, title).surface(Surface::Popover);
        let content = div()
            .id("commit-sheet")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss(cx);
                }
            }))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Commit changes"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c.secondary)
                            .child("Review and confirm your commit. Leave the message empty to have one written."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .child(div().text_color(c.secondary).child("Branch"))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(git.as_ref().and_then(|git| git.branch.clone()).unwrap_or("No branch".into())),
                    )
                    .child(div().flex_1())
                    .when(git.as_ref().is_some_and(|git| git.default), |row| {
                        row.child(div().text_color(c.warning).child("Default branch"))
                    }),
            )
            .child(self.file_list(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child("Commit message (optional)"))
                    .child(
                        div()
                            .h(px(84.))
                            .px(px(4.))
                            .py(px(6.))
                            .bg(Surface::Popover.next().color(c))
                            .rounded(px(Radius::CONTROL))
                            .border_1()
                            .border_color(c.border)
                            .text_size(px(12.5))
                            .child(
                                Textarea::new(&self.message)
                                    .appearance(false)
                                    .xsmall()
                                    .text_size(px(12.5))
                                    .px_0()
                                    .py_0()
                                    .h_full(),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(div().flex_1())
                    .child(button("commit-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))))
                    .child(
                        button("commit-new-branch", "Commit on New Branch")
                            .disabled(nothing)
                            .on_click(cx.listener(|this, _, _, cx| this.commit(true, cx))),
                    )
                    .child(
                        button("commit", "Commit")
                            .primary()
                            .disabled(nothing)
                            .on_click(cx.listener(|this, _, _, cx| this.commit(false, cx))),
                    ),
            );
        sheet("commit", 460., content, cx)
    }
}
