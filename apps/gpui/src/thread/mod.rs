//! The right side of the window: the open thread with the composer over its end, or the start
//! of a new one.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::composer::Composer;
use crate::models::last_component;
use crate::store::Store;
use crate::theme::{self, colors};
use crate::ui::button::Button;
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::{TOOLBAR_WIDTH, highlight, icons, logos};

/// The room above the composer, which the transcript fades out in.
pub const COMPOSER_GAP: f32 = 24.;
const HEADLINE_SIZE: f32 = 28.;

pub struct ThreadPane {
    store: Entity<Store>,
    composer: Entity<Composer>,
    transcript: Entity<crate::transcript::view::TranscriptView>,
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    pub title_inset: f32,
    /// The side panel is beside the pane, so the window's last button isn't over it.
    pub beside_panel: bool,
    /// How tall the composer was last drawn, which the transcript leaves room for.
    composer_height: Rc<Cell<f32>>,
    project_menu: Anchor,
    git_menu: Anchor,
    commit_sheet: Option<Entity<crate::git::CommitSheet>>,
}

impl ThreadPane {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_in(&store, window, |this, store, window, cx| {
            this.follow_commit_sheet(&store, window, cx);
            cx.notify();
        })
        .detach();
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
        let model = store.read(cx).transcript.clone();
        let transcript = cx.new(|cx| crate::transcript::view::TranscriptView::new(store.clone(), model, false, cx));
        Self {
            store,
            composer,
            transcript,
            title_inset: 20.,
            beside_panel: false,
            composer_height: Rc::new(Cell::new(120.)),
            project_menu: Anchor::default(),
            git_menu: Anchor::default(),
            commit_sheet: None,
        }
    }

    /// The commit sheet is up while the store says which project commits.
    fn follow_commit_sheet(&mut self, store: &Entity<Store>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = store.read(cx).committing_project.clone() else {
            self.commit_sheet = None;
            return;
        };
        if self.commit_sheet.is_none() {
            self.commit_sheet = Some(cx.new(|cx| crate::git::CommitSheet::new(store.clone(), project, window, cx)));
        }
    }

    /// How wide the pane is, which decides how much the composer's controls say.
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let composer = (width - 2. * theme::CONTENT_PADDING).min(theme::CONTENT_WIDTH);
        self.composer.update(cx, |view, cx| {
            if (view.width - composer).abs() > 0.5 {
                view.width = composer;
                cx.notify();
            }
        });
    }

    /// The thread's project and name, and its git button, drawn in the window's top bar.
    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.composer_project();
        let folder = store.selected_thread().map(|thread| last_component(&thread.cwd));
        let name = project.as_ref().map(|project| project.name.clone()).or(folder);
        let project_line = name.map(|name| match project.as_ref().and_then(|project| project.branch.clone()) {
            Some(branch) => format!("{name} · {branch}"),
            None => name,
        });
        let title = store.selected_thread().map(|thread| thread.title.clone()).unwrap_or("New thread".into());
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::TOP_BAR))
            .pl(px(self.title_inset))
            .pr(px(if self.beside_panel { 4. } else { TOOLBAR_WIDTH + 12. }))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .min_w_0()
                    .when_some(project_line, |title, line| {
                        title.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(logos::project_icon(project.as_ref(), 14., cx))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(c.secondary)
                                        .truncate()
                                        .child(line),
                                ),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.text)
                            .truncate()
                            .child(title),
                    ),
            )
            .child(div().flex_1())
            .children(crate::git::git_button(&self.store, &self.git_menu, cx))
    }

    /// The empty state of a new thread: a question, and the composer in the middle of the pane.
    fn start(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let no_projects = store.projects.is_empty();
        let connected = store.servers.iter().any(|server| server.connected());
        let add = self.store.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(26.))
            .child(div().flex_1())
            .when(no_projects, |start| {
                start.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(10.))
                        .child(div().text_size(px(28.)).child("Add a project to start"))
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(c.secondary)
                                .child("A project is a folder on your server that threads work in."),
                        )
                        .child(
                            div().pt(px(8.)).child(
                                Button::new("add-first-project", "Add Project")
                                    .large()
                                    .symbol("folder.badge.plus")
                                    .disabled(!connected)
                                    .on_click(move |_, _, cx| {
                                        add.update(cx, |store, cx| {
                                            store.add_project();
                                            cx.notify();
                                        })
                                    }),
                            ),
                        ),
                )
            })
            .when(!no_projects, |start| {
                start.child(self.headline(cx)).child(
                    div().w_full().px(px(theme::CONTENT_PADDING)).flex().justify_center().child(self.composer.clone()),
                )
            })
            .child(div().flex_1())
            .child(div().flex_1())
    }

    fn headline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.project(store.selected_draft().and_then(|draft| draft.project_id.as_deref())).cloned();
        let anchor = self.project_menu.clone();
        let handle = self.store.clone();
        let several_servers = store.servers.len() > 1;
        let projects: Vec<(String, String, Option<String>)> = store
            .recent_projects()
            .into_iter()
            .map(|project| {
                let name = if several_servers {
                    let server =
                        store.server(Some(&project.server_id)).map(|server| server.name.clone()).unwrap_or_default();
                    format!("{} · {server}", project.name)
                } else {
                    project.name.clone()
                };
                (project.id.clone(), name, project.icon_path.clone())
            })
            .collect();
        div()
            .flex()
            .items_center()
            .text_size(px(HEADLINE_SIZE))
            .child(div().text_color(c.secondary).child("Let’s build in "))
            .child(
                div()
                    .id("headline-project")
                    .group("headline-project")
                    .relative()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(c.strong_border)
                    .child(highlight("headline-project", 10., crate::ui::even(0.), false, cx))
                    .child(
                        div()
                            .relative()
                            .pl(px(9.))
                            .pr(px(11.))
                            .py(px(3.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(anchor.track())
                            .child(logos::project_icon(selected.as_ref(), 22., cx))
                            .child(selected.as_ref().map(|project| project.name.clone()).unwrap_or("a project".into()))
                            .child(icons::symbol("chevron.down", 13.).text_color(c.tertiary)),
                    )
                    .on_click(move |_, window, cx| {
                        let mut menu = Menu::new();
                        for (id, name, icon) in &projects {
                            let (store, id) = (handle.clone(), id.clone());
                            let icon = match icon {
                                Some(path) => MenuIcon::File(path.clone().into()),
                                None => MenuIcon::Symbol("folder"),
                            };
                            menu = menu.icon_item(name.clone(), icon, true, None, move |_, cx| {
                                store.update(cx, |store, cx| {
                                    store.set_new_thread_project(Some(id.clone()), cx);
                                    cx.notify();
                                })
                            });
                        }
                        let add = handle.clone();
                        menu = menu.separator().item("Add Project…", move |_, cx| {
                            add.update(cx, |store, cx| {
                                store.add_project();
                                cx.notify();
                            })
                        });
                        if let Some(project) = selected.clone() {
                            let (choose, reset, remove) = (handle.clone(), handle.clone(), handle.clone());
                            let (chosen, reset_project, removed) = (project.clone(), project.clone(), project.clone());
                            menu = menu
                                .item(format!("Choose an Icon for “{}”…", project.name), move |_, cx| {
                                    choose.update(cx, |store, cx| {
                                        store.icon_project = Some(chosen.clone());
                                        cx.notify();
                                    })
                                })
                                .item("Use the Icon in Its Folder", move |_, cx| {
                                    reset.update(cx, |store, cx| {
                                        store.set_icon(&reset_project, None);
                                        cx.notify();
                                    })
                                })
                                .item(format!("Remove “{}” from Projects", project.name), move |_, cx| {
                                    remove.update(cx, |store, cx| {
                                        store.remove_project(&removed);
                                        cx.notify();
                                    })
                                });
                        }
                        menu.show(anchor.below(), window, cx);
                    }),
            )
    }
}

impl Render for ThreadPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let is_start = store
            .selected_draft()
            .is_some_and(|draft| store.transcript_is_empty && !store.sending_draft_ids.contains(&draft.id));
        let pending_git = store.pending_git.clone();
        let notice = store
            .git_notice
            .clone()
            .filter(|notice| Some(&notice.project_id) == store.git_project().map(|project| project.id).as_ref());
        let measured = self.composer_height.clone();
        let inset = self.composer_height.get();
        self.transcript.update(cx, |transcript, cx| {
            if transcript.bottom_inset != inset {
                transcript.bottom_inset = inset;
                cx.notify();
            }
        });
        div()
            .id("thread-pane")
            .size_full()
            .relative()
            .child(if is_start {
                div().size_full().pt(px(theme::TOP_BAR)).child(self.start(cx)).into_any_element()
            } else {
                div()
                    .size_full()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top(px(theme::TOP_BAR))
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(self.transcript.clone()),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .px(px(theme::CONTENT_PADDING))
                            .pt(px(COMPOSER_GAP))
                            .pb(px(16.))
                            .flex()
                            .justify_center()
                            .child(
                                canvas(
                                    move |bounds, _, _| measured.set(f32::from(bounds.size.height)),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .inset_0(),
                            )
                            .child(self.composer.clone()),
                    )
                    .into_any_element()
            })
            .child(self.top_bar(cx))
            .when_some(notice, |pane, notice| {
                pane.child(div().absolute().top(px(theme::TOP_BAR + 6.)).right(px(14.)).child(crate::git::git_notice(
                    &self.store,
                    &notice,
                    cx,
                )))
            })
            .children(self.commit_sheet.clone())
            .when_some(pending_git, |pane, pending| pane.child(crate::git::confirm_alert(&self.store, &pending, cx)))
    }
}
