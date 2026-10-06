//! The Linear tabs: a workspace's issues under their statuses with what chooses them over the
//! list, how to connect a server that isn't, and one issue with its description and comments,
//! as the Mac app's `LinearView.swift` and `LinearList.swift` draw them.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::linear::{Group, Page, Row};
use motile_protocol::wire::{LinearChange, LinearState, LinearStateKind, LinearTeam, LinearUser, NewLinearIssue};

use super::state::{Loaded, PanelTarget};
use crate::models::ago;
use crate::store::Store;
use crate::store::linear::{KINDS, LinearChoice, PRIORITIES, priority_name, priority_symbol, state_symbol};
use crate::theme::{ControlSize, Radius, Surface, colors};
use crate::ui::menu::Menu;
use crate::ui::rich_text::markdown;
use crate::ui::sheet::sheet;
use crate::ui::{ActionButton, ActionMenu, Chip, InputField, Spinner, Variant, divider, icons, tooltip};

/// Under this the bar is two rows.
const WRAPS: f32 = 520.;

type Change = Rc<dyn Fn(String, LinearChange, &mut App)>;

/// One Linear tab: the workspace's issues, or with `issue` one issue of `workspace`.
pub struct LinearView {
    store: Entity<Store>,
    workspace: String,
    issue: Option<String>,
    search: Entity<InputState>,
    comment: Entity<TextareaState>,
    /// Counts up when the user asks for the issues again.
    asked: u64,
    /// The statuses folded up, by id.
    collapsed: HashSet<String>,
    /// How wide the tab was last drawn.
    width: Rc<Cell<f32>>,
    filing: Option<Entity<NewIssue>>,
    filing_watch: Option<Subscription>,
    /// What each load last asked its server with, by what loads.
    triggers: HashMap<&'static str, String>,
    debounce: Option<Task<()>>,
    /// The comment was said; its field empties on the next draw.
    clear_comment: bool,
    _subscriptions: Vec<Subscription>,
}

impl LinearView {
    pub fn new(
        store: Entity<Store>,
        workspace: String,
        issue: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let comment = cx.new(|cx| TextareaState::new(window, cx).auto_grow(4, 10).placeholder("Leave a comment"));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&comment, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            store,
            workspace,
            issue,
            search,
            comment,
            asked: 0,
            collapsed: HashSet::new(),
            width: Rc::new(Cell::new(f32::MAX)),
            filing: None,
            filing_watch: None,
            triggers: HashMap::new(),
            debounce: None,
            clear_comment: false,
            _subscriptions: subscriptions,
        }
    }

    /// Asks the server for what the tab shows when what it shows may have changed. With a
    /// `delay` it waits for the typing to stop first.
    fn follow(
        &mut self,
        key: &'static str,
        trigger: String,
        delay: Option<Duration>,
        load: impl FnOnce(&mut Store) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.triggers.get(key) == Some(&trigger) {
            return;
        }
        self.triggers.insert(key, trigger);
        let store = self.store.clone();
        let run = move |cx: &mut App| {
            store.update(cx, |store, cx| {
                load(store);
                cx.notify();
            });
        };
        let Some(delay) = delay else {
            cx.defer(run);
            return;
        };
        self.debounce = Some(cx.spawn(async move |_, cx| {
            cx.background_executor().timer(delay).await;
            cx.update(run);
        }));
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asked += 1;
        cx.notify();
    }

    /// Keeps the tab's width, and draws again when it crosses what wraps the bar.
    fn measure(&self, cx: &Context<Self>) -> impl IntoElement {
        let width = self.width.clone();
        let this = cx.entity().downgrade();
        canvas(
            move |bounds, _, cx| {
                let now = f32::from(bounds.size.width);
                let was = width.replace(now);
                if (was < WRAPS) == (now < WRAPS) {
                    return;
                }
                let this = this.clone();
                cx.defer(move |cx| {
                    let _ = this.update(cx, |_, cx| cx.notify());
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    // The list

    fn issues_surface(&mut self, target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let choice = self.store.read(cx).linear_choice(target);
        let search = self.search.read(cx).value().to_string();
        let narrow = self.width.get() < WRAPS;
        let trigger = format!("{target:?}|{choice:?}|{search}|{}", self.asked);
        let delay = (!search.is_empty()).then(|| Duration::from_millis(300));
        let (load_target, load_search) = (target.clone(), search.clone());
        self.follow("list", trigger, delay, move |store| store.linear_load(&load_target, &load_search), cx);

        let error = self.store.read(cx).linear.error.clone();
        let pickers = self.pickers(target, &choice, cx);
        let tools = self.tools(target, &choice, narrow, cx);
        let bar = if narrow {
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .child(bar_row(cx).children(pickers).child(div().flex_1()))
                .child(bar_row(cx).children(tools))
                .child(divider(cx))
        } else {
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .child(bar_row(cx).children(pickers).child(div().flex_1().min_w(px(4.))).children(tools))
                .child(divider(cx))
        };
        let content = self.issues_content(target, &choice, &search, narrow, cx);
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .child(self.measure(cx))
            .child(bar)
            .when_some(error, |tab, error| tab.child(note(error, cx)))
            .child(div().flex_1().min_h_0().child(content))
            .children(self.filing.clone())
            .into_any_element()
    }

    fn pickers(&self, target: &PanelTarget, choice: &LinearChoice, cx: &Context<Self>) -> Vec<AnyElement> {
        let store = self.store.read(cx);
        let connected = store.linear.connected(&target.server_id).to_vec();
        let workspace = connected.iter().find(|connection| Some(&connection.id) == choice.workspace.as_ref()).cloned();
        let teams: Vec<LinearTeam> =
            store.linear.teams.get(choice.workspace.as_deref().unwrap_or_default()).cloned().unwrap_or_default();
        let title = workspace.as_ref().map(|workspace| workspace.workspace.clone()).unwrap_or_else(|| "Linear".into());
        let workspaces = ActionMenu::new("linear-workspace", title, {
            let (store, target, chosen) = (self.store.clone(), target.clone(), choice.workspace.clone());
            let workspace = workspace.clone();
            move |_, _| {
                let mut menu = Menu::new();
                for connection in &connected {
                    let (store, target, id) = (store.clone(), target.clone(), connection.id.clone());
                    menu = menu.checked(
                        Some(&connection.id) == chosen.as_ref(),
                        connection.workspace.clone(),
                        move |_, cx| {
                            let id = id.clone();
                            store.update(cx, |store, cx| {
                                store.linear_choose(&target, |choice| {
                                    choice.workspace = Some(id);
                                    choice.team = None;
                                });
                                cx.notify();
                            });
                        },
                    );
                }
                menu = menu.separator().item("Add Workspace", {
                    let (store, server) = (store.clone(), target.server_id.clone());
                    move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.linear_connect(&server);
                            cx.notify();
                        })
                    }
                });
                menu.when_some(workspace.clone(), |menu, workspace| {
                    let (store, server) = (store.clone(), target.server_id.clone());
                    menu.item(format!("Disconnect {}", workspace.workspace), move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.linear_disconnect(&workspace.id, &server);
                            cx.notify();
                        })
                    })
                })
            }
        })
        .button(|button| button.help("Which workspace to show"));
        let team_title = teams
            .iter()
            .find(|team| Some(&team.id) == choice.team.as_ref())
            .map(|team| team.name.clone())
            .unwrap_or_else(|| "All Teams".into());
        let team_menu = ActionMenu::new("linear-team", team_title, {
            let (store, target, chosen) = (self.store.clone(), target.clone(), choice.team.clone());
            move |_, _| {
                let pick = |store: &Entity<Store>, target: &PanelTarget, team: Option<String>| {
                    let (store, target) = (store.clone(), target.clone());
                    move |_: &mut Window, cx: &mut App| {
                        let team = team.clone();
                        store.update(cx, |store, cx| {
                            store.linear_choose(&target, |choice| choice.team = team);
                            cx.notify();
                        });
                    }
                };
                let mut menu = Menu::new().checked(chosen.is_none(), "All Teams", pick(&store, &target, None));
                for team in &teams {
                    let chosen = Some(&team.id) == chosen.as_ref();
                    menu = menu.checked(chosen, team.name.clone(), pick(&store, &target, Some(team.id.clone())));
                }
                menu
            }
        })
        .button(|button| button.help("Which team's issues to show"));
        vec![div().ml(px(-8.)).child(workspaces).into_any_element(), team_menu.into_any_element()]
    }

    fn tools(&self, target: &PanelTarget, choice: &LinearChoice, narrow: bool, cx: &Context<Self>) -> Vec<AnyElement> {
        let store = self.store.read(cx);
        let reading = store.linear.is_working("issues");
        let this = cx.entity().downgrade();
        let search = div()
            .when(narrow, |field| field.flex_1().min_w_0())
            .when(!narrow, |field| field.w(px(180.)))
            .child(InputField::new(&self.search).icon("search").clearable(true));
        let filter = ActionMenu::icon("linear-filter", "list-filter", "Which issues to show", {
            let (store, target, choice) = (self.store.clone(), target.clone(), choice.clone());
            move |_, _| {
                let choose = |change: Box<dyn Fn(&mut LinearChoice)>| {
                    let (store, target) = (store.clone(), target.clone());
                    move |_: &mut Window, cx: &mut App| {
                        store.update(cx, |store, cx| {
                            store.linear_choose(&target, |choice| change(choice));
                            cx.notify();
                        });
                    }
                };
                let mine = choice.mine;
                let mut menu = Menu::new()
                    .checked(mine, "Assigned to Me", choose(Box::new(move |choice| choice.mine = !mine)))
                    .separator();
                for (_, kind, name) in KINDS {
                    let listed = choice.states.iter().any(|state| state == kind);
                    menu = menu.checked(
                        listed,
                        name,
                        choose(Box::new(move |choice| {
                            choice.states.retain(|state| state != kind);
                            if !listed {
                                choice.states.push(kind.to_string());
                            }
                        })),
                    );
                }
                menu
            }
        });
        let refresh =
            ActionButton::icon("linear-refresh", "rotate-cw", "Read the issues again").pending(reading).on_click({
                let this = this.clone();
                move |_, _, cx| {
                    let _ = this.update(cx, |this, cx| this.refresh(cx));
                }
            });
        let file = ActionButton::icon("linear-new", "plus", "New Issue").on_click({
            let (target, workspace, team) = (target.clone(), choice.workspace.clone(), choice.team.clone());
            move |_, window, cx| {
                let Some(workspace) = workspace.clone() else { return };
                let (target, team) = (target.clone(), team.clone());
                let _ = this.update(cx, |this, cx| this.file(target, workspace, team, window, cx));
            }
        });
        vec![search.into_any_element(), filter.into_any_element(), refresh.into_any_element(), file.into_any_element()]
    }

    /// Opens the sheet that files an issue.
    fn file(
        &mut self,
        target: PanelTarget,
        workspace: String,
        team: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let store = self.store.clone();
        let filing = cx.new(|cx| NewIssue::new(store, target, workspace, team, window, cx));
        self.filing_watch = Some(cx.observe(&filing, |this, filing, cx| {
            if filing.read(cx).closed {
                this.filing = None;
                this.filing_watch = None;
                cx.notify();
            }
        }));
        self.filing = Some(filing);
        cx.notify();
    }

    fn issues_content(
        &self,
        target: &PanelTarget,
        choice: &LinearChoice,
        search: &str,
        narrow: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let store = self.store.read(cx);
        let workspace = choice.workspace.clone().unwrap_or_default();
        match store.linear.issues() {
            None | Some(Loaded::Loading) => loading(cx),
            Some(Loaded::Failed(reason)) => message(reason.clone(), true, cx),
            Some(Loaded::Ready(groups)) if groups.is_empty() => {
                let filtered = choice.mine || !choice.states.is_empty();
                let text = if !search.is_empty() {
                    format!("None have “{search}”.")
                } else if filtered {
                    "No issues match the filter.".to_string()
                } else {
                    "No issues.".to_string()
                };
                message(text, false, cx)
            }
            Some(Loaded::Ready(groups)) => self.list(target, &workspace, groups, narrow, cx),
        }
    }

    fn list(
        &self,
        target: &PanelTarget,
        workspace: &str,
        groups: &[Group],
        narrow: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let store = self.store.read(cx);
        let users: Vec<LinearUser> = store.linear.users.get(workspace).cloned().unwrap_or_default();
        let pending = store.linear.working.clone();
        let change: Change = {
            let (store, workspace, server) = (self.store.clone(), workspace.to_string(), target.server_id.clone());
            Rc::new(move |id, change, cx| {
                store.update(cx, |store, cx| {
                    store.linear_change(&id, change, &workspace, &server);
                    cx.notify();
                });
            })
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        for group in groups {
            let folded = self.collapsed.contains(&group.state.id);
            rows.push(self.header(&group.state, group.rows.len(), folded, cx));
            if folded {
                continue;
            }
            for row in &group.rows {
                let states = store.linear.states_of(workspace, &row.team);
                let spinning = pending.contains(&row.id);
                rows.push(self.issue_row(workspace, row, &group.state, states, &users, &change, spinning, narrow, cx));
            }
        }
        div()
            .id("linear-list")
            .size_full()
            .overflow_y_scroll()
            .p(px(8.))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }

    /// A status over its issues, on the secondary background, which a click folds up and out.
    fn header(&self, state: &LinearState, count: usize, folded: bool, cx: &Context<Self>) -> AnyElement {
        let c = colors(cx);
        let this = cx.entity().downgrade();
        let id = state.id.clone();
        div()
            .id(SharedString::from(format!("linear-header-{}", state.id)))
            .mt(px(7.))
            .mb(px(1.))
            .h(px(30.))
            .w_full()
            .flex_shrink_0()
            .rounded(px(Radius::CONTROL))
            .bg(c.background_secondary)
            .hover(|header| header.bg(c.background_tertiary))
            .flex()
            .items_center()
            .px(px(10.))
            .gap(px(8.))
            .child(
                div()
                    .w(px(16.))
                    .flex()
                    .justify_center()
                    .child(icons::symbol(state_symbol(state.kind), 12.).text_color(hex_color(&state.color))),
            )
            .child(
                div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child(state.name.clone()),
            )
            .child(div().text_size(px(12.)).text_color(c.tertiary).child(count.to_string()))
            .child(div().flex_1())
            .child(
                div()
                    .w(px(16.))
                    .flex()
                    .justify_center()
                    .text_color(c.tertiary)
                    .child(icons::symbol(if folded { "chevron-right" } else { "chevron-down" }, 10.)),
            )
            .on_click(move |_, _, cx| {
                let id = id.clone();
                let _ = this.update(cx, |this, cx| {
                    if !this.collapsed.remove(&id) {
                        this.collapsed.insert(id);
                    }
                    cx.notify();
                });
            })
            .into_any_element()
    }

    /// An issue on one line as Linear lists it: its priority and its status, which a click
    /// changes, its identifier and title, then its labels, who has it and when it last changed.
    /// A click opens it; a right click offers what else can be done with it.
    #[allow(clippy::too_many_arguments)]
    fn issue_row(
        &self,
        workspace: &str,
        row: &Row,
        state: &LinearState,
        states: Vec<LinearState>,
        users: &[LinearUser],
        change: &Change,
        spinning: bool,
        narrow: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let priority = ActionMenu::icon(
            SharedString::from(format!("linear-priority-{}", row.id)),
            priority_symbol(row.priority),
            row.priority_label,
            {
                let (change, id, current) = (change.clone(), row.id.clone(), row.priority);
                move |_, _| {
                    priorities_menu(
                        current,
                        Rc::new({
                            let (change, id) = (change.clone(), id.clone());
                            move |priority, cx: &mut App| {
                                change(id.clone(), LinearChange { priority: Some(priority), ..Default::default() }, cx)
                            }
                        }),
                    )
                }
            },
        )
        .button(|button| button.small().tint((row.priority == 1).then_some(c.warning)));
        let status = ActionMenu::icon(
            SharedString::from(format!("linear-status-{}", row.id)),
            state_symbol(state.kind),
            format!("Status: {}", state.name),
            {
                let (change, id, current) = (change.clone(), row.id.clone(), state.id.clone());
                move |_, _| {
                    states_menu(
                        &states,
                        Some(&current),
                        Rc::new({
                            let (change, id) = (change.clone(), id.clone());
                            move |state, cx: &mut App| {
                                change(id.clone(), LinearChange { state: Some(state), ..Default::default() }, cx)
                            }
                        }),
                    )
                }
            },
        )
        .button(|button| button.small().tint(Some(hex_color(&state.color))).pending(spinning));
        let labels: Vec<AnyElement> = if narrow {
            Vec::new()
        } else {
            row.labels
                .iter()
                .take(2)
                .map(|label| Chip::new(label.name.clone()).dot(hex_color(&label.color)).into_any_element())
                .collect()
        };
        let initials = row.initials.clone().map(|initials| (initials, row.assignee.clone().unwrap_or_default()));
        let open = {
            let (store, workspace, id, identifier) =
                (self.store.clone(), workspace.to_string(), row.id.clone(), row.identifier.clone());
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                store.update(cx, |store, cx| {
                    store.open_linear_issue(workspace.clone(), id.clone(), identifier.clone());
                    cx.notify();
                });
            }
        };
        let actions = {
            let (change, users, row) = (change.clone(), users.to_vec(), row.clone());
            move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                cx.stop_propagation();
                row_menu(&row, &users, &change, event.position).show(event.position, window, cx);
            }
        };
        div()
            .id(SharedString::from(format!("linear-row-{}", row.id)))
            .my(px(1.))
            .h(px(34.))
            .w_full()
            .flex_shrink_0()
            .rounded(px(Radius::CONTROL))
            .hover(|row| row.bg(c.background_secondary))
            .flex()
            .items_center()
            .pl(px(6.))
            .pr(px(10.))
            .gap(px(4.))
            .child(priority)
            .child(status)
            .child(
                div()
                    .ml(px(4.))
                    .flex_shrink_0()
                    .text_size(px(11.5))
                    .text_color(c.tertiary)
                    .child(row.identifier.clone()),
            )
            .child(
                div()
                    .ml(px(4.))
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(c.text)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(row.title.clone()),
            )
            .child(
                div()
                    .ml(px(4.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(labels)
                    .when_some(initials, |side, (initials, name)| side.child(initials_disc(initials, name, cx)))
                    .child(div().text_size(px(11.5)).text_color(c.tertiary).child(ago(row.updated_at))),
            )
            .on_click(open)
            .on_mouse_down(MouseButton::Right, actions)
            .into_any_element()
    }

    // How to connect

    fn connect_surface(&self, target: &PanelTarget, cx: &Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let connecting = store.linear.connecting.as_deref() == Some(target.server_id.as_str());
        let error = store.linear.error.clone();
        let (store, server) = (self.store.clone(), target.server_id.clone());
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .child(
                div()
                    .max_w(px(320.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(16.))
                    .child(div().text_color(c.text).child(icons::symbol("linear", 28.)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c.text)
                                    .child("Connect Linear"),
                            )
                            .child(div().text_size(px(12.5)).text_color(c.secondary).text_center().child(
                                "See your issues and hand them to your agents. The connection is kept on your server.",
                            )),
                    )
                    .child(
                        ActionButton::new("linear-connect", "Connect Linear").primary().pending(connecting).on_click(
                            move |_, _, cx| {
                                store.update(cx, |store, cx| {
                                    store.linear_connect(&server);
                                    cx.notify();
                                });
                            },
                        ),
                    )
                    .when_some(error, |column, error| {
                        column.child(div().text_size(px(12.5)).text_color(c.danger).text_center().child(error))
                    }),
            )
            .into_any_element()
    }

    // One issue

    fn issue_surface(
        &mut self,
        target: &PanelTarget,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.clear_comment {
            self.clear_comment = false;
            self.comment.update(cx, |comment, cx| comment.set_value("", window, cx));
        }
        let trigger = format!("{}|{}", target.server_id, self.asked);
        let (server, workspace, issue) = (target.server_id.clone(), self.workspace.clone(), id.to_string());
        self.follow(
            "issue",
            trigger,
            None,
            move |store| {
                store.linear_need_teams(&workspace, &server);
                store.linear_load_issue(&issue, &workspace, &server);
            },
            cx,
        );
        let c = colors(cx);
        let store = self.store.read(cx);
        let page = store.linear.pages.get(id).cloned().unwrap_or(Loaded::Loading);
        let error = store.linear.error.clone();
        let reading = store.linear.is_working(&format!("read:{id}"));
        let identifier = page.value().map(|page| page.row.identifier.clone()).unwrap_or_default();
        let url = page.value().map(|page| page.row.url.clone()).filter(|url| !url.is_empty());
        let this = cx.entity().downgrade();
        let bar = bar_row(cx)
            .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(c.secondary).child(identifier))
            .child(div().flex_1().min_w(px(4.)))
            .when_some(url, |bar, url| {
                bar.child(
                    ActionButton::icon("linear-open", "square-arrow-out-up-right", "Open in Linear")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .child(ActionButton::icon("linear-reread", "rotate-cw", "Read the issue again").pending(reading).on_click(
                move |_, _, cx| {
                    let _ = this.update(cx, |this, cx| this.refresh(cx));
                },
            ));
        let content = match &page {
            Loaded::Loading => loading(cx),
            Loaded::Failed(reason) => message(reason.clone(), true, cx),
            Loaded::Ready(page) => self.issue_content(target, page, cx),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_shrink_0().flex().flex_col().child(bar).child(divider(cx)))
            .when_some(error, |tab, error| tab.child(note(error, cx)))
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }

    fn issue_content(&self, target: &PanelTarget, page: &Page, cx: &Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let workspace = self.workspace.clone();
        let id = page.row.id.clone();
        let states = store.linear.states_of(&workspace, &page.row.team);
        let users: Vec<LinearUser> = store.linear.users.get(&workspace).cloned().unwrap_or_default();
        let changing = store.linear.is_working(&id);
        let commenting = store.linear.is_working(&format!("comment:{id}"));
        let change: Pick<LinearChange> = {
            let (store, id, workspace, server) =
                (self.store.clone(), id.clone(), workspace.clone(), target.server_id.clone());
            Rc::new(move |change, cx| {
                store.update(cx, |store, cx| {
                    store.linear_change(&id, change, &workspace, &server);
                    cx.notify();
                });
            })
        };
        let status = ActionMenu::new("issue-status", page.state.name.clone(), {
            let (change, current) = (change.clone(), page.state.id.clone());
            move |_, _| {
                let change = change.clone();
                states_menu(
                    &states,
                    Some(&current),
                    Rc::new(move |state, cx: &mut App| {
                        change(LinearChange { state: Some(state), ..Default::default() }, cx)
                    }),
                )
            }
        })
        .button(|button| {
            button
                .variant(Variant::Secondary)
                .small()
                .help("Status")
                .picture(
                    icons::symbol(state_symbol(page.state.kind), ControlSize::Small.symbol())
                        .text_color(hex_color(&page.state.color)),
                )
                .pending(changing)
        });
        let priority = ActionMenu::new("issue-priority", priority_name(page.row.priority), {
            let (change, current) = (change.clone(), page.row.priority);
            move |_, _| {
                let change = change.clone();
                priorities_menu(
                    current,
                    Rc::new(move |priority, cx: &mut App| {
                        change(LinearChange { priority: Some(priority), ..Default::default() }, cx)
                    }),
                )
            }
        })
        .button(|button| {
            button.variant(Variant::Secondary).small().symbol(priority_symbol(page.row.priority)).help("Priority")
        });
        let assignee =
            ActionMenu::new("issue-assignee", page.row.assignee.clone().unwrap_or_else(|| "Unassigned".into()), {
                let (change, current) = (change.clone(), page.row.assignee_id.clone());
                move |_, _| {
                    let change = change.clone();
                    assignees_menu(
                        &users,
                        current.as_deref(),
                        Rc::new(move |assignee: Option<String>, cx: &mut App| {
                            change(
                                LinearChange { assignee: Some(assignee.unwrap_or_default()), ..Default::default() },
                                cx,
                            )
                        }),
                    )
                }
            })
            .button(|button| button.variant(Variant::Secondary).small().symbol("circle-user").help("Assignee"));
        let work = ActionButton::new("issue-work", "Work on Issue")
            .primary()
            .symbol("play")
            .help("Start a thread on this issue")
            .on_click({
                let (store, page, workspace, target) =
                    (self.store.clone(), page.clone(), workspace.clone(), target.clone());
                move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.linear_work(&page, &workspace, &target, cx);
                        cx.notify();
                    });
                }
            });
        let description: AnyElement = if page.description.is_empty() {
            div().text_size(px(12.5)).text_color(c.tertiary).child("No description.").into_any_element()
        } else {
            markdown("issue-description", &page.description, cx).into_any_element()
        };
        let comments = page.comments.iter().map(|comment| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(initials_disc(comment.initials.clone(), comment.author.clone(), cx))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .flex()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(c.text)
                                        .child(comment.author.clone()),
                                )
                                .child(div().text_color(c.tertiary).child(format!(" · {}", ago(comment.at)))),
                        ),
                )
                .child(markdown(&format!("issue-comment-{}", comment.id), &comment.body, cx))
        });
        let words = self.comment.read(cx).value().trim().to_string();
        let send =
            ActionButton::new("issue-comment", "Comment").pending(commenting).disabled(words.is_empty()).on_click({
                let (store, this, id, workspace, server) = (
                    self.store.clone(),
                    cx.entity().downgrade(),
                    id.clone(),
                    workspace.clone(),
                    target.server_id.clone(),
                );
                move |_, _, cx| {
                    let (this, words) = (this.clone(), words.clone());
                    store.update(cx, |store, cx| {
                        store.linear_comment(&id, &workspace, &server, words, move |_, cx| {
                            let _ = this.update(cx, |this, cx| {
                                this.clear_comment = true;
                                cx.notify();
                            });
                        });
                        cx.notify();
                    });
                }
            });
        let writing = div()
            .w_full()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(8.))
            .child(writing_field(&self.comment, 84., cx))
            .child(send);
        div()
            .id("linear-issue-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.text)
                            .child(page.row.title.clone()),
                    )
                    .child(div().flex().flex_wrap().gap(px(6.)).child(status).child(priority).child(assignee))
                    .when(!page.row.labels.is_empty(), |column| {
                        column.child(
                            div().flex().flex_wrap().gap(px(6.)).children(
                                page.row
                                    .labels
                                    .iter()
                                    .map(|label| Chip::new(label.name.clone()).dot(hex_color(&label.color))),
                            ),
                        )
                    })
                    .child(work)
                    .child(description)
                    .child(divider(cx))
                    .children(comments)
                    .child(writing),
            )
            .into_any_element()
    }
}

impl Render for LinearView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        if let Some(reason) = store.linear_unavailable() {
            return message(reason, false, cx);
        }
        let Some(target) = store.panel_target() else { return div().into_any_element() };
        if let Some(issue) = self.issue.clone() {
            return self.issue_surface(&target, &issue, window, cx);
        }
        let server = target.server_id.clone();
        self.follow("status", server.clone(), None, move |store| store.linear_read(&server), cx);
        if self.store.read(cx).linear.connected(&target.server_id).is_empty() {
            return self.connect_surface(&target, cx);
        }
        self.issues_surface(&target, cx)
    }
}

/// The sheet that files an issue.
pub struct NewIssue {
    store: Entity<Store>,
    target: PanelTarget,
    workspace: String,
    /// The team the list shows, when it shows one.
    team: Option<String>,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    team_id: Option<String>,
    state_id: Option<String>,
    assignee_id: Option<String>,
    priority: u8,
    pub closed: bool,
    _subscriptions: Vec<Subscription>,
}

impl NewIssue {
    fn new(
        store: Entity<Store>,
        target: PanelTarget,
        workspace: String,
        team: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        store.update(cx, |store, _| store.linear_need_teams(&workspace, &target.server_id));
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let description = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 12).placeholder("Add a description"));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe(&title, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        title.update(cx, |title, cx| title.focus(window, cx));
        Self {
            store,
            target,
            workspace,
            team,
            title,
            description,
            team_id: None,
            state_id: None,
            assignee_id: None,
            priority: 0,
            closed: false,
            _subscriptions: subscriptions,
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        cx.notify();
    }
}

impl Render for NewIssue {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let teams: Vec<LinearTeam> = store.linear.teams.get(&self.workspace).cloned().unwrap_or_default();
        let users: Vec<LinearUser> = store.linear.users.get(&self.workspace).cloned().unwrap_or_default();
        let wanted = self.team_id.clone().or_else(|| self.team.clone());
        let picked = teams.iter().find(|team| Some(&team.id) == wanted.as_ref()).or(teams.first()).cloned();
        let states: Vec<LinearState> = picked.as_ref().map(|team| team.states.clone()).unwrap_or_default();
        let state = states
            .iter()
            .find(|state| Some(&state.id) == self.state_id.as_ref())
            .or_else(|| states.iter().find(|state| state.kind == LinearStateKind::Unstarted))
            .or(states.first())
            .cloned();
        let error = store.linear.error.clone();
        let creating = store.linear.is_working("create");
        let title = self.title.read(cx).value().trim().to_string();
        let this = cx.entity().downgrade();

        let team_menu = ActionMenu::new(
            "new-issue-team",
            picked.as_ref().map(|team| team.name.clone()).unwrap_or_else(|| "Team".into()),
            {
                let (this, teams, picked) = (this.clone(), teams.clone(), picked.as_ref().map(|team| team.id.clone()));
                move |_, _| {
                    let mut menu = Menu::new();
                    for team in &teams {
                        let (this, id) = (this.clone(), team.id.clone());
                        menu = menu.checked(Some(&team.id) == picked.as_ref(), team.name.clone(), move |_, cx| {
                            let id = id.clone();
                            let _ = this.update(cx, |this, cx| {
                                this.team_id = Some(id);
                                this.state_id = None;
                                cx.notify();
                            });
                        });
                    }
                    menu
                }
            },
        )
        .button(|button| button.variant(Variant::Secondary).small().help("Team"));
        let state_menu = ActionMenu::new(
            "new-issue-state",
            state.as_ref().map(|state| state.name.clone()).unwrap_or_else(|| "Status".into()),
            {
                let (this, states, current) =
                    (this.clone(), states.clone(), state.as_ref().map(|state| state.id.clone()));
                move |_, _| {
                    let this = this.clone();
                    states_menu(
                        &states,
                        current.as_deref(),
                        Rc::new(move |state, cx: &mut App| {
                            let _ = this.update(cx, |this, cx| {
                                this.state_id = Some(state);
                                cx.notify();
                            });
                        }),
                    )
                }
            },
        )
        .button(|button| {
            let button = button.variant(Variant::Secondary).small().help("Status");
            match &state {
                Some(state) => button.symbol(state_symbol(state.kind)),
                None => button,
            }
        });
        let priority_menu = ActionMenu::new("new-issue-priority", priority_name(self.priority), {
            let (this, current) = (this.clone(), self.priority);
            move |_, _| {
                let this = this.clone();
                priorities_menu(
                    current,
                    Rc::new(move |priority, cx: &mut App| {
                        let _ = this.update(cx, |this, cx| {
                            this.priority = priority;
                            cx.notify();
                        });
                    }),
                )
            }
        })
        .button(|button| {
            button.variant(Variant::Secondary).small().symbol(priority_symbol(self.priority)).help("Priority")
        });
        let assignee_name = users
            .iter()
            .find(|user| Some(&user.id) == self.assignee_id.as_ref())
            .map(|user| user.name.clone())
            .unwrap_or_else(|| "Unassigned".into());
        let assignee_menu = ActionMenu::new("new-issue-assignee", assignee_name, {
            let (this, users, current) = (this.clone(), users.clone(), self.assignee_id.clone());
            move |_, _| {
                let this = this.clone();
                assignees_menu(
                    &users,
                    current.as_deref(),
                    Rc::new(move |assignee: Option<String>, cx: &mut App| {
                        let _ = this.update(cx, |this, cx| {
                            this.assignee_id = assignee;
                            cx.notify();
                        });
                    }),
                )
            }
        })
        .button(|button| button.variant(Variant::Secondary).small().symbol("circle-user").help("Assignee"));

        let cancel = ActionButton::new("new-issue-cancel", "Cancel").surface(Surface::Popover).on_click({
            let this = this.clone();
            move |_, _, cx| {
                let _ = this.update(cx, |this, cx| this.close(cx));
            }
        });
        let create = ActionButton::new("new-issue-create", "Create Issue")
            .primary()
            .surface(Surface::Popover)
            .pending(creating)
            .disabled(picked.is_none() || title.is_empty())
            .on_click({
                let (store, this, target, workspace) =
                    (self.store.clone(), this.clone(), self.target.clone(), self.workspace.clone());
                let (picked, state, assignee, priority) =
                    (picked.clone(), state.clone(), self.assignee_id.clone(), self.priority);
                let description = self.description.clone();
                move |_, _, cx| {
                    let Some(team) = picked.as_ref() else { return };
                    let issue = NewLinearIssue {
                        team: team.id.clone(),
                        title: title.clone(),
                        description: description.read(cx).value().to_string(),
                        state: state.as_ref().map(|state| state.id.clone()),
                        assignee: assignee.clone(),
                        priority,
                    };
                    let (this, workspace) = (this.clone(), workspace.clone());
                    store.update(cx, |store, cx| {
                        let opened = workspace.clone();
                        store.linear_create(issue, &workspace, &target.server_id, move |store, id, identifier, cx| {
                            let _ = this.update(cx, |this, cx| this.close(cx));
                            store.open_linear_issue(opened, id, identifier);
                        });
                        cx.notify();
                    });
                }
            });

        let content = div()
            .h(px(400.))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child("New issue"))
            .child(InputField::new(&self.title).surface(Surface::Popover))
            .child(div().flex_1().min_h_0().child(writing_field(&self.description, 0., cx)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .child(team_menu)
                    .child(state_menu)
                    .child(priority_menu)
                    .child(assignee_menu),
            )
            .when_some(error, |column, error| column.child(div().text_size(px(12.5)).text_color(c.danger).child(error)))
            .child(div().flex().justify_end().gap(px(8.)).child(cancel).child(create));
        sheet("linear-new-issue", 520., content, cx)
    }
}

// What the menus that change an issue offer

type Pick<T> = Rc<dyn Fn(T, &mut App)>;

fn states_menu(states: &[LinearState], current: Option<&str>, pick: Pick<String>) -> Menu {
    let mut menu = Menu::new();
    for state in states {
        let (pick, id) = (pick.clone(), state.id.clone());
        menu = menu.checked(Some(state.id.as_str()) == current, state.name.clone(), move |_, cx| pick(id.clone(), cx));
    }
    menu
}

fn priorities_menu(current: u8, pick: Pick<u8>) -> Menu {
    let mut menu = Menu::new();
    for (value, name) in PRIORITIES {
        let pick = pick.clone();
        menu = menu.checked(value == current, name, move |_, cx| pick(value, cx));
    }
    menu
}

/// `pick` is given nothing for nobody.
fn assignees_menu(users: &[LinearUser], current: Option<&str>, pick: Pick<Option<String>>) -> Menu {
    let nobody = pick.clone();
    let mut menu = Menu::new().checked(current.is_none(), "Unassigned", move |_, cx| nobody(None, cx)).separator();
    for user in users {
        let (pick, id) = (pick.clone(), user.id.clone());
        let name = if user.me { format!("{} (you)", user.name) } else { user.name.clone() };
        menu = menu.checked(Some(user.id.as_str()) == current, name, move |_, cx| pick(Some(id.clone()), cx));
    }
    menu
}

/// What a right click on a row offers.
fn row_menu(row: &Row, users: &[LinearUser], change: &Change, at: Point<Pixels>) -> Menu {
    let assign = {
        let (change, users, id, current) = (change.clone(), users.to_vec(), row.id.clone(), row.assignee_id.clone());
        move |window: &mut Window, cx: &mut App| {
            let (change, id) = (change.clone(), id.clone());
            let pick = Rc::new(move |assignee: Option<String>, cx: &mut App| {
                change(
                    id.clone(),
                    LinearChange { assignee: Some(assignee.unwrap_or_default()), ..Default::default() },
                    cx,
                )
            });
            assignees_menu(&users, current.as_deref(), pick).show(at, window, cx);
        }
    };
    let url = (!row.url.is_empty()).then(|| row.url.clone());
    let identifier = row.identifier.clone();
    Menu::new()
        .symbol_item("Assign To…", "circle-user", assign)
        .when_some(url, |menu, url| {
            let link = url.clone();
            menu.symbol_item("Open in Linear", "square-arrow-out-up-right", move |_, cx| cx.open_url(&url)).symbol_item(
                "Copy Link",
                "link",
                move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(link.clone())),
            )
        })
        .symbol_item("Copy Identifier", "copy", move |_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(identifier.clone()))
        })
}

// The pieces

/// Whose the issue is, by initials in a disc that says the name under the pointer.
fn initials_disc(initials: String, name: String, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    div()
        .id(SharedString::from(format!("initials-{name}")))
        .size(px(18.))
        .flex_shrink_0()
        .rounded_full()
        .bg(c.background_tertiary)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c.secondary)
        .child(initials)
        .tooltip(tooltip(name))
}

/// A box to write in, with a word in it while it is empty. Without a `height` it is as tall as
/// there is room for.
fn writing_field(state: &Entity<TextareaState>, height: f32, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .w_full()
        .when(height > 0., |field| field.min_h(px(height)))
        .when(height <= 0., |field| field.h_full())
        .px(px(4.))
        .py(px(6.))
        .rounded(px(Radius::CONTROL))
        .border_1()
        .border_color(c.border)
        .text_size(px(12.5))
        .child(Textarea::new(state).appearance(false).px_0().py_0().text_size(px(12.5)))
}

/// A row of the strip under the tabs with what the open tab is about and its buttons.
fn bar_row(_: &App) -> Div {
    div().h(px(36.)).pl(px(12.)).pr(px(4.)).flex().items_center().gap(px(4.))
}

/// What the tab says in the middle of the panel when it has nothing to show.
fn message(text: impl Into<SharedString>, failed: bool, cx: &App) -> AnyElement {
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

fn loading(cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(c.tertiary)
        .child(Spinner::new(ControlSize::Large.symbol()).render(cx))
        .into_any_element()
}

/// What went wrong, under the tab's bar.
fn note(text: String, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .px(px(12.))
                .py(px(7.))
                .bg(c.warning_background)
                .text_size(px(12.))
                .text_color(c.warning)
                .child(text),
        )
        .child(divider(cx))
}

/// Linear's colours come as hex without the `#`.
fn hex_color(hex: &str) -> Hsla {
    let hex = hex.trim_start_matches('#');
    Rgba::try_from(format!("#{hex}").as_str()).map(Hsla::from).unwrap_or_else(|_| hsla(0., 0., 0.5, 1.))
}
