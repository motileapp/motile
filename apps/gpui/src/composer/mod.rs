//! Where messages are written. A strip above says what the agent waits for or that it is
//! monitoring. The model, the reasoning effort and how much the agent may do without asking are
//! under the text, and a strip below says where the thread works: the server, the folder and
//! the branch.

pub mod attachments;
mod questions;
mod strips;

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Access, Agent};

use self::questions::Questions;
use self::strips::{BranchPicker, context_strip, monitoring_strip, waiting_strip};
use crate::models::{ACCESSES, ThreadInfo, access_detail, access_label, agent_name};
use crate::store::Store;
use crate::theme::{self, ControlSize, Surface, colors};
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::{ActionButton, ActionMenu, Variant, edges, icons, logos};

pub const RADIUS: f32 = 22.;
/// Narrower than this, the model and the access are only their icons.
const COMPACT_BELOW: f32 = 560.;
/// The text's font and line, as the Mac sets them: 14 with 3 between lines.
const TEXT_SIZE: f32 = 14.;
const LINE_HEIGHT: f32 = 20.;
const TEXT_INSET: f32 = 4.;
/// Two lines, so the composer only grows when a third one starts.
const MIN_TEXT_HEIGHT: f32 = LINE_HEIGHT * 2. + TEXT_INSET * 2.;
const MAX_TEXT_HEIGHT: f32 = 220.;

pub struct Composer {
    store: Entity<Store>,
    text: Entity<TextareaState>,
    /// What the text was loaded from: the draft or thread it belongs to, and the counts that
    /// say it changed outside the composer or should take the keyboard.
    loaded: (String, u64, u64, u64),
    branch_anchor: Anchor,
    branch_picker: Option<Entity<BranchPicker>>,
    questions: HashMap<String, Entity<Questions>>,
    placeholder_shown: String,
    /// Redraws the monitoring strip's timer every second while the agent monitors.
    ticker: Option<Task<()>>,
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
            branch_anchor: Anchor::default(),
            branch_picker: None,
            questions: HashMap::new(),
            placeholder_shown: String::new(),
            ticker: None,
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
        let monitoring = store.selected_thread().is_some() && store.activity.monitoring;
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
        self.follow_approvals(window, cx);
        self.tick(monitoring, cx);
        cx.notify();
    }

    /// Keeps a view for the questions of each tool call that asks some.
    fn follow_approvals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            if self.questions.contains_key(&approval.id) {
                continue;
            }
            let store = self.store.clone();
            let id = approval.id.clone();
            self.questions.insert(id, cx.new(|cx| Questions::new(store, approval, window, cx)));
        }
    }

    fn tick(&mut self, monitoring: bool, cx: &mut Context<Self>) {
        if !monitoring {
            self.ticker = None;
            return;
        }
        if self.ticker.is_some() {
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        }));
    }

    fn placeholder(&self, cx: &App) -> String {
        let store = self.store.read(cx);
        let Some(server) = store.composer_server() else { return "Ask anything".into() };
        if store.selected_thread().is_some_and(ThreadInfo::is_done) {
            return "Message to bring it back".into();
        }
        if !server.connected() {
            return format!("Waiting for {}…", server.name);
        }
        if server.models.is_empty() && server.known {
            return "Install Claude Code or Codex".into();
        }
        if store.activity.running {
            return "Send a follow-up".into();
        }
        "Ask anything".into()
    }

    fn done_banner(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let (store, id) = (self.store.clone(), thread.id.clone());
        div()
            .pl(px(16.))
            .pr(px(16. - ControlSize::Small.padding()))
            .pt(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .child(icons::symbol("circle-check", 13.).text_color(c.success))
            .child(div().font_weight(FontWeight::MEDIUM).child("Done"))
            .child(div().flex_1())
            .child(div().my(px(-4.)).child(
                ActionButton::new("mark-undone", "Mark Undone").link().small().surface(Surface::Composer).on_click(
                    move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.set_done(std::slice::from_ref(&id), false, false, cx);
                            cx.notify();
                        })
                    },
                ),
            ))
    }

    /// The row under the text. When it is too narrow for all of it, the model and the access
    /// are only their icons. The space between its controls and around them is their margins,
    /// so each takes clicks up to the next one and to the composer's edges.
    fn controls(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let models = store.composer_models();
        let current = store.composer_model();
        let effort = store.composer_effort();
        let access = store.composer_access();
        let plan = store.composer_plan();
        let name = current.as_ref().map(|model| model.name.clone()).unwrap_or("No agent".into());

        let model_menu = {
            let store = self.store.clone();
            let (models, disabled) = (models.clone(), models.is_empty());
            let current_id = current.as_ref().map(|model| model.id.clone());
            let logo = current.as_ref().map(|model| logos::agent_icon(model.agent, ControlSize::Regular.symbol(), cx));
            let title = if compact && current.is_some() { String::new() } else { name.clone() };
            ActionMenu::new("model", title, move |_, _| {
                let mut menu = Menu::new();
                for agent in [Agent::Claude, Agent::Codex] {
                    let of_agent: Vec<_> = models.iter().filter(|model| model.agent == agent).cloned().collect();
                    if of_agent.is_empty() {
                        continue;
                    }
                    if !menu.is_empty() {
                        menu = menu.separator();
                    }
                    menu = menu.note(agent_name(agent));
                    for model in of_agent {
                        let store = store.clone();
                        let chosen = Some(&model.id) == current_id.as_ref();
                        menu =
                            menu.choice(chosen, model.name.clone(), Some(MenuIcon::Logo(agent)), None, move |_, cx| {
                                store.update(cx, |store, cx| {
                                    store.set_model(&model);
                                    cx.notify();
                                })
                            });
                    }
                }
                menu
            })
            .button(move |button| {
                button
                    .when_some(logo, |button, logo| button.picture(logo))
                    .help(name.clone())
                    .surface(Surface::Composer)
                    .margin(margin(8., 1.))
                    .disabled(disabled)
            })
        };

        let effort_menu = current.as_ref().filter(|model| !model.efforts.is_empty()).map(|model| {
            let store = self.store.clone();
            let efforts = model.efforts.clone();
            let chosen = effort.clone();
            ActionMenu::new("effort", effort_label(effort.as_deref().unwrap_or("")), move |_, _| {
                let mut menu = Menu::new();
                for effort in &efforts {
                    let store = store.clone();
                    let value = effort.clone();
                    menu =
                        menu.choice(Some(effort) == chosen.as_ref(), effort_label(effort), None, None, move |_, cx| {
                            store.update(cx, |store, cx| {
                                store.set_effort(&value);
                                cx.notify();
                            })
                        });
                }
                menu
            })
            .button(|button| button.surface(Surface::Composer).margin(margin(1., 1.)))
        });

        let access_menu = {
            let store = self.store.clone();
            let label = if plan { "Plan".to_string() } else { access_label(access).to_string() };
            let symbol = if plan { "clipboard-list" } else { access_symbol(access) };
            let help =
                if plan { "The agent only reads and proposes.".to_string() } else { access_detail(access).to_string() };
            ActionMenu::new("access", if compact { String::new() } else { label }, move |_, _| {
                let mut menu = Menu::new();
                for option in ACCESSES {
                    let store = store.clone();
                    let icon = Some(MenuIcon::Symbol(access_symbol(option)));
                    let detail = Some(SharedString::from(access_detail(option)));
                    menu = menu.choice(option == access, access_label(option), icon, detail, move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.set_access(option);
                            cx.notify();
                        })
                    });
                }
                let toggle = store.clone();
                menu.separator().choice(plan, "Plan mode", None, None, move |_, cx| {
                    toggle.update(cx, |store, cx| {
                        store.set_plan(!plan);
                        cx.notify();
                    })
                })
            })
            .button(move |button| button.symbol(symbol).help(help).surface(Surface::Composer).margin(margin(1., 1.)))
        };

        let attach = {
            let store = self.store.clone();
            ActionButton::icon("attach", "paperclip", "Attach files")
                .round(true)
                .surface(Surface::Composer)
                .margin(margin(1., 4.))
                .on_click(move |_, _, cx| choose_files(store.clone(), cx))
        };

        div()
            .flex()
            .items_center()
            .child(model_menu)
            .children(effort_menu)
            .child(access_menu)
            .child(div().flex_1().min_w(px(10.)))
            .child(attach)
            .child(self.send_buttons(cx))
    }

    /// Stops the turn that runs, and sends what is written or queues it behind that turn.
    fn send_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let running = store.activity.running && store.selected_thread().is_some();
        let can_send = store.can_send();
        let sends = !running || can_send;
        let steers = store.prefs.bool(Store::STEERS_KEY);
        let help = store.attachments_hold().map(String::from).unwrap_or_else(|| {
            match (running, steers) {
                (true, true) => "Send now",
                (true, false) => "Queue message",
                (false, _) => "Send",
            }
            .into()
        });
        let (stop, send) = (self.store.clone(), self.store.clone());
        div()
            .flex()
            .items_center()
            .when(running, |buttons| {
                buttons.child(
                    ActionButton::new("stop", "")
                        .picture(div().size(px(10.)).rounded(px(2.5)).bg(white()))
                        .help("Stop (⌘.)")
                        .danger()
                        .round(true)
                        .surface(Surface::Composer)
                        .margin(margin(4., if sends { 4. } else { 8. }))
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
                    ActionButton::icon("send", "arrow-up", help)
                        .variant(Variant::Primary)
                        .round(true)
                        .surface(Surface::Composer)
                        .margin(margin(4., 8.))
                        .disabled(!can_send)
                        .on_click(move |_, _, cx| {
                            send.update(cx, |store, cx| {
                                store.send_message(cx);
                                cx.notify();
                            })
                        }),
                )
            })
    }

    /// The box: the done banner, the attachments, the text and the controls under it.
    fn box_(&self, thread: Option<&ThreadInfo>, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let has_attachments = !store.attachments().is_empty();
        let drop_targeted = store.drop_targeted;
        let store_handle = self.store.clone();
        let text = Textarea::new(&self.text)
            .appearance(false)
            .xsmall()
            .text_size(px(TEXT_SIZE))
            .px(px(2.))
            .py(px(TEXT_INSET))
            .on_paste(move |clipboard, _, cx| {
                let mut files = Vec::new();
                let mut images = Vec::new();
                for entry in clipboard.entries() {
                    match entry {
                        ClipboardEntry::ExternalPaths(paths) => files.extend(paths.paths().iter().cloned()),
                        ClipboardEntry::Image(image) => images.push(image.clone()),
                        _ => {}
                    }
                }
                // Files copied in Finder and copied images are attached; anything else is
                // pasted as plain text.
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
            });
        let focus = self.text.clone();
        div()
            .id("composer-box")
            .relative()
            .flex()
            .flex_col()
            .bg(c.composer)
            .rounded(px(RADIUS))
            .border_1()
            .border_color(if drop_targeted { c.primary } else { c.border })
            .group_drag_over::<ExternalPaths>("main-drop", move |style| style.border_color(c.primary))
            .cursor_text()
            .on_click(move |_, window, cx| focus.update(cx, |text, cx| text.focus(window, cx)))
            .when_some(thread.filter(|thread| thread.is_done()), |composer, thread| {
                composer.child(self.done_banner(thread, cx))
            })
            .when(has_attachments, |composer| composer.child(attachments::attachments(&self.store, cx)))
            .child(
                div()
                    .px(px(14.))
                    .pt(px(12.))
                    .min_h(px(MIN_TEXT_HEIGHT))
                    .max_h(px(MAX_TEXT_HEIGHT))
                    .text_size(px(TEXT_SIZE))
                    .line_height(px(LINE_HEIGHT))
                    .child(text),
            )
            .child(self.controls(compact, cx))
    }
}

/// The room around a control of the row under the text.
fn margin(leading: f32, trailing: f32) -> Edges<f32> {
    edges(8., leading, 8., trailing)
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
        Access::Supervised => "shield",
        Access::AcceptEdits => "pen-line",
        Access::Auto => "sparkles",
        Access::Full => "lock-open",
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
        let store = self.store.read(cx);
        let thread = store.selected_thread().cloned();
        let approval = thread.as_ref().and_then(|_| store.activity.approvals.first().cloned());
        let count = store.activity.approvals.len();
        let monitoring = thread.is_some() && store.activity.monitoring;
        let project = store.composer_project();
        let server = project.as_ref().and_then(|project| store.server(Some(&project.server_id)).cloned());
        let compact = self.width < COMPACT_BELOW;

        div()
            .w_full()
            .max_w(px(theme::COMPOSER_WIDTH))
            .flex()
            .flex_col()
            .when_some(approval.as_ref(), |composer, approval| {
                let questions = self.questions.get(&approval.id).cloned();
                composer.child(waiting_strip(&self.store, approval, count, questions, cx))
            })
            .when_some(thread.as_ref().filter(|_| monitoring && approval.is_none()), |composer, thread| {
                composer.child(monitoring_strip(&self.store, thread, cx))
            })
            .child(self.box_(thread.as_ref(), compact, cx))
            .when_some(project, |composer, project| {
                composer.child(context_strip(
                    &self.store,
                    &project,
                    server.as_ref(),
                    &self.branch_anchor,
                    self.branch_picker.clone(),
                    cx,
                ))
            })
    }
}
