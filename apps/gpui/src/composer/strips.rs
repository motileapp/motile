//! The strips against the composer's top and bottom, on the composer's surface: rounded on their
//! outer corners and open where they meet the composer. They stand in from the composer's sides
//! by its corner radius, so they meet its straight edge.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Children;
use motile_core::render::rows::Waiting;
use motile_protocol::wire::{Branch, Interruption};

use crate::composer::questions::Questions;
use crate::composer::{RADIUS, composer_divider};
use crate::models::{
    Project, Server, ThreadInfo, elapsed, interruption_detail, interruption_label, interruption_title, now,
};
use crate::panel::state::PanelTab;
use crate::store::Store;
use crate::theme::{self, ControlSize, Radius, Surface, colors};
use crate::ui::control::{ControlIcon, ControlLabel};
use crate::ui::menu::{Anchor, MenuIcon};
use crate::ui::rich_text::RichText;
use crate::ui::{
    ActionButton, ActionMenu, FadingScroll, Variant, edges, icons, logos, tabular_numbers, tabular_text_width,
    text_width,
};

pub const STRIP_RADIUS: f32 = Radius::LG;
/// The room between a control and the strip's visible edges.
const INSET: f32 = 3.;
/// A regular control and the room around it.
pub const HEIGHT: f32 = ControlSize::Regular.height() + INSET * 2.;
/// How far the top strip's open edge goes under the composer, which is drawn after it and
/// covers its outline there. The bottom strip starts under the composer's outline instead.
const OVERLAP: f32 = 1.;
/// The room around a control in a strip, which is the control's to click. One that ends the
/// strip stands as far from its side as from its top and bottom.
pub fn margin() -> Edges<f32> {
    edges(INSET, 4., INSET, INSET)
}

/// A strip of `height`, or as tall as what is in it with none. Its outline is inside it, so it is
/// a line taller.
pub(super) fn strip(top: bool, height: Option<f32>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .mx(px(RADIUS))
        .when_some(height, |strip, height| strip.h(px(height + 1. + if top { OVERLAP } else { 0. })))
        .flex()
        .items_center()
        .bg(c.composer)
        .border_color(c.border_composer)
        .border_l_1()
        .border_r_1()
        .when(top, |strip| strip.border_t_1().rounded_t(px(STRIP_RADIUS)).pb(px(OVERLAP)).mb(px(-OVERLAP)))
        .when(!top, |strip| strip.border_b_1().rounded_b(px(STRIP_RADIUS)))
}

/// The agent is watching something it left running: the strip says for how long, which agents
/// it started work meanwhile, and has the button that stops it.
pub fn monitoring_strip(
    store: &Entity<Store>,
    thread: &ThreadInfo,
    width: f32,
    window: &mut Window,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    let children = thread.children.as_ref().filter(|children| children.running_text.is_some());
    let help = if children.is_some() { "Stop monitoring and its agents" } else { "Stop monitoring" };
    let stop = stop_button(store, "stop-monitoring", help, cx);
    let label = format!("Monitoring: {}", elapsed(thread.monitoring_since()));
    let size = ControlSize::Regular;
    let room = width
        - strip_sides()
        - monitoring_eye_room()
        - tabular_text_width(&label, size.text_size(), FontWeight::MEDIUM, window)
        - AGENTS_DOT
        - agents_button_lead()
        - SPACER
        - stop.natural_width(window, cx);
    let agents = children.map(|children| agents_button(store, children, room, window, cx));
    strip(true, Some(HEIGHT), cx)
        .child(
            div()
                .ml(px(INSET + agents_button_lead()))
                .mr(px(SYMBOL_GAP))
                .flex_shrink_0()
                .child(icons::symbol("eye", size.symbol()).text_color(c.foreground)),
        )
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(px(size.text_size()))
                .font_weight(FontWeight::MEDIUM)
                .font_features(tabular_numbers())
                .text_color(c.foreground)
                .child(label),
        )
        .when_some(agents, |strip, agents| {
            // The button's own room before its symbol is the dot's on its other side.
            strip
                .child(
                    div()
                        .ml(px(agents_button_lead()))
                        .size(px(AGENTS_DOT))
                        .rounded_full()
                        .bg(c.muted_stronger_foreground)
                        .flex_shrink_0(),
                )
                .child(agents)
        })
        .child(div().flex_1().min_w(px(SPACER)))
        .child(stop)
}

/// The agents the thread's agent started work: the strip says which, and once its turn is over
/// has the button that stops them. During the turn the composer's own Stop does.
pub fn agents_strip(
    store: &Entity<Store>,
    children: &Children,
    stops: bool,
    width: f32,
    window: &mut Window,
    cx: &App,
) -> impl IntoElement {
    let stop = stops.then(|| stop_button(store, "stop-agents", "Stop its agents", cx));
    let room = width - strip_sides() - INSET - SPACER - stop.as_ref().map_or(0., |stop| stop.natural_width(window, cx));
    strip(true, Some(HEIGHT), cx)
        .child(div().pl(px(INSET)).flex_shrink_0().child(agents_button(store, children, room, window, cx)))
        .child(div().flex_1().min_w(px(SPACER)))
        .children(stop)
}

const SPACER: f32 = 8.;
const AGENTS_DOT: f32 = 3.;

/// The room between the agents button's side and its bot.
fn agents_button_lead() -> f32 {
    ControlSize::Regular.padding() - ControlSize::Regular.symbol_outset()
}

/// The room between the monitoring strip's eye and bot and their words.
const SYMBOL_GAP: f32 = 4.;

/// The eye before "Monitoring:" and the room either side of it.
fn monitoring_eye_room() -> f32 {
    INSET + agents_button_lead() + ControlSize::Regular.symbol_side() + SYMBOL_GAP
}

/// What a strip's sides take from the composer's width: its distance from the composer's
/// sides and its outline.
fn strip_sides() -> f32 {
    RADIUS * 2. + 2.
}

fn stop_button(store: &Entity<Store>, id: &'static str, help: &str, cx: &App) -> ActionButton {
    let help = store.read(cx).shortcuts.help(help, "thread.stop");
    let store = store.clone();
    ActionButton::new(id, "Stop").help(help).ghost().surface(Surface::Composer).margin(margin()).on_click(
        move |_, _, cx| {
            store.update(cx, |store, cx| {
                store.stop_thread(cx);
                cx.notify();
            })
        },
    )
}

/// The agents that work, as the way to the Agents tab. Where its words don't fit in `room` it
/// only counts them.
fn agents_button(store: &Entity<Store>, children: &Children, room: f32, window: &mut Window, cx: &App) -> ActionButton {
    let text = children.running_text.clone().unwrap_or_default();
    let button = |words: String| {
        let store = store.clone();
        ActionButton::new("composer-agents", words)
            .symbol("bot")
            .gap(SYMBOL_GAP)
            .help(text.clone())
            .ghost()
            .tint(Some(colors(cx).process))
            .surface(Surface::Composer)
            .margin(edges(INSET, 0., INSET, 0.))
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.open_tab(PanelTab::Agents);
                    cx.notify();
                })
            })
    };
    let full = button(format!("{} working", children.running));
    if full.natural_width(window, cx) <= room {
        return full;
    }
    button(children.running.to_string())
}

/// Why the agent stopped before it finished: the usage limit, with when the thread goes on by
/// itself, or a restart of the server. The agent is continued from here.
pub fn interruption_strip(store: &Entity<Store>, interruption: Interruption, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let at = now();
    let title = interruption_title(interruption);
    let detail = interruption_detail(interruption, at);
    let symbol = interruption_label(interruption).1;
    let button = |id: &'static str, title: &'static str, help: &'static str, run: fn(&mut Store)| {
        let store = store.clone();
        ActionButton::new(id, title).help(help).ghost().surface(Surface::Composer).margin(margin()).on_click(
            move |_, _, cx| {
                store.update(cx, |store, cx| {
                    run(store);
                    cx.notify();
                })
            },
        )
    };
    let action = match interruption {
        Interruption::Limit { resets_at: Some(resets_at), continues } if resets_at > at => Some(if continues {
            button("interruption-cancel", "Cancel", "Don't continue when the limit resets", |store| {
                store.set_continues(false)
            })
        } else {
            button("interruption-later", "Continue at Reset", "Continue the agent once the limit resets", |store| {
                store.set_continues(true)
            })
        }),
        Interruption::Limit { resets_at: Some(_), continues: true } => None,
        _ => Some(button("interruption-continue", "Continue", "Have the agent go on where it left off", |store| {
            store.continue_thread()
        })),
    };
    let help = format!("{title}. {detail}.");
    strip(true, Some(HEIGHT), cx)
        .child(div().ml(px(14.)).mr(px(7.)).flex_shrink_0().child(icons::symbol(symbol, 13.).text_color(c.warning)))
        .child(
            div()
                .id("interruption")
                .min_w_0()
                .text_size(px(12.5))
                .tooltip(crate::ui::tooltip(help))
                .child(NarrowFallback::new(title, detail)),
        )
        .child(div().flex_1().min_w(px(8.)))
        .children(action)
}

/// The interruption's title and detail where both fit; only the detail, in the warning colour
/// and cut in its middle, where they don't.
#[derive(IntoElement)]
struct NarrowFallback {
    title: SharedString,
    detail: SharedString,
}

impl NarrowFallback {
    fn new(title: &str, detail: String) -> Self {
        Self { title: title.to_string().into(), detail: detail.into() }
    }
}

impl RenderOnce for NarrowFallback {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let width = window.use_state(cx, |_, _| std::rc::Rc::new(std::cell::Cell::new(f32::MAX)));
        let room = width.read(cx).clone();
        let measure = |text: &str, weight: FontWeight, window: &mut Window| text_width(text, 12.5, weight, window);
        let full =
            measure(&self.title, FontWeight::MEDIUM, window) + 8. + measure(&self.detail, FontWeight::NORMAL, window);
        let fits = full <= room.get();
        let kept = room.clone();
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                canvas(
                    move |bounds, window, _| {
                        let width = f32::from(bounds.size.width);
                        if (kept.get() - width).abs() > 0.5 {
                            kept.set(width);
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(fits, |text| {
                text.flex()
                    .gap(px(8.))
                    .whitespace_nowrap()
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(c.warning).child(self.title))
                    .child(div().text_color(c.muted_foreground).child(self.detail.clone()))
            })
            .when(!fits, |text| {
                text.text_color(c.warning)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .child(self.detail)
            })
    }
}

/// What a waiting strip is about, and which of several it is.
pub fn waiting_title(title: String, symbol: &'static str, place: Option<String>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(icons::symbol(symbol, 13.))
        .child(div().min_w_0().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).truncate().child(title))
        .when_some(place, |title, place| {
            title.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .font_features(crate::ui::tabular_numbers())
                    .child(place),
            )
        })
}

const PADDING: f32 = 12.;
const TARGET_INSET: f32 = 10.;
/// Eight lines of a command, after which it scrolls inside its box.
const TARGET_HEIGHT: f32 = theme::CODE_LINE_HEIGHT * 8. + TARGET_INSET * 2.;

/// The tool call the agent waits with, above the composer: it is allowed or refused, and one
/// that asks questions is answered. The calls behind it wait their turn.
pub fn waiting_strip(
    store: &Entity<Store>,
    approval: &Waiting,
    count: usize,
    questions: Option<Entity<Questions>>,
    cx: &App,
) -> impl IntoElement {
    let content = match questions {
        Some(questions) => div().p(px(PADDING)).child(questions).into_any_element(),
        None => waiting_call(store, approval, count, cx).into_any_element(),
    };
    strip(true, None, cx).child(div().w_full().min_w_0().text_size(px(12.5)).child(content))
}

fn waiting_call(store: &Entity<Store>, approval: &Waiting, count: usize, cx: &App) -> Div {
    let c = colors(cx);
    let (refuse, allow) = (store.clone(), store.clone());
    let (refuse_id, allow_id) = (approval.id.clone(), approval.id.clone());
    let place = (count > 1).then(|| format!("1 of {count}"));
    let has_target = !approval.target.is_empty();
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .pt(px(PADDING))
                .px(px(PADDING))
                .pb(px(if has_target { 8. } else { PADDING }))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    waiting_title(approval.title.clone(), icons::tool_symbol(approval.icon), place, cx)
                        .text_color(c.warning),
                )
                .child(div().flex_1().min_w(px(8.)))
                .child(ActionButton::new("refuse", approval.refuse).small().surface(Surface::Composer).on_click(
                    move |_, _, cx| {
                        refuse.update(cx, |store, cx| {
                            store.answer(&refuse_id, false, Default::default());
                            cx.notify();
                        })
                    },
                ))
                .child(
                    ActionButton::new("allow", approval.allow)
                        .variant(Variant::Warning)
                        .small()
                        .surface(Surface::Composer)
                        .on_click(move |_, _, cx| {
                            allow.update(cx, |store, cx| {
                                store.answer(&allow_id, true, Default::default());
                                cx.notify();
                            })
                        }),
                ),
        )
        .when(has_target, |call| {
            call.child(
                div().px(px(PADDING)).pb(px(PADDING)).child(
                    div().rounded(px(Radius::MD)).bg(c.border_composer).overflow_hidden().child(
                        FadingScroll::new(SharedString::from(format!("target-{}", approval.id)), TARGET_HEIGHT)
                            .padding(TARGET_INSET)
                            .fill(c.border_composer)
                            .child(
                                div()
                                    .font_family(theme::MONO_FONT)
                                    .text_size(px(theme::CODE_SIZE))
                                    .line_height(px(theme::CODE_LINE_HEIGHT))
                                    .child(command(&approval.id, &approval.target, &approval.spans, cx)),
                            ),
                    ),
                ),
            )
        })
}

/// The command in the colours the core gave its stretches: their start, their length and the
/// colour, in threes, counted in UTF-16. It can be selected and copied.
fn command(id: &str, text: &str, spans: &[u32], cx: &App) -> RichText {
    let c = colors(cx);
    let mut bytes = Vec::with_capacity(text.len() + 1);
    for (index, character) in text.char_indices() {
        bytes.extend(std::iter::repeat_n(index, character.len_utf16()));
    }
    bytes.push(text.len());
    let byte = |unit: u32| bytes.get(unit as usize).copied().unwrap_or(text.len());
    let font = crate::transcript::rows::font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let run = |len: usize, color: Hsla| TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = Vec::new();
    let mut at = 0;
    for span in spans.as_chunks::<3>().0 {
        let (start, end) = (byte(span[0]), byte(span[0] + span[1]));
        let Some(color) = c.syntax.get(span[2] as usize).copied() else { continue };
        if start < at || end <= start {
            continue;
        }
        if start > at {
            runs.push(run(start - at, c.foreground));
        }
        runs.push(run(end - start, color));
        at = end;
    }
    if at < text.len() {
        runs.push(run(text.len() - at, c.foreground));
    }
    RichText::with_runs(SharedString::from(format!("command-{id}")), text.to_string(), runs)
}

fn part(title: String, icon: AnyElement, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(colors(cx).muted_foreground)
        .child(div().flex_shrink_0().child(icon))
        .when(!title.is_empty(), |part| part.child(div().whitespace_nowrap().text_size(px(12.)).child(title)))
}

/// The label of a branch that can't be switched from here.
fn branch_control(branch: String) -> ControlLabel {
    let mut label = ControlLabel::new(ControlSize::Regular);
    label.title = Some(branch.into());
    label.icon = Some(ControlIcon::Symbol("git-branch".into()));
    label
}

/// A branch that can't be switched from here, set as the button that switches one is.
fn branch_label(label: ControlLabel, cx: &App) -> Div {
    let m = margin();
    div()
        .pt(px(m.top))
        .pl(px(m.left))
        .pb(px(m.bottom))
        .pr(px(m.right))
        .text_color(colors(cx).muted_foreground)
        .child(label.render(cx))
}

/// The room a strip's part takes: its least, where its words are gone, and what it would like,
/// where they are whole. A spacer would like all there is.
#[derive(Clone, Copy)]
struct Room {
    least: f32,
    ideal: f32,
}

impl Room {
    const SPACER: Room = Room { least: 8., ideal: f32::INFINITY };

    fn spacer(&self) -> bool {
        self.ideal.is_infinite()
    }
}

/// How SwiftUI's `HStack` shares `room` between parts: a part of a higher priority first, after
/// the least of the others. Within a priority the spacers are kept to their least, and the other
/// parts, the least flexible first, are each offered an even share of what is left and take what
/// they would like within their least. The spacers take what is left over.
fn share(room: f32, parts: &[(Room, u8)]) -> Vec<f32> {
    let mut widths = vec![0.; parts.len()];
    let mut left = room;
    let mut priorities: Vec<u8> = parts.iter().map(|(_, priority)| *priority).collect();
    priorities.sort_unstable_by(|a, b| b.cmp(a));
    priorities.dedup();
    for priority in priorities {
        let lower: f32 = parts.iter().filter(|(_, other)| *other < priority).map(|(room, _)| room.least).sum();
        let group: Vec<usize> = (0..parts.len()).filter(|index| parts[*index].1 == priority).collect();
        let (spacers, mut sized): (Vec<usize>, Vec<usize>) =
            group.into_iter().partition(|index| parts[*index].0.spacer());
        sized.sort_by(|a, b| {
            let flexibility = |index: usize| parts[index].0.ideal - parts[index].0.least;
            flexibility(*a).total_cmp(&flexibility(*b))
        });
        let kept: f32 = spacers.iter().map(|index| parts[*index].0.least).sum();
        let mut available = left - lower - kept;
        for (taken, index) in sized.iter().enumerate() {
            let Room { least, ideal } = parts[*index].0;
            let width = (available / (sized.len() - taken) as f32).clamp(least, ideal.max(least));
            widths[*index] = width;
            available -= width;
            left -= width;
        }
        for (taken, index) in spacers.iter().enumerate() {
            let width = ((left - lower) / (spacers.len() - taken) as f32).max(parts[*index].0.least);
            widths[*index] = width;
            left -= width;
        }
    }
    widths
}

/// What a part of the strip says, in 12 points of `weight`, and the room around it.
struct Words {
    title: String,
    weight: FontWeight,
    around: f32,
    /// It never gives up its words, as a menu's button doesn't.
    fixed: bool,
}

impl Words {
    fn new(title: impl Into<String>, weight: FontWeight, around: f32) -> Self {
        Self { title: title.into(), weight, around, fixed: false }
    }

    fn fixed(self) -> Self {
        Self { fixed: true, ..self }
    }

    /// The words of a control, around which is the rest of its natural width.
    fn of_control(title: impl Into<String>, natural: f32, window: &mut Window) -> Self {
        let title = title.into();
        let around = natural - text_width(&title, ControlSize::Regular.text_size(), FontWeight::MEDIUM, window);
        Self::new(title, FontWeight::MEDIUM, around)
    }

    fn room(&self, window: &mut Window) -> Room {
        let ideal = self.around + text_width(&self.title, 12., self.weight, window);
        Room { least: if self.fixed { ideal } else { self.around }, ideal }
    }

    /// The words cut in their middle to fit `width` with the room around them, as a Text that is
    /// given less than it would like is: only as wide as what is left of them.
    fn fitted(&self, width: f32, window: &mut Window, cx: &mut App) -> String {
        if width >= self.room(window).ideal - 0.5 {
            return self.title.clone();
        }
        let room = (width - self.around).max(0.);
        let font = crate::transcript::rows::font(theme::UI_FONT, self.weight, false);
        crate::ui::elide_middle(&self.title.clone().into(), &font, px(12.), px(room), cx).to_string()
    }
}

/// Where the composer's thread works: the server, the folder or a worktree of its own, and the
/// branch checked out there, which opens the picker to switch. A thread that starts in a new
/// worktree picks the branch it starts from there instead. Where it is too narrow for all of
/// them, the workspace is only its icon, as the Mac's `ViewThatFits` picks, and the room is shared
/// as its `HStack` shares it: the menu of the workspace keeps its words, and the others are cut in
/// their middles to an even share.
#[allow(clippy::too_many_arguments)]
pub fn context_strip(
    store: &Entity<Store>,
    project: &Project,
    server: Option<&Server>,
    anchor: &Anchor,
    picker: Option<Entity<BranchPicker>>,
    width: f32,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let c = *colors(cx);
    let state = store.read(cx);
    let starts_in_worktree = state.draft_uses_worktree();
    let branch = if starts_in_worktree { state.draft_base() } else { project.branch.clone() };
    let has_thread = state.selected_thread().is_some();
    let can_switch = state.can_switch_branches(project);
    let can_use_worktrees = state.can_use_worktrees(project);
    let no_commit = project.git.as_ref().is_some_and(|git| git.no_commit);
    let has_workspace = project.worktree.is_some()
        || (has_thread && project.branch.is_some())
        || (!has_thread && can_use_worktrees)
        || (!has_thread && no_commit);
    let in_worktree = starts_in_worktree;
    let workspace_help = state.shortcuts.help(
        if in_worktree {
            "The thread works in a folder and on a branch of its own"
        } else {
            "The thread works in the project's folder"
        },
        "composer.workspace",
    );
    let start = branch.as_ref().map(|branch| state.draft_start().unwrap_or(branch.clone()));
    let branch_help = branch.as_ref().zip(start.as_ref()).map(|(branch, start)| {
        let help = if !starts_in_worktree {
            format!("Switch the branch of {}", project.name)
        } else if start != branch {
            format!("The worktree starts from {start}, which has commits {branch} doesn't have here yet")
        } else {
            "The branch the worktree's branch starts from".to_string()
        };
        state.shortcuts.help(&help, "composer.branch")
    });
    // The symbols of the parts that only say where the thread works.
    let mark = theme::symbol_side(11.);

    /// Where the thread works, which is told and can't be changed here.
    struct Working {
        id: &'static str,
        title: &'static str,
        symbol: &'static str,
        help: String,
    }
    enum Workspace {
        Told(Working),
        Chosen,
    }
    let workspace = if let Some(worktree) = &project.worktree {
        Some(Workspace::Told(Working {
            id: "worktree",
            title: "Worktree",
            symbol: "folder-git-2",
            help: worktree.path.clone(),
        }))
    } else if has_thread && project.branch.is_some() {
        Some(Workspace::Told(Working {
            id: "checkout",
            title: "Local checkout",
            symbol: "folder",
            help: "The thread works in the project's folder".into(),
        }))
    } else if !has_thread && can_use_worktrees {
        Some(Workspace::Chosen)
    } else if !has_thread && no_commit {
        let branch = project.branch.clone().unwrap_or_default();
        Some(Workspace::Told(Working {
            id: "checkout",
            title: "Current checkout",
            symbol: "folder",
            help: format!("A worktree can start once {branch} has a commit"),
        }))
    } else {
        None
    };
    let workspace_menu = |title: String| {
        let choose = store.clone();
        let symbol = if in_worktree { "folder-git-2" } else { "folder" };
        let help = workspace_help.clone();
        ActionMenu::new("workspace", title, move |_, _| {
            let (current, new) = (choose.clone(), choose.clone());
            crate::ui::menu::Menu::new()
                .note("Workspace")
                .choice(!in_worktree, "Current checkout", Some(MenuIcon::Symbol("folder")), None, move |_, cx| {
                    current.update(cx, |store, cx| {
                        store.set_draft_worktree(false, cx);
                        cx.notify();
                    })
                })
                .choice(in_worktree, "New worktree", Some(MenuIcon::Symbol("folder-git-2")), None, move |_, cx| {
                    new.update(cx, |store, cx| {
                        store.set_draft_worktree(true, cx);
                        cx.notify();
                    })
                })
        })
        .button(|button| {
            button.symbol(symbol).help(help).surface(Surface::Composer).margin(edges(INSET, 4., INSET, 4.))
        })
        .shortcut("composer.workspace")
    };
    let branch_button = |title: String| {
        let (open, project) = (store.clone(), project.clone());
        ActionButton::new("branch", title)
            .symbol("git-branch")
            .help(branch_help.clone().unwrap_or_default())
            .ghost()
            .opens(true)
            .surface(Surface::Composer)
            .margin(margin())
            .on_click(move |_, _, cx| {
                open.update(cx, |store, cx| {
                    if store.shows_branches {
                        store.shows_branches = false;
                    } else {
                        store.show_branches(&project);
                    }
                    cx.notify();
                })
            })
    };

    let server_words = server.map(|server| Words::new(&server.short_name, FontWeight::NORMAL, 14. + mark + 5.));
    let project_words =
        Words::new(&project.name, FontWeight::NORMAL, if server.is_none() { 14. } else { 0. } + 13. + 5.);
    let words_of_workspace = |labeled: bool, window: &mut Window, cx: &mut App| {
        workspace.as_ref().map(|workspace| match workspace {
            Workspace::Told(working) if labeled => Words::new(working.title, FontWeight::NORMAL, 26. + mark + 5.),
            Workspace::Told(_) => Words::new("", FontWeight::NORMAL, 26. + mark),
            Workspace::Chosen => {
                let title = match (labeled, in_worktree) {
                    (false, _) => "",
                    (true, true) => "New worktree",
                    (true, false) => "Current checkout",
                };
                Words::of_control(title, workspace_menu(title.into()).natural_width(window, cx), window).fixed()
            }
        })
    };
    let switches = branch.is_some() && project.worktree.is_none() && can_switch;
    let branch_words = branch.as_ref().map(|branch| {
        if switches {
            let title =
                if starts_in_worktree { format!("From {}", start.clone().unwrap_or_default()) } else { branch.clone() };
            let natural = branch_button(title.clone()).natural_width(window, cx);
            return Words::of_control(title, natural, window);
        }
        let natural = margin().left + margin().right + branch_control(branch.clone()).natural_width(window);
        Words::of_control(branch.clone(), natural, window)
    });

    // The strip's sides and the divider after the server take no part.
    let fixed = 2. + if server.is_some() { 20. } else { 0. };
    let room = width - 2. * RADIUS - fixed;
    let labeled = words_of_workspace(true, window, cx);
    let ideal: f32 = server_words
        .iter()
        .chain([&project_words])
        .chain(labeled.iter())
        .chain(branch_words.iter())
        .map(|words| words.room(window).ideal)
        .sum();
    let workspace_words =
        if ideal + Room::SPACER.least <= room { labeled } else { words_of_workspace(false, window, cx) };

    let mut rooms = Vec::new();
    rooms.extend(server_words.as_ref().map(|words| (words.room(window), 0)));
    rooms.push((project_words.room(window), 0));
    rooms.push((Room::SPACER, 0));
    rooms.extend(workspace_words.as_ref().map(|words| (words.room(window), 0)));
    // The Mac means the branch to keep its room first, but the padding around it takes its priority.
    rooms.extend(branch_words.as_ref().map(|words| (words.room(window), 0)));
    let mut widths = share(room, &rooms).into_iter();
    let mut next = || widths.next().unwrap_or(f32::INFINITY);
    let server_title = server_words.as_ref().map(|words| words.fitted(next(), window, cx));
    let project_title = project_words.fitted(next(), window, cx);
    let _spacer = next();
    let workspace_title = workspace_words.as_ref().map(|words| words.fitted(next(), window, cx));
    let branch_title = branch_words.as_ref().map(|words| words.fitted(next(), window, cx));

    let server_part = server.zip(server_title).map(|(server, title)| {
        let tint = server.state_tint(&c).unwrap_or(c.muted_foreground);
        div()
            .id("strip-server")
            .flex_shrink_0()
            .pl(px(14.))
            .tooltip(crate::ui::tooltip(format!("On {}", server.name)))
            .child(part(title, icons::symbol("server", 11.).text_color(tint).into_any_element(), cx))
    });
    let project_part = div()
        .id("strip-project")
        .flex_shrink_0()
        .when(server.is_none(), |part| part.pl(px(14.)))
        .tooltip(crate::ui::tooltip(project.path.clone()))
        .child(part(project_title, logos::project_icon(Some(project), 13., cx), cx));
    let workspace_part = workspace.zip(workspace_title).map(|(workspace, title)| match workspace {
        Workspace::Told(working) => div()
            .id(working.id)
            .flex_shrink_0()
            .px(px(13.))
            .tooltip(crate::ui::tooltip(working.help))
            .child(part(title, icons::symbol(working.symbol, 11.).into_any_element(), cx))
            .into_any_element(),
        Workspace::Chosen => workspace_menu(title).into_any_element(),
    });
    let branch_part = branch_title.map(|title| {
        if switches {
            let viewport = f32::from(crate::window_frame::content_bounds(window).bottom());
            return div()
                .relative()
                .flex_shrink_0()
                .flex()
                .child(anchor.track())
                .child(branch_button(title))
                .when_some(picker, |button, picker| {
                    let height = picker.read(cx).height(cx);
                    button.child(
                        deferred(
                            anchored()
                                .position(picker_origin(anchor, height, viewport))
                                .snap_to_window_with_margin(px(WINDOW_MARGIN))
                                .child(picker),
                        )
                        .with_priority(1),
                    )
                })
                .into_any_element();
        }
        let help = if project.worktree.is_some() {
            "The branch of this thread's worktree".to_string()
        } else {
            match server {
                Some(server) if server.known => format!("Update {} to switch branches from here", server.name),
                _ => "The branch checked out there".to_string(),
            }
        };
        div()
            .id("branch")
            .flex_shrink_0()
            .tooltip(crate::ui::tooltip(help))
            .child(branch_label(branch_control(title), cx))
            .into_any_element()
    });
    let divides = has_workspace && branch_part.is_some();

    strip(false, Some(HEIGHT), cx)
        .when_some(server_part, |strip, part| {
            strip.child(part).child(div().flex_shrink_0().mx(px(10.)).child(composer_divider(&c)))
        })
        .child(project_part)
        .child(div().flex_1().min_w(px(Room::SPACER.least)))
        .children(workspace_part)
        .when(divides, |strip| strip.child(composer_divider(&c)))
        .children(branch_part.map(|part| div().flex_shrink_0().child(part)))
}

const PICKER_WIDTH: f32 = 300.;
/// The room between the picker and its button.
const PICKER_GAP: f32 = 6.;
/// The least room between the picker and the window's edges.
const WINDOW_MARGIN: f32 = 8.;

/// Where the branch picker opens, as a popover does: centred under its button, or over it when
/// there is no room under it, as under an open thread's composer; where neither has room, on the
/// side with more.
fn picker_origin(anchor: &Anchor, height: f32, viewport: f32) -> Point<Pixels> {
    let bounds = anchor.bounds();
    let x = bounds.center().x - px(PICKER_WIDTH / 2.);
    let (top, bottom) = (f32::from(bounds.top()), f32::from(bounds.bottom()));
    let below = viewport - WINDOW_MARGIN - (bottom + PICKER_GAP);
    let above = top - PICKER_GAP - WINDOW_MARGIN;
    if height <= below || below >= above {
        return point(x, px(bottom + PICKER_GAP));
    }
    point(x, px(top - PICKER_GAP - height))
}

#[derive(Clone)]
enum Choice {
    Branch(Branch),
    Create(String),
}

/// The branches of the project's repository, to switch to one or make a new one. What is typed
/// narrows the list, and becomes the name of a branch to make when it matches none. When the
/// open draft starts in a new worktree it picks the branch that starts from instead, and
/// switches nothing.
pub struct BranchPicker {
    store: Entity<Store>,
    query: Entity<InputState>,
    problem: Option<String>,
    highlighted: usize,
    switching: bool,
    scroll: UniformListScrollHandle,
    /// The button it opens from, whose presses are the button's to close it.
    button: Anchor,
    /// How tall it was last drawn, for where it opens.
    drawn: Rc<Cell<Option<f32>>>,
    _subscriptions: Vec<Subscription>,
}

const ROW_HEIGHT: f32 = 30.;
const LIST_PADDING: f32 = 8.;
const MAX_LIST_HEIGHT: f32 = 300.;
const SEARCH_HEIGHT: f32 = 38.;
const PLACEHOLDERS: usize = 5;
const PLACEHOLDER_WIDTHS: [f32; PLACEHOLDERS] = [90., 150., 120., 170., 105.];

impl BranchPicker {
    pub fn new(store: Entity<Store>, button: Anchor, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let base = store.read(cx).draft_uses_worktree();
        let prompt = if base { "Start from a branch" } else { "Switch or create a branch" };
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(prompt));
        query.update(cx, |query, cx| query.focus(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&query, window, |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.highlighted = 0;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.choose(this.highlighted, cx),
                _ => {}
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self {
            store,
            query,
            problem: None,
            highlighted: 0,
            switching: false,
            scroll: UniformListScrollHandle::new(),
            button,
            drawn: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    fn base(&self, cx: &App) -> Option<String> {
        let store = self.store.read(cx);
        if store.draft_uses_worktree() { store.draft_base() } else { None }
    }

    fn project(&self, cx: &App) -> Option<Project> {
        self.store.read(cx).composer_project()
    }

    fn working(&self, cx: &App) -> bool {
        self.base(cx).is_none() && self.project(cx).is_some_and(|project| self.store.read(cx).is_working_in(&project))
    }

    fn note(&self, cx: &App) -> Option<String> {
        if self.working(cx) {
            return Some("An agent is working in this project. Switch when it has finished.".into());
        }
        self.problem.clone()
    }

    /// The height of all the branches, also while fewer match: a popover that shrinks leaves the
    /// button it opened from.
    fn list_height(&self, cx: &App) -> f32 {
        let rows = match &self.store.read(cx).listed_branches {
            None => PLACEHOLDERS,
            Some(Ok(branches)) => branches.len(),
            Some(Err(_)) => 0,
        };
        MAX_LIST_HEIGHT.min(rows as f32 * ROW_HEIGHT + 2. * LIST_PADDING)
    }

    /// How tall the picker is drawn, for where it opens: as it was last drawn, or before that as
    /// its list and a line of note would make it.
    pub fn height(&self, cx: &App) -> f32 {
        if let Some(drawn) = self.drawn.get() {
            return drawn;
        }
        let list = match &self.store.read(cx).listed_branches {
            Some(Ok(_)) if !self.choices(cx).is_empty() => self.list_height(cx),
            None => self.list_height(cx),
            _ => 50.,
        };
        let note = if self.note(cx).is_some() { 1. + 2. * 8. + 16. } else { 0. };
        2. + SEARCH_HEIGHT + 1. + list + note
    }

    fn choices(&self, cx: &App) -> Vec<Choice> {
        let Some(Ok(branches)) = &self.store.read(cx).listed_branches else { return Vec::new() };
        let needle = self.query.read(cx).value().trim().to_string();
        let lower = needle.to_lowercase();
        let mut choices: Vec<Choice> = branches
            .iter()
            .filter(|branch| needle.is_empty() || branch.name.to_lowercase().contains(&lower))
            .cloned()
            .map(Choice::Branch)
            .collect();
        if self.base(cx).is_none() && !needle.is_empty() && !branches.iter().any(|branch| branch.name == needle) {
            choices.push(Choice::Create(needle));
        }
        choices
    }

    fn close(&self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.shows_branches = false;
            store.composer_focus += 1;
            cx.notify();
        });
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        let choices = self.choices(cx);
        if self.switching || self.working(cx) || index >= choices.len() {
            return;
        }
        let (name, create) = match &choices[index] {
            Choice::Branch(branch) => (branch.name.clone(), false),
            Choice::Create(name) => (name.clone(), true),
        };
        if self.base(cx).is_some() {
            self.store.update(cx, |store, _| store.set_draft_base(name));
            return self.close(cx);
        }
        let current = matches!(&choices[index], Choice::Branch(branch) if branch.current);
        if !create && current {
            return self.close(cx);
        }
        let Some(project) = self.project(cx) else { return };
        self.switching = true;
        self.problem = None;
        let this = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.switch_branch(&project, name, create, move |store, failure, cx| {
                let Some(failure) = failure else {
                    store.shows_branches = false;
                    store.composer_focus += 1;
                    return;
                };
                let _ = this.update(cx, |this, cx| {
                    this.switching = false;
                    this.problem = Some(failure);
                    cx.notify();
                });
            });
        });
    }

    fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.choices(cx).len();
        if count == 0 {
            return;
        }
        self.highlighted = (self.highlighted as isize + step).clamp(0, count as isize - 1) as usize;
        self.scroll.scroll_to_item(self.highlighted, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn row(
        &self,
        index: usize,
        choice: &Choice,
        base: Option<&String>,
        working: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = colors(cx);
        let lit = Surface::Popover.layer(theme::Layer::Row, c);
        let row = div()
            .id(("branch-choice", index))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(Radius::MD))
            .text_size(px(12.5))
            .text_color(c.foreground)
            .when(index == self.highlighted, |row| row.bg(lit))
            .when(working, |row| row.opacity(c.opacity(theme::Opacity::Disabled)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.highlighted = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)));
        let mark = |symbol: &'static str, color: Hsla| {
            div().w(px(14.)).flex_shrink_0().flex().justify_center().child(icons::symbol(symbol, 11.).text_color(color))
        };
        match choice {
            Choice::Branch(branch) => {
                let chosen = base.map_or(branch.current, |base| base == &branch.name);
                let tag = if base.is_some() && branch.current {
                    Some("current")
                } else if branch.default {
                    Some("default")
                } else if branch.remote {
                    Some("remote")
                } else {
                    None
                };
                row.child(mark(
                    if chosen { "check" } else { "git-branch" },
                    if chosen { c.foreground } else { c.muted_stronger_foreground },
                ))
                .child(crate::ui::middle_truncated(branch.name.clone()))
                .child(div().flex_1().min_w(px(8.)))
                .when_some(tag, |row, tag| {
                    row.child(div().text_size(px(11.)).text_color(c.muted_stronger_foreground).child(tag))
                })
            }
            Choice::Create(name) => row
                .child(mark("plus", c.muted_stronger_foreground))
                .child(crate::ui::middle_truncated(format!("Create branch “{name}”")))
                .child(div().flex_1().min_w(px(8.))),
        }
    }
}

/// A branch's row before the branches are listed: bars where its icon and name will be, fading
/// in and out.
fn placeholder(index: usize, cx: &App) -> impl IntoElement {
    let fill = Surface::Popover.layer(theme::Layer::Control, colors(cx));
    let faded = colors(cx).opacity(theme::Opacity::Disabled);
    div()
        .h(px(ROW_HEIGHT))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(7.))
        .child(
            div()
                .w(px(14.))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .child(div().size(px(11.)).rounded(px(Radius::XS)).bg(fill)),
        )
        .child(div().w(px(PLACEHOLDER_WIDTHS[index % PLACEHOLDERS])).h(px(8.)).rounded_full().bg(fill))
        .with_animation(
            ("branch-placeholder", index),
            Animation::new(std::time::Duration::from_millis(800)).repeat().with_easing(bounce(ease_in_out)),
            move |row, delta| row.opacity(1. - (1. - faded) * delta),
        )
}

impl Render for BranchPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let choices = self.choices(cx);
        let base = self.base(cx);
        let working = self.working(cx);
        let listed = self.store.read(cx).listed_branches.clone();
        let list_height = self.list_height(cx);
        let note = self.note(cx);
        let list: AnyElement = match &listed {
            None => div()
                .h(px(list_height))
                .p(px(LIST_PADDING))
                .children((0..PLACEHOLDERS).map(|index| placeholder(index, cx)))
                .into_any_element(),
            Some(Err(error)) => {
                div().p(px(16.)).text_size(px(12.5)).text_color(c.destructive).child(error.clone()).into_any_element()
            }
            Some(Ok(_)) if choices.is_empty() => div()
                .p(px(16.))
                .text_size(px(12.5))
                .text_color(c.muted_foreground)
                .child("No branch matches.")
                .into_any_element(),
            Some(Ok(_)) => {
                let choices = Rc::new(choices);
                let rows = uniform_list(
                    "branches",
                    choices.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| this.row(index, &choices[index], base.as_ref(), working, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .size_full()
                .p(px(LIST_PADDING));
                div()
                    .relative()
                    .h(px(list_height))
                    .child(rows)
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .child(Scrollbar::vertical(&self.scroll).id("branches-scroller").viewport_from_layout()),
                    )
                    .into_any_element()
            }
        };
        let drawn = self.drawn.clone();
        let button = self.button.clone();
        crate::ui::dropdown_card(div(), cx)
            .id("branch-picker")
            .relative()
            .w(px(PICKER_WIDTH))
            .rounded(px(Radius::LG))
            .occlude()
            .flex()
            .flex_col()
            .child(
                canvas(
                    move |bounds, window, _| {
                        let height = f32::from(bounds.size.height);
                        if drawn.get().is_none_or(|drawn| (drawn - height).abs() > 0.5) {
                            drawn.set(Some(height));
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if !button.bounds().contains(&event.position) {
                    this.close(cx);
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| match event.keystroke.key.as_str() {
                "down" => this.move_highlight(1, cx),
                "up" => this.move_highlight(-1, cx),
                "escape" => {
                    cx.stop_propagation();
                    this.close(cx)
                }
                _ => {}
            }))
            .child(
                div()
                    .h(px(SEARCH_HEIGHT))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol("search", 12.).text_color(c.muted_stronger_foreground))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.query).appearance(false).px_0().py_0().text_size(px(13.))),
                    ),
            )
            .child(crate::ui::divider_on(Surface::Popover, cx))
            .child(list)
            .when_some(note, |picker, note| {
                picker.child(crate::ui::divider_on(Surface::Popover, cx)).child(
                    div()
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(11.5))
                        .line_height(px(16.))
                        .text_color(if working { c.muted_foreground } else { c.destructive })
                        .child(note),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Room, share};

    fn room(least: f32, ideal: f32) -> Room {
        Room { least, ideal }
    }

    /// The parts of the context strip: the server, the divider after it, the project, the
    /// spacer, the workspace and the branch, as SwiftUI shared them in an `HStack` of each width.
    fn strip(width: f32) -> Vec<f32> {
        let parts = [
            (room(32., 68.), 0),
            (room(20., 20.), 0),
            (room(18., 35.), 0),
            (Room::SPACER, 0),
            (room(44., 130.), 0),
            (room(53., 141.), 1),
        ];
        share(width, &parts)
    }

    #[test]
    fn parts_that_fit_are_whole_and_the_spacer_takes_the_rest() {
        assert_eq!(strip(500.), vec![68., 20., 35., 106., 130., 141.]);
    }

    #[test]
    fn the_branch_keeps_its_words_and_the_others_share_an_even_room() {
        assert_eq!(strip(330.), vec![63., 20., 35., 8., 63., 141.]);
        assert_eq!(strip(300.), vec![48., 20., 35., 8., 48., 141.]);
    }

    #[test]
    fn the_branch_gives_up_its_words_only_after_the_others_gave_up_theirs() {
        assert_eq!(strip(260.)[5], 138.);
    }

    #[test]
    fn a_spacer_alone_leaves_the_words_all_but_its_least() {
        assert_eq!(share(200., &[(room(0., 300.), 0), (Room::SPACER, 0)]), vec![192., 8.]);
    }
}
