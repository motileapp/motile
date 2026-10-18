//! Where messages are written. A strip above says what the agent waits for or that it is
//! monitoring. The model, the reasoning effort and how much the agent may do without asking are
//! under the text, and a strip below says where the thread works: the server, the folder and
//! the branch.

pub mod attachments;
mod questions;
mod skills;
mod strips;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Enter, InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Access, Interruption};

use self::questions::Questions;
use self::strips::{BranchPicker, context_strip, interruption_strip, monitoring_strip, waiting_strip};
use crate::models::{ACCESSES, ModelInfoExt, ThreadInfo, access_detail, access_label};
use crate::store::Store;
use crate::theme::{self, ControlSize, Surface, colors};
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::{ActionButton, ActionMenu, Variant, edges, icons, logos, text_width};

pub const RADIUS: f32 = theme::Radius::XXL;
/// The text's font and line, as the Mac sets them: 14 with 3 between lines.
const TEXT_SIZE: f32 = 14.;
const LINE_HEIGHT: f32 = 20.;
/// The room above the first line and under the last, which puts the lines where the Mac's are:
/// its lines are only spaced between each other.
const TEXT_TOP: f32 = 2.;
const TEXT_BOTTOM: f32 = 1.;
/// Two lines, so the composer only grows when a third one starts.
const MIN_TEXT_HEIGHT: f32 = LINE_HEIGHT * 2. + TEXT_TOP + TEXT_BOTTOM;
const MAX_TEXT_HEIGHT: f32 = 220.;
/// The room from the composer's sides to the text, less the text's own.
const TEXT_SIDE: f32 = 12.;
/// The room between the model's or the access's symbol and its words.
const MENU_GAP: f32 = 5.;
/// The least room between the menus and the buttons on the right.
const SPACER: f32 = 10.;

/// Which menus of the row under the text say what they are set to, the first of these that fits.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Labels {
    #[default]
    All,
    /// The effort and the access are only their icons.
    Model,
    /// The model is only its icon too.
    None,
}

impl Labels {
    /// The first that fits in `room`, for menus that were `menus` wide with `drawn`, whose words
    /// take `settings` for the effort and the access and `model` for the model.
    fn fitting(menus: f32, drawn: Labels, settings: f32, model: f32, room: f32) -> Self {
        let hidden = match drawn {
            Labels::All => 0.,
            Labels::Model => settings,
            Labels::None => settings + model,
        };
        let full = menus + hidden;
        if full <= room {
            return Labels::All;
        }
        if full - settings <= room {
            return Labels::Model;
        }
        Labels::None
    }
}

/// How wide the parts of the row under the text were last drawn, to tell whether the menus fit
/// with their words.
#[derive(Clone, Copy, Default)]
struct RowWidths {
    /// The menus, and which of them were drawn with their words.
    menus: f32,
    labels: Labels,
    /// The buttons on the right.
    buttons: f32,
}

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
    /// Redraws the strips' times: every second while the agent monitors, every 30 seconds while
    /// an interruption says when the thread goes on.
    ticker: Option<(u64, Task<()>)>,
    row: Rc<Cell<RowWidths>>,
    /// How tall the strips above and under the box were last drawn, for the outline that casts
    /// their shadow.
    strips: Rc<Cell<(f32, f32)>>,
    /// The draft's worktree and its project's git state as last seen, which say when to ask what
    /// the worktree would start at.
    worktree_key: Option<String>,
    project_git: Option<motile_protocol::wire::GitStatus>,
    /// The project whose skills were last listed ahead of the menu.
    skills_key: Option<(String, String)>,
    /// How wide the composer is, as the thread lays it out.
    pub width: f32,
    skills: skills::SkillTyping,
    _subscriptions: Vec<Subscription>,
}

impl Composer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 11).placeholder("Ask anything"));
        let subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| this.follow_store(window, cx)),
            cx.subscribe_in(&text, window, |this, text, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = text.read(cx).value().to_string();
                    this.write(value, cx);
                    this.chipify_typed(window, cx);
                }
                InputEvent::Focus | InputEvent::Blur => this.report(window, cx),
                _ => {}
            }),
            cx.observe_in(&text, window, |this, _, window, cx| this.report(window, cx)),
        ];
        let picked = store.read(cx).skills.pick.as_ref().map_or(0, |pick| pick.id);
        let mut composer = Self {
            store,
            text,
            loaded: (String::new(), u64::MAX, u64::MAX, u64::MAX),
            branch_anchor: Anchor::default(),
            branch_picker: None,
            questions: HashMap::new(),
            placeholder_shown: String::new(),
            ticker: None,
            row: Rc::default(),
            strips: Rc::default(),
            worktree_key: None,
            project_git: None,
            skills_key: None,
            width: theme::CONTENT_WIDTH,
            skills: skills::SkillTyping::after(picked),
            _subscriptions: subscriptions,
        };
        composer.follow_store(window, cx);
        composer
    }

    /// Keeps what is written in the store. Only whether there is anything to send is shown
    /// outside the composer, so only that change is told.
    fn write(&self, value: String, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            let before = store.draft();
            if before == value {
                return;
            }
            let told = before.trim().is_empty() != value.trim().is_empty();
            store.set_draft(value);
            if told {
                cx.notify();
            }
        });
    }

    /// Return sends; with Shift or Option it is a line break. While an input method is composing,
    /// Return belongs to it.
    fn on_enter(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if action.shift || self.composing(window, cx) {
            return;
        }
        cx.stop_propagation();
        self.store.update(cx, |store, cx| {
            store.send_message(false, cx);
            cx.notify();
        });
    }

    /// Option-Return and Control-Return are line breaks too, which no key binding of the text has.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key != "enter" || !(modifiers.alt || modifiers.control) || self.composing(window, cx) {
            return;
        }
        cx.stop_propagation();
        self.text.update(cx, |text, cx| text.insert("\n", window, cx));
        let value = self.text.read(cx).value().to_string();
        self.write(value, cx);
    }

    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.text.update(cx, |text, cx| text.marked_text_range(window, cx).is_some())
    }

    /// A click in the composer's empty room starts typing where the cursor was.
    fn press_beside_text(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.text.read(cx).input_bounds().contains(&position) {
            return;
        }
        self.text.update(cx, |text, cx| text.focus(window, cx));
    }

    /// Shows the text of the draft or thread that is open, and takes the keyboard when it opens.
    fn follow_store(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let key = (store.draft_key(), store.draft_version, store.composer_focus, store.blur_composer);
        let placeholder = self.placeholder(cx);
        let wants_picker = store.shows_branches;
        let tick = self.tick_period(cx);
        if self.loaded != key {
            let draft = store.draft();
            let (focus, blur) = (key.0 != self.loaded.0 || key.2 != self.loaded.2, key.3 != self.loaded.3);
            self.loaded = key;
            let loads = self.text.update(cx, |text, cx| {
                let loads = text.value().as_ref() != draft;
                if loads {
                    let end = draft.len();
                    text.set_value(draft, window, cx);
                    text.set_selected_range(end..end, cx);
                }
                if focus {
                    text.focus(window, cx);
                }
                loads
            });
            if blur {
                window.blur(cx);
            }
            if loads || focus {
                self.loaded_text(window, cx);
            }
        }
        self.follow_skills(window, cx);
        if self.placeholder_shown != placeholder {
            self.placeholder_shown = placeholder.clone();
            self.text.update(cx, |text, cx| text.set_placeholder(placeholder, window, cx));
        }
        if wants_picker && self.branch_picker.is_none() {
            let (store, button) = (self.store.clone(), self.branch_anchor.clone());
            self.branch_picker = Some(cx.new(|cx| BranchPicker::new(store, button, window, cx)));
        } else if !wants_picker {
            self.branch_picker = None;
        }
        crate::ui::popover_shown("branch-picker", wants_picker, cx);
        self.follow_approvals(window, cx);
        self.tick(tick, cx);
        self.follow_worktree_start(cx);
        self.follow_skills_key(cx);
        cx.notify();
    }

    /// Has the skills of the composer's project listed once it has one, before its menu opens.
    fn follow_skills_key(&mut self, cx: &mut Context<Self>) {
        let key = self.store.read(cx).composer_skills_key();
        if key == self.skills_key {
            return;
        }
        self.skills_key = key;
        self.store.update(cx, |store, _| store.prefetch_skills());
    }

    /// Asks what the draft's worktree would start at when it is another one, fetching first, and
    /// again without fetching when its project's git state changes.
    fn follow_worktree_start(&mut self, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let key = store.draft_worktree_key();
        let git = store.composer_project().and_then(|project| project.git);
        let fetch = key != self.worktree_key;
        let changed = git != self.project_git;
        self.worktree_key = key.clone();
        self.project_git = git;
        if key.is_none() || !(fetch || changed) {
            return;
        }
        self.store.update(cx, |store, _| store.read_worktree_start(fetch));
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

    /// How often a strip's time changes, in seconds, while one shows a time.
    fn tick_period(&self, cx: &App) -> Option<u64> {
        let store = self.store.read(cx);
        let thread = store.selected_thread()?;
        if store.activity.monitoring {
            return Some(1);
        }
        interruption(thread, store).map(|_| 30)
    }

    fn tick(&mut self, period: Option<u64>, cx: &mut Context<Self>) {
        let Some(period) = period else {
            self.ticker = None;
            return;
        };
        if self.ticker.as_ref().is_some_and(|(running, _)| *running == period) {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(period)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        });
        self.ticker = Some((period, task));
    }

    fn placeholder(&self, cx: &App) -> String {
        let store = self.store.read(cx);
        let Some(server) = store.composer_server() else { return "Ask anything".into() };
        if store.selected_thread().is_some_and(ThreadInfo::is_done) {
            return "Message to bring it back".into();
        }
        if !server.connected() {
            return format!("Waiting for {}", server.name);
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

    /// Which menus of the row under the text have room to say what they are set to, as the
    /// Mac's `ViewThatFits` picks: from how wide they were last drawn, and how wide their words
    /// are.
    fn labels(&self, window: &mut Window, cx: &App) -> Labels {
        let store = self.store.read(cx);
        let words = |text: &str, window: &mut Window| {
            text_width(text, ControlSize::Regular.text_size(), FontWeight::MEDIUM, window) + MENU_GAP
        };
        let mut settings = words(access_label(store.composer_access()), window);
        let model = store.composer_model();
        if model.as_ref().is_some_and(|model| !model.efforts.is_empty()) {
            settings += words(&effort_label(&store.composer_effort().unwrap_or_default()), window);
        }
        let model = match model {
            Some(_) => words(&store.composer_model_label().unwrap_or_default(), window),
            None => 0.,
        };
        let row = self.row.get();
        Labels::fitting(row.menus, row.labels, settings, model, self.width - SPACER - row.buttons)
    }

    /// The row under the text. When it is too narrow for all of it, the effort and the access
    /// are only their icons, then the model too. The space between its controls and around them
    /// is their margins, so each takes clicks up to the next one and to the composer's edges.
    fn controls(&self, labels: Labels, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let choices = store.composer_choices();
        let disabled = store.composer_models().is_empty();
        let current = store.composer_model();
        let account = store.composer_account().map(|account| account.id);
        let effort = store.composer_effort();
        let access = store.composer_access();
        let name = store.composer_model_label().unwrap_or("No agent".into());
        let keys = &store.shortcuts;
        let model_help = keys.help(current.as_ref().map_or(&name, |model| &model.name), "modelPicker.toggle");
        let effort_help = keys.help("Reasoning effort", "composer.effort");
        let access_help = keys.help(access_detail(access), "composer.access");

        let model_menu = {
            let store = self.store.clone();
            let current_id = current.as_ref().map(|model| model.id.clone());
            let logo = current.as_ref().map(|model| logos::agent_icon(model.agent, ControlSize::Regular.symbol(), cx));
            let title = if labels == Labels::None && current.is_some() { String::new() } else { name.clone() };
            ActionMenu::new("model", title, move |_, _| {
                let mut menu = Menu::new();
                for (title, account_choice, models) in &choices {
                    if !menu.is_empty() {
                        menu = menu.separator();
                    }
                    menu = menu.note(title.clone());
                    for model in models {
                        let (store, model, chosen_account) = (store.clone(), model.clone(), account_choice.clone());
                        let chosen =
                            Some(&model.id) == current_id.as_ref() && Some(&account_choice.id) == account.as_ref();
                        let icon = Some(MenuIcon::Logo(model.agent));
                        menu = menu.choice(chosen, model.name.clone(), icon, None, move |_, cx| {
                            store.update(cx, |store, cx| {
                                store.set_model(&model, &chosen_account);
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
                    .help(model_help)
                    .gap(MENU_GAP)
                    .surface(Surface::Composer)
                    .margin(margin(8., 4.))
                    .disabled(disabled)
            })
            .shortcut("modelPicker.toggle")
        };

        let effort_menu = current.as_ref().filter(|model| !model.efforts.is_empty()).map(|model| {
            let store = self.store.clone();
            let efforts = model.effort_choices();
            let chosen = effort.clone();
            let current = effort.clone().unwrap_or_default();
            let title = if labels == Labels::All { effort_label(&current) } else { String::new() };
            ActionMenu::new("effort", title, move |_, _| {
                let mut menu = Menu::new();
                for effort in &efforts {
                    let store = store.clone();
                    let value = effort.clone();
                    let icon = Some(MenuIcon::Symbol(effort_symbol(effort)));
                    menu =
                        menu.choice(Some(effort) == chosen.as_ref(), effort_label(effort), icon, None, move |_, cx| {
                            store.update(cx, |store, cx| {
                                store.set_effort(&value);
                                cx.notify();
                            })
                        });
                }
                menu
            })
            .button(move |button| {
                button
                    .symbol(effort_symbol(&current))
                    .symbol_size(ControlSize::Small.symbol())
                    .gap(MENU_GAP)
                    .help(effort_help)
                    .surface(Surface::Composer)
                    .margin(margin(4., 4.))
            })
            .shortcut("composer.effort")
        });

        let access_menu = {
            let store = self.store.clone();
            let label = access_label(access).to_string();
            let symbol = access_symbol(access);
            ActionMenu::new("access", if labels == Labels::All { label } else { String::new() }, move |_, _| {
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
                menu
            })
            .button(move |button| {
                button
                    .symbol(symbol)
                    .symbol_size(ControlSize::Small.symbol())
                    .gap(MENU_GAP)
                    .help(access_help)
                    .surface(Surface::Composer)
                    .margin(margin(4., 1.))
            })
            .shortcut("composer.access")
        };

        let attach = {
            let store = self.store.clone();
            ActionButton::icon("attach", "paperclip", "Attach files")
                .round(true)
                .surface(Surface::Composer)
                .margin(margin(1., 4.))
                .on_click(move |_, _, cx| choose_files(store.clone(), cx))
        };

        let (menus, buttons) = (self.row.clone(), self.row.clone());
        div()
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                let mut row = menus.get();
                                let width = f32::from(bounds.size.width);
                                if (row.menus - width).abs() > 0.5 || row.labels != labels {
                                    row.menus = width;
                                    row.labels = labels;
                                    menus.set(row);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(model_menu)
                    .child(composer_divider(c))
                    .when_some(effort_menu, |row, effort| row.child(effort).child(composer_divider(c)))
                    .child(access_menu),
            )
            .child(self.typing_area("controls-room", cx).flex_1().min_w(px(SPACER)).self_stretch())
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                let mut row = buttons.get();
                                let width = f32::from(bounds.size.width);
                                if (row.buttons - width).abs() > 0.5 {
                                    row.buttons = width;
                                    buttons.set(row);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(attach)
                    .child(self.send_buttons(cx)),
            )
    }

    /// Empty room in the composer: a click there starts typing, under the cursor of text. It is
    /// never under a control, whose menu keeps the arrow.
    fn typing_area(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        div().id(id).cursor_text().on_click(
            cx.listener(|this, event: &ClickEvent, window, cx| this.press_beside_text(event.position(), window, cx)),
        )
    }

    /// Stops the turn that runs, and sends what is written or queues it behind that turn.
    fn send_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let running = store.activity.running && store.selected_thread().is_some();
        let can_send = store.can_send();
        let sends = !running || can_send;
        let steers = store.steers();
        let stop_help = store.shortcuts.help("Stop", "thread.stop");
        let help = store.attachments_hold().map(String::from).unwrap_or_else(|| {
            if !running {
                return "Send".into();
            }
            let what = if steers { "Steer" } else { "Queue" };
            let context = ["composerFocus", "threadOpen", "turnRunning"];
            match store.shortcuts.label_in("composer.sendAlternate", &context) {
                Some(keys) => format!("{what} ({keys} {})", if steers { "queues" } else { "steers" }),
                None => what.into(),
            }
        });
        let (stop, send) = (self.store.clone(), self.store.clone());
        div()
            .flex()
            .items_center()
            .when(running, |buttons| {
                buttons.child(
                    ActionButton::new("stop", "")
                        .picture(div().size(px(10.)).rounded(px(2.5)).bg(c.destructive_foreground))
                        .help(stop_help)
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
                                store.send_message(false, cx);
                                cx.notify();
                            })
                        }),
                )
            })
    }

    /// The box: the done banner, the attachments, the text and the controls under it.
    fn box_(&self, thread: Option<&ThreadInfo>, labels: Labels, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let continues = store.selected_draft().is_some_and(|draft| draft.from.is_some());
        let has_attachments = continues || !store.attachments().is_empty();
        let store_handle = self.store.clone();
        let pasted = self.skills.pasted.clone();
        let text = Textarea::new(&self.text)
            .appearance(false)
            .xsmall()
            .text_size(px(TEXT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .px(px(0.))
            .pt(px(TEXT_TOP))
            .pb(px(TEXT_BOTTOM))
            .max_h(px(MAX_TEXT_HEIGHT))
            .token(|token, _, cx| {
                crate::ui::skill_chip::chip(token.token().label().clone(), token.is_selected(), token.line_height(), cx)
            })
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
                // Files copied in Finder and copied images are attached, a PNG before a TIFF;
                // anything else is pasted as plain text.
                let image = [ImageFormat::Png, ImageFormat::Tiff]
                    .into_iter()
                    .find_map(|format| images.iter().find(|image| image.format == format).cloned());
                let has_text = clipboard.text().is_some();
                if files.is_empty() && (image.is_none() || has_text) {
                    pasted.set(true);
                    return false;
                }
                store_handle.update(cx, |store, cx| {
                    match image.filter(|_| files.is_empty()) {
                        Some(image) => store.attach_image(image, cx),
                        None => store.attach(files),
                    }
                    cx.notify();
                });
                true
            });
        let focus = self.text.clone();
        div()
            .id("composer-box")
            .key_context("Composer")
            .capture_action(cx.listener(Self::on_enter))
            .capture_key_down(cx.listener(Self::on_key_down))
            .relative()
            .flex()
            .flex_col()
            .bg(c.composer)
            .rounded(px(RADIUS))
            .border_1()
            .border_color(c.border_composer)
            .shadow(c.shadow(theme::ShadowSize::Md, theme::Opacity::Shadow))
            .on_click(move |_, window, cx| focus.update(cx, |text, cx| text.focus(window, cx)))
            .child(
                self.typing_area("typing-area", cx)
                    .flex()
                    .flex_col()
                    .when_some(thread.filter(|thread| thread.is_done()), |composer, thread| {
                        composer.child(self.done_banner(thread, cx))
                    })
                    .when(has_attachments, |composer| composer.child(attachments::attachments(&self.store, cx)))
                    .child(
                        div()
                            .px(px(TEXT_SIDE))
                            .pt(px(12.))
                            .min_h(px(MIN_TEXT_HEIGHT + 12.))
                            .max_h(px(MAX_TEXT_HEIGHT + 12.))
                            .text_size(px(TEXT_SIZE))
                            .line_height(px(LINE_HEIGHT))
                            .child(text),
                    ),
            )
            .child(self.controls(labels, cx))
    }
}

/// The line between the parts of the composer's rows. It takes no room and no clicks, so the
/// controls on either side of it reach its middle.
pub fn composer_divider(c: &theme::Colors) -> Div {
    div()
        .relative()
        .w(px(0.))
        .h(px(14.))
        .flex_shrink_0()
        .child(div().absolute().top_0().bottom_0().left(px(-0.5)).w(px(1.)).bg(c.border_composer))
}

/// Why the open thread's agent stopped before it finished, when its strip shows: the thread is
/// at rest.
fn interruption(thread: &ThreadInfo, store: &Store) -> Option<Interruption> {
    if thread.busy() || store.activity.busy() {
        return None;
    }
    thread.interruption
}

/// The room around a control of the row under the text.
fn margin(leading: f32, trailing: f32) -> Edges<f32> {
    edges(8., leading, 8., trailing)
}

pub fn effort_label(effort: &str) -> String {
    match effort {
        "xhigh" => "Extra".into(),
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

pub fn effort_symbol(effort: &str) -> &'static str {
    match effort {
        "none" | "minimal" | "low" => "effort-low",
        "medium" => "effort-medium",
        "high" => "effort-high",
        "xhigh" => "zap",
        "max" => "rocket",
        "ultracode" => "bot-message-square",
        _ => "gauge",
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let labels = self.labels(window, cx);
        let store = self.store.read(cx);
        let thread = store.selected_thread().cloned();
        let approval = thread.as_ref().and_then(|_| store.activity.approvals.first().cloned());
        let count = store.activity.approvals.len();
        let monitoring = thread.is_some() && store.activity.monitoring;
        let interrupted = thread.as_ref().and_then(|thread| interruption(thread, store));
        let project = store.composer_project();
        let server = project.as_ref().and_then(|project| store.server(Some(&project.server_id)).cloned());
        let top_strip: Option<AnyElement> = if let Some(approval) = &approval {
            let questions = self.questions.get(&approval.id).cloned();
            Some(waiting_strip(&self.store, approval, count, questions, cx).into_any_element())
        } else if let Some(thread) = thread.as_ref().filter(|_| monitoring) {
            Some(monitoring_strip(&self.store, thread, cx).into_any_element())
        } else {
            interrupted.map(|interruption| interruption_strip(&self.store, interruption, cx).into_any_element())
        };
        let bottom_strip = project.map(|project| {
            let picker = self.branch_picker.clone();
            context_strip(&self.store, &project, server.as_ref(), &self.branch_anchor, picker, self.width, window, cx)
                .into_any_element()
        });

        let (top_height, bottom_height) = self.strips.get();
        let measure = |top: bool, strips: Rc<Cell<(f32, f32)>>| {
            canvas(
                move |bounds, window, _| {
                    let (mut above, mut below) = strips.get();
                    let height = f32::from(bounds.size.height);
                    let kept = if top { &mut above } else { &mut below };
                    if (*kept - height).abs() > 0.5 {
                        *kept = height;
                        strips.set((above, below));
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
        };
        let shown_top = top_strip.is_some();
        let shown_bottom = bottom_strip.is_some();
        if !shown_top || !shown_bottom {
            let (above, below) = self.strips.get();
            self.strips.set((if shown_top { above } else { 0. }, if shown_bottom { below } else { 0. }));
        }
        let shadow = c.shadow(theme::ShadowSize::Lg, theme::Opacity::Shadow);
        div()
            .w_full()
            .max_w(px(theme::COMPOSER_WIDTH))
            .relative()
            .flex()
            .flex_col()
            // The wide shadow the box and its strips cast together, from their outlines behind them all.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .when(shown_top, |outline| {
                        outline.child(
                            div()
                                .flex_none()
                                .h(px(top_height))
                                .mx(px(RADIUS))
                                .rounded_t(px(strips::STRIP_RADIUS))
                                .bg(c.composer)
                                .shadow(shadow.clone()),
                        )
                    })
                    .child(div().flex_1().rounded(px(RADIUS)).bg(c.composer).shadow(shadow.clone()))
                    .when(shown_bottom, |outline| {
                        outline.child(
                            div()
                                .flex_none()
                                .h(px(bottom_height))
                                .mx(px(RADIUS))
                                .rounded_b(px(strips::STRIP_RADIUS))
                                .bg(c.composer)
                                .shadow(shadow),
                        )
                    }),
            )
            .child(self.skill_menu(cx))
            .children(top_strip.map(|strip| div().relative().child(measure(true, self.strips.clone())).child(strip)))
            .child(self.box_(thread.as_ref(), labels, cx))
            .children(
                bottom_strip.map(|strip| div().relative().child(measure(false, self.strips.clone())).child(strip)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Labels;

    #[test]
    fn the_effort_and_the_access_lose_their_words_before_the_model() {
        // All labelled, the menus take 300: 120 of it the settings' words, 60 the model's.
        assert_eq!(Labels::fitting(300., Labels::All, 120., 60., 300.), Labels::All);
        assert_eq!(Labels::fitting(300., Labels::All, 120., 60., 299.), Labels::Model);
        assert_eq!(Labels::fitting(300., Labels::All, 120., 60., 180.), Labels::Model);
        assert_eq!(Labels::fitting(300., Labels::All, 120., 60., 179.), Labels::None);
        // Drawn without words, they are measured as they would be with them.
        assert_eq!(Labels::fitting(120., Labels::None, 120., 60., 300.), Labels::All);
        assert_eq!(Labels::fitting(180., Labels::Model, 120., 60., 200.), Labels::Model);
    }
}
