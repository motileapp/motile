//! Where messages are written: the text, and under it the model, the reasoning effort and how
//! much the agent may do without asking. A strip above says when the agent is monitoring, and
//! one below says where the thread works: the server, the folder and the branch.

pub mod attachments;
mod questions;
mod strips;

use std::collections::HashMap;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Access, Agent};

use self::questions::Questions;
use self::strips::{BranchPicker, context_strip, monitoring_strip};
use crate::models::{ACCESSES, ThreadInfo, access_detail, access_label, agent_name};
use crate::store::Store;
use crate::theme::{self, colors};
use crate::ui::button::Button;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::{IconButton, edges, highlight, icons, logos};

pub const RADIUS: f32 = 22.;
/// Narrower than this, the model and the access are only their icons.
const COMPACT_BELOW: f32 = 560.;

pub struct Composer {
    store: Entity<Store>,
    text: Entity<TextareaState>,
    /// What the text was loaded from: the draft or thread it belongs to, and the counts that
    /// say it changed outside the composer or should take the keyboard.
    loaded: (String, u64, u64, u64),
    model_menu: Anchor,
    effort_menu: Anchor,
    access_menu: Anchor,
    workspace_menu: Anchor,
    branch_anchor: Anchor,
    branch_picker: Option<Entity<BranchPicker>>,
    questions: HashMap<String, Entity<Questions>>,
    placeholder_shown: String,
    /// How wide the composer is, as the thread lays it out.
    pub width: f32,
    _subscriptions: Vec<Subscription>,
}

impl Composer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| {
            TextareaState::new(window, cx).auto_grow(2, 10).submit_on_enter(true).placeholder("Ask anything")
        });
        let subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| this.follow_store(window, cx)),
            cx.subscribe_in(&text, window, |this, text, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let value = text.read(cx).value().to_string();
                    this.store.update(cx, |store, cx| {
                        if store.draft() != value {
                            store.set_draft(value);
                            cx.notify();
                        }
                    });
                }
                InputEvent::PressEnter { shift: false, .. } => {
                    this.store.update(cx, |store, cx| {
                        store.send_message(cx);
                        cx.notify();
                    });
                }
                _ => {}
            }),
        ];
        let mut composer = Self {
            store,
            text,
            loaded: (String::new(), u64::MAX, u64::MAX, u64::MAX),
            model_menu: Anchor::default(),
            effort_menu: Anchor::default(),
            access_menu: Anchor::default(),
            workspace_menu: Anchor::default(),
            branch_anchor: Anchor::default(),
            branch_picker: None,
            questions: HashMap::new(),
            placeholder_shown: String::new(),
            width: theme::CONTENT_WIDTH,
            _subscriptions: subscriptions,
        };
        composer.follow_store(window, cx);
        composer
    }

    /// Shows the text of the draft or thread that is open, and takes the keyboard when it opens.
    fn follow_store(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let key = (store.draft_key(), store.draft_version, store.composer_focus, store.blur_composer);
        let placeholder = self.placeholder(cx);
        let wants_picker = store.shows_branches;
        if self.loaded != key {
            let draft = store.draft();
            let (focus, blur) = (key.0 != self.loaded.0 || key.2 != self.loaded.2, key.3 != self.loaded.3);
            self.loaded = key;
            self.text.update(cx, |text, cx| {
                if text.value().as_ref() != draft {
                    text.set_value(draft, window, cx);
                }
                if focus {
                    text.focus(window, cx);
                }
            });
            if blur {
                window.blur(cx);
            }
        }
        if self.placeholder_shown != placeholder {
            self.placeholder_shown = placeholder.clone();
            self.text.update(cx, |text, cx| text.set_placeholder(placeholder, window, cx));
        }
        if wants_picker && self.branch_picker.is_none() {
            let store = self.store.clone();
            self.branch_picker = Some(cx.new(|cx| BranchPicker::new(store, window, cx)));
        } else if !wants_picker {
            self.branch_picker = None;
        }
        let approvals: Vec<_> = self
            .store
            .read(cx)
            .activity
            .approvals
            .iter()
            .filter(|approval| !approval.questions.is_empty())
            .cloned()
            .collect();
        self.questions.retain(|id, _| approvals.iter().any(|approval| &approval.id == id));
        for approval in approvals {
            if !self.questions.contains_key(&approval.id) {
                let store = self.store.clone();
                let id = approval.id.clone();
                self.questions.insert(id, cx.new(|cx| Questions::new(store, approval, window, cx)));
            }
        }
        cx.notify();
    }

    fn placeholder(&self, cx: &App) -> String {
        let store = self.store.read(cx);
        let Some(server) = store.composer_server() else { return "Ask anything".into() };
        if !server.connected() {
            return format!("Waiting for {} to connect…", server.name);
        }
        if server.models.is_empty() && server.known {
            return format!("Install Claude Code or Codex on {} to start", server.name);
        }
        if store.activity.running {
            return "Send a follow-up; it waits for the agent's turn to end".into();
        }
        "Ask anything, or describe what to build".into()
    }

    fn done_banner(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let (store, id) = (self.store.clone(), thread.id.clone());
        div()
            .pl(px(16.))
            .pr(px(16. - 9.))
            .pt(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .child(icons::symbol("checkmark.circle", 13.).text_color(c.success))
            .child(div().font_weight(FontWeight::MEDIUM).child("This thread is done."))
            .child(div().text_color(c.secondary).child("Send a message to bring it back."))
            .child(div().flex_1())
            .child(
                div()
                    .id("mark-undone")
                    .group("mark-undone")
                    .relative()
                    .my(px(-4.))
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .child(crate::ui::highlight_in(
                        "mark-undone",
                        7.,
                        edges(0., 0., 0., 0.),
                        false,
                        c.primary_hover,
                        cx,
                    ))
                    .child(div().relative().font_weight(FontWeight::MEDIUM).text_color(c.primary).child("Mark Undone"))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.set_done(std::slice::from_ref(&id), false, false, cx);
                            cx.notify();
                        })
                    }),
            )
    }

    /// The tool calls the agent waits with: each is allowed or refused, and one that asks
    /// questions is answered.
    fn approvals(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let approvals = self.store.read(cx).activity.approvals.clone();
        div()
            .mx(px(10.))
            .mt(px(10.))
            .p(px(12.))
            .rounded(px(12.))
            .bg(c.warning_background)
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(12.5))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.warning)
                    .child("Waiting for you"),
            )
            .children(approvals.into_iter().map(|approval| {
                if let Some(questions) = self.questions.get(&approval.id) {
                    return questions.clone().into_any_element();
                }
                let (refuse, allow) = (self.store.clone(), self.store.clone());
                let (refuse_id, allow_id) = (approval.id.clone(), approval.id.clone());
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol(icons::tool_symbol(approval.icon), 12.).text_color(c.secondary))
                    .child(div().font_weight(FontWeight::MEDIUM).flex_shrink_0().child(approval.title.clone()))
                    .child(
                        div()
                            .min_w_0()
                            .font_family(theme::MONO_FONT)
                            .text_size(px(12.))
                            .truncate()
                            .child(approval.target.clone()),
                    )
                    .child(div().flex_1().min_w(px(8.)))
                    .child(
                        Button::new(SharedString::from(format!("refuse-{}", approval.id)), approval.refuse)
                            .small()
                            .on_click(move |_, _, cx| {
                                refuse.update(cx, |store, cx| {
                                    store.answer(&refuse_id, false, HashMap::new());
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("allow-{}", approval.id)), approval.allow)
                            .small()
                            .prominent()
                            .on_click(move |_, _, cx| {
                                allow.update(cx, |store, cx| {
                                    store.answer(&allow_id, true, HashMap::new());
                                    cx.notify();
                                })
                            }),
                    )
                    .into_any_element()
            }))
    }

    /// A control of the row under the text: a menu's current choice, with a chevron.
    fn control(
        &self,
        id: &'static str,
        title: Option<String>,
        icon: Option<AnyElement>,
        margin: Edges<f32>,
        anchor: &Anchor,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = colors(cx);
        div()
            .id(id)
            .group(id)
            .relative()
            .flex_shrink_0()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .pl(px(margin.left))
            .pr(px(margin.right))
            .child(highlight(id, 9., margin, false, cx))
            .child(
                div()
                    .relative()
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(c.secondary)
                    .child(anchor.track())
                    .children(icon)
                    .when_some(title, |control, title| {
                        control.child(
                            div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).whitespace_nowrap().child(title),
                        )
                    })
                    .child(icons::symbol("chevron.down", 9.).text_color(c.tertiary)),
            )
    }

    fn controls(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let models = store.composer_models();
        let current = store.composer_model();
        let effort = store.composer_effort();
        let access = store.composer_access();
        let plan = store.composer_plan();
        let name = current.as_ref().map(|model| model.name.clone()).unwrap_or("No agent".into());

        let margin = |leading: f32, trailing: f32| edges(8., leading, 8., trailing);
        let model_icon = current.as_ref().map(|model| logos::agent_icon(model.agent, 14., cx).into_any_element());
        let model_title = if compact && current.is_some() { None } else { Some(name.clone()) };
        let model_menu = {
            let anchor = self.model_menu.clone();
            let store = self.store.clone();
            let models = models.clone();
            let current_id = current.as_ref().map(|model| model.id.clone());
            self.control("model", model_title, model_icon, margin(7., 1.), &self.model_menu, cx)
                .tooltip(crate::ui::tooltip(name))
                .when(!models.is_empty(), |control| {
                    control.on_click(move |_, window, cx| {
                        let mut menu = Menu::new();
                        for agent in [Agent::Claude, Agent::Codex] {
                            let of_agent: Vec<_> =
                                models.iter().filter(|model| model.agent == agent).cloned().collect();
                            if of_agent.is_empty() {
                                continue;
                            }
                            if menu_has_items(&menu) {
                                menu = menu.separator();
                            }
                            menu = menu.item_if(false, agent_name(agent), |_, _| {});
                            for model in of_agent {
                                let store = store.clone();
                                let chosen = Some(&model.id) == current_id.as_ref();
                                let logo = if agent == Agent::Claude { "icons/claude.svg" } else { "icons/openai.svg" };
                                let _ = logo;
                                menu = menu.checked(chosen, model.name.clone(), move |_, cx| {
                                    store.update(cx, |store, cx| {
                                        store.set_model(&model);
                                        cx.notify();
                                    })
                                });
                            }
                        }
                        menu.show(anchor.below(), window, cx);
                    })
                })
        };

        let effort_menu = current.as_ref().filter(|model| !model.efforts.is_empty()).map(|model| {
            let anchor = self.effort_menu.clone();
            let store = self.store.clone();
            let efforts = model.efforts.clone();
            let chosen = effort.clone();
            self.control(
                "effort",
                Some(effort_label(effort.as_deref().unwrap_or(""))),
                None,
                margin(1., 1.),
                &self.effort_menu,
                cx,
            )
            .on_click(move |_, window, cx| {
                let mut menu = Menu::new();
                for effort in &efforts {
                    let store = store.clone();
                    let value = effort.clone();
                    menu = menu.checked(Some(effort) == chosen.as_ref(), effort_label(effort), move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.set_effort(&value);
                            cx.notify();
                        })
                    });
                }
                menu.show(anchor.below(), window, cx);
            })
        });

        let access_menu = {
            let anchor = self.access_menu.clone();
            let store = self.store.clone();
            let label = if plan { "Plan".to_string() } else { access_label(access).to_string() };
            let symbol = if plan { "list.bullet.clipboard" } else { access_symbol(access) };
            let help =
                if plan { "The agent only reads and proposes.".to_string() } else { access_detail(access).to_string() };
            self.control(
                "access",
                (!compact).then_some(label),
                Some(icons::symbol(symbol, 13.).into_any_element()),
                margin(1., 1.),
                &self.access_menu,
                cx,
            )
            .tooltip(crate::ui::tooltip(help))
            .on_click(move |_, window, cx| {
                let mut menu = Menu::new();
                for option in ACCESSES {
                    let store = store.clone();
                    menu = menu.checked(option == access, access_label(option), move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.set_access(option);
                            cx.notify();
                        })
                    });
                }
                let toggle = store.clone();
                menu.separator()
                    .checked(plan, "Plan mode", move |_, cx| {
                        toggle.update(cx, |store, cx| {
                            store.set_plan(!plan);
                            cx.notify();
                        })
                    })
                    .show(anchor.below(), window, cx);
            })
        };

        let attach = {
            let store = self.store.clone();
            div().flex().text_color(c.secondary).child(
                IconButton::new("attach", "paperclip")
                    .help("Attach files")
                    .size(30.)
                    .symbol_size(15.)
                    .color(c.secondary)
                    .inset(margin(1., 4.))
                    .on_click(move |_, _, cx| choose_files(store.clone(), cx)),
            )
        };

        div()
            .flex()
            .items_center()
            .child(model_menu)
            .children(effort_menu)
            .child(access_menu)
            .child(div().flex_1().min_w(px(10.)))
            .child(attach)
            .child(self.primary_buttons(cx))
    }

    fn primary_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let running = store.activity.running && store.selected_thread().is_some();
        let can_send = store.can_send();
        let sends = !running || can_send;
        let help = store
            .attachments_hold()
            .map(String::from)
            .unwrap_or_else(|| if running { "Queue message".into() } else { "Send".into() });
        let (stop, send) = (self.store.clone(), self.store.clone());
        div()
            .flex()
            .items_center()
            .when(running, |buttons| {
                buttons.child(
                    div()
                        .id("stop")
                        .py(px(8.))
                        .pl(px(4.))
                        .pr(px(if sends { 4. } else { 8. }))
                        .tooltip(crate::ui::tooltip("Stop (⌘.)"))
                        .child(
                            div()
                                .size(px(30.))
                                .rounded_full()
                                .bg(c.danger.opacity(0.9))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(div().size(px(10.)).rounded(px(2.5)).bg(white())),
                        )
                        .on_click(move |_, _, cx| {
                            stop.update(cx, |store, cx| {
                                store.stop_thread();
                                cx.notify();
                            })
                        }),
                )
            })
            .when(sends, |buttons| {
                buttons.child(
                    div()
                        .id("send")
                        .py(px(8.))
                        .pl(px(4.))
                        .pr(px(8.))
                        .tooltip(crate::ui::tooltip(help))
                        .child(
                            div()
                                .size(px(30.))
                                .rounded_full()
                                .bg(if can_send { c.primary } else { c.selected })
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icons::symbol("arrow.up", 14.).text_color(if can_send {
                                    white()
                                } else {
                                    c.secondary
                                })),
                        )
                        .when(can_send, |button| {
                            button.on_click(move |_, _, cx| {
                                send.update(cx, |store, cx| {
                                    store.send_message(cx);
                                    cx.notify();
                                })
                            })
                        }),
                )
            })
    }
}

fn menu_has_items(menu: &Menu) -> bool {
    !menu.is_empty()
}

pub fn effort_label(effort: &str) -> String {
    match effort {
        "xhigh" => "Extra high".into(),
        "" => "Default".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

pub fn access_symbol(access: Access) -> &'static str {
    match access {
        Access::Supervised => "lock",
        Access::AcceptEdits => "pencil.line",
        Access::Auto => "sparkles",
        Access::Full => "lock.open",
    }
}

/// Asks for files to attach, in the system's file dialog.
pub fn choose_files(store: Entity<Store>, cx: &mut App) {
    let chosen =
        cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = chosen.await else { return };
        store.update(cx, |store, cx| {
            store.attach(paths);
            cx.notify();
        });
    })
    .detach();
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let thread = store.selected_thread().cloned();
        let monitoring = thread.is_some() && store.activity.monitoring;
        let has_approvals = thread.is_some() && !store.activity.approvals.is_empty();
        let has_attachments = !store.attachments().is_empty();
        let project = store.composer_project();
        let server = project.as_ref().and_then(|project| store.server(Some(&project.server_id)).cloned());
        let compact = self.width < COMPACT_BELOW;
        let drop_targeted = store.drop_targeted;
        let store_handle = self.store.clone();

        let text = Textarea::new(&self.text).appearance(false).xsmall().text_size(px(14.)).px_0().py_0();

        div()
            .w_full()
            .max_w(px(theme::CONTENT_WIDTH))
            .flex()
            .flex_col()
            .when(monitoring, |composer| composer.child(monitoring_strip(&self.store, cx)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .bg(c.composer)
                    .rounded(px(RADIUS))
                    .border(px(if drop_targeted { 2. } else { 1. }))
                    .border_color(if drop_targeted { c.primary } else { c.strong_border })
                    .group_drag_over::<ExternalPaths>("main-drop", |style| style.border_color(c.primary))
                    .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.10), 8., 18.))
                    .when_some(thread.as_ref().filter(|thread| thread.is_done()), |composer, thread| {
                        composer.child(self.done_banner(thread, cx))
                    })
                    .when(has_approvals, |composer| composer.child(self.approvals(cx)))
                    .when(has_attachments, |composer| composer.child(attachments::attachments(&self.store, cx)))
                    .child(
                        div()
                            .pl(px(12.))
                            .pr(px(14.))
                            .pt(px(14.5))
                            .min_h(px(55.))
                            .max_h(px(220.))
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .child(text.on_paste(move |clipboard, _, cx| {
                                let mut files = Vec::new();
                                let mut images = Vec::new();
                                for entry in clipboard.entries() {
                                    match entry {
                                        ClipboardEntry::ExternalPaths(paths) => {
                                            files.extend(paths.paths().iter().cloned())
                                        }
                                        ClipboardEntry::Image(image) => images.push(image.clone()),
                                        _ => {}
                                    }
                                }
                                let has_text = clipboard.text().is_some();
                                if files.is_empty() && (images.is_empty() || has_text) {
                                    return false;
                                }
                                store_handle.update(cx, |store, cx| {
                                    if !files.is_empty() {
                                        store.attach(files);
                                    } else {
                                        for image in &images {
                                            store.attach_image(image);
                                        }
                                    }
                                    cx.notify();
                                });
                                true
                            })),
                    )
                    .child(self.controls(compact, cx)),
            )
            .when_some(project, |composer, project| {
                composer.child(context_strip(
                    &self.store,
                    &project,
                    server.as_ref(),
                    &self.workspace_menu,
                    &self.branch_anchor,
                    self.branch_picker.clone(),
                    cx,
                ))
            })
    }
}
