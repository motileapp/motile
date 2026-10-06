//! A pull request of the repository: the one of the branch the thread works on, or another one by
//! its number. Where it stands, what holds it up, the button its state calls for, what was said on
//! it, and a box to say more. The pieces every pull request view shares are here too.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{
    DiffScope, GitAction, PullRequestAction, PullRequestEdit, Reaction, ReactionKind, ReviewVerdict,
};

use super::state::{Loaded, PanelTab, PanelTarget, file_symbol};
use crate::models::ago;
use crate::store::Store;
use crate::store::pull_requests::{
    Button, Check, Choice, Entry, EntryKind, Notice, Page, PendingLineComment, REACTIONS, Stack, Status, StatusKind,
    Style, Text, Thread, Toggle, Tone, emoji, read_blocks,
};
use crate::theme::{self, ControlSize, Radius, colors};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::menu::Menu;
use crate::ui::sheet::sheet;
use crate::ui::{
    ActionButton, ActionMenu, Chip, InputField, Segmented, Variant, divider, edges, highlight, icons, spinner,
};

/// The height of a line of the panel's text, which the buttons beside it centre on.
const LINE: f32 = 17.;

// MARK: Pieces every pull request view uses

/// What a tab says in the middle of the panel when it has nothing to show.
pub fn panel_message(text: impl Into<SharedString>, failed: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .size_full()
        .p(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.5))
        .text_color(if failed { c.danger } else { c.secondary })
        .text_center()
        .child(text.into())
        .into_any_element()
}

pub fn panel_loading(cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(c.tertiary)
        .child(spinner(ControlSize::Large.symbol(), cx))
        .into_any_element()
}

/// The strip under the tabs with what the open tab is about and its buttons.
pub fn panel_bar(content: impl IntoElement, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(div().h(px(36.)).pl(px(12.)).pr(px(6.)).flex().items_center().gap(px(4.)).child(content))
        .child(divider(cx))
}

pub fn label(text: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div().text_size(px(size)).text_color(color).child(text.into())
}

fn ui_font(weight: FontWeight) -> Font {
    Font {
        family: SharedString::from(theme::UI_FONT),
        features: FontFeatures::default(),
        fallbacks: None,
        weight,
        style: FontStyle::Normal,
    }
}

/// One line of text in several colours and weights: "yekta approved these changes · 3h".
pub fn styled_line(parts: Vec<(String, Hsla, FontWeight)>) -> StyledText {
    let mut text = String::new();
    let mut runs = Vec::new();
    for (part, color, weight) in parts {
        if part.is_empty() {
            continue;
        }
        runs.push(TextRun {
            len: part.len(),
            font: ui_font(weight),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&part);
    }
    StyledText::new(text).with_runs(runs)
}

/// "+12 −3" in green and red, or nothing when nothing changed.
pub fn line_counts(added: u32, removed: u32, cx: &App) -> Option<Div> {
    if added + removed == 0 {
        return None;
    }
    let (text, runs) = super::code_view::counts_text(added, removed, colors(cx));
    Some(div().text_size(px(11.5)).child(StyledText::new(text).with_runs(runs)))
}

/// Markdown from the pull request, drawn as the transcript draws a reply.
pub fn text_blocks(id: &str, blocks: &[Text], cx: &App) -> Div {
    div().w_full().flex().flex_col().gap(px(10.)).children(blocks.iter().enumerate().map(|(index, block)| {
        let block_id = format!("{id}-{index}");
        match block {
            Text::Prose(prose) => crate::ui::rich_text::prose_blocks(&block_id, prose, 13., cx),
            Text::Code { code, spans, .. } => crate::ui::rich_text::code_block(&block_id, code, spans, cx),
        }
    }))
}

/// The look of a button as the core names it.
pub fn variant_of(style: Style) -> Variant {
    match style {
        Style::Primary => Variant::Primary,
        Style::Danger => Variant::Danger,
        Style::Plain => Variant::Secondary,
    }
}

/// A label's colour, from GitHub's hex.
pub fn label_color(hex: &str) -> Hsla {
    rgb(u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x888888)).into()
}

/// The box on the next surface that a card lies in.
fn layered(cx: &App) -> Div {
    div().w_full().bg(colors(cx).background_secondary).rounded(px(Radius::CARD))
}

/// A box to write in, with a word in it while it is empty. Without a height it is as tall as
/// there is room for.
pub fn writing_field(state: &Entity<TextareaState>, height: Option<f32>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .w_full()
        .when_some(height, |field, height| field.h(px(height)))
        .when(height.is_none(), |field| field.flex_1().min_h_0())
        .px(px(4.))
        .py(px(6.))
        .bg(c.background_secondary)
        .rounded(px(Radius::CONTROL))
        .border_1()
        .border_color(c.border)
        .text_size(px(12.5))
        .child(Textarea::new(state).appearance(false).xsmall().text_size(px(12.5)).px_0().py_0().h_full())
}

/// The number in "#12", "12" or "https://github.com/acme/app/pull/12".
pub fn parse_number(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    last.trim_start_matches('#').parse().ok()
}

/// A field that takes a pull request's number or address and links it to the thread.
pub fn link_field(
    id: &'static str,
    state: &Entity<InputState>,
    link: impl Fn(u64, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let number = parse_number(state.read(cx).value().as_ref());
    let clear = state.clone();
    div().w_full().flex().items_center().gap(px(6.)).child(InputField::new(state)).child(
        ActionButton::new(id, "Link").disabled(number.is_none()).on_click(move |_, window, cx| {
            let Some(number) = number else { return };
            link(number, window, cx);
            clear.update(cx, |state, cx| state.set_value("", window, cx));
        }),
    )
}

/// What the last action did or why it couldn't, under the tab's bar.
pub fn notice_bar(notice: &Notice, store: &Entity<Store>, cx: &App) -> Div {
    let c = colors(cx);
    let url = notice.url.clone();
    let dismiss = store.clone();
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(
            div()
                .pl(px(12.))
                .pr(px(6.))
                .py(px(8.))
                .bg(if notice.failed { c.danger_background } else { c.success.opacity(0.08) })
                .flex()
                .items_start()
                .gap(px(8.))
                .child(
                    div().h(px(LINE)).flex().items_center().child(
                        icons::symbol(if notice.failed { "circle-alert" } else { "circle-check" }, 13.)
                            .text_color(if notice.failed { c.danger } else { c.success }),
                    ),
                )
                .child(div().flex_1().min_w_0().min_h(px(LINE)).child(label(notice.text.clone(), 12.5, c.text)))
                .child(
                    div()
                        .h(px(LINE))
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .when_some(url, |row, url| {
                            row.child(
                                ActionButton::new("pr-notice-open", "Open")
                                    .link()
                                    .small()
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            )
                        })
                        .child(ActionButton::icon("pr-notice-close", "x", "Close").small().on_click(
                            move |_, _, cx| {
                                dismiss.update(cx, |store, cx| {
                                    store.dismiss_pull_request_notice();
                                    cx.notify();
                                })
                            },
                        )),
                ),
        )
        .child(divider(cx))
}

/// Has the user react so, or take it back.
pub type React = Rc<dyn Fn(ReactionKind, bool, &mut Window, &mut App)>;

/// The menu of GitHub's reactions, to add one or take it back.
pub fn reaction_picker(
    id: impl Into<ElementId>,
    reactions: &[Reaction],
    size: ControlSize,
    react: React,
) -> ActionMenu {
    let mine: Vec<ReactionKind> =
        reactions.iter().filter(|reaction| reaction.mine).map(|reaction| reaction.kind).collect();
    ActionMenu::icon(id, "smile-plus", "Add a reaction", move |_, _| {
        let mut menu = Menu::new();
        for (kind, emoji) in REACTIONS {
            let reacted = mine.contains(&kind);
            let react = react.clone();
            menu = menu.item(format!("{emoji}  {}", if reacted { "Take Back" } else { "React" }), move |window, cx| {
                react(kind, !reacted, window, cx)
            });
        }
        menu
    })
    .button(move |button| button.size(size))
}

/// The reactions to a comment or a review, each one a toggle, and a menu to add another.
pub fn reaction_bar(id: &str, reactions: &[Reaction], react: React) -> Div {
    div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(px(4.))
        .children(reactions.iter().map(|reaction| {
            let (kind, mine) = (reaction.kind, reaction.mine);
            let react = react.clone();
            ActionButton::new(
                SharedString::from(format!("{id}-{kind:?}")),
                format!("{} {}", emoji(kind), reaction.count),
            )
            .variant(if mine { Variant::Accent } else { Variant::Secondary })
            .small()
            .round(true)
            .help(if mine { "Take your reaction back" } else { "React so too" })
            .on_click(move |_, window, cx| react(kind, !mine, window, cx))
        }))
        .child(reaction_picker(SharedString::from(format!("{id}-add")), reactions, ControlSize::Small, react))
}

// MARK: The tab

/// What makes the tab ask its server again: another folder or pull request, a turn that ended
/// there, or the button that asks.
#[derive(Clone, PartialEq, Debug)]
struct Trigger {
    target: PanelTarget,
    number: u64,
    version: u64,
    asked: u64,
}

/// What the tab can do to the pull request, for the parts of the page that do it.
#[derive(Clone)]
struct Actions {
    store: Entity<Store>,
    view: WeakEntity<PullRequestView>,
    target: PanelTarget,
    number: u64,
    working: Option<String>,
    /// The description can be edited from its row.
    edits_description: bool,
}

impl Actions {
    fn busy(&self) -> bool {
        self.working.is_some()
    }

    fn is_working(&self, key: &str) -> bool {
        self.working.as_deref() == Some(key)
    }

    fn edit(&self, edit: PullRequestEdit, key: Option<String>, cx: &mut App) {
        let (target, number) = (self.target.clone(), self.number);
        self.store.update(cx, |store, cx| {
            store.pull_request_edit(edit, key, &target, number);
            cx.notify();
        });
    }

    fn react(&self, subject: String) -> React {
        let actions = self.clone();
        Rc::new(move |reaction, on, _, cx| {
            actions.edit(PullRequestEdit::React { subject: subject.clone(), reaction, on }, None, cx);
        })
    }

    fn hand_off(&self, prompt: String, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.hand_off(prompt);
            cx.notify();
        });
    }

    fn show_commit(&self, sha: String, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.show_diff(Some(DiffScope::Commit { sha }), None);
            cx.notify();
        });
    }
}

type Toggler = Rc<dyn Fn(String, bool, &mut App)>;

pub struct PullRequestView {
    store: Entity<Store>,
    /// Another pull request than the thread's own.
    number: Option<u64>,
    asked: u64,
    trigger: Option<Trigger>,
    /// The page as last read, kept so that drawing doesn't copy it.
    page: Option<(u64, Option<Rc<Page>>)>,
    /// The read the wait for GitHub to settle was started on.
    settled_at: Option<u64>,
    settle: Option<Task<()>>,
    comment: Entity<TextareaState>,
    placeholder: &'static str,
    linking: Entity<InputState>,
    /// The title while it is being edited.
    title: Option<Entity<InputState>>,
    /// The conversation a reply is being written to, and the reply.
    reply: Option<(String, Entity<TextareaState>)>,
    /// The conversations opened or folded by hand.
    open_threads: HashMap<String, bool>,
    shows_all_checks: bool,
    confirming: Option<Button>,
    describing: Option<Entity<DescriptionEditor>>,
    seen_finished: u64,
}

impl PullRequestView {
    /// The tab of the thread's own pull request, or with a `number` of another one.
    pub fn new(store: Entity<Store>, number: Option<u64>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let comment = cx.new(|cx| TextareaState::new(window, cx).placeholder("Leave a comment"));
        cx.subscribe(&comment, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let linking = cx.new(|cx| InputState::new(window, cx).placeholder("Link a PR by number or address"));
        cx.subscribe_in(&linking, window, |this, linking, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } => {
                let Some(number) = parse_number(linking.read(cx).value().as_ref()) else { return };
                this.link(Some(number), cx);
                linking.update(cx, |state, cx| state.set_value("", window, cx));
            }
            InputEvent::Change => cx.notify(),
            _ => {}
        })
        .detach();
        Self {
            store,
            number,
            asked: 0,
            trigger: None,
            page: None,
            settled_at: None,
            settle: None,
            comment,
            placeholder: "Leave a comment",
            linking,
            title: None,
            reply: None,
            open_threads: HashMap::new(),
            shows_all_checks: false,
            confirming: None,
            describing: None,
            seen_finished: 0,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asked += 1;
        cx.notify();
    }

    /// Asks the server for the pull request when what the tab shows may have changed.
    fn follow(&mut self, trigger: Trigger, cx: &mut Context<Self>) {
        if self.trigger.as_ref() == Some(&trigger) {
            return;
        }
        self.trigger = Some(trigger.clone());
        let store = self.store.clone();
        cx.defer(move |cx| {
            store.update(cx, |store, cx| {
                store.load_pull_request(&trigger.target, trigger.number);
                cx.notify();
            });
        });
    }

    /// While GitHub is still working something out, the pull request is asked for again.
    fn settle(&mut self, reads: u64, cx: &mut Context<Self>) {
        if self.settled_at == Some(reads) {
            return;
        }
        self.settled_at = Some(reads);
        self.settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(10)).await;
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        }));
    }

    /// The control that started the action that worked finishes with it.
    fn finish(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        match key {
            "comment" | "close" | "approve" | "request_changes" => {
                self.comment.update(cx, |comment, cx| comment.set_value("", window, cx));
            }
            "title" => self.title = None,
            "menu:body" => self.describing = None,
            key if key.starts_with("reply:") => self.reply = None,
            _ => {}
        }
    }

    fn link(&mut self, number: Option<u64>, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            let Some(thread) = store.selected_thread() else { return };
            let (thread_id, server_id) = (thread.id.clone(), thread.server_id.clone());
            store.link_pull_request(number, thread_id, &server_id);
            cx.notify();
        });
    }

    fn watch(&mut self, on: bool, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            let Some(thread) = store.selected_thread() else { return };
            let (thread_id, server_id) = (thread.id.clone(), thread.server_id.clone());
            store.watch_pull_request(on, thread_id, &server_id);
            cx.notify();
        });
    }

    // MARK: Running

    /// A button's prompt goes to the composer; its action runs, after asking when it says to.
    fn run(&mut self, button: Button, from_menu: bool, target: &PanelTarget, number: u64, cx: &mut Context<Self>) {
        if let Some(prompt) = button.prompt.clone() {
            self.store.update(cx, |store, cx| {
                store.hand_off(prompt);
                cx.notify();
            });
            return;
        }
        let button = if from_menu { button.in_menu() } else { button };
        if button.confirm.is_some() {
            self.confirming = Some(button);
            cx.notify();
            return;
        }
        self.perform(button, target, number, cx);
    }

    fn perform(&mut self, button: Button, target: &PanelTarget, number: u64, cx: &mut Context<Self>) {
        let Some(action) = button.action else { return };
        self.store.update(cx, |store, cx| {
            store.pull_request_act(action, button.method, None, button.key(), target, number);
            cx.notify();
        });
    }

    /// A choice of the comment box, with what is written in it.
    fn choose(&mut self, choice: &Choice, key: String, target: &PanelTarget, number: u64, cx: &mut Context<Self>) {
        let text = self.comment.read(cx).value().trim().to_string();
        let (action, method) = (choice.action, choice.method);
        self.store.update(cx, |store, cx| {
            store.pull_request_act(action, method, Some(text), key, target, number);
            cx.notify();
        });
    }

    fn review(
        &mut self,
        verdict: ReviewVerdict,
        key: String,
        target: &PanelTarget,
        number: u64,
        cx: &mut Context<Self>,
    ) {
        let body = self.comment.read(cx).value().trim().to_string();
        self.store.update(cx, |store, cx| {
            store.pull_request_review(verdict, body, key, target, number);
            cx.notify();
        });
    }

    fn edit_title(&mut self, title: &str, window: &mut Window, cx: &mut Context<Self>) {
        let state = cx.new(|cx| InputState::new(window, cx).placeholder("Title").default_value(title.to_string()));
        cx.subscribe(&state, |this, _, event: &InputEvent, cx| match event {
            InputEvent::PressEnter { .. } => this.save_title(cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })
        .detach();
        state.update(cx, |state, cx| state.focus(window, cx));
        self.title = Some(state);
        cx.notify();
    }

    fn save_title(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &self.title else { return };
        let edited = state.read(cx).value().trim().to_string();
        let Some((_, Some(page))) = &self.page else { return };
        if edited.is_empty() || edited == page.title {
            return;
        }
        let Some(target) = self.store.read(cx).panel_target() else { return };
        let number = page.number;
        self.store.update(cx, |store, cx| {
            store.pull_request_edit(PullRequestEdit::Title { title: edited }, Some("title".into()), &target, number);
            cx.notify();
        });
    }

    fn describe(&mut self, page: &Rc<Page>, target: &PanelTarget, window: &mut Window, cx: &mut Context<Self>) {
        let (store, number, body) = (self.store.clone(), page.number, page.body.clone());
        let target = target.clone();
        let this = cx.entity().downgrade();
        self.describing = Some(cx.new(|cx| DescriptionEditor::new(store, this, target, number, body, window, cx)));
        cx.notify();
    }

    // MARK: Drawing

    fn bar(
        &self,
        page: Option<&Rc<Page>>,
        target: Option<&PanelTarget>,
        shown: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let store = self.store.read(cx);
        let unavailable = store.pull_requests_unavailable().is_some();
        let watching = store.selected_thread().is_some_and(|thread| thread.watching)
            && page.is_some_and(|page| Some(page.number) == target.and_then(|target| target.pull_request));
        let url = page.map(|page| page.url.clone()).filter(|url| !url.is_empty());
        let this = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .w_full()
            .gap(px(4.))
            .when_some(page, |bar, page| {
                bar.child(icons::symbol(page.state.symbol(), 12.).text_color(page.state.color(c)))
                    .child(label(format!("#{}", page.number), 12.5, c.text).pl(px(2.)).font_weight(FontWeight::MEDIUM))
                    .when(watching, |bar| {
                        bar.child(
                            div()
                                .id("pr-watching")
                                .pl(px(4.))
                                .child(icons::symbol("eye", 12.).text_color(c.link))
                                .tooltip(crate::ui::tooltip(
                                    "The agent hears when its checks finish, someone comments or it conflicts",
                                )),
                        )
                    })
            })
            .when(page.is_none(), |bar| bar.child(label("Pull Request", 12.5, c.text).font_weight(FontWeight::MEDIUM)))
            .child(div().flex_1().min_w(px(4.)))
            .when_some(url, |bar, url| {
                bar.child(
                    ActionButton::icon("pr-open-github", "square-arrow-out-up-right", "Open on GitHub")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .when(shown.is_some() && !unavailable, |bar| {
                bar.child(ActionButton::icon("pr-refresh", "rotate-cw", "Read the pull request again").on_click(
                    move |_, _, cx| {
                        let _ = this.update(cx, |this, cx| this.refresh(cx));
                    },
                ))
            })
    }

    /// The thread's tab while its branch has no pull request: the way to open one, or to link one.
    fn no_pull_request(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let extended = store.pull_requests_extended();
        let has_thread = store.selected_thread().is_some();
        let create = store.git_project().and_then(|project| {
            let item = project
                .git_control
                .as_ref()?
                .menu
                .iter()
                .find(|item| item.action == GitAction::CreatePr && item.reason.is_none())?
                .clone();
            Some((item, project))
        });
        let (create_store, list_store) = (self.store.clone(), self.store.clone());
        let this = cx.entity().downgrade();
        div()
            .size_full()
            .p(px(24.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.))
            .child(icons::symbol("git-pull-request", 22.).text_color(c.tertiary))
            .child(label("This branch has no pull request yet.", 13., c.secondary))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when_some(create, |row, (item, project)| {
                        row.child(ActionButton::new("pr-create", "Create PR").primary().on_click(move |_, _, cx| {
                            create_store.update(cx, |store, cx| {
                                store.choose_git(&item, &project, cx);
                                cx.notify();
                            })
                        }))
                    })
                    .when(extended, |row| {
                        row.child(ActionButton::new("pr-show-all", "Show All Pull Requests").on_click(
                            move |_, _, cx| {
                                list_store.update(cx, |store, cx| {
                                    store.open_tab(PanelTab::PullRequests);
                                    cx.notify();
                                })
                            },
                        ))
                    }),
            )
            .when(extended && has_thread, |column| {
                column.child(div().w_full().max_w(px(300.)).child(link_field(
                    "pr-link",
                    &self.linking,
                    move |number, _, cx| {
                        let _ = this.update(cx, |this, cx| this.link(Some(number), cx));
                    },
                    cx,
                )))
            })
            .into_any_element()
    }

    fn page_view(
        &mut self,
        page: &Rc<Page>,
        target: &PanelTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let extended = store.pull_requests_extended();
        let working = store.pull_requests.working.clone();
        let pending = store.pull_requests.pending_for(page.number);
        let actions = Actions {
            store: self.store.clone(),
            view: cx.entity().downgrade(),
            target: target.clone(),
            number: page.number,
            working: working.clone(),
            edits_description: extended && page.can_edit,
        };
        let placeholder =
            if pending.is_empty() { "Leave a comment" } else { "Say something with your review (optional)" };
        if self.placeholder != placeholder {
            self.placeholder = placeholder;
            self.comment.update(cx, |comment, cx| comment.set_placeholder(placeholder, window, cx));
        }
        let stack = page.stack.as_ref().filter(|stack| extended && stack.layers.len() > 1);
        let activity_title = div()
            .pt(px(6.))
            .flex()
            .items_baseline()
            .gap(px(6.))
            .child(label("Activity", 13., c.text).font_weight(FontWeight::SEMIBOLD))
            .child(label("Oldest first", 12., c.tertiary));
        div()
            .id("pr-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(16.))
                    .child(self.header(page, target, extended, &actions, cx))
                    .when_some(stack, |column, stack| column.child(self.stack_card(stack, target, cx)))
                    .child(self.merge_box(page, target, working.as_deref(), cx))
                    .child(activity_title)
                    .children(
                        page.activity
                            .iter()
                            .enumerate()
                            .map(|(index, entry)| self.activity_row(index, entry, &actions, cx)),
                    )
                    .when(page.state != crate::store::pull_requests::State::Merged, |column| {
                        column.child(self.comment_box(page, target, &pending, working.as_deref(), cx))
                    }),
            )
            .into_any_element()
    }

    // MARK: Header

    fn header(
        &self,
        page: &Rc<Page>,
        target: &PanelTarget,
        extended: bool,
        actions: &Actions,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let diff_store = self.store.clone();
        let number = page.number;
        let files = div()
            .id("pr-files")
            .group("pr-files")
            .relative()
            .h(px(24.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(highlight("pr-files", 6., edges(0., 0., 0., 0.), false, cx))
            .child(div().relative().child(icons::symbol("diff", 11.).text_color(c.secondary)))
            .child(div().relative().child(label(
                if page.files == 1 { "1 file".to_string() } else { format!("{} files", page.files) },
                12.,
                c.text,
            )))
            .children(line_counts(page.additions, page.deletions, cx).map(|counts| div().relative().child(counts)))
            .tooltip(crate::ui::tooltip("Show what it changes"))
            .on_click(move |_, _, cx| {
                diff_store.update(cx, |store, cx| {
                    store.show_diff(Some(DiffScope::PullRequest { number }), None);
                    cx.notify();
                })
            });
        let title_row = match &self.title {
            Some(state) => self.title_editor(state, page, actions, cx),
            None => div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(15.))
                        .line_height(px(20.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c.text)
                        .child(page.title.clone()),
                )
                .child(div().mt(px(-4.)).child(self.more_menu(page, target, extended, actions, cx))),
        };
        let stacked_on = page.stacked_on.as_ref().filter(|_| extended).map(|below| {
            let open = self.store.clone();
            let (below_number, below_target) = (below.number, target.clone());
            div()
                .id("pr-stacked-on")
                .group("pr-stacked-on")
                .relative()
                .ml(px(-6.))
                .h(px(24.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(highlight("pr-stacked-on", 6., edges(0., 0., 0., 0.), false, cx))
                .child(div().relative().child(icons::symbol("layers", 11.).text_color(c.secondary)))
                .child(div().relative().min_w_0().text_size(px(12.)).truncate().child(styled_line(vec![
                    ("Stacked on ".into(), c.secondary, FontWeight::NORMAL),
                    (format!("#{} ", below.number), below.state.color(c), FontWeight::NORMAL),
                    (below.title.clone(), c.text, FontWeight::NORMAL),
                ])))
                .tooltip(crate::ui::tooltip("Open the pull request it merges into"))
                .on_click(move |_, _, cx| {
                    open.update(cx, |store, cx| {
                        store.show_pull_request_tab(below_number, &below_target);
                        cx.notify();
                    })
                })
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.))
            .child(title_row)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(Chip::new(page.state.title()).icon(page.state.symbol()).tone(page.state.color(c)))
                    .child(div().flex_1().min_w_0().pt(px(2.)).child(label(page.byline.clone(), 12., c.secondary))),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(Chip::new(page.base.clone()).monospaced(true))
                    .child(icons::symbol("arrow-left", 10.).text_color(c.tertiary))
                    .child(Chip::new(page.head.clone()).monospaced(true))
                    .child(div().flex_1().min_w(px(8.)))
                    .child(files),
            )
            .children(stacked_on)
            .when(extended, |column| column.children(self.people(page, actions, cx)))
    }

    fn title_editor(
        &self,
        state: &Entity<InputState>,
        page: &Rc<Page>,
        actions: &Actions,
        cx: &mut Context<Self>,
    ) -> Div {
        let edited = state.read(cx).value().trim().to_string();
        let working = actions.is_working("title");
        let unchanged = edited.is_empty() || edited == page.title;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.title = None;
                    cx.notify();
                }
            }))
            .child(InputField::new(state).size(ControlSize::Large))
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(ActionButton::new("pr-title-cancel", "Cancel").disabled(working).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.title = None;
                            cx.notify();
                        },
                    )))
                    .child(
                        ActionButton::new("pr-title-save", "Save")
                            .primary()
                            .pending(working)
                            .disabled(working || unchanged)
                            .on_click(cx.listener(|this, _, _, cx| this.save_title(cx))),
                    ),
            )
    }

    /// Who reviews it and its labels, each with a menu to change them when the user may.
    fn people(&self, page: &Rc<Page>, actions: &Actions, cx: &mut Context<Self>) -> Option<Div> {
        let c = colors(cx);
        let reviews = !page.reviewers.is_empty() || !page.reviewer_choices.is_empty();
        let labelled = !page.labels.is_empty() || !page.label_choices.is_empty();
        if !reviews && !labelled {
            return None;
        }
        let row = |title: &'static str,
                   empty: bool,
                   choices: &[Toggle],
                   help: &'static str,
                   pending: bool,
                   chips: Vec<AnyElement>,
                   toggle: Toggler| {
            let choices = choices.to_vec();
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().w(px(70.)).flex_shrink_0().child(label(title, 12., c.tertiary)))
                .when(empty, |row| row.child(label("None yet", 12., c.tertiary)))
                .when(!empty, |row| row.child(div().flex().flex_wrap().items_center().gap(px(5.)).children(chips)))
                .when(!choices.is_empty(), |row| {
                    row.child(
                        ActionMenu::icon(SharedString::from(format!("pr-{title}-menu")), "plus", help, move |_, _| {
                            let mut menu = Menu::new();
                            for choice in &choices {
                                let (name, on) = (choice.name.clone(), choice.on);
                                let toggle = toggle.clone();
                                menu = menu.checked(on, name.clone(), move |_, cx| toggle(name.clone(), !on, cx));
                            }
                            menu
                        })
                        .button(move |button| button.small().pending(pending)),
                    )
                })
        };
        let reviewer_actions = actions.clone();
        let label_actions = actions.clone();
        Some(
            div()
                .w_full()
                .pt(px(2.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .when(reviews, |column| {
                    column.child(row(
                        "Reviewers",
                        page.reviewers.is_empty(),
                        &page.reviewer_choices,
                        "Ask for a review",
                        actions.is_working("menu:reviewers"),
                        page.reviewers
                            .iter()
                            .enumerate()
                            .map(|(index, reviewer)| {
                                let dot =
                                    if reviewer.tone == Tone::Neutral { c.tertiary } else { reviewer.tone.color(c) };
                                div()
                                    .id(("pr-reviewer", index))
                                    .child(Chip::new(reviewer.name.clone()).dot(dot))
                                    .tooltip(crate::ui::tooltip(format!("{}: {}", reviewer.name, reviewer.label)))
                                    .into_any_element()
                            })
                            .collect(),
                        Rc::new(move |name, on, cx| {
                            let (add, remove) = if on { (vec![name], Vec::new()) } else { (Vec::new(), vec![name]) };
                            reviewer_actions.edit(
                                PullRequestEdit::Reviewers { add, remove },
                                Some("menu:reviewers".into()),
                                cx,
                            );
                        }),
                    ))
                })
                .when(labelled, |column| {
                    column.child(row(
                        "Labels",
                        page.labels.is_empty(),
                        &page.label_choices,
                        "Change the labels",
                        actions.is_working("menu:labels"),
                        page.labels
                            .iter()
                            .map(|found| {
                                let color = label_color(&found.color);
                                Chip::new(found.name.clone()).dot(color).tone(color).into_any_element()
                            })
                            .collect(),
                        Rc::new(move |name, on, cx| {
                            let (add, remove) = if on { (vec![name], Vec::new()) } else { (Vec::new(), vec![name]) };
                            label_actions.edit(PullRequestEdit::Labels { add, remove }, Some("menu:labels".into()), cx);
                        }),
                    ))
                }),
        )
    }

    /// Everything else there is to do, after what the merge box offers.
    fn more_menu(
        &self,
        page: &Rc<Page>,
        target: &PanelTarget,
        extended: bool,
        actions: &Actions,
        cx: &mut Context<Self>,
    ) -> ActionMenu {
        let store = self.store.read(cx);
        let thread = store.selected_thread().map(|thread| (thread.id.clone(), thread.watching));
        let own = thread.is_some() && target.pull_request == Some(page.number);
        let working = actions.busy();
        // What a menu started has no button of its own to be pending.
        let pending = actions
            .working
            .as_deref()
            .is_some_and(|key| key.starts_with("menu:") && !["menu:reviewers", "menu:labels"].contains(&key));
        let this = cx.entity().downgrade();
        let (page, target) = (page.clone(), target.clone());
        let store = self.store.clone();
        ActionMenu::icon("pr-more", "ellipsis", "More", move |_, _| {
            let mut menu = Menu::new();
            for button in &page.menu {
                let (this, target, button) = (this.clone(), target.clone(), button.clone());
                let number = page.number;
                let enabled = !(working && button.prompt.is_none());
                let (danger, label) = (button.style == Style::Danger, button.label.clone());
                let run = move |_: &mut Window, cx: &mut App| {
                    let _ = this.update(cx, |view, cx| view.run(button.clone(), true, &target, number, cx));
                };
                menu = if danger && enabled { menu.danger_item(label, run) } else { menu.item_if(enabled, label, run) };
            }
            if extended {
                menu = menu.separator();
                if page.can_edit {
                    let (edit_title, page_title) = (this.clone(), page.title.clone());
                    menu = menu.item("Edit Title", move |window, cx| {
                        let _ = edit_title.update(cx, |view, cx| view.edit_title(&page_title, window, cx));
                    });
                    let (describe, describe_page, describe_target) = (this.clone(), page.clone(), target.clone());
                    menu = menu.item("Edit Description…", move |window, cx| {
                        let _ =
                            describe.update(cx, |view, cx| view.describe(&describe_page, &describe_target, window, cx));
                    });
                }
                if let Some((_, watching)) = thread {
                    if own {
                        if page.watchable {
                            let watch = this.clone();
                            menu = menu.item(
                                if watching { "Stop Watching" } else { "Watch for Changes" },
                                move |_, cx| {
                                    let _ = watch.update(cx, |view, cx| view.watch(!watching, cx));
                                },
                            );
                        }
                        let unlink = this.clone();
                        menu = menu.item("Unlink from This Thread", move |_, cx| {
                            let _ = unlink.update(cx, |view, cx| view.link(None, cx));
                        });
                    } else {
                        let (link, number) = (this.clone(), page.number);
                        menu = menu.item("Link to This Thread", move |_, cx| {
                            let _ = link.update(cx, |view, cx| view.link(Some(number), cx));
                        });
                    }
                }
                let list = store.clone();
                menu = menu.item("Show All Pull Requests", move |_, cx| {
                    list.update(cx, |store, cx| {
                        store.open_tab(PanelTab::PullRequests);
                        cx.notify();
                    })
                });
            }
            menu = menu.separator();
            if !page.url.is_empty() {
                let url = page.url.clone();
                menu =
                    menu.item("Copy Link", move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(url.clone())));
            }
            let head = page.head.clone();
            menu.item("Copy Branch Name", move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(head.clone())))
        })
        .button(move |button| button.pending(pending))
    }

    /// The stack the pull request is in, the top first, each one to open.
    fn stack_card(&self, stack: &Stack, target: &PanelTarget, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let url = Some(stack.url.clone()).filter(|url| !url.is_empty());
        layered(cx)
            .pb(px(6.))
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.))
                    .pt(px(4.))
                    .min_h(px(ControlSize::Small.height()))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icons::symbol("layers", 12.).text_color(c.secondary))
                    .child(label("Stack", 13., c.text).font_weight(FontWeight::MEDIUM))
                    .child(label(format!("{} pull requests onto {}", stack.layers.len(), stack.base), 12., c.tertiary))
                    .child(div().flex_1().min_w(px(4.)))
                    .when_some(url, |row, url| {
                        row.child(
                            ActionButton::new("pr-stack-open", "Open on GitHub")
                                .link()
                                .small()
                                .help("A stack merges on GitHub, bottom first")
                                .margin(edges(0., 0., 0., -8.))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            )
            .children(stack.layers.iter().rev().map(|layer| {
                let group: SharedString = format!("pr-layer-{}", layer.number).into();
                let (open, number, target) = (self.store.clone(), layer.number, target.clone());
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .relative()
                    .h(px(28.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(!layer.current, |row| row.child(highlight(group, 6., edges(0., 4., 0., 4.), false, cx)))
                    .child(
                        div()
                            .relative()
                            .w(px(16.))
                            .flex()
                            .justify_center()
                            .child(icons::symbol(layer.state.symbol(), 11.).text_color(layer.state.color(c))),
                    )
                    .child(
                        div().relative().child(
                            label(format!("#{}", layer.number), 12., c.secondary).font_weight(FontWeight::MEDIUM),
                        ),
                    )
                    .child(
                        div()
                            .relative()
                            .min_w_0()
                            .text_size(px(12.5))
                            .font_weight(if layer.current { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                            .text_color(c.text)
                            .truncate()
                            .child(layer.title.clone()),
                    )
                    .child(div().flex_1().min_w(px(4.)))
                    .when(layer.current, |row| row.child(div().relative().child(label("This one", 11.5, c.tertiary))))
                    .when(!layer.current, |row| {
                        row.on_click(move |_, _, cx| {
                            open.update(cx, |store, cx| {
                                store.show_pull_request_tab(number, &target);
                                cx.notify();
                            })
                        })
                    })
            }))
    }

    // MARK: Merge box

    /// What stands between the pull request and merging, and the button its state calls for.
    fn merge_box(&self, page: &Rc<Page>, target: &PanelTarget, working: Option<&str>, cx: &mut Context<Self>) -> Div {
        let mut card = layered(cx).flex().flex_col();
        for (index, status) in page.statuses.iter().enumerate() {
            if index > 0 {
                card = card.child(divider(cx));
            }
            card = card.child(self.status_row(index, status, target, page.number, working, cx));
            if status.kind == StatusKind::Checks {
                card = card.child(self.checks(page, cx));
            }
        }
        if let Some(primary) = &page.primary {
            card = card.child(divider(cx)).child(self.actions(page, primary, target, working, cx));
        }
        card
    }

    fn status_row(
        &self,
        index: usize,
        status: &Status,
        target: &PanelTarget,
        number: u64,
        working: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let symbol = match (status.kind, status.tone) {
            (StatusKind::Merged, _) | (StatusKind::AutoMerge, _) => "git-merge",
            (StatusKind::Closed, _) => "git-pull-request-closed",
            (StatusKind::Draft, _) => "git-pull-request-draft",
            (StatusKind::Behind, _) => "circle-arrow-down",
            (StatusKind::Review, Tone::Warning) => "eye",
            (_, Tone::Success) => "circle-check",
            (StatusKind::Conflicts, Tone::Danger) => "triangle-alert",
            (_, Tone::Danger) => "circle-x",
            (_, Tone::Pending) => "loader-circle",
            _ => "circle-alert",
        };
        let busy = working.is_some();
        div()
            .w_full()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .items_start()
            .gap(px(10.))
            .child(
                div()
                    .w(px(18.))
                    .h(px(LINE))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol(symbol, 13.).text_color(status.tone.color(c))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(label(status.title.clone(), 13., c.text).font_weight(FontWeight::MEDIUM))
                            .when_some(status.at, |row, at| row.child(label(ago(at), 12., c.tertiary))),
                    )
                    .when_some(status.detail.clone(), |column, detail| column.child(label(detail, 12., c.secondary)))
                    .when(!status.buttons.is_empty(), |column| {
                        column.child(div().pt(px(5.)).flex().items_center().gap(px(6.)).children(
                            status.buttons.iter().enumerate().map(|(button_index, button)| {
                                let pending = working == Some(button.key().as_str());
                                let (run, target) = (button.clone(), target.clone());
                                ActionButton::new(("pr-status-button", index * 10 + button_index), button.label.clone())
                                    .variant(variant_of(button.style))
                                    .pending(pending)
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run(run.clone(), false, &target, number, cx)
                                    }))
                            }),
                        ))
                    }),
            )
    }

    /// The checks that need looking at and the running ones, and the rest when asked for.
    fn checks(&self, page: &Rc<Page>, cx: &mut Context<Self>) -> Div {
        let finished = |check: &&Check| matches!(check.tone, Tone::Success | Tone::Neutral);
        let settled = page.checks.iter().filter(finished).count();
        let shown: Vec<&Check> = page.checks.iter().filter(|check| self.shows_all_checks || !finished(check)).collect();
        let title = if self.shows_all_checks {
            "Hide Finished Checks".to_string()
        } else {
            format!("Show {settled} Finished {}", if settled == 1 { "Check" } else { "Checks" })
        };
        div()
            .w_full()
            .pb(px(6.))
            .flex()
            .flex_col()
            .items_start()
            .children(shown.into_iter().enumerate().map(|(index, check)| self.check_row(index, check, cx)))
            .when(settled > 0, |column| {
                column.child(div().pl(px(30.)).child(
                    ActionButton::new("pr-all-checks", title).link().small().on_click(cx.listener(|this, _, _, cx| {
                        this.shows_all_checks = !this.shows_all_checks;
                        cx.notify();
                    })),
                ))
            })
    }

    fn check_row(&self, index: usize, check: &Check, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let store = self.store.clone();
        let fix = check.fix.clone();
        let url = check.url.clone();
        div()
            .w_full()
            .pl(px(12.))
            .pr(px(6.))
            .min_h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .w(px(18.))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(div().size(px(7.)).rounded_full().bg(check.tone.color(c))),
            )
            .child(
                div()
                    .id(("pr-check-name", index))
                    .min_w_0()
                    .flex_shrink_0()
                    .max_w(relative(0.6))
                    .text_size(px(12.5))
                    .text_color(c.text)
                    .truncate()
                    .child(check.name.clone())
                    .tooltip(crate::ui::tooltip(check.description.clone().unwrap_or_else(|| check.name.clone()))),
            )
            .when_some(check.workflow.clone(), |row, workflow| {
                row.child(div().min_w_0().text_size(px(12.)).text_color(c.tertiary).truncate().child(workflow))
            })
            .child(div().flex_1().min_w(px(6.)))
            .child(label(
                check.label.clone(),
                12.,
                if check.tone == Tone::Neutral { c.tertiary } else { check.tone.color(c) },
            ))
            .when_some(fix, |row, fix| {
                row.child(
                    ActionButton::new(("pr-check-fix", index), "Fix")
                        .link()
                        .small()
                        .help("Have the agent fix it")
                        .on_click(move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                store.hand_off(fix.clone());
                                cx.notify();
                            })
                        }),
                )
            })
            .when_some(url, |row, url| {
                row.child(
                    ActionButton::icon(("pr-check-open", index), "square-arrow-out-up-right", "Show its details")
                        .small()
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
    }

    fn actions(
        &self,
        page: &Rc<Page>,
        primary: &Button,
        target: &PanelTarget,
        working: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        // The merge button lets the way it merges be chosen, when there are several.
        let chooses = !page.methods.is_empty()
            && matches!(primary.action, Some(PullRequestAction::Merge | PullRequestAction::EnableAutoMerge));
        let variant = variant_of(primary.style);
        let busy = working.is_some();
        let (run, run_target, number) = (primary.clone(), target.clone(), page.number);
        let mut button = ActionButton::new("pr-primary", primary.label.clone())
            .variant(variant)
            .pending(working == Some(primary.key().as_str()))
            .joined(false, chooses)
            .disabled(busy)
            .on_click(cx.listener(move |this, _, _, cx| this.run(run.clone(), false, &run_target, number, cx)));
        if let Some(pending) = &primary.pending_label {
            button = button.pending_title(pending.clone());
        }
        let (store, methods, chosen, method_target) =
            (self.store.clone(), page.methods.clone(), page.method, target.clone());
        div().w_full().p(px(10.)).flex().items_center().gap(px(8.)).child(
            div().flex().items_center().gap(px(1.)).child(button).when(chooses, |split| {
                split.child(
                    ActionMenu::new("pr-method", "", move |_, _| {
                        let mut menu = Menu::new();
                        for choice in &methods {
                            let Some(method) = choice.method else { continue };
                            let (store, target) = (store.clone(), method_target.clone());
                            menu = menu.checked(Some(method) == chosen, choice.label.clone(), move |_, cx| {
                                store.update(cx, |store, cx| {
                                    store.choose_merge_method(method, &target, number);
                                    cx.notify();
                                })
                            });
                        }
                        menu
                    })
                    .button(move |button| {
                        button.variant(variant).joined(true, false).help("Choose how it merges").disabled(busy)
                    }),
                )
            }),
        )
    }

    // MARK: Activity

    /// One thing that happened on the pull request: its opening with the description, commits,
    /// comments, reviews and conversations on lines, and how it ended.
    fn activity_row(&self, index: usize, entry: &Entry, actions: &Actions, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let symbol = match (entry.kind, entry.tone) {
            (EntryKind::Opened, _) => "git-pull-request",
            (EntryKind::Commits, _) => "git-commit-horizontal",
            (EntryKind::Merged, _) => "git-merge",
            (EntryKind::Closed, _) => "git-pull-request-closed",
            (EntryKind::Review, Tone::Success) => "circle-check",
            (EntryKind::Review, Tone::Danger) => "circle-alert",
            (EntryKind::Review, _) => "eye",
            (EntryKind::Thread, _) => "code",
            _ => "message-square-text",
        };
        let id = format!("pr-entry-{index}");
        let subject = entry.subject.clone().filter(|_| entry.thread.is_none());
        let reacts = subject.as_ref().map(|subject| actions.react(subject.clone()));
        let color = if entry.tone == Tone::Neutral { c.secondary } else { entry.tone.color(c) };
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(10.))
            .child(
                div()
                    .w(px(18.))
                    .h(px(LINE))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol(symbol, 12.).text_color(color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.))
                    .child(self.byline(&id, entry, reacts.clone(), actions, cx))
                    .when(!entry.commits.is_empty(), |column| column.child(self.commits(&id, entry, actions, cx)))
                    .when_some(entry.thread.as_ref(), |column, thread| {
                        column.child(self.thread_card(thread, actions, cx))
                    })
                    .when(entry.thread.is_none() && !entry.body.is_empty(), |column| {
                        column.child(layered(cx).p(px(12.)).child(text_blocks(&id, &entry.body, cx)))
                    })
                    .when_some(reacts.filter(|_| !entry.reactions.is_empty()), |column, react| {
                        column.child(reaction_bar(&id, &entry.reactions, react))
                    }),
            )
    }

    fn byline(&self, id: &str, entry: &Entry, reacts: Option<React>, actions: &Actions, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let said = if entry.author.is_empty() { entry.said.clone() } else { format!(" {}", entry.said) };
        let line = styled_line(vec![
            (entry.author.clone(), c.text, FontWeight::SEMIBOLD),
            (said, c.secondary, FontWeight::NORMAL),
            (format!(" · {}", ago(entry.at)), c.tertiary, FontWeight::NORMAL),
        ]);
        let url = entry.url.clone();
        let edits = entry.kind == EntryKind::Opened && actions.edits_description;
        let edit = actions.view.clone();
        let edit_pending = actions.is_working("menu:body");
        div()
            .w_full()
            .flex()
            .items_start()
            .child(div().flex_1().min_w_0().text_size(px(12.5)).line_height(px(LINE)).child(line))
            .child(div().w(px(6.)).flex_shrink_0())
            .child(
                div()
                    .h(px(LINE))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .when_some(reacts.filter(|_| entry.reactions.is_empty()), |row, react| {
                        row.child(reaction_picker(
                            SharedString::from(format!("{id}-react")),
                            &entry.reactions,
                            ControlSize::Regular,
                            react,
                        ))
                    })
                    .when(edits, |row| {
                        row.child(
                            ActionButton::icon(
                                SharedString::from(format!("{id}-edit")),
                                "pencil",
                                "Edit the description",
                            )
                            .pending(edit_pending)
                            .on_click(move |_, window, cx| {
                                let _ = edit.update(cx, |view, cx| {
                                    let Some((_, Some(page))) = view.page.clone() else { return };
                                    let Some(target) = view.store.read(cx).panel_target() else { return };
                                    view.describe(&page, &target, window, cx);
                                });
                            }),
                        )
                    })
                    .when_some(url, |row, url| {
                        row.child(
                            ActionButton::icon(
                                SharedString::from(format!("{id}-open")),
                                "square-arrow-out-up-right",
                                "Open on GitHub",
                            )
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            )
    }

    /// The commits, each one opening what it changed.
    fn commits(&self, id: &str, entry: &Entry, actions: &Actions, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        div().w_full().ml(px(-6.)).flex().flex_col().children(entry.commits.iter().enumerate().map(
            |(index, commit)| {
                let group: SharedString = format!("{id}-commit-{index}").into();
                let enabled = !commit.sha.is_empty();
                let (actions, sha) = (actions.clone(), commit.sha.clone());
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .relative()
                    .w_full()
                    .h(px(24.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(enabled, |row| row.child(highlight(group, 6., edges(0., 0., 0., 0.), false, cx)))
                    .child(
                        div()
                            .relative()
                            .font_family(theme::MONO_FONT)
                            .text_size(px(11.5))
                            .text_color(c.link)
                            .child(commit.oid.clone()),
                    )
                    .child(
                        div()
                            .relative()
                            .min_w_0()
                            .text_size(px(12.5))
                            .text_color(c.text)
                            .truncate()
                            .child(commit.headline.clone()),
                    )
                    .tooltip(crate::ui::tooltip("Show what this commit changed"))
                    .when(enabled, |row| row.on_click(move |_, _, cx| actions.show_commit(sha.clone(), cx)))
            },
        ))
    }

    /// A conversation on a line: the line it is on, what was said, and a reply. A resolved one is
    /// folded until it is opened.
    fn thread_card(&self, thread: &Thread, actions: &Actions, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let open = self.open_threads.get(&thread.id).copied().unwrap_or(!thread.resolved);
        let id = thread.id.clone();
        let comments = thread.comments.len();
        let header =
            div()
                .id(SharedString::from(format!("pr-thread-{}", thread.id)))
                .group(SharedString::from(format!("pr-thread-{}", thread.id)))
                .relative()
                .w_full()
                .h(px(32.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(highlight(
                    SharedString::from(format!("pr-thread-{}", thread.id)),
                    0.,
                    edges(0., 0., 0., 0.),
                    false,
                    cx,
                ))
                .child(div().relative().w(px(10.)).flex().justify_center().child(
                    icons::symbol(if open { "chevron-down" } else { "chevron-right" }, 8.).text_color(c.tertiary),
                ))
                .child(div().relative().child(icons::symbol(file_symbol(&thread.path), 11.).text_color(c.secondary)))
                .child(
                    div()
                        .relative()
                        .min_w_0()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .truncate()
                        .child(thread.path.clone()),
                )
                .when_some(thread.line, |row, line| {
                    row.child(div().relative().flex_shrink_0().child(label(format!("line {line}"), 12., c.tertiary)))
                })
                .when(thread.outdated, |row| row.child(div().relative().child(Chip::new("Outdated").tone(c.warning))))
                .when(thread.resolved, |row| row.child(div().relative().child(Chip::new("Resolved").tone(c.success))))
                .child(div().flex_1().min_w(px(0.)))
                .when(!open, |row| {
                    row.child(div().relative().flex_shrink_0().child(label(
                        if comments == 1 { "1 comment".to_string() } else { format!("{comments} comments") },
                        11.5,
                        c.tertiary,
                    )))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let now = this.open_threads.get(&id).copied().unwrap_or(open);
                    this.open_threads.insert(id.clone(), !now);
                    cx.notify();
                }));
        let mut card = layered(cx).overflow_hidden().flex().flex_col().child(header);
        if !open {
            return card;
        }
        if !thread.hunk.is_empty() {
            card = card.child(self.hunk(&thread.hunk, cx));
        }
        for (index, comment) in thread.comments.iter().enumerate() {
            card = card.child(divider(cx)).child(self.comment_view(
                &format!("pr-thread-{}-{index}", thread.id),
                comment,
                actions,
                cx,
            ));
        }
        card.child(divider(cx)).child(self.thread_footer(thread, actions, cx))
    }

    /// The lines of the diff a conversation was written under, the line itself last.
    fn hunk(&self, lines: &[String], cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let count = lines.len();
        div().w_full().pb(px(6.)).flex().flex_col().children(lines.iter().enumerate().map(|(index, line)| {
            let fill = if line.starts_with('+') {
                c.success.opacity(0.14)
            } else if line.starts_with('-') {
                c.danger.opacity(0.14)
            } else {
                transparent_black()
            };
            div()
                .w_full()
                .min_h(px(18.))
                .px(px(12.))
                .flex()
                .items_center()
                .bg(fill)
                .when(index + 1 != count, |row| row.opacity(0.7))
                .font_family(theme::MONO_FONT)
                .text_size(px(11.5))
                .text_color(c.text)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(div().truncate().child(if line.is_empty() { " ".to_string() } else { line.clone() }))
        }))
    }

    /// A comment in a conversation on a line.
    fn comment_view(
        &self,
        id: &str,
        comment: &crate::store::pull_requests::Comment,
        actions: &Actions,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let react = actions.react(comment.id.clone());
        let url = comment.url.clone();
        div()
            .w_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(6.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .child(div().flex_1().min_w_0().text_size(px(12.5)).child(styled_line(vec![
                        (comment.author.clone(), c.text, FontWeight::SEMIBOLD),
                        (format!(" · {}", ago(comment.at)), c.tertiary, FontWeight::NORMAL),
                    ])))
                    .child(div().w(px(4.)).flex_shrink_0())
                    .child(
                        div()
                            .my(px(-4.))
                            .mr(px(-6.))
                            .flex()
                            .items_center()
                            .when(comment.reactions.is_empty(), |row| {
                                row.child(reaction_picker(
                                    SharedString::from(format!("{id}-react")),
                                    &comment.reactions,
                                    ControlSize::Small,
                                    react.clone(),
                                ))
                            })
                            .when_some(url, |row, url| {
                                row.child(
                                    ActionButton::icon(
                                        SharedString::from(format!("{id}-open")),
                                        "square-arrow-out-up-right",
                                        "Open on GitHub",
                                    )
                                    .small()
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                                )
                            }),
                    ),
            )
            .child(text_blocks(id, &comment.body, cx))
            .when(!comment.reactions.is_empty(), |column| column.child(reaction_bar(id, &comment.reactions, react)))
    }

    fn thread_footer(&self, thread: &Thread, actions: &Actions, cx: &mut Context<Self>) -> Div {
        let busy = actions.busy();
        let reply_key = format!("reply:{}", thread.id);
        if let Some((replying_to, reply)) = self.reply.as_ref().filter(|(id, _)| *id == thread.id) {
            let written = reply.read(cx).value().trim().to_string();
            let (send, thread_id) = (actions.clone(), replying_to.clone());
            let pending = actions.is_working(&reply_key);
            return div()
                .w_full()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(writing_field(reply, Some(64.), cx))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(ActionButton::new("pr-reply-cancel", "Cancel").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.reply = None;
                                cx.notify();
                            },
                        )))
                        .child(
                            ActionButton::new("pr-reply-send", "Reply")
                                .primary()
                                .pending(pending)
                                .disabled(busy || written.is_empty())
                                .on_click(move |_, _, cx| {
                                    let edit =
                                        PullRequestEdit::Reply { thread: thread_id.clone(), body: written.clone() };
                                    send.edit(edit, Some(format!("reply:{thread_id}")), cx);
                                }),
                        ),
                );
        }
        let (resolve, resolve_id, resolved) = (actions.clone(), thread.id.clone(), thread.resolved);
        let (fix_actions, fix) = (actions.clone(), thread.fix.clone());
        let thread_id = thread.id.clone();
        div()
            .w_full()
            .p(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(ActionButton::new("pr-reply", "Reply").on_click(cx.listener(move |this, _, window, cx| {
                let reply = cx.new(|cx| TextareaState::new(window, cx).placeholder("Reply"));
                cx.subscribe(&reply, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
                reply.update(cx, |reply, cx| reply.focus(window, cx));
                this.reply = Some((thread_id.clone(), reply));
                cx.notify();
            })))
            .when_some(fix, |row, fix| {
                row.child(
                    ActionButton::new("pr-thread-fix", "Fix")
                        .help("Have the agent do what it asks")
                        .on_click(move |_, _, cx| fix_actions.hand_off(fix.clone(), cx)),
                )
            })
            .child(div().flex_1())
            .when(thread.can_resolve, |row| {
                row.child(
                    ActionButton::new("pr-resolve", if resolved { "Unresolve" } else { "Resolve" })
                        .pending(actions.is_working(&format!("resolve:{}", thread.id)))
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let edit = PullRequestEdit::Resolve { thread: resolve_id.clone(), resolved: !resolved };
                            resolve.edit(edit, Some(format!("resolve:{resolve_id}")), cx);
                        }),
                )
            })
    }

    // MARK: Comment box

    fn comment_box(
        &self,
        page: &Rc<Page>,
        target: &PanelTarget,
        pending: &[PendingLineComment],
        working: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let busy = working.is_some();
        let written = self.comment.read(cx).value().trim().to_string();
        let number = page.number;
        let with_comment = page.with_comment.clone().filter(|_| pending.is_empty());
        let verdicts: Vec<Choice> = page.verdicts.clone();
        let reviewing = !pending.is_empty();
        let (comment_target, review_target) = (target.clone(), target.clone());
        let written_now = written.clone();
        div()
            .w_full()
            .pt(px(4.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(!pending.is_empty(), |column| column.child(self.pending_comments(number, pending, cx)))
            .child(writing_field(&self.comment, Some(84.), cx))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when_some(with_comment, |row, close| {
                        let target = target.clone();
                        row.child(
                            ActionButton::new("pr-close-with-comment", close.label.clone())
                                .pending(working == Some("close"))
                                .disabled(busy || written.is_empty())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose(&close, "close".into(), &target, number, cx)
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .children(verdicts.into_iter().enumerate().map(|(index, verdict)| {
                        let key = Store::verdict_key(verdict.action);
                        let asks_for_words =
                            verdict.action == PullRequestAction::RequestChanges && written.is_empty() && !reviewing;
                        let target = target.clone();
                        ActionButton::new(("pr-verdict", index), verdict.label.clone())
                            .pending(working == Some(key.as_str()))
                            .disabled(busy || asks_for_words)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !reviewing {
                                    return this.choose(&verdict, key.clone(), &target, number, cx);
                                }
                                let given = match verdict.action {
                                    PullRequestAction::Approve => ReviewVerdict::Approve,
                                    PullRequestAction::RequestChanges => ReviewVerdict::RequestChanges,
                                    _ => ReviewVerdict::Comment,
                                };
                                this.review(given, key.clone(), &target, number, cx);
                            }))
                    }))
                    .child(
                        ActionButton::new("pr-comment", if reviewing { "Send Review" } else { "Comment" })
                            .primary()
                            .pending(working == Some("comment"))
                            .disabled(busy || (written_now.is_empty() && !reviewing))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if reviewing {
                                    return this.review(
                                        ReviewVerdict::Comment,
                                        "comment".into(),
                                        &review_target,
                                        number,
                                        cx,
                                    );
                                }
                                let text = this.comment.read(cx).value().trim().to_string();
                                let target = comment_target.clone();
                                this.store.update(cx, |store, cx| {
                                    store.pull_request_act(
                                        PullRequestAction::Comment,
                                        None,
                                        Some(text),
                                        "comment".into(),
                                        &target,
                                        number,
                                    );
                                    cx.notify();
                                });
                            })),
                    ),
            )
    }

    /// The comments on lines that wait for the review, each one to take back.
    fn pending_comments(&self, number: u64, comments: &[PendingLineComment], cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let title = if comments.len() == 1 {
            "1 comment on a line goes with your review".to_string()
        } else {
            format!("{} comments on lines go with your review", comments.len())
        };
        layered(cx)
            .pb(px(6.))
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.))
                    .pt(px(9.))
                    .pb(px(4.))
                    .child(label(title, 12., c.secondary).font_weight(FontWeight::MEDIUM)),
            )
            .children(comments.iter().map(|comment| {
                let (store, id) = (self.store.clone(), comment.id);
                div()
                    .w_full()
                    .px(px(12.))
                    .py(px(4.))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(theme::MONO_FONT)
                            .text_size(px(11.5))
                            .text_color(c.link)
                            .child(format!("{}:{}", crate::models::last_component(&comment.path), comment.line)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.5))
                            .text_color(c.text)
                            .line_clamp(2)
                            .child(comment.body.clone()),
                    )
                    .child(div().w(px(4.)).flex_shrink_0())
                    .child(
                        ActionButton::icon(("pr-pending-remove", id as usize), "x", "Take it back")
                            .small()
                            .margin(edges(-4., 0., -4., 0.))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |store, cx| {
                                    store.remove_pending_comment(number, id);
                                    cx.notify();
                                })
                            }),
                    )
            }))
    }

    /// Asked before an action that says to.
    fn confirm_alert(
        &self,
        target: Option<&PanelTarget>,
        number: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let button = self.confirming.clone()?;
        let confirm = button.confirm.clone()?;
        let (target, number) = (target?.clone(), number?);
        let danger = button.style == Style::Danger;
        let (cancel_view, go_view) = (cx.entity().downgrade(), cx.entity().downgrade());
        let cancel = AlertButton::new("Cancel", move |_, cx| {
            let _ = cancel_view.update(cx, |this, cx| {
                this.confirming = None;
                cx.notify();
            });
        });
        let go = AlertButton::new(confirm.button.clone(), move |_, cx| {
            let _ = go_view.update(cx, |this, cx| {
                this.confirming = None;
                this.perform(button.clone(), &target, number, cx);
            });
        });
        let go = if danger { go.destructive() } else { go.prominent() };
        Some(
            alert("pr-confirm", confirm.title, Some(confirm.message.into()), None, vec![cancel, go], cx)
                .into_any_element(),
        )
    }
}

impl Render for PullRequestView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (target, shown, unavailable, version, reads, notice, finished, loaded) = {
            let store = self.store.read(cx);
            let target = store.panel_target();
            let shown = self.number.or(target.as_ref().and_then(|target| target.pull_request));
            let page = &store.pull_requests.page;
            let loaded = match page {
                Loaded::Loading => Loaded::Loading,
                Loaded::Failed(message) => Loaded::Failed(message.clone()),
                Loaded::Ready(page) => Loaded::Ready(page.number),
            };
            (
                target,
                shown,
                store.pull_requests_unavailable(),
                store.workspace_version,
                store.pull_requests.reads,
                store.pull_requests.notice.clone(),
                store.pull_requests.finished.clone(),
                loaded,
            )
        };
        if self.page.as_ref().map(|cached| cached.0) != Some(reads) {
            let page = self.store.read(cx).pull_requests.page.value().map(|page| Rc::new(page.clone()));
            self.page = Some((reads, page));
        }
        if let Some((key, count)) = finished
            && count != self.seen_finished
        {
            self.seen_finished = count;
            self.finish(&key, window, cx);
        }
        let page = self.page.as_ref().and_then(|cached| cached.1.clone()).filter(|page| Some(page.number) == shown);
        if let (Some(target), Some(number), None) = (&target, shown, &unavailable) {
            let trigger = Trigger { target: target.clone(), number, version, asked: self.asked };
            self.follow(trigger, cx);
            if page.as_ref().is_some_and(|page| page.settling) {
                self.settle(reads, cx);
            }
        }
        let bar = panel_bar(self.bar(page.as_ref(), target.as_ref(), shown, cx), cx);
        let body: AnyElement = match (&unavailable, shown, &target) {
            (Some(reason), _, _) => panel_message(reason.clone(), false, cx),
            (None, Some(number), Some(target)) => match (&page, &loaded) {
                (Some(page), _) => self.page_view(page, target, window, cx),
                (None, Loaded::Failed(message)) => panel_message(message.clone(), true, cx),
                _ => {
                    let _ = number;
                    panel_loading(cx)
                }
            },
            _ => self.no_pull_request(cx),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar)
            .when_some(notice, |column, notice| column.child(notice_bar(&notice, &self.store, cx)))
            .child(div().flex_1().min_h_0().child(body))
            .children(self.confirm_alert(target.as_ref(), shown, cx))
            .children(self.describing.clone())
    }
}

// MARK: The description editor

/// The sheet that edits the description, with how it will look.
pub struct DescriptionEditor {
    store: Entity<Store>,
    view: WeakEntity<PullRequestView>,
    target: PanelTarget,
    number: u64,
    original: String,
    text: Entity<TextareaState>,
    previewing: bool,
    preview: Vec<Text>,
    focus: FocusHandle,
}

impl DescriptionEditor {
    fn new(
        store: Entity<Store>,
        view: WeakEntity<PullRequestView>,
        target: PanelTarget,
        number: u64,
        original: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("Say what this changes and why").default_value(original.clone())
        });
        cx.subscribe(&text, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        text.update(cx, |text, cx| text.focus(window, cx));
        Self {
            store,
            view,
            target,
            number,
            original,
            text,
            previewing: false,
            preview: Vec::new(),
            focus: cx.focus_handle(),
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        let _ = self.view.update(cx, |view, cx| {
            view.describing = None;
            cx.notify();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let body = self.text.read(cx).value().to_string();
        let (target, number) = (self.target.clone(), self.number);
        self.store.update(cx, |store, cx| {
            store.pull_request_edit(PullRequestEdit::Body { body }, Some("menu:body".into()), &target, number);
            cx.notify();
        });
        self.dismiss(cx);
    }

    fn preview(&mut self, cx: &mut Context<Self>) {
        self.previewing = true;
        let text = self.text.read(cx).value().to_string();
        let this = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.ask(Command::Markdown { text }, move |_, reply, cx| {
                let Ok(answer) = reply else { return };
                let _ = this.update(cx, |editor, cx| {
                    editor.preview = read_blocks(&answer["blocks"]);
                    cx.notify();
                });
            });
        });
        cx.notify();
    }
}

impl Render for DescriptionEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let text = self.text.read(cx).value().to_string();
        let unchanged = text == self.original;
        let empty = text.trim().is_empty();
        let editor = cx.entity().downgrade();
        let body: AnyElement = if self.previewing {
            div()
                .id("pr-description-preview")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(12.))
                .bg(c.background_secondary)
                .rounded(px(Radius::CONTROL))
                .when(empty, |preview| preview.child(label("Nothing to preview", 12.5, c.tertiary)))
                .when(!empty, |preview| preview.child(text_blocks("pr-description-preview", &self.preview, cx)))
                .into_any_element()
        } else {
            writing_field(&self.text, None, cx).into_any_element()
        };
        let content = div()
            .id("pr-description-editor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss(cx);
                }
            }))
            .h(px(440.))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .child(label("Edit description", 15., c.text).font_weight(FontWeight::SEMIBOLD))
                    .child(div().flex_1())
                    .child(
                        Segmented::new(
                            "pr-description-mode",
                            vec!["Write".into(), "Preview".into()],
                            self.previewing as usize,
                        )
                        .on_select(move |index, _, cx| {
                            let _ = editor.update(cx, |this, cx| {
                                if index == 1 {
                                    this.preview(cx);
                                } else {
                                    this.previewing = false;
                                    cx.notify();
                                }
                            });
                        }),
                    ),
            )
            .child(body)
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ActionButton::new("pr-description-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    )
                    .child(
                        ActionButton::new("pr-description-save", "Save")
                            .primary()
                            .disabled(unchanged)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        sheet("pr-description", 560., content, cx)
    }
}
