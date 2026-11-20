//! Renaming a thread and asking before deleting one, which its menus start: its row's in the
//! sidebar and its title's in the top bar. The Mac app's `ThreadPrompts.swift`.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::Store;
use crate::theme::TextSize;
use crate::ui::menu::Menu;
use crate::ui::modal::{Answer, Confirmation, confirmation, modal, presented};
use crate::ui::{ActionButton, InputField, beside};

/// A thread being renamed or deleted, with what its alert needs.
#[derive(Clone)]
enum Asking {
    Rename { thread_id: String, title: Entity<InputState> },
    Delete { thread_id: String, title: String, in_worktree: bool },
}

pub struct ThreadPrompts {
    store: Entity<Store>,
    asking: Option<Asking>,
}

impl ThreadPrompts {
    pub fn new(store: Entity<Store>) -> Self {
        Self { store, asking: None }
    }

    pub fn rename(&mut self, thread_id: String, window: &mut Window, cx: &mut Context<Self>) {
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

    pub fn delete(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let Some(thread) = store.threads.get(&thread_id) else { return };
        let in_worktree =
            store.project(Some(&thread.project_id)).is_some_and(|project| project.seen(thread).worktree.is_some());
        self.asking = Some(Asking::Delete { thread_id, title: thread.title.clone(), in_worktree });
        cx.notify();
    }

    fn alert(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (asking, leaving) = presented("asking", self.asking.clone(), window, cx)?;
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
                let (rename, submit) = (this.clone(), this);
                let blank = title.read(cx).value().trim().is_empty();
                let content = div()
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(div().text_size(TextSize::Lg).font_weight(FontWeight::SEMIBOLD).child("Rename Thread"))
                    .child(InputField::new(&title))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .child(
                                ActionButton::new("rename-cancel", "Cancel").ghost().margin(beside(0., 4.)).on_click({
                                    let cancel = cancel.clone();
                                    move |_, window, cx| cancel(window, cx)
                                }),
                            )
                            .child(
                                ActionButton::new("rename", "Rename")
                                    .primary()
                                    .disabled(blank)
                                    .margin(beside(4., 0.))
                                    .on_click(move |_, _, cx| {
                                        let _ = rename.update(cx, |this, cx| this.finish_rename(cx));
                                    }),
                            ),
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
                let store = self.store.clone();
                let message = if in_worktree {
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

impl Render for ThreadPrompts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().absolute().children(self.alert(window, cx))
    }
}

/// What the open thread's title offers: finding in it, then what its row offers. Nothing while
/// an agent is shown in its place.
pub fn title_menu(store: &Entity<Store>, prompts: &Entity<ThreadPrompts>, cx: &App) -> Option<Menu> {
    let state = store.read(cx);
    let thread = state.selected_thread().filter(|_| state.right_panel.shown_agent.is_none())?;
    let find = store.clone();
    let menu = Menu::new().item("Search in thread", move |_, cx| {
        find.update(cx, |store, cx| store.open_find(None, None, cx));
    });
    let Some(row) = crate::sidebar::thread_menu(store, prompts, &thread.id, cx) else { return Some(menu) };
    Some(menu.separator().append(row))
}
