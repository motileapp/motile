//! An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
//! variables its agent is given, with the command that signs it in.

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

pub struct AccountSheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    server: Server,
    account: AgentAccount,
    name: Entity<InputState>,
    folder: Entity<InputState>,
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
        let subscriptions = vec![
            cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.suggest_folder(window, cx);
                }
            }),
            cx.subscribe(&folder, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        name.update(cx, |name, cx| name.focus(window, cx));
        let mut sheet = Self {
            store,
            page,
            server,
            account: account.clone(),
            name,
            folder,
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
        if !self.is_new() || !(folder.is_empty() || folder == self.suggested_folder) {
            return cx.notify();
        }
        self.suggested_folder = self.edited(cx).suggested_folder(&self.server.agent_accounts);
        let suggested = self.suggested_folder.clone();
        self.folder.update(cx, |folder, cx| folder.set_value(suggested, window, cx));
        cx.notify();
    }

    fn choose_agent(&mut self, agent: Agent, window: &mut Window, cx: &mut Context<Self>) {
        self.account.agent = agent;
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
        let mut saved = self.edited(cx);
        saved.variables = self
            .variables
            .iter()
            .map(|edited| Variable {
                name: edited.name.read(cx).value().to_string(),
                value: edited.value.read(cx).value().to_string(),
                sensitive: edited.sensitive,
            })
            .filter(|variable| !variable.name.trim().is_empty())
            .collect();
        if saved.agent != Agent::Codex {
            saved.shares_sessions = false;
        }
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
    if agent == Agent::Claude { "~/.claude-personal" } else { "~/.codex-personal" }
}

/// A field's title over it, and its caption under it.
fn field(title: &str, caption: Option<String>, content: impl IntoElement, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(div().px(px(4.)).text_size(px(12.)).font_weight(FontWeight::MEDIUM).child(title.to_string()))
        .child(content)
        .when_some(caption, |field, caption| {
            field.child(div().px(px(4.)).text_size(px(11.5)).text_color(c.muted_stronger_foreground).child(caption))
        })
}

impl Render for AccountSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.show_kept(window, cx);
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
        let folder = (!account.is_default()).then(|| {
            let caption = format!("Where its sign-in is kept, as {}", account.folder_variable());
            field("Folder", Some(caption), InputField::new(&self.folder).monospaced(true), cx)
        });
        let shares = (!account.is_default() && account.agent == Agent::Codex).then(|| {
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
                                .child("Share sessions with the default account"),
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
        let sign_in_caption =
            format!("Run it in a terminal on {server}. Who is signed in shows in the list once the agent says.");
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .children(picker.map(|picker| div().flex().child(picker)))
            .child(field("Name", None, InputField::new(&self.name), cx))
            .children(folder)
            .children(shares)
            .child(field("Variables", Some(variables_caption), variables.w_full(), cx))
            .child(field(
                "Sign In",
                Some(sign_in_caption),
                CommandBox::new("sign-in", Some(account.sign_in_command())),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ActionButton::new("account-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        ActionButton::new("account-save", "Save")
                            .primary()
                            .pending(self.saving)
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("agent-account", div().id("agent-account-scroll").overflow_y_scroll().child(content), move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}
