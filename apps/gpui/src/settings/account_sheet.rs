//! An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
//! variables its agent is given, with the way to sign it in.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Agent, AgentAccount, Variable};

use super::page::SettingsPage;
use crate::models::{AgentAccountExt, Server, agent_name};
use crate::store::Store;
use crate::theme::{ControlSize, colors};
use crate::ui::modal::modal;
use crate::ui::{ActionButton, CommandBox, InputField, Segmented, Switch};

/// A variable as the sheet edits it.
struct EditedVariable {
    id: usize,
    name: Entity<InputState>,
    value: Entity<InputState>,
    sensitive: bool,
    /// Its value field says the server keeps the value.
    shows_kept: bool,
}

/// Where signing a Claude Code account in is: its server runs the CLI's sign-in, the user signs
/// in on the page it links to, and pastes the code that page shows.
#[derive(Clone, PartialEq, Default)]
enum SignInStep {
    #[default]
    Idle,
    Starting,
    Waiting {
        id: String,
        url: String,
    },
    SignedIn(String),
}

#[derive(Default)]
struct SignIn {
    step: SignInStep,
    finishing: bool,
    error: Option<String>,
    /// Who signed in before this sign-in, who still is if it fails.
    before: SignInStep,
}

pub struct AccountSheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    server: Server,
    account: AgentAccount,
    name: Entity<InputState>,
    folder: Entity<InputState>,
    code: Entity<InputState>,
    sign_in: SignIn,
    /// The account as signing in saved it, until something else changes.
    saved_by_sign_in: Option<AgentAccount>,
    /// The code's field has just appeared, empty, and takes the keys.
    focus_code: bool,
    variables: Vec<EditedVariable>,
    next_variable: usize,
    /// The folder last filled in from the name, which a typed one takes the place of.
    suggested_folder: String,
    saving: bool,
    /// It has closed and fades out.
    pub leaving: bool,
    _subscriptions: Vec<Subscription>,
}

impl AccountSheet {
    pub fn new(
        store: Entity<Store>,
        server: Server,
        account: AgentAccount,
        page: WeakEntity<SettingsPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Personal").default_value(account.name.clone()));
        let folder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(folder_placeholder(account.agent))
                .default_value(account.folder.clone())
        });
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Paste the code"));
        let subscriptions = vec![
            cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.suggest_folder(window, cx);
                }
            }),
            cx.subscribe(&folder, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.cancel_sign_in(cx);
                }
                cx.notify();
            }),
            cx.subscribe(&code, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.on_release(|this, cx| this.cancel_sign_in(cx)),
        ];
        name.update(cx, |name, cx| name.focus(window, cx));
        let mut sheet = Self {
            store,
            page,
            server,
            account: account.clone(),
            name,
            folder,
            code,
            sign_in: SignIn::default(),
            saved_by_sign_in: None,
            focus_code: false,
            variables: Vec::new(),
            next_variable: 0,
            suggested_folder: String::new(),
            saving: false,
            leaving: false,
            _subscriptions: subscriptions,
        };
        for variable in &account.variables {
            sheet.add_variable(variable.clone(), window, cx);
        }
        sheet
    }

    fn is_new(&self) -> bool {
        self.account.id.is_empty()
    }

    fn installed(&self) -> Vec<Agent> {
        [Agent::Claude, Agent::Codex].into_iter().filter(|agent| self.server.agents.contains_key(agent)).collect()
    }

    /// The account as the fields have it now.
    fn edited(&self, cx: &App) -> AgentAccount {
        AgentAccount {
            name: self.name.read(cx).value().to_string(),
            folder: self.folder.read(cx).value().to_string(),
            ..self.account.clone()
        }
    }

    /// Fills a new account's folder from its name until the folder is typed by hand.
    fn suggest_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.folder.read(cx).value().to_string();
        let typed = !(folder.is_empty() || folder == self.suggested_folder);
        if !self.is_new() || self.sign_in.step != SignInStep::Idle || typed {
            return cx.notify();
        }
        self.suggested_folder = self.edited(cx).suggested_folder(&self.server.agent_accounts);
        let suggested = self.suggested_folder.clone();
        self.folder.update(cx, |folder, cx| folder.set_value(suggested, window, cx));
        cx.notify();
    }

    fn choose_agent(&mut self, agent: Agent, window: &mut Window, cx: &mut Context<Self>) {
        self.account.agent = agent;
        self.cancel_sign_in(cx);
        self.folder.update(cx, |folder, cx| folder.set_placeholder(folder_placeholder(agent), window, cx));
        self.suggest_folder(window, cx);
    }

    fn add_variable(&mut self, variable: Variable, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("NAME").default_value(variable.name.clone()));
        let value = cx.new(|cx| InputState::new(window, cx).placeholder("value").default_value(variable.value.clone()));
        self._subscriptions.push(cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()));
        let edited =
            EditedVariable { id: self.next_variable, name, value, sensitive: variable.sensitive, shows_kept: false };
        self.variables.push(edited);
        self.next_variable += 1;
        cx.notify();
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| page.close_account_sheet(cx));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.name.read(cx).value().trim().is_empty() {
            return;
        }
        let saved = self.form(cx);
        self.saving = true;
        cx.notify();
        let (sheet, server) = (cx.weak_entity(), self.server.clone());
        self.store.update(cx, |store, _| {
            store.save_agent_account(saved, &server, move |kept, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    sheet.saving = false;
                    cx.notify();
                    if kept {
                        sheet.close(cx);
                    }
                });
            })
        });
    }

    /// The account as Save, and signing in, keep it.
    fn form(&self, cx: &App) -> AgentAccount {
        let mut form = self.edited(cx);
        form.variables = self
            .variables
            .iter()
            .map(|edited| Variable {
                name: edited.name.read(cx).value().to_string(),
                value: edited.value.read(cx).value().to_string(),
                sensitive: edited.sensitive,
            })
            .filter(|variable| !variable.name.trim().is_empty())
            .collect();
        if form.agent != Agent::Codex {
            form.shares_sessions = false;
        }
        form
    }

    /// Signing in saved everything the form has, so there is nothing left to save or to throw away.
    fn saved_as_is(&self, cx: &App) -> bool {
        self.saved_by_sign_in.as_ref().is_some_and(|saved| *saved == self.form(cx))
    }

    /// Return finishes the sign-in from the code's field, and saves, or closes what signing in
    /// saved, from any other.
    fn press_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.code.read(cx).focus_handle(cx).is_focused(window) {
            return self.finish_sign_in(cx);
        }
        if self.saved_as_is(cx) {
            return self.close(cx);
        }
        self.save(cx);
    }

    /// Claude Code signs in here; Codex, and a server too old for it, with a command.
    fn signs_in_here(&self, account: &AgentAccount) -> bool {
        account.agent == Agent::Claude && self.server.signs_in_accounts()
    }

    fn can_finish_sign_in(&self, cx: &App) -> bool {
        !self.sign_in.finishing
            && !self.code.read(cx).value().trim().is_empty()
            && !self.name.read(cx).value().trim().is_empty()
    }

    /// Who is signed in to the folder the form has, as far as is known.
    fn signed_in_as(&self, account: &AgentAccount) -> Option<String> {
        if let SignInStep::SignedIn(who) = &self.sign_in.step {
            return Some(who.clone());
        }
        let opened = &self.account;
        let email = opened.email.as_ref().filter(|_| account.folder == opened.folder)?;
        Some(match &opened.plan {
            Some(plan) => format!("{email} · {plan}"),
            None => email.clone(),
        })
    }

    fn sign_in_caption(&self, cx: &App) -> String {
        if matches!(self.sign_in.step, SignInStep::Waiting { .. }) {
            return "Sign in on the page, then paste the code it shows.".into();
        }
        if self.saved_as_is(cx) {
            return "Signing in saved the account.".into();
        }
        format!("Opens Claude's sign-in page. Signing in saves the account on {}.", self.server.name)
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.sign_in.step == SignInStep::Starting {
            return;
        }
        let signed_in = self.sign_in.step.clone();
        self.cancel_sign_in(cx);
        if matches!(signed_in, SignInStep::SignedIn(_)) {
            self.sign_in.before = signed_in;
        }
        self.sign_in.step = SignInStep::Starting;
        let account = self.form(cx);
        let (sheet, server) = (cx.weak_entity(), self.server.clone());
        self.store.update(cx, |store, _| {
            store.start_agent_sign_in(account, &server, move |started, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    if sheet.sign_in.step != SignInStep::Starting {
                        return;
                    }
                    match started {
                        Ok((id, url)) => {
                            sheet.sign_in.step = SignInStep::Waiting { id, url };
                            sheet.focus_code = true;
                        }
                        Err(message) => sheet.fail_sign_in(message),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    fn finish_sign_in(&mut self, cx: &mut Context<Self>) {
        let SignInStep::Waiting { id, .. } = &self.sign_in.step else { return };
        if !self.can_finish_sign_in(cx) {
            return;
        }
        let (id, code) = (id.clone(), self.code.read(cx).value().trim().to_string());
        let account = self.form(cx);
        self.sign_in.finishing = true;
        self.sign_in.error = None;
        let (sheet, server_id, waiting) = (cx.weak_entity(), self.server.id.clone(), id.clone());
        self.store.update(cx, |store, _| {
            store.finish_agent_sign_in(&server_id, id, code, account, move |signed_in, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    if !matches!(&sheet.sign_in.step, SignInStep::Waiting { id, .. } if *id == waiting) {
                        return;
                    }
                    sheet.sign_in.finishing = false;
                    match signed_in {
                        Ok((saved, who)) => {
                            sheet.account.id = saved;
                            sheet.saved_by_sign_in = Some(sheet.form(cx));
                            sheet.sign_in.step = SignInStep::SignedIn(who);
                        }
                        Err(message) => sheet.refuse_sign_in(message),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    /// A sign-in that is over says so; one refused for the account it would save waits for the
    /// code again.
    fn refuse_sign_in(&mut self, message: String) {
        if !message.ends_with("Sign in again.") {
            self.sign_in.error = Some(message);
            return;
        }
        self.fail_sign_in(message);
    }

    /// Stops a sign-in that waits for its code, as closing the sheet or changing the folder does.
    fn cancel_sign_in(&mut self, cx: &mut App) {
        if let SignInStep::Waiting { id, .. } = &self.sign_in.step {
            let (server_id, id) = (self.server.id.clone(), id.clone());
            self.store.update(cx, |store, _| store.cancel_agent_sign_in(&server_id, id));
        }
        self.sign_in = SignIn::default();
    }

    fn fail_sign_in(&mut self, message: String) {
        self.sign_in.step = self.sign_in.before.clone();
        self.sign_in.error = Some(message);
    }

    fn sign_in_steps(&self, account: &AgentAccount, cx: &mut Context<Self>) -> AnyElement {
        if let SignInStep::Waiting { url, .. } = &self.sign_in.step {
            let code = div().flex_1().min_w_0().child(InputField::new(&self.code).monospaced(true));
            let finish = ActionButton::new("finish-sign-in", "Sign In")
                .primary()
                .pending(self.sign_in.finishing)
                .disabled(!self.can_finish_sign_in(cx))
                .on_click(cx.listener(|this, _, _, cx| this.finish_sign_in(cx)));
            return div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(CommandBox::new("sign-in-link", Some(url.clone())).link(url.clone()))
                .child(div().flex().items_center().gap(px(6.)).child(code).child(finish))
                .into_any_element();
        }
        let c = colors(cx);
        let who = self.signed_in_as(account);
        let start = ActionButton::new("start-sign-in", if who.is_none() { "Sign In" } else { "Sign In Again" })
            .pending(self.sign_in.step == SignInStep::Starting)
            .on_click(cx.listener(|this, _, _, cx| this.start_sign_in(cx)));
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .min_w_0()
                    .px(px(4.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if who.is_none() { c.muted_foreground } else { c.foreground })
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .child(who.map_or("Not signed in".to_string(), |who| format!("Signed in as {who}"))),
            )
            .child(div().flex_1().min_w(px(12.)))
            .child(start)
            .into_any_element()
    }

    /// Whether the server keeps the variable's value, which it never sends.
    fn kept(&self, edited: &EditedVariable, cx: &App) -> bool {
        let name = edited.name.read(cx).value();
        edited.sensitive
            && self.account.variables.iter().any(|variable| variable.sensitive && variable.name == name.as_ref())
    }

    /// Says in each value's field whether the server keeps the value.
    fn show_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kept: Vec<bool> = self.variables.iter().map(|edited| self.kept(edited, cx)).collect();
        for (edited, kept) in self.variables.iter_mut().zip(kept) {
            if edited.shows_kept == kept {
                continue;
            }
            edited.shows_kept = kept;
            let placeholder = if kept { "••••••••" } else { "value" };
            edited.value.update(cx, |value, cx| value.set_placeholder(placeholder, window, cx));
        }
    }

    fn variable_row(&self, index: usize, edited: &EditedVariable, cx: &mut Context<Self>) -> AnyElement {
        let server = &self.server.name;
        let (sensitive, id) = (edited.sensitive, edited.id);
        let help = if sensitive {
            format!("Sensitive: hidden and kept on {server}")
        } else {
            "Shown: mark it sensitive".to_string()
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .w(px(150.))
                    .flex_shrink_0()
                    .child(InputField::new(&edited.name).size(ControlSize::Small).monospaced(true)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(InputField::new(&edited.value).size(ControlSize::Small).monospaced(true).secure(sensitive)),
            )
            .child(
                ActionButton::icon(("sensitive", index), if sensitive { "eye-off" } else { "eye" }, help)
                    .small()
                    .selected(sensitive)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(edited) = this.variables.iter_mut().find(|edited| edited.id == id) {
                            edited.sensitive = !edited.sensitive;
                        }
                        cx.notify();
                    })),
            )
            .child(ActionButton::icon(("remove-variable", index), "x", "Remove the variable").small().on_click(
                cx.listener(move |this, _, _, cx| {
                    this.variables.retain(|edited| edited.id != id);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }
}

fn folder_placeholder(agent: Agent) -> &'static str {
    if agent == Agent::Claude { "~/.claude" } else { "~/.codex" }
}

/// A field's title over it, and its caption under it.
fn field(title: &str, caption: Option<String>, content: impl IntoElement, cx: &App) -> Div {
    failing_field(title, caption, None, content, cx)
}

/// A field whose failure, while it has one, takes the place of its caption.
fn failing_field(
    title: &str,
    caption: Option<String>,
    failure: Option<String>,
    content: impl IntoElement,
    cx: &App,
) -> Div {
    let c = colors(cx);
    let tint = if failure.is_some() { c.destructive } else { c.muted_stronger_foreground };
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(div().px(px(4.)).text_size(px(12.)).font_weight(FontWeight::MEDIUM).child(title.to_string()))
        .child(content)
        .when_some(failure.or(caption), |field, caption| {
            field.child(div().px(px(4.)).text_size(px(11.5)).text_color(tint).child(caption))
        })
}

impl Render for AccountSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.show_kept(window, cx);
        if std::mem::take(&mut self.focus_code) {
            self.code.update(cx, |code, cx| {
                code.set_value("", window, cx);
                code.focus(window, cx);
            });
        }
        let c = colors(cx);
        let account = self.edited(cx);
        let server = self.server.name.clone();
        let installed = self.installed();
        let title = if self.is_new() {
            format!("New Account on {server}")
        } else {
            format!("{} Account on {server}", agent_name(account.agent))
        };
        let can_save = !account.name.trim().is_empty();
        let picker = (self.is_new() && installed.len() > 1).then(|| {
            let chosen = installed.iter().position(|agent| *agent == account.agent).unwrap_or(0);
            let agents = installed.clone();
            let sheet = cx.weak_entity();
            Segmented::new("account-agent", installed.iter().map(|agent| agent_name(*agent).into()).collect(), chosen)
                .on_select(move |index, window, cx| {
                    let agent = agents[index];
                    let _ = sheet.update(cx, |sheet, cx| sheet.choose_agent(agent, window, cx));
                })
        });
        let caption = format!(
            "Where its sign-in is kept, as {}. Empty for {}.",
            account.folder_variable(),
            account.usual_folder()
        );
        let folder = field("Folder", Some(caption), InputField::new(&self.folder).monospaced(true), cx);
        let shares = (account.agent == Agent::Codex).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("Share sessions with {}", account.usual_folder())),
                        )
                        .child(div().text_size(px(11.5)).text_color(c.muted_foreground).child(
                            "The folder keeps only the sign-in. Threads move between the two and go on where they were.",
                        )),
                )
                .child(Switch::new("shares-sessions", account.shares_sessions).on_change({
                    let sheet = cx.weak_entity();
                    move |on, _, cx| {
                        let _ = sheet.update(cx, |sheet, cx| {
                            sheet.account.shares_sessions = on;
                            cx.notify();
                        });
                    }
                }))
        });
        let rows: Vec<AnyElement> =
            self.variables.iter().enumerate().map(|(index, edited)| self.variable_row(index, edited, cx)).collect();
        let variables = div().flex().flex_col().items_start().gap(px(6.)).children(rows).child(
            ActionButton::new("add-variable", "Add a Variable").symbol("plus").small().on_click(cx.listener(
                |this, _, window, cx| {
                    let variable = Variable { name: String::new(), value: String::new(), sensitive: true };
                    this.add_variable(variable, window, cx);
                },
            )),
        );
        let variables_caption = format!(
            "Given to its agent: an API key or a router. A sensitive value stays on {server} and is never shown again."
        );
        let sign_in = if self.signs_in_here(&account) {
            let caption = self.sign_in_caption(cx);
            let steps = self.sign_in_steps(&account, cx);
            failing_field("Sign In", Some(caption), self.sign_in.error.clone(), steps, cx)
        } else {
            let caption =
                format!("Run it in a terminal on {server}. Who is signed in shows in the list once the agent says.");
            field("Sign In", Some(caption), CommandBox::new("sign-in", Some(account.sign_in_command())), cx)
        };
        let footer = if self.saved_as_is(cx) {
            let done = ActionButton::new("account-done", "Done")
                .primary()
                .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
            div().flex().justify_end().child(done)
        } else {
            let cancel =
                ActionButton::new("account-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| this.close(cx)));
            let save = ActionButton::new("account-save", "Save")
                .primary()
                .pending(self.saving)
                .disabled(!can_save)
                .on_click(cx.listener(|this, _, _, cx| this.save(cx)));
            div().flex().justify_end().gap(px(8.)).child(cancel).child(save)
        };
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .children(picker.map(|picker| div().flex().child(picker)))
            .child(field("Name", None, InputField::new(&self.name), cx))
            .child(folder)
            .children(shares)
            .child(field("Variables", Some(variables_caption), variables.w_full(), cx))
            .child(sign_in)
            .child(footer);
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("agent-account", div().id("agent-account-scroll").overflow_y_scroll().child(content), move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |window, cx| {
            save.update(cx, |this, cx| this.press_return(window, cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}
