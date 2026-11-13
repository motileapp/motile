//! The avatar beside a message the user didn't write: a button that drops a card of who sent it
//! and what it did, in the look of the sidebar's thread card, as the Mac's `MessageAvatarView`
//! and `SenderCard.swift` have it.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::{MessageAvatar, MessageSender};
use motile_protocol::wire::Agent;

use crate::models::{agent_name, last_component};
use crate::store::Store;
use crate::theme::{IconSize, Surface, TextSize, colors};
use crate::ui::dropdown::{backdrop, drop_from, over_content};
use crate::ui::menu::Anchor;
use crate::ui::{GatewayMark, divider_on, dropdown_card, icons, logos, overlay_opened, proxied_dotted, tooltip};

const MAX_WIDTH: f32 = 320.;
const ICON: IconSize = IconSize::Sm;
/// Each line's height, which its icon stands level with when its words go on to more lines.
const LINE: f32 = 16.;

#[derive(IntoElement)]
pub struct SenderAvatar {
    id: ElementId,
    avatar: MessageAvatar,
    sender: MessageSender,
    side: f32,
    store: Entity<Store>,
    /// The thread the message is in.
    thread_id: Option<String>,
}

struct Dropped {
    open: bool,
    anchor: Anchor,
    focus: FocusHandle,
}

impl SenderAvatar {
    pub fn new(
        id: impl Into<ElementId>,
        avatar: MessageAvatar,
        sender: MessageSender,
        side: f32,
        store: Entity<Store>,
        thread_id: Option<String>,
    ) -> Self {
        Self { id: id.into(), avatar, sender, side, store, thread_id }
    }
}

impl RenderOnce for SenderAvatar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let key = ElementId::NamedChild(Arc::new(self.id.clone()), "sender".into());
        let state = window.use_keyed_state(key, cx, |_, cx| Dropped {
            open: false,
            anchor: Anchor::default(),
            focus: cx.focus_handle(),
        });
        let (open, anchor, focus) = {
            let dropped = state.read(cx);
            (dropped.open, dropped.anchor.clone(), dropped.focus.clone())
        };
        let show = Rc::new(move |shown: bool, window: &mut Window, cx: &mut App| {
            let focus = state.update(cx, |dropped, _| {
                dropped.open = shown;
                dropped.focus.clone()
            });
            if shown {
                overlay_opened(cx);
                window.focus(&focus, cx);
            }
            window.refresh();
        });
        let (icon, tip) = match agent_of(self.avatar) {
            Some(agent) => (
                logos::agent_icon(agent, IconSize::Lg, cx).into_any_element(),
                format!("Sent by {}", agent_name(agent)),
            ),
            None => (
                icons::symbol("server", IconSize::Lg).text_color(c.foreground).into_any_element(),
                "Sent by your server".to_string(),
            ),
        };
        let dropdown = open.then(|| {
            let close = show.clone();
            let escape = show.clone();
            let card = sender_card(self.avatar, &self.sender, self.thread_id.as_deref(), &self.store, cx)
                .id("sender-card")
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            let layer = backdrop("sender-card-layer", move |window, cx| close(false, window, cx), window)
                .track_focus(&focus)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    cx.stop_propagation();
                    escape(false, window, cx)
                })
                .child(drop_from(anchor.bounds(), false, window).child(card));
            over_content(layer, window)
        });
        let lit = c.background_secondary_accent;
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .size(px(self.side))
            .rounded_full()
            .bg(if open { lit } else { c.background_secondary })
            .hover(move |style| style.bg(lit))
            .flex()
            .items_center()
            .justify_center()
            .when(!open, |avatar| avatar.tooltip(tooltip(tip)))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                show(!open, window, cx)
            })
            .child(anchor.track())
            .child(icon)
            .children(dropdown)
    }
}

fn agent_of(avatar: MessageAvatar) -> Option<Agent> {
    match avatar {
        MessageAvatar::Claude => Some(Agent::Claude),
        MessageAvatar::Codex => Some(Agent::Codex),
        MessageAvatar::None | MessageAvatar::Server => None,
    }
}

fn sender_card(
    avatar: MessageAvatar,
    sender: &MessageSender,
    thread_id: Option<&str>,
    store: &Entity<Store>,
    cx: &App,
) -> Div {
    let card = dropdown_card(div(), cx).max_w(px(MAX_WIDTH)).p(px(12.)).flex().flex_col();
    match agent_of(avatar) {
        Some(agent) => agent_lines(card, agent, sender, thread_id, store, cx),
        None => server_lines(card, sender, thread_id, store, cx),
    }
}

fn agent_lines(
    card: Div,
    agent: Agent,
    sender: &MessageSender,
    thread_id: Option<&str>,
    store: &Entity<Store>,
    cx: &App,
) -> Div {
    let store = store.read(cx);
    let sending = store.threads.get(&sender.thread_id);
    let server_id =
        thread_id.and_then(|id| store.threads.get(id)).map(|thread| thread.server_id.as_str()).unwrap_or_default();
    let model = Some(sender.model.as_str()).filter(|model| !model.is_empty());
    let label = store.label_of_model(model, &sender.account, agent, server_id);
    let mark = store.gateway_mark_of(&sender.account, agent, server_id);
    let where_line = match sending {
        Some(sending) => {
            let project = store.project(Some(&sending.project_id)).map(|project| project.seen(sending));
            let name =
                project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| last_component(&sending.cwd));
            line(name, logos::project_icon(project.as_ref(), ICON, cx), cx)
        }
        None => line("Its thread was deleted".into(), icons::symbol("trash-2", ICON).into_any_element(), cx),
    };
    let title = sending.map(|thread| thread.title.clone()).unwrap_or_else(|| agent_name(agent).to_string());
    card.child(title_text(title, cx)).child(
        div()
            .pt(px(10.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(model_line(label, mark, logos::agent_icon(agent, ICON, cx).into_any_element(), cx))
            .when(!sender.note.is_empty(), |lines| {
                lines.child(line(
                    sender.note.clone(),
                    icons::symbol("message-square-text", ICON).into_any_element(),
                    cx,
                ))
            })
            .child(where_line),
    )
}

fn server_lines(card: Div, sender: &MessageSender, thread_id: Option<&str>, store: &Entity<Store>, cx: &App) -> Div {
    let c = colors(cx);
    let store = store.read(cx);
    let thread = thread_id.and_then(|id| store.threads.get(id));
    let server = thread.and_then(|thread| store.server(Some(&thread.server_id)));
    let name = server.map(|server| server.name.clone()).unwrap_or_else(|| "The server".into());
    let pull_request = thread.filter(|_| sender.pull_request).and_then(|thread| {
        store
            .project(Some(&thread.project_id))
            .map(|project| project.seen(thread))
            .and_then(|project| project.pull_request(thread))
    });
    card.child(
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icons::symbol("server", ICON).text_color(c.foreground))
            .child(title_text(name, cx)),
    )
    .child(div().pt(px(4.)).text_size(TextSize::Xs).text_color(c.muted_foreground).child(sender.note.clone()))
    .when_some(pull_request, |card, pull_request| {
        card.child(div().py(px(10.)).child(divider_on(Surface::Popover, cx))).child(logos::pull_request_line(
            &pull_request,
            ICON,
            true,
            cx,
        ))
    })
}

fn title_text(text: String, cx: &App) -> Div {
    div().text_size(TextSize::Sm).font_weight(FontWeight::MEDIUM).text_color(colors(cx).foreground).child(text)
}

/// A line of the card: its icon, level with its first line, and its words, which wrap.
fn line(text: String, icon: AnyElement, cx: &App) -> Div {
    line_of(div().min_w_0().child(text), icon, cx)
}

fn model_line(text: String, mark: GatewayMark, icon: AnyElement, cx: &App) -> Div {
    let c = colors(cx);
    let words = match mark {
        GatewayMark::Hidden => div().min_w_0().child(crate::ui::control::dotted(text, c)),
        mark => proxied_dotted(text, mark, TextSize::Xs, ICON, c),
    };
    line_of(words, icon, cx)
}

fn line_of(words: Div, icon: AnyElement, cx: &App) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_start()
        .gap(px(8.))
        .text_size(TextSize::Xs)
        .line_height(px(LINE))
        .text_color(colors(cx).muted_foreground)
        .child(div().flex_none().h(px(LINE)).flex().items_center().child(icon))
        .child(words)
}
