//! Signs an agent's account in again once its sign-in stopped working, and has the thread's agent
//! go on: the Mac app's `SignInAgainSheet`. A server too old to sign the agent in shows the command
//! to run on it.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Agent, AgentAccount};

use super::sign_in_steps::{AccountSignIn, SignInStep, SignInSteps};
use crate::models::{AgentAccountExt, Server, agent_name};
use crate::store::{Store, ThreadSignIn};
use crate::theme::{IconSize, TextSize, colors};
use crate::ui::{ActionButton, CommandBox, beside, icons};

pub struct SignInAgainSheet {
    store: Entity<Store>,
    pub request: ThreadSignIn,
    sign_in: Entity<AccountSignIn>,
    _subscription: Subscription,
}

impl SignInAgainSheet {
    pub fn new(store: Entity<Store>, request: ThreadSignIn, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let server_id = store.read(cx).threads.get(&request.thread_id).map(|thread| thread.server_id.clone());
        let sign_in = cx.new(|cx| AccountSignIn::new(store.clone(), server_id.unwrap_or_default(), window, cx));
        let subscription = cx.observe(&sign_in, |_, _, cx| cx.notify());
        let sheet = Self { store, request, sign_in, _subscription: subscription };
        sheet.start(cx);
        sheet
    }

    fn server<'a>(&self, store: &'a Store) -> Option<&'a Server> {
        let thread = store.threads.get(&self.request.thread_id)?;
        store.server(Some(&thread.server_id))
    }

    fn account(&self, cx: &App) -> Option<AgentAccount> {
        let server = self.server(self.store.read(cx))?;
        server.agent_accounts.iter().find(|account| account.id == self.request.account_id).cloned()
    }

    fn signs_in_here(&self, cx: &App) -> bool {
        let Some(account) = self.account(cx) else { return false };
        self.server(self.store.read(cx)).is_some_and(|server| server.signs_in(account.agent))
    }

    fn signed_in(&self, cx: &App) -> bool {
        matches!(self.sign_in.read(cx).step, SignInStep::SignedIn(_))
    }

    fn start(&self, cx: &mut Context<Self>) {
        let Some(account) = self.account(cx) else { return };
        if !self.signs_in_here(cx) || self.sign_in.read(cx).step != SignInStep::Idle {
            return;
        }
        self.sign_in.update(cx, |sign_in, cx| sign_in.start(account, |_, _| {}, cx));
    }

    fn finish(&self, cx: &mut Context<Self>) {
        let Some(account) = self.account(cx) else { return };
        self.sign_in.update(cx, |sign_in, cx| sign_in.finish(account, |_, _| {}, cx));
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.signing_in_thread = None;
            cx.notify();
        });
    }

    fn go_on(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.signing_in_thread = None;
            store.continue_thread();
            cx.notify();
        });
    }

    /// Return finishes the sign-in from the code's field, and otherwise continues once it can.
    pub fn press_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sign_in.read(cx).code_focused(window, cx) {
            return self.finish(cx);
        }
        if self.signed_in(cx) || !self.signs_in_here(cx) {
            self.go_on(cx);
        }
    }

    fn steps(&self, account: Option<&AgentAccount>, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        if let Some(who) = self.sign_in.read(cx).signed_in_as() {
            return div()
                .px(px(4.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icons::symbol("check", IconSize::Sm).text_color(c.success))
                .child(
                    div()
                        .min_w_0()
                        .text_size(TextSize::Sm)
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis_middle()
                        .child(format!("Signed in as {who}")),
                )
                .into_any_element();
        }
        if self.signs_in_here(cx) {
            let (start, finish) = (cx.weak_entity(), cx.weak_entity());
            return SignInSteps::new(
                &self.sign_in,
                None,
                move |_, cx| {
                    let _ = start.update(cx, |sheet, cx| sheet.start(cx));
                },
                move |_, cx| {
                    let _ = finish.update(cx, |sheet, cx| sheet.finish(cx));
                },
            )
            .signs_in_again(false)
            .into_any_element();
        }
        let command = account.map(|account| account.sign_in_command());
        CommandBox::new("sign-in-command", command).into_any_element()
    }
}

impl Render for SignInAgainSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let server_name = self.server(store).map_or("its server".to_string(), |server| server.short_name.clone());
        let others = self.server(store).map(|server| server.agent_accounts.clone()).unwrap_or_default();
        let account = self.account(cx);
        let agent = account.as_ref().map_or("the agent", |account| agent_name(account.agent));
        let account_title = match &account {
            Some(account)
                if others.iter().filter(|other| other.agent == account.agent && !other.gateway).count() > 1 =>
            {
                format!("{agent} ({})", account.name)
            }
            _ => agent.to_string(),
        };
        let message = format!(
            "{account_title} on {server_name} was signed out, so the agent stopped. Sign in again and it goes on where it left off."
        );
        let (signs_in_here, signed_in) = (self.signs_in_here(cx), self.signed_in(cx));
        let sign_in = self.sign_in.read(cx);
        let caption = if !signs_in_here {
            format!("Run it in a terminal on {server_name}, then continue.")
        } else if signed_in {
            "Continue has the agent go on where it left off.".to_string()
        } else if let Some(waiting) = sign_in.waiting_caption() {
            waiting.to_string()
        } else {
            let page = AccountSignIn::page(account.as_ref().map_or(Agent::Claude, |account| account.agent));
            format!("Opens {page}.")
        };
        let error = sign_in.error.clone();
        let footer = if signed_in || !signs_in_here {
            let close = ActionButton::new("sign-in-again-close", if signed_in { "Close" } else { "Cancel" })
                .ghost()
                .margin(beside(0., 4.))
                .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
            let go_on = ActionButton::new("sign-in-again-continue", "Continue")
                .primary()
                .margin(beside(4., 0.))
                .on_click(cx.listener(|this, _, _, cx| this.go_on(cx)));
            div().flex().justify_end().child(close).child(go_on)
        } else {
            let cancel = ActionButton::new("sign-in-again-cancel", "Cancel")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
            div().flex().justify_end().child(cancel)
        };
        let steps = self.steps(account.as_ref(), cx);
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(TextSize::Lg)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Sign In to {agent}")),
                    )
                    .child(div().text_size(TextSize::Sm).text_color(c.muted_foreground).child(message)),
            )
            .child(
                div().flex().flex_col().gap(px(6.)).child(steps).child(
                    div()
                        .px(px(4.))
                        .text_size(TextSize::Xs)
                        .text_color(if error.is_some() { c.destructive } else { c.muted_stronger_foreground })
                        .child(error.unwrap_or(caption)),
                ),
            )
            .child(footer)
    }
}
