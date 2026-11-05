//! Where messages are written. A strip above says what the agent waits for or that it is
//! monitoring. Under the text, attaching is on one side and on the other, by the send button,
//! the model, the reasoning effort and how much the agent may do without asking, which open the
//! picker. A strip below says where the thread works: the server, the folder and the branch.

pub mod attachments;
pub mod children;
mod picker;
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
use motile_core::api::Section;
use motile_core::link::State;
use motile_protocol::wire::{Access, Interruption};

use self::picker::ComposerPicker;
use self::questions::Questions;
use self::strips::{BranchPicker, agents_strip, context_strip, interruption_strip, monitoring_strip, waiting_strip};
use crate::models::{ComposerPickerPart, ThreadInfo, access_label, section_label};
use crate::store::Store;
use crate::theme::{self, ControlSize, IconSize, Surface, colors};
use crate::ui::control::Look;
use crate::ui::menu::Anchor;
use crate::ui::{ActionButton, Variant, edges, icons, logos, text_width};

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
/// The least room between the attach button and the settings.
const SPACER: f32 = 10.;
/// The room before a divider of the settings, after a part's words, and after it, before the next
/// part's symbol, which stands closer to its edge than words do: both look 9 points wide.
const ROOM_AFTER_WORDS: f32 = 8.;
const ROOM_BEFORE_SYMBOL: f32 = 13.;

/// Which parts of the settings under the text say what they are set to, the first of these that fits.
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
    /// The first that fits in `room`, for settings that were `menus` wide with `drawn`, whose
    /// words take `settings` for the effort and the access and `model` for the model.
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

/// How wide the parts of the row under the text were last drawn, to tell whether the settings
/// fit with their words.
#[derive(Clone, Copy, Default)]
struct RowWidths {
    /// The settings, and which of their parts were drawn with their words.
    menus: f32,
    labels: Labels,
    /// The attach button, and the buttons on the right.
    attach: f32,
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
    /// The settings button's background, which the picker opens over.
    settings_anchor: Anchor,
    picker: Option<Entity<ComposerPicker>>,
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
            settings_anchor: Anchor::default(),
            picker: None,
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
        let wants_settings = store.composer_picker.is_some();
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
            let store = self.store.clone();
            self.branch_picker = Some(cx.new(|cx| BranchPicker::new(store, window, cx)));
        } else if !wants_picker {
            self.branch_picker = None;
        }
        crate::ui::popover_shown("branch-picker", wants_picker, cx);
        if wants_settings && self.picker.is_none() {
            let (store, button) = (self.store.clone(), self.settings_anchor.clone());
            self.picker = Some(cx.new(|cx| ComposerPicker::new(store, button, window, cx)));
        } else if !wants_settings {
            self.picker = None;
        }
        crate::ui::popover_shown("composer-picker", wants_settings, cx);
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
        if store.selected_thread().is_some_and(ThreadInfo::is_archived) {
            return "Message to bring it back".into();
        }
        if store.selected_thread().is_some_and(|thread| thread.section == Section::Snoozed) {
            return "Message to wake it".into();
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

    /// Says the open thread is archived or snoozed, with the way back.
    fn put_away_banner(&self, thread: &ThreadInfo, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let c = colors(cx);
        let (store, id) = (self.store.clone(), thread.id.clone());
        let (title, action) = match (thread.section, thread.snoozed_until) {
            (Section::Archived, _) => ("Archived".to_string(), "Unarchive"),
            (Section::Snoozed, Some(until)) => (format!("Snoozed until {}", crate::format::stamp(until)), "Wake"),
            _ => return None,
        };
        let (_, symbol) = section_label(thread.section);
        let section = thread.section;
        Some(
            div()
                .pl(px(16.))
                .pr(px(16. - ControlSize::Small.padding()))
                .pt(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .children(symbol.map(|symbol| icons::symbol(symbol, IconSize::Sm).text_color(c.muted_foreground)))
                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().flex_1())
                .child(div().my(px(-4.)).child(
                    ActionButton::new("bring-back", action).link().small().surface(Surface::Composer).on_click(
                        move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                if section == Section::Snoozed {
                                    store.wake(std::slice::from_ref(&id));
                                } else {
                                    store.set_archived(std::slice::from_ref(&id), false, false, cx);
                                }
                                cx.notify();
                            })
                        },
                    ),
                )),
        )
    }

    /// Which parts of the settings under the text have room to say what they are set to, as the
    /// Mac's `ViewThatFits` picks: from how wide they were last drawn, and how wide their words
    /// are.
    fn labels(&self, window: &mut Window, cx: &App) -> Labels {
        let store = self.store.read(cx);
        let words = |text: &str, window: &mut Window| {
            text_width(text, ControlSize::Regular.text_size(), FontWeight::MEDIUM, window) + MENU_GAP
        };
        let mut settings = words(access_label(store.composer_access()), window);
        let effort = store.composer_effort();
        let context = store.composer_context().map(|context| context.label);
        if effort.is_some() || context.is_some() {
            settings += words(&effort_words(effort.as_deref(), context.as_deref()), window);
            settings -= context.map_or(0., |context| words(&context, window));
        }
        let model = match store.composer_model() {
            Some(_) => {
                words(&store.composer_model_label().unwrap_or_default(), window)
                    + if store.composer_proxied() {
                        crate::ui::gateway_mark_width_as(
                            ControlSize::Regular.text_size(),
                            ControlSize::Regular.symbol(),
                        )
                    } else {
                        0.
                    }
            }
            None => 0.,
        };
        let row = self.row.get();
        Labels::fitting(row.menus, row.labels, settings, model, self.width - SPACER - row.attach - row.buttons)
    }

    /// The row under the text: attaching on one side, the settings by the send button on the
    /// other. When it is too narrow for all of it, the effort and the access are only their
    /// icons, then the model too. The space between its controls and around them is their
    /// margins, so each takes clicks up to the next one and to the composer's edges.
    fn controls(&self, labels: Labels, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let attach = {
            let store = self.store.clone();
            ActionButton::icon("attach", "plus", "Attach files")
                .symbol_size(IconSize::Lg)
                .round(true)
                .tint(Some(c.foreground))
                .surface(Surface::Composer)
                .margin(margin(8., 1.))
                .on_click(move |_, _, cx| choose_files(store.clone(), cx))
        };
        let measured = |part: fn(&mut RowWidths) -> &mut f32, row: Rc<Cell<RowWidths>>, labels: Option<Labels>| {
            canvas(
                move |bounds, window, _| {
                    let mut widths = row.get();
                    let width = f32::from(bounds.size.width);
                    let changed = labels.is_some_and(|labels| widths.labels != labels);
                    if (*part(&mut widths) - width).abs() > 0.5 || changed {
                        *part(&mut widths) = width;
                        if let Some(labels) = labels {
                            widths.labels = labels;
                        }
                        row.set(widths);
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
        };
        div()
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .child(measured(|row| &mut row.attach, self.row.clone(), None))
                    .child(attach),
            )
            .child(self.typing_area("controls-room", cx).flex_1().min_w(px(SPACER)).self_stretch())
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .child(measured(|row| &mut row.menus, self.row.clone(), Some(labels)))
                    .child(self.settings_button(labels, window, cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(measured(|row| &mut row.buttons, self.row.clone(), None))
                    .child(self.send_buttons(cx)),
            )
    }

    /// The model with its account, the effort with the context window, and the access: one
    /// button that opens the picker which chooses all of them.
    fn settings_button(&self, labels: Labels, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = ElementId::from("composer-settings");
        let hovered = crate::ui::hover_state(&id, window, cx);
        let lit = *hovered.read(cx);
        let c = colors(cx);
        let store = self.store.read(cx);
        let model = store.composer_model();
        let effort = store.composer_effort();
        let context = store.composer_context().map(|context| context.label);
        let access = store.composer_access();
        let open = store.composer_picker.is_some();
        let disabled = store.composer_models().is_empty() || crate::ui::inert(cx);
        let help = store.shortcuts.help("Model, effort and access", "modelPicker.toggle");
        let proxied = store.composer_proxied();
        let model_title = (labels != Labels::None || model.is_none())
            .then(|| store.composer_model_label().unwrap_or("No agent".into()));
        let effort_title = match labels {
            Labels::All => Some(effort_words(effort.as_deref(), context.as_deref())),
            _ => context.clone(),
        };
        let mut look = Look::new(Variant::Ghost, ControlSize::Regular);
        look.surface = Surface::Composer;
        look.selected = open;
        let group = SharedString::from("composer-settings");
        let symbol = ControlSize::Regular.symbol();
        let logo = model.as_ref().map(|model| logos::agent_icon(model.agent, symbol, cx).into_any_element());
        let divider =
            || div().flex_shrink_0().pl(px(ROOM_AFTER_WORDS)).pr(px(ROOM_BEFORE_SYMBOL)).child(composer_divider(c));
        let store = self.store.clone();
        div()
            .id(id)
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .pt(px(8.))
            .pb(px(8.))
            .pl(px(1.))
            .pr(px(4.))
            .cursor_default()
            .when(disabled, |button| button.opacity(c.opacity(theme::Opacity::Disabled)))
            .child({
                let store = self.store.clone();
                crate::keyboard::target_pressing("modelPicker.toggle", move |_, cx| toggle_picker(&store, cx))
            })
            .child(
                div()
                    .id("face")
                    .relative()
                    .h(px(ControlSize::Regular.height()))
                    .px(px(ControlSize::Regular.padding()))
                    .flex()
                    .items_center()
                    .rounded(px(look.radius()))
                    .bg(look.fill(false, c))
                    .text_color(look.foreground(lit && !disabled, c))
                    .when(!disabled, |face| {
                        let lit = look.fill(true, c);
                        face.group_hover(group.clone(), move |face| face.bg(lit))
                            .group_active(group.clone(), move |face| face.bg(lit))
                    })
                    .child(self.settings_anchor.track())
                    .child(settings_part(logo, model_title, proxied, c))
                    .when(effort.is_some() || context.is_some(), |face| {
                        let mark = effort
                            .as_deref()
                            .map(|effort| icons::symbol(effort_symbol(effort), symbol).into_any_element());
                        face.child(divider()).child(settings_part(mark, effort_title, false, c))
                    })
                    .child(divider())
                    .child(settings_part(
                        Some(icons::symbol(access_symbol(access), symbol).into_any_element()),
                        (labels == Labels::All).then(|| access_label(access).to_string()),
                        false,
                        c,
                    )),
            )
            .on_hover(crate::ui::track_hover(&hovered))
            .tooltip(crate::ui::tooltip(help))
            .when(!disabled, |button| {
                button.on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    toggle_picker(&store, cx)
                })
            })
    }

    /// Empty room in the composer: a click there starts typing, under the cursor of text. It is
    /// never under a control, whose menu keeps the arrow.
    fn typing_area(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        div().id(id).cursor_text().on_click(
            cx.listener(|this, event: &ClickEvent, window, cx| this.press_beside_text(event.position(), window, cx)),
        )
    }

    /// Stops the turn that runs, and sends what is written or queues it behind that turn. While the
    /// client tries to reach the server neither works, and the last of them waits for it, sending
    /// signals.
    fn send_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let running = store.activity.running && store.selected_thread().is_some();
        let can_send = store.can_send();
        let sends = !running || store.has_message();
        let state = store.composer_server().map(|server| server.state);
        let connecting = matches!(state, Some(State::Connecting | State::Disconnected));
        let stop_waits = connecting && !sends;
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
                        .picture(div().size(px(11.)).rounded(px(2.5)).bg(if stop_waits {
                            c.pending_foreground
                        } else {
                            c.destructive_foreground
                        }))
                        .help(stop_help)
                        .variant(if stop_waits { Variant::Waiting } else { Variant::Danger })
                        .signals(stop_waits)
                        .round(true)
                        .surface(Surface::Composer)
                        .margin(margin(4., if sends { 4. } else { 8. }))
                        .disabled(state != Some(State::Connected))
                        .on_click(move |_, _, cx| {
                            stop.update(cx, |store, cx| {
                                store.stop_thread(cx);
                                cx.notify();
                            })
                        }),
                )
            })
            .when(sends, |buttons| {
                buttons.child(
                    ActionButton::icon("send", "arrow-up", help)
                        .variant(if connecting { Variant::Waiting } else { Variant::Primary })
                        .signals(connecting)
                        .symbol_size(IconSize::Md)
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
    fn box_(
        &self,
        thread: Option<&ThreadInfo>,
        labels: Labels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                    .children(thread.and_then(|thread| self.put_away_banner(thread, cx)))
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
            .child(self.controls(labels, window, cx))
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

/// Opens the model picker, or closes it and gives the keys back to the composer.
fn toggle_picker(store: &Entity<Store>, cx: &mut App) {
    store.update(cx, |store, cx| {
        store.composer_picker = match store.composer_picker {
            None => Some(ComposerPickerPart::Model),
            Some(_) => None,
        };
        store.composer_access_open = false;
        if store.composer_picker.is_none() {
            store.composer_focus += 1;
        }
        cx.notify();
    })
}

/// A part of the settings button: its symbol or picture, and its words when it has room for
/// them, the symbol standing closer to the edge than words do, with words or without.
fn settings_part(mark: Option<AnyElement>, title: Option<String>, proxied: bool, c: &theme::Colors) -> Div {
    let size = ControlSize::Regular;
    let square = |mark: AnyElement| {
        div().size(px(size.symbol().side())).flex_shrink_0().flex().items_center().justify_center().child(mark)
    };
    let outset = if mark.is_some() { size.symbol_outset() } else { 0. };
    let Some(title) = title else {
        return div().h(px(size.height())).ml(px(-outset)).flex().items_center().children(mark.map(square));
    };
    div().h(px(size.height())).ml(px(-outset)).flex().items_center().gap(px(MENU_GAP)).children(mark.map(square)).child(
        div()
            .text_size(px(size.text_size()))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .child(crate::ui::proxied_dotted(title, proxied, size.text_size(), size.symbol(), c)),
    )
}

/// "Medium · 1M": the effort, and the context window where the model offers more than one.
fn effort_words(effort: Option<&str>, context: Option<&str>) -> String {
    let effort = effort.map(effort_label);
    [effort.as_deref(), context].into_iter().flatten().collect::<Vec<_>>().join(" · ")
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
        Access::Supervised => "lock",
        Access::AcceptEdits => "pen-line",
        Access::Auto => "sparkles",
        Access::Full => "shield-off",
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
        let waiting_children = thread.as_ref().and_then(|thread| {
            let children = thread.children.as_ref()?;
            Some((children.waiting_text.clone()?, children.waiting.first()?.clone(), children.waiting.len()))
        });
        let monitoring = thread.is_some() && store.activity.monitoring;
        let working_children = thread.as_ref().and_then(|thread| {
            let children = thread.children.clone().filter(|children| children.running_text.is_some())?;
            Some((children, thread.waits_on_agents() && !store.activity.running))
        });
        let interrupted = thread.as_ref().and_then(|thread| interruption(thread, store));
        let project = store.composer_project();
        let server = project.as_ref().and_then(|project| store.server(Some(&project.server_id)).cloned());
        let skill_menu_open = store.skills.menu.is_some() || store.skills.loading;
        let top_strip: Option<Div> = if let Some(approval) = &approval {
            let questions = self.questions.get(&approval.id).cloned();
            Some(waiting_strip(&self.store, approval, count, questions, cx))
        } else if let Some((text, first, count)) = waiting_children {
            Some(children::children_strip(&self.store, text, &first, count, cx))
        } else if let Some(interruption) = interrupted {
            Some(interruption_strip(&self.store, interruption, cx))
        } else if let Some(thread) = thread.as_ref().filter(|_| monitoring) {
            Some(monitoring_strip(&self.store, thread, self.width, window, cx))
        } else {
            working_children
                .map(|(children, stops)| agents_strip(&self.store, &children, stops, self.width, window, cx))
        };
        // Under the skill menu, whose corners are the rounded ones, the strip is square.
        let top_radius = if skill_menu_open { 0. } else { strips::STRIP_RADIUS };
        let top_strip = top_strip.map(|strip| strip.rounded_t(px(top_radius)));
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
            // The outlines keep the transcript under them from taking the pointer.
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
                                .rounded_t(px(top_radius))
                                .bg(c.composer)
                                .shadow(shadow.clone())
                                .block_mouse_except_scroll(),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .rounded(px(RADIUS))
                            .bg(c.composer)
                            .shadow(shadow.clone())
                            .block_mouse_except_scroll(),
                    )
                    .when(shown_bottom, |outline| {
                        outline.child(
                            div()
                                .flex_none()
                                .h(px(bottom_height))
                                .mx(px(RADIUS))
                                .rounded_b(px(strips::STRIP_RADIUS))
                                .bg(c.composer)
                                .shadow(shadow)
                                .block_mouse_except_scroll(),
                        )
                    }),
            )
            .child(self.skill_menu(cx))
            .children(self.picker.clone().map(|picker| picker::placed(picker, &self.settings_anchor, window)))
            .children(top_strip.map(|strip| div().relative().child(measure(true, self.strips.clone())).child(strip)))
            .child(self.box_(thread.as_ref(), labels, window, cx))
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
