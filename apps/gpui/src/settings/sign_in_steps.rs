//! Signing an agent's account in, as the Mac app's `AccountSignIn` and `SignInSteps` do: its server
//! runs the CLI's sign-in and the user signs in on the page it links to. Claude Code's page shows
//! a code to paste here; Codex's takes the code shown here, and the sign-in finishes by itself once
//! it was entered. Signing in saves the account too.

use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Agent, AgentAccount};

use crate::store::Store;
use crate::store::settings::SignInLink;
use crate::theme::{IconSize, MONO_FONT, TextSize, colors};
use crate::ui::{ActionButton, CommandBox, CopyButton, InputField, Spinner};

#[derive(Clone, PartialEq, Default)]
pub enum SignInStep {
    #[default]
    Idle,
    Starting,
    /// `device_code` is the code to enter on the page, for Codex.
    Waiting {
        id: String,
        url: String,
        device_code: Option<String>,
    },
    SignedIn(String),
}

/// What the server says while Codex waits for its code to be entered, which is asked again.
const STILL_WAITING: &str = "Codex is still waiting";

type Saved = Rc<dyn Fn(String, &mut App)>;
type Run = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct AccountSignIn {
    store: Entity<Store>,
    server_id: String,
    pub step: SignInStep,
    pub finishing: bool,
    pub error: Option<String>,
    pub code: Entity<InputState>,
    /// Who signed in before this sign-in, who still is if it fails.
    before: SignInStep,
    /// The code's field has just appeared, empty, and takes the keys.
    focus_code: bool,
    _subscriptions: Vec<Subscription>,
}

impl AccountSignIn {
    pub fn new(store: Entity<Store>, server_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Paste the code"));
        let subscriptions = vec![
            cx.subscribe(&code, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.on_release(|this, cx| this.cancel(cx)),
        ];
        Self {
            store,
            server_id,
            step: SignInStep::Idle,
            finishing: false,
            error: None,
            code,
            before: SignInStep::Idle,
            focus_code: false,
            _subscriptions: subscriptions,
        }
    }

    /// What to do on the page while the sign-in waits.
    pub fn waiting_caption(&self) -> Option<&'static str> {
        match &self.step {
            SignInStep::Waiting { device_code: None, .. } => Some("Sign in on the page, then paste the code it shows."),
            SignInStep::Waiting { device_code: Some(_), .. } => {
                Some("Sign in on the page and enter the code shown here.")
            }
            _ => None,
        }
    }

    /// Whose page a sign-in of the agent opens.
    pub fn page(agent: Agent) -> &'static str {
        if agent == Agent::Claude { "Claude's sign-in page" } else { "OpenAI's sign-in page" }
    }

    pub fn signed_in_as(&self) -> Option<String> {
        let SignInStep::SignedIn(who) = &self.step else { return None };
        Some(who.clone())
    }

    pub fn can_finish(&self, cx: &App) -> bool {
        if self.finishing {
            return false;
        }
        if matches!(self.step, SignInStep::Waiting { device_code: Some(_), .. }) {
            return true;
        }
        !self.code.read(cx).value().trim().is_empty()
    }

    pub fn code_focused(&self, window: &Window, cx: &App) -> bool {
        self.code.read(cx).focus_handle(cx).is_focused(window)
    }

    /// Starts the sign-in; one that finishes by itself saves `account` and hands `saved` its id.
    pub fn start(&mut self, account: AgentAccount, saved: impl Fn(String, &mut App) + 'static, cx: &mut Context<Self>) {
        if self.step == SignInStep::Starting {
            return;
        }
        let signed_in = self.step.clone();
        self.cancel(cx);
        if matches!(signed_in, SignInStep::SignedIn(_)) {
            self.before = signed_in;
        }
        self.step = SignInStep::Starting;
        let saved: Saved = Rc::new(saved);
        let (this, server_id) = (cx.weak_entity(), self.server_id.clone());
        self.store.update(cx, |store, _| {
            store.start_agent_sign_in(account.clone(), &server_id, move |started, cx| {
                let _ = this.update(cx, |sign_in, cx| {
                    if sign_in.step != SignInStep::Starting {
                        return;
                    }
                    match started {
                        Ok(SignInLink { id, url, device_code }) => {
                            let finishes = device_code.is_some();
                            sign_in.step = SignInStep::Waiting { id, url, device_code };
                            sign_in.focus_code = !finishes;
                            if finishes {
                                sign_in.finish_later(account, saved, cx);
                            }
                        }
                        Err(message) => sign_in.fail(message),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    /// Hands the code to the sign-in, or waits for Codex's to be entered, and saves `account` with
    /// it; `saved` gets the saved account's id.
    pub fn finish(
        &mut self,
        account: AgentAccount,
        saved: impl Fn(String, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.finish_with(account, Rc::new(saved), cx);
    }

    fn finish_with(&mut self, account: AgentAccount, saved: Saved, cx: &mut Context<Self>) {
        let SignInStep::Waiting { id, device_code, .. } = &self.step else { return };
        if !self.can_finish(cx) {
            return;
        }
        let (id, waits) = (id.clone(), device_code.is_some());
        let code = self.code.read(cx).value().trim().to_string();
        self.finishing = true;
        self.error = None;
        let (this, server_id, waiting) = (cx.weak_entity(), self.server_id.clone(), id.clone());
        self.store.update(cx, |store, _| {
            store.finish_agent_sign_in(&server_id, id, code, account.clone(), move |signed_in, cx| {
                let _ = this.update(cx, |sign_in, cx| {
                    if !matches!(&sign_in.step, SignInStep::Waiting { id, .. } if *id == waiting) {
                        return;
                    }
                    sign_in.finishing = false;
                    match signed_in {
                        Err(message) if waits && message.starts_with(STILL_WAITING) => {
                            sign_in.finish_later(account, saved, cx)
                        }
                        Ok((id, who)) => {
                            saved(id, cx);
                            sign_in.step = SignInStep::SignedIn(who);
                        }
                        Err(message) => sign_in.refuse(message),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    /// Asks again once the store's answer is over, as Codex's sign-in waits for its code.
    fn finish_later(&self, account: AgentAccount, saved: Saved, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, |sign_in, cx| sign_in.finish_with(account, saved, cx));
        });
    }

    /// A sign-in that is over says so; one refused for the account it would save waits for the
    /// code again.
    fn refuse(&mut self, message: String) {
        if !message.ends_with("Sign in again.") {
            self.error = Some(message);
            return;
        }
        self.fail(message);
    }

    /// Stops a sign-in that waits for its code, as closing the sheet or changing the folder does.
    pub fn cancel(&mut self, cx: &mut App) {
        if let SignInStep::Waiting { id, .. } = &self.step {
            let (server_id, id) = (self.server_id.clone(), id.clone());
            self.store.update(cx, |store, _| store.cancel_agent_sign_in(&server_id, id));
        }
        self.step = SignInStep::Idle;
        self.before = SignInStep::Idle;
        self.finishing = false;
        self.error = None;
    }

    fn fail(&mut self, message: String) {
        self.step = self.before.clone();
        self.error = Some(message);
    }
}

/// The page to sign in on and the code to paste or to enter there, or who is signed in with the
/// way to sign in again.
#[derive(IntoElement)]
pub struct SignInSteps {
    sign_in: Entity<AccountSignIn>,
    signed_in_as: Option<String>,
    /// Whatever else the code waits for besides being typed.
    can_finish: bool,
    /// Once signed in, it offers to sign in again.
    signs_in_again: bool,
    start: Run,
    finish: Run,
}

impl SignInSteps {
    pub fn new(
        sign_in: &Entity<AccountSignIn>,
        signed_in_as: Option<String>,
        start: impl Fn(&mut Window, &mut App) + 'static,
        finish: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sign_in: sign_in.clone(),
            signed_in_as,
            can_finish: true,
            signs_in_again: true,
            start: Rc::new(start),
            finish: Rc::new(finish),
        }
    }

    pub fn can_finish(mut self, can_finish: bool) -> Self {
        self.can_finish = can_finish;
        self
    }

    pub fn signs_in_again(mut self, signs_in_again: bool) -> Self {
        self.signs_in_again = signs_in_again;
        self
    }
}

impl RenderOnce for SignInSteps {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = self.sign_in.update(cx, |sign_in, _| std::mem::take(&mut sign_in.focus_code));
        let code = self.sign_in.read(cx).code.clone();
        if focus {
            code.update(cx, |code, cx| {
                code.set_value("", window, cx);
                code.focus(window, cx);
            });
        }
        let c = colors(cx);
        let sign_in = self.sign_in.read(cx);
        if let SignInStep::Waiting { url, device_code, .. } = &sign_in.step {
            let link = CommandBox::new("sign-in-link", Some(url.clone())).link(url.clone());
            let steps = div().flex().flex_col().gap(px(6.)).child(link);
            if let Some(device_code) = device_code {
                return steps
                    .child(div().py(px(6.)).child(DeviceCode(device_code.clone().into())))
                    .child(div().px(px(4.)).child(WaitingForCode))
                    .into_any_element();
            }
            let finish = self.finish.clone();
            let button = ActionButton::new("finish-sign-in", "Sign In")
                .primary()
                .pending(sign_in.finishing)
                .disabled(!sign_in.can_finish(cx) || !self.can_finish)
                .on_click(move |_, window, cx| finish(window, cx));
            let field = div().flex_1().min_w_0().child(InputField::new(&code).monospaced(true));
            return steps.child(div().flex().items_center().gap(px(6.)).child(field).child(button)).into_any_element();
        }
        let who = self.signed_in_as;
        let start = self.start.clone();
        let button = (who.is_none() || self.signs_in_again).then(|| {
            ActionButton::new("start-sign-in", if who.is_none() { "Sign In" } else { "Sign In Again" })
                .pending(sign_in.step == SignInStep::Starting)
                .disabled(!self.can_finish)
                .on_click(move |_, window, cx| start(window, cx))
        });
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .min_w_0()
                    .px(px(4.))
                    .text_size(TextSize::Sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if who.is_none() { c.muted_foreground } else { c.foreground })
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .child(who.map_or("Not signed in".to_string(), |who| format!("Signed in as {who}"))),
            )
            .child(div().flex_1().min_w(px(12.)))
            .children(button)
            .into_any_element()
    }
}

/// The code Codex's sign-in shows, to enter on its page, with the way to copy it.
#[derive(IntoElement)]
struct DeviceCode(SharedString);

impl RenderOnce for DeviceCode {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let copied = self.0.clone();
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(
                div()
                    .text_size(TextSize::Xxl)
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_family(MONO_FONT)
                    .text_color(colors(cx).foreground)
                    .child(self.0),
            )
            .child(CopyButton::new("copy-device-code", "Copy the code", move |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
            }))
    }
}

/// What a sign-in that finishes by itself shows while it waits.
#[derive(IntoElement)]
struct WaitingForCode;

impl RenderOnce for WaitingForCode {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(TextSize::Sm)
            .text_color(colors(cx).muted_foreground)
            .child(Spinner::new(IconSize::Md).render(cx))
            .child("Waiting for you to enter the code")
    }
}
