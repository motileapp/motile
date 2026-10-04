//! The git button in the top bar of a thread, its menu, the commit sheet, and what git said.

use std::collections::HashSet;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Change, ChangedFile, GitAction};

use crate::models::Project;
use crate::store::{GitNotice, PendingGit, Store, stage_label};
use crate::theme::{colors, is_dark};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::button::Button;
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::sheet::sheet;
use crate::ui::{IconButton, TOOLBAR_MARGIN, TOOLBAR_WIDTH, icons, link_button, spinner, tooltip};

const HEIGHT: f32 = TOOLBAR_WIDTH - 2. * TOOLBAR_MARGIN;
/// What leaves the room under the button that the composer's menus leave over theirs.
const MENU_GAP: f32 = 16.;

fn purple(cx: &App) -> Hsla {
    if is_dark(cx) { rgb(0xbf5af2).into() } else { rgb(0xaf52de).into() }
}

/// Its left half does the one thing the repository calls for, at once: commit, push, open a pull
/// request, pull. Its right half opens the menu of all of them, where an item that can't run now
/// says why. While an action runs, the button says which stage it is at.
pub fn git_button(store: &Entity<Store>, anchor: &Anchor, cx: &App) -> Option<AnyElement> {
    let c = colors(cx);
    let project = store.read(cx).git_project()?;
    let control = project.git_control.clone()?;
    let stage = store.read(cx).git_stages.get(&project.id).copied();
    let quick = control.quick.clone();
    let runs = quick.action.is_some() || quick.url.is_some();
    let merged = quick.url.is_some()
        && project.git.as_ref().and_then(|git| git.pull_request.as_ref()).is_some_and(|pr| pr.merged);
    let help = quick
        .hint
        .clone()
        .or_else(|| project.git.as_ref().and_then(|git| git.pull_request.as_ref()).map(|pr| pr.title.clone()))
        .unwrap_or(quick.label.clone());
    let label = stage.map(|stage| stage_label(stage).to_string()).unwrap_or(quick.label.clone());
    let symbol_color = if merged {
        purple(cx)
    } else if runs {
        c.text
    } else {
        c.tertiary
    };
    let quick_store = store.clone();
    let quick_project = project.clone();
    let menu_store = store.clone();
    let menu_anchor = anchor.clone();
    Some(
        div()
            .mx(px(6.))
            .flex_shrink_0()
            .child(
                div()
                    .relative()
                    .flex()
                    .h(px(HEIGHT))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(c.strong_border)
                    .overflow_hidden()
                    .child(anchor.track())
                    .child(
                        div()
                            .id("git-quick")
                            .h_full()
                            .px(px(9.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if runs || stage.is_some() { c.text } else { c.tertiary })
                            .when(stage.is_none(), |half| half.hover(|half| half.bg(c.hover)))
                            .child(match stage {
                                Some(_) => spinner(12., cx).into_any_element(),
                                None => icons::symbol(icons::git_symbol(quick.action), 12.)
                                    .text_color(symbol_color)
                                    .into_any_element(),
                            })
                            .child(label)
                            .tooltip(tooltip(help))
                            .when(stage.is_none(), |half| {
                                half.on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    quick_store.update(cx, |store, cx| {
                                        store.run_quick_git(&quick_project, cx);
                                        cx.notify();
                                    });
                                })
                            }),
                    )
                    .child(div().w(px(1.)).h_full().bg(c.strong_border))
                    .child(
                        div()
                            .id("git-menu")
                            .w(px(24.))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|half| half.bg(c.hover))
                            .child(icons::symbol("chevron.down", 9.).text_color(c.secondary))
                            .tooltip(tooltip("Commit, push or open a pull request"))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                git_menu(&menu_store, &project, cx).show_right_aligned(
                                    menu_anchor.below_right(MENU_GAP),
                                    window,
                                    cx,
                                );
                            }),
                    ),
            )
            .into_any_element(),
    )
}

/// Every action, greyed with the reason when it can't run now, and the warning under them.
fn git_menu(store: &Entity<Store>, project: &Project, cx: &App) -> Menu {
    let Some(control) = project.git_control.clone() else { return Menu::new() };
    let running = store.read(cx).git_stages.contains_key(&project.id);
    let mut menu = Menu::new();
    for item in control.menu {
        let store = store.clone();
        let project = project.clone();
        let enabled = item.reason.is_none() && !running;
        let symbol = icons::git_symbol(Some(item.action));
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
    let abort = store.clone();
    alert(
        "git-confirm",
        pending.confirm.title.clone(),
        Some(pending.confirm.description.clone().into()),
        None,
        vec![
            AlertButton::new(pending.confirm.proceed.clone(), on(false)).prominent(),
            AlertButton::new(pending.confirm.branch_off.clone(), on(true)),
            AlertButton::new("Abort", move |_, cx| {
                abort.update(cx, |store, cx| {
                    store.pending_git = None;
                    cx.notify();
                })
            }),
        ],
        cx,
    )
}

const NOTICE_PADDING: f32 = 12.;
const CLOSE_SIZE: f32 = 24.;
/// The close button is this far from the top and the right, and its corners follow the notice's.
const CLOSE_MARGIN: f32 = 5.;
const NOTICE_RADIUS: f32 = 10.;

/// What the last git action did, or what git refused, under the button. What follows is one click
/// away: the push after a commit, the pull request after a push.
pub fn git_notice(store: &Entity<Store>, notice: &GitNotice, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let close = store.clone();
    let next = store.clone();
    div()
        .relative()
        .w(px(300.))
        .p(px(NOTICE_PADDING))
        .flex()
        .flex_col()
        .gap(px(6.))
        .bg(c.raised)
        .rounded(px(NOTICE_RADIUS))
        .border_1()
        .border_color(c.strong_border)
        .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.12), 4., 24.))
        .child(
            div()
                .pr(px(CLOSE_MARGIN + CLOSE_SIZE + 4. - NOTICE_PADDING))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.text)
                .child(notice.title.clone()),
        )
        .when_some(notice.description.clone(), |box_, description| {
            box_.child(if notice.failed {
                // The end is where a hook says what it found.
                let shown = description
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                div().text_size(px(12.)).font_family(crate::theme::MONO_FONT).text_color(c.danger).child(shown)
            } else {
                div().text_size(px(12.)).text_color(c.secondary).line_clamp(2).child(description)
            })
        })
        .when_some(notice.url.clone(), |box_, url| {
            box_.child(div().flex().child(link_button("notice-pr", "View PR", cx, move |_, _, cx| cx.open_url(&url))))
        })
        .when_some(notice.next_label(), |box_, label| {
            box_.child(div().flex().child(link_button("notice-next", label, cx, move |_, _, cx| {
                next.update(cx, |store, cx| {
                    store.run_next_git(cx);
                    cx.notify();
                })
            })))
        })
        .child(
            div().absolute().top(px(CLOSE_MARGIN)).right(px(CLOSE_MARGIN)).child(
                IconButton::new("notice-close", "xmark")
                    .help("Close")
                    .size(CLOSE_SIZE)
                    .symbol_size(11.)
                    .radius(NOTICE_RADIUS - CLOSE_MARGIN)
                    .color(c.secondary)
                    .on_click(move |_, _, cx| {
                        close.update(cx, |store, cx| {
                            store.dismiss_git_notice();
                            cx.notify();
                        })
                    }),
            ),
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
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child("Files"))
                    .when(!self.excluded.is_empty() && !self.editing, |header| {
                        header.child(div().text_color(c.secondary).child(format!("{included} of {total}")))
                    })
                    .child(div().flex_1())
                    .when(self.editing, |header| {
                        let label = if self.excluded.is_empty() { "Select None" } else { "Select All" };
                        header.child(div().mr(px(8.)).child(link_button(
                            "commit-select",
                            label,
                            cx,
                            cx.listener(|this, _, _, cx| {
                                this.excluded = if this.excluded.is_empty() {
                                    this.files.iter().map(|file| file.path.clone()).collect()
                                } else {
                                    HashSet::new()
                                };
                                cx.notify();
                            }),
                        )))
                    })
                    .child(link_button(
                        "commit-edit",
                        if self.editing { "Done" } else { "Edit" },
                        cx,
                        cx.listener(|this, _, _, cx| {
                            this.editing = !this.editing;
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .id("commit-files")
                    .h(px(height))
                    .overflow_y_scroll()
                    .px(px(10.))
                    .py(px(4.))
                    .bg(c.composer)
                    .rounded(px(7.))
                    .border_1()
                    .border_color(c.strong_border)
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

/// A checkbox as the Mac draws one: a small rounded square, filled with the accent when on.
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
        .when(checked, |square| square.bg(c.accent).child(icons::symbol("checkmark", 9.).text_color(white())))
        .when(!checked, |square| square.bg(c.composer).border_1().border_color(c.strong_border))
}

impl Render for CommitSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let git = self.project.git.clone();
        let nothing = self.included().is_empty();
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
                            .bg(c.composer)
                            .rounded(px(7.))
                            .border_1()
                            .border_color(c.strong_border)
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
                    .child(
                        Button::new("commit-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    )
                    .child(
                        Button::new("commit-new-branch", "Commit on New Branch")
                            .disabled(nothing)
                            .on_click(cx.listener(|this, _, _, cx| this.commit(true, cx))),
                    )
                    .child(
                        Button::new("commit", "Commit")
                            .prominent()
                            .disabled(nothing)
                            .on_click(cx.listener(|this, _, _, cx| this.commit(false, cx))),
                    ),
            );
        sheet("commit", 460., content, cx)
    }
}
