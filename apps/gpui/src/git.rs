//! The git button in the top bar of a thread, its menu and the commit sheet.

use std::collections::HashSet;

use gpui_kit::component::input::TextareaState;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Change, ChangedFile, GitAction};

use crate::models::Project;
use crate::store::{PendingGit, Store, stage_label};
use crate::theme::{ControlSize, Layer, Radius, Surface, colors};
use crate::ui::logos::{pull_request_color, pull_request_symbol};
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::modal::{Answer, Confirmation, Modal, confirmation, modal};
use crate::ui::{ActionButton, PickMark, TextArea, Variant, text_area_state};

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
    let pull_request = quick.pull_request.clone();
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
        (Some(_), _, _) => c.foreground,
        (None, Some(_), Some(pull_request)) if pull_request.draft => c.foreground,
        (None, Some(_), Some(pull_request)) => pull_request_color(pull_request, c),
        (None, None, _) if quick.action.is_none() => c.muted_stronger_foreground,
        _ => c.foreground,
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
            .rounded(px(Radius::MD))
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
            .child(div().w(px(1.)).h(px(ControlSize::Regular.height())).bg(c.border))
            .child(
                ActionButton::chevron("git-menu", "Commit, push or open a pull request").joined(true, true).on_click(
                    move |_, window, cx| {
                        git_menu(&menu_store, &project, cx).show_under_right(&menu_anchor, MENU_GAP, window, cx);
                    },
                ),
            )
            .child(div().absolute().inset_0().rounded(px(Radius::MD)).border_1().border_color(c.border))
            .into_any_element(),
    )
}

/// What stands in for the git button in a folder that is no git repository: it makes it one.
pub fn initialize_git_button(store: &Entity<Store>, project: &Project, cx: &App) -> impl IntoElement {
    let pending = store.read(cx).initializing_git.contains(&project.id);
    let (store, project) = (store.clone(), project.clone());
    div().mx(px(6.)).flex_shrink_0().child(
        ActionButton::new("initialize-git", "Initialize git")
            .symbol("git-branch")
            .help(format!("Make {} a git repository", project.name))
            .ghost()
            .pending(pending)
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.initialize_git(&project);
                    cx.notify();
                })
            }),
    )
}

/// What stands in for the git button in a draft that starts a new worktree, while the remote's
/// base has commits the local one lacks: it brings the local one up to date.
pub fn pull_base_button(store: &Entity<Store>, base: &str, cx: &App) -> impl IntoElement {
    let state = store.read(cx);
    let pending = state.pulling_draft_base();
    let start = state.draft_start().unwrap_or(base.to_string());
    let store = store.clone();
    div().mx(px(6.)).flex_shrink_0().child(
        ActionButton::new("pull-base", format!("Pull {base}"))
            .symbol(git_symbol(Some(GitAction::Pull)))
            .help(format!("Bring {base} up to date with {start}"))
            .ghost()
            .pending(pending)
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.pull_draft_base();
                    cx.notify();
                })
            }),
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

/// Asks where an action that would push from the default branch should happen: Cancel, branch
/// off, or go ahead there.
pub fn confirm_alert(store: &Entity<Store>, pending: &PendingGit, cx: &App) -> Modal {
    let on = |new_branch: bool| {
        let store = store.clone();
        let pending = pending.clone();
        move |_: &mut Window, cx: &mut App| {
            store.update(cx, |store, cx| {
                store.confirm_git(pending.clone(), new_branch, cx);
                cx.notify();
            })
        }
    };
    let cancel = store.clone();
    let asked = Confirmation::new(pending.confirm.title.clone())
        .message(pending.confirm.description.clone())
        .answer(Answer::new(pending.confirm.branch_off.clone(), on(true)).variant(Variant::Secondary))
        .answer(Answer::new(pending.confirm.proceed.clone(), on(false)));
    confirmation(
        "git-confirm",
        asked,
        move |_, cx| {
            cancel.update(cx, |store, cx| {
                store.pending_git = None;
                cx.notify();
            })
        },
        cx,
    )
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
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    /// It has closed and fades out.
    pub leaving: bool,
}

impl CommitSheet {
    pub fn new(store: Entity<Store>, project: Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = store.read(cx).git_files.clone();
        let message = text_area_state("Leave empty to have one written", 4, 12, window, cx);
        let focus = cx.focus_handle();
        Self {
            store,
            project,
            files,
            message,
            excluded: HashSet::new(),
            editing: false,
            scroll: UniformListScrollHandle::new(),
            focus,
            leaving: false,
        }
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
        let rows = uniform_list(
            "commit-files",
            total,
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                range.map(|index| this.row(index, &this.files[index], cx)).collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full()
        .px(px(10.))
        .py(px(4.));
        let link = |id: &'static str, title: &'static str| ActionButton::new(id, title).variant(Variant::Link).small();
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
                        header.child(div().text_color(c.muted_foreground).child(format!("{included} of {total}")))
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
                    .relative()
                    .h(px(height))
                    .bg(Surface::Background.layer(Layer::Box, c))
                    .rounded(px(Radius::MD))
                    .border_1()
                    .border_color(Surface::Background.border(c))
                    .child(rows)
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .child(Scrollbar::vertical(&self.scroll).id("commit-scroller").viewport_from_layout()),
                    ),
            )
    }

    fn row(&self, index: usize, file: &ChangedFile, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let excluded = self.excluded.contains(&file.path);
        let path = file.path.clone();
        div()
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .when(self.editing, |row| {
                row.child(div().id(("commit-file", index)).child(PickMark::new("pick", !excluded, true)).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if !this.excluded.remove(&path) {
                            this.excluded.insert(path.clone());
                        }
                        cx.notify();
                    }),
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .text_color(if excluded { c.muted_stronger_foreground } else { c.foreground })
                    .child(file.path.clone()),
            )
            .child(div().ml(px(8.)).flex_shrink_0().map(|end| {
                if excluded {
                    end.text_color(c.muted_stronger_foreground).child("Excluded")
                } else if file.added + file.removed > 0 {
                    end.child(line_counts(file.added, file.removed, cx))
                } else {
                    let change = match file.change {
                        Change::Added => "new",
                        Change::Modified => "modified",
                        Change::Deleted => "deleted",
                        Change::Renamed => "renamed",
                    };
                    end.text_color(c.muted_stronger_foreground).child(change)
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
        .font_features(crate::ui::tabular_numbers())
        .when(added > 0, |counts| counts.child(div().text_color(c.success).child(format!("+{added}"))))
        .when(removed > 0, |counts| counts.child(div().text_color(c.destructive).child(format!("−{removed}"))))
}

impl Render for CommitSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let git = self.project.git.clone();
        let nothing = self.included().is_empty();
        let button = |id: &'static str, title: &'static str| ActionButton::new(id, title);
        let content = div()
            .id("commit-sheet")
            .track_focus(&self.focus)
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Commit Changes"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c.muted_foreground)
                            .child("Review and confirm your commit. Leave the message empty to have one written."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .child(div().text_color(c.muted_foreground).child("Branch"))
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
                    .child(TextArea::new(&self.message)),
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
        let close = cx.entity().downgrade();
        let commit = cx.entity().downgrade();
        modal("commit", content, move |_, cx| {
            close.update(cx, |sheet, cx| sheet.dismiss(cx)).ok();
        })
        .on_return(move |_, cx| {
            commit.update(cx, |sheet, cx| sheet.commit(false, cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}
