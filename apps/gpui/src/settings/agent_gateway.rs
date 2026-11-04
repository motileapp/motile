//! Settings → Agent Gateway: the account's gateway, the accounts it shares with every server, its
//! settings and the requests it served, with the sheets that add a gateway, sign an account in, rename
//! one and show a captured request. `GatewayHeader` is the top of both the gateway's pages.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::gateway::{AccountRow, AccountState, View};
use motile_protocol::gateway_wire::{AccountChange, Capture};
use motile_protocol::wire::Agent;

use super::layout::{CONTENT_WIDTH, Cut, INSET, PAGE_PADDING, TRAILING_GAP, footed_label, group, label, more, row};
use crate::models::{agent_name, now};
use crate::panel::code_view::{CodeActions, CodeSnapshot, CodeView};
use crate::panel::state::{CodeDocument, CodeMarks, Loaded};
use crate::store::Store;
use crate::store::gateway::{Captured, Enroll, LogRow};
use crate::store::usage::LimitWindow;
use crate::theme::{ControlSize, IconSize, Layer, Opacity, Surface, colors};
use crate::ui::menu::Menu;
use crate::ui::modal::{Answer, Confirmation, confirmation, modal, present_sheet, presented};
use crate::ui::parts::FadingScroll;
use crate::ui::{
    ActionButton, ActionMenu, Chip, CommandBox, CopyButton, InputField, Segmented, Spinner, Switch, Variant, beside,
    card_default, divider_on, icons, logos,
};
use crate::usage::limits::{BESIDE_WIDTH, limit_row};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

const AGENTS: [Agent; 2] = [Agent::Claude, Agent::Codex];
/// How many threads the capture menu offers.
const CAPTURE_THREADS: usize = 12;
/// How many requests "Capture the next" captures.
const CAPTURE_NEXT: u32 = 20;
const CODE_SIZE: f32 = 26.;
/// How tall the log grows before it scrolls.
const LOG_HEIGHT: f32 = 420.;

/// What the Agent Gateway page says it does with the gateway.
const AGENT_CAPTION: &str = "Holds your Claude Code and Codex accounts. Every server's Gateway accounts use them.";
/// What the MCP Gateway page says it does with the gateway.
pub(super) const MCP_CAPTION: &str =
    "Signs in to each MCP server once. Every agent on every server and account gets its tools.";

/// One of the gateway's accounts, which the user is asked to remove.
#[derive(Clone)]
struct Removal {
    id: String,
    name: String,
}

pub struct AgentGatewayPage {
    store: Entity<Store>,
    header: Entity<GatewayHeader>,
    removal: Option<Removal>,
    rename: Option<Entity<RenameSheet>>,
    sign_in: Option<Entity<SignInSheet>>,
    captured: Option<Entity<CapturedSheet>>,
    _subscription: Subscription,
}

impl AgentGatewayPage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        let header = cx.new(|cx| GatewayHeader::new(store.clone(), AGENT_CAPTION, cx));
        Self { store, header, removal: None, rename: None, sign_in: None, captured: None, _subscription: subscription }
    }

    fn ask_removal(&self, removal: Removal, cx: &mut Context<Self>) -> Run {
        let page = cx.weak_entity();
        Rc::new(move |_, cx| {
            let removal = removal.clone();
            let _ = page.update(cx, |page, cx| {
                page.removal = Some(removal);
                cx.notify();
            });
        })
    }

    /// Each agent's accounts, a card each, and the way to add another.
    fn accounts(&self, view: &View, width: f32, cx: &mut Context<Self>) -> Vec<AnyElement> {
        AGENTS
            .into_iter()
            .enumerate()
            .map(|(place, agent)| {
                let accounts: Vec<&AccountRow> =
                    view.accounts.iter().filter(|account| account.agent == agent).collect();
                let mut cards: Vec<AnyElement> = accounts
                    .iter()
                    .enumerate()
                    .map(|(index, account)| self.account_card(place * 1000 + index, account, accounts.len(), width, cx))
                    .collect();
                let store = self.store.clone();
                let add = ActionButton::new(("add-account", place), format!("Add {} Account", agent_name(agent)))
                    .symbol("plus")
                    .variant(Variant::Outline)
                    .size(ControlSize::Large)
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.start_gateway_sign_in(agent, None);
                            cx.notify();
                        })
                    });
                cards.insert(0, div().flex().child(add).into_any_element());
                let id = match agent {
                    Agent::Claude => "gateway-accounts",
                    Agent::Codex => "gateway-accounts-codex",
                };
                group(id, format!("{} Accounts", agent_name(agent)), None, false, cards, cx)
            })
            .collect()
    }

    fn account_card(
        &self,
        index: usize,
        account: &AccountRow,
        count: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let id = account.id.clone();
        let store = self.store.clone();
        let switch = Switch::new(("account-enabled", index), account.enabled).on_change(move |enabled, _, cx| {
            let id = id.clone();
            store.update(cx, |store, cx| {
                store.update_gateway_account(id, AccountChange::Enabled { enabled });
                cx.notify();
            })
        });
        let signs_in_again = account.status == AccountState::SignInAgain;
        let sign_in_again = signs_in_again.then(|| {
            let (store, agent, again) = (self.store.clone(), account.agent, account.id.clone());
            large(("account-sign-in", index), "Sign In Again").margin(beside(0., TRAILING_GAP / 2.)).on_click(
                move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.start_gateway_sign_in(agent, Some(again.clone()));
                        cx.notify();
                    })
                },
            )
        });
        let switch = switch.margin(beside(if signs_in_again { TRAILING_GAP / 2. } else { 0. }, TRAILING_GAP / 2.));
        let menu = self.account_menu(index, account, count, cx).margin(beside(TRAILING_GAP / 2., 0.));
        let trailing = div().flex().items_center().children(sign_in_again).child(switch).child(menu);
        // As far from the card's top as its limits' numbers are from theirs.
        let leading =
            vec![div().flex_1().min_w_0().flex().py(px(6.)).child(account_label(account, cx)).into_any_element()];
        let mut rows = vec![row(leading, vec![trailing.into_any_element()]).into_any_element()];
        let beside = width >= BESIDE_WIDTH;
        let windows = account.windows.iter().enumerate().map(|(place, window)| {
            let window = LimitWindow::from(window);
            limit_row((10_000 + index, place), &window, beside, cx)
        });
        let off = !account.enabled;
        for window in windows {
            rows.push(divider_on(Surface::Card, cx).into_any_element());
            rows.push(
                div().when(off, |row| row.opacity(c.opacity(Opacity::Disabled))).child(window).into_any_element(),
            );
        }
        if let Some(note) = &account.note {
            rows.push(divider_on(Surface::Card, cx).into_any_element());
            rows.push(
                div()
                    .px(px(INSET))
                    .py(px(12.))
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .child(note.clone())
                    .into_any_element(),
            );
        }
        card_default(div().flex().flex_col(), cx).children(rows).into_any_element()
    }

    fn account_menu(&self, index: usize, account: &AccountRow, count: usize, cx: &mut Context<Self>) -> ActionMenu {
        let (page, store) = (cx.weak_entity(), self.store.clone());
        let remove = self.ask_removal(Removal { id: account.id.clone(), name: account.name.clone() }, cx);
        let account = account.clone();
        more(("account-menu", index), format!("What to do with {}", account.name), move |_, _| {
            let rename = {
                let (page, id, name) = (page.clone(), account.id.clone(), account.name.clone());
                move |window: &mut Window, cx: &mut App| {
                    let (id, name) = (id.clone(), name.clone());
                    let _ = page.update(cx, |page, cx| page.open_rename(id, name, window, cx));
                }
            };
            let priorities = (0..count.max(1) as u32).fold(Menu::new(), |menu, priority| {
                let (store, id) = (store.clone(), account.id.clone());
                menu.checked(priority == account.priority, format!("Priority {}", priority + 1), move |_, cx| {
                    let id = id.clone();
                    store.update(cx, |store, cx| {
                        store.update_gateway_account(id, AccountChange::Priority { priority });
                        cx.notify();
                    })
                })
            });
            let sign_in = {
                let (store, agent, again) = (store.clone(), account.agent, account.id.clone());
                move |_: &mut Window, cx: &mut App| {
                    store.update(cx, |store, cx| {
                        store.start_gateway_sign_in(agent, Some(again.clone()));
                        cx.notify();
                    })
                }
            };
            let remove = remove.clone();
            Menu::new()
                .symbol_item("Rename Account", "pencil", rename)
                .submenu(account.priority_text.clone(), priorities)
                .symbol_item("Sign In Again", "log-in", sign_in)
                .separator()
                .danger_symbol_item("Remove Account", "trash-2", move |window, cx| remove(window, cx))
        })
    }

    fn open_rename(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.rename = Some(cx.new(|cx| RenameSheet::new(store, page, id, name, window, cx)));
        cx.notify();
    }

    /// What is being captured and the way to capture more, then the requests, newest first.
    fn log(&self, view: &View, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let threads: Vec<(String, String, String)> = store
            .active_threads()
            .iter()
            .filter(|thread| !store.is_removed(&thread.server_id))
            .take(CAPTURE_THREADS)
            .map(|thread| (thread.server_id.clone(), thread.id.clone(), thread.title.clone()))
            .collect();
        let log = &store.gateway.log;
        let (rows, more_rows, loading, error, errors_only) =
            (log.rows.clone(), log.more, log.loading, log.error.clone(), log.errors_only);
        let handle = self.store.clone();
        let capture = ActionMenu::new("gateway-capture", "Capture", move |_, _| {
            let next = {
                let handle = handle.clone();
                move |_: &mut Window, cx: &mut App| {
                    handle.update(cx, |store, cx| {
                        store.capture_gateway(Capture::Next { left: CAPTURE_NEXT });
                        cx.notify();
                    })
                }
            };
            let picked = threads.iter().fold(Menu::new(), |menu, (server, thread, title)| {
                let (handle, server, thread) = (handle.clone(), server.clone(), thread.clone());
                menu.item(title.clone(), move |_, cx| {
                    let capture = Capture::Thread { server: server.clone(), thread: thread.clone() };
                    handle.update(cx, |store, cx| {
                        store.capture_gateway(capture);
                        cx.notify();
                    })
                })
            });
            let picked = if picked.is_empty() { Menu::new().note("No thread is working") } else { picked };
            Menu::new()
                .item(format!("Capture the Next {CAPTURE_NEXT} Requests"), next)
                .submenu("Capture a Thread", picked)
        })
        .button(outline)
        .margin(beside(if view.capture.on { TRAILING_GAP / 2. } else { 0. }, 0.));
        let stop = view.capture.on.then(|| {
            let handle = self.store.clone();
            large("gateway-capture-stop", "Stop").margin(beside(0., TRAILING_GAP / 2.)).on_click(move |_, _, cx| {
                handle.update(cx, |store, cx| {
                    store.capture_gateway(Capture::Off);
                    cx.notify();
                })
            })
        });
        let trailing = div().flex().items_center().children(stop).child(capture).into_any_element();
        let leading = label(view.capture.text.clone(), None, None, Cut::Wraps, cx);
        let capture_card = card_default(div().flex().flex_col(), cx).child(row(vec![leading], vec![trailing]));

        let handle = self.store.clone();
        let filter =
            Segmented::new("gateway-log-filter", vec!["All".into(), "Errors".into()], usize::from(errors_only))
                .on_select(move |index, _, cx| {
                    handle.update(cx, |store, cx| {
                        store.show_gateway_errors_only(index == 1);
                        cx.notify();
                    })
                });
        let toolbar = div().px(px(INSET)).py(px(10.)).flex().items_center().gap(px(8.)).child(filter);
        let mut list = Vec::new();
        let empty = match (&error, errors_only) {
            (Some(error), _) if rows.is_empty() => Some(error.clone()),
            _ if !rows.is_empty() || loading => None,
            (_, true) => Some("No errors".into()),
            (_, false) => Some("No requests yet".into()),
        };
        if let Some(empty) = empty {
            list.push(divider_on(Surface::Card, cx).into_any_element());
            list.push(
                div()
                    .px(px(INSET))
                    .py(px(14.))
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .child(empty)
                    .into_any_element(),
            );
        }
        for (index, entry) in rows.iter().enumerate() {
            list.push(divider_on(Surface::Card, cx).into_any_element());
            list.push(self.log_row(index, entry, cx));
        }
        if more_rows {
            list.push(divider_on(Surface::Card, cx).into_any_element());
            list.push(self.more_rows(loading, cx));
        }
        let scroll = FadingScroll::new("gateway-log-rows", LOG_HEIGHT).holds_wheel(true).fill(c.card).children(list);
        let log_card = card_default(div().flex().flex_col().overflow_hidden(), cx).child(toolbar).child(scroll);
        let content = div().flex().flex_col().gap(px(12.)).child(capture_card).child(log_card);
        group("gateway-log", "Log", None, false, vec![content.into_any_element()], cx)
    }

    fn log_row(&self, index: usize, entry: &LogRow, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let status = Chip::new(entry.status.to_string()).monospaced(true).surface(Surface::Card);
        let status = if entry.ok { status } else { status.tone(c.warning) };
        let first = [Some(entry.server.clone()), Some(entry.thread.clone())].into_iter().flatten().collect::<Vec<_>>();
        let second: Vec<String> = [
            entry.account.clone(),
            entry.model.clone(),
            Some(entry.path.clone()),
            Some(entry.tokens.clone()),
            entry.retries.clone(),
            Some(entry.size.clone()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let line = |text: String, color: Hsla| {
            div().min_w_0().text_size(px(11.5)).text_color(color).child(crate::ui::control::dotted(text, c))
        };
        let content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .font_features(crate::ui::tabular_numbers())
                            .text_color(c.muted_foreground)
                            .child(entry.time.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(crate::ui::control::dotted(first.join(" · "), c)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .font_features(crate::ui::tabular_numbers())
                            .text_color(c.muted_foreground)
                            .child(entry.latency.clone()),
                    )
                    .child(status),
            )
            .child(line(second.join(" · "), c.muted_foreground))
            .when_some(entry.error.clone(), |content, error| {
                content.child(div().text_size(px(11.5)).text_color(c.warning).child(error))
            });
        let row = div()
            .id(("gateway-log-row", index))
            .px(px(INSET))
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(content);
        if !entry.captured {
            return row.into_any_element();
        }
        let (store, entry) = (self.store.clone(), entry.id);
        row.cursor_default()
            .hover(|row| row.bg(Surface::Card.layer(Layer::Row, c)))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(c.muted_foreground)
                    .child(icons::symbol("chevron-right", IconSize::Md)),
            )
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.open_gateway_captured(entry);
                    cx.notify();
                })
            })
            .into_any_element()
    }

    /// The end of the rows read, which reads the next page once it comes into the window.
    fn more_rows(&self, loading: bool, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.clone();
        let reaches = canvas(
            move |bounds, window, cx| {
                if loading || bounds.top() > window.content_mask().bounds.bottom() {
                    return;
                }
                let store = store.clone();
                cx.defer(move |cx| {
                    store.update(cx, |store, cx| {
                        store.load_gateway_log(false);
                        cx.notify();
                    })
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .h(px(44.))
            .flex()
            .items_center()
            .justify_center()
            .child(reaches)
            .child(Spinner::regular().render(cx))
            .into_any_element()
    }

    fn removal_confirmation(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (removal, leaving) = presented("gateway-removal", self.removal.clone(), window, cx)?;
        let store = self.store.clone();
        let shown = Confirmation::new(format!("Remove “{}” from Your Gateway?", removal.name))
            .message("Your gateway forgets its sign-in, and its threads move to your other accounts.")
            .answer(
                Answer::new("Remove", move |_, cx| {
                    let id = removal.id.clone();
                    store.update(cx, |store, cx| {
                        store.remove_gateway_account(id);
                        cx.notify();
                    })
                })
                .danger(),
            );
        let page = cx.weak_entity();
        let close = move |_: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| {
                page.removal = None;
                cx.notify();
            });
        };
        Some(confirmation("gateway-removal", shown, close, cx).leaving(leaving).into_any_element())
    }

    /// Keeps the sheets that follow the store open while it has what they show.
    fn follow_sheets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (signing_in, captured) = {
            let gateway = &self.store.read(cx).gateway;
            (gateway.signing_in.is_some(), gateway.captured.as_ref().map(|(entry, _)| *entry))
        };
        match (signing_in, &self.sign_in) {
            (true, None) => {
                let store = self.store.clone();
                self.sign_in = Some(cx.new(|cx| SignInSheet::new(store, window, cx)));
            }
            (false, Some(_)) => self.sign_in = None,
            _ => {}
        }
        let shown = self.captured.as_ref().map(|sheet| sheet.read(cx).entry);
        if captured != shown {
            let store = self.store.clone();
            self.captured = captured.map(|entry| cx.new(|cx| CapturedSheet::new(store, entry, cx)));
        }
    }

    /// How wide the page's column is.
    fn width(&self, window: &Window, cx: &App) -> f32 {
        let window_width = f32::from(crate::window_frame::content_size(window).width);
        let sidebar = crate::main_view::shown_sidebar_width(self.store.read(cx), window_width);
        (window_width - sidebar - 1. - 2. * PAGE_PADDING).clamp(0., CONTENT_WIDTH)
    }
}

impl Render for AgentGatewayPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_sheets(window, cx);
        let view = self.store.read(cx).gateway.view.clone();
        let width = self.width(window, cx);
        let mut groups = vec![self.header.clone().into_any_element()];
        if let Some(view) = view.as_ref().filter(|view| view.exists) {
            groups.extend(self.accounts(view, width, cx));
            groups.push(self.log(view, cx));
        }
        let removal = self.removal_confirmation(window, cx);
        let rename =
            present_sheet("gateway-rename", self.rename.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        let sign_in = present_sheet(
            "gateway-sign-in",
            self.sign_in.clone(),
            |sheet, leaving| sheet.leaving = leaving,
            window,
            cx,
        );
        let captured = present_sheet(
            "gateway-captured",
            self.captured.clone(),
            |sheet, leaving| sheet.leaving = leaving,
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap(px(super::layout::GROUP_GAP))
            .children(groups)
            .children(removal)
            .children(rename)
            .children(sign_in)
            .children(captured)
    }
}

/// An account's agent and name, and under it its state, who is signed in, the plan and the threads on it.
fn account_label(account: &AccountRow, cx: &App) -> AnyElement {
    let c = colors(cx);
    let status = account.status_text.clone().map(|text| match account.status {
        AccountState::SignInAgain => Chip::new(text).tone(c.warning),
        AccountState::Resting => Chip::new(text).tone(c.pending),
        AccountState::Off | AccountState::Ready => Chip::new(text).muted(true),
    });
    let description: Vec<String> =
        [account.email.clone(), account.plan.clone(), account.threads_text.clone()].into_iter().flatten().collect();
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(logos::agent_icon(account.agent, IconSize::Md, cx))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(account.name.clone()),
                )
                .children(status.map(|chip| chip.surface(Surface::Card)))
                .when(account.next, |line| line.child(Chip::new("Next").tone(c.primary))),
        )
        .when(!description.is_empty(), |label| {
            label.child(
                div()
                    .min_w_0()
                    .text_size(px(11.5))
                    .text_color(c.muted_foreground)
                    .truncate()
                    .child(description.join(" · ")),
            )
        })
        .into_any_element()
}

/// The gateway and how the client stands with it, or what one is and the way to add it, under the
/// page's caption: the top of both the gateway's pages, with the sheet that adds one.
pub(super) struct GatewayHeader {
    store: Entity<Store>,
    caption: &'static str,
    /// The name of the gateway the user is asked to remove.
    removal: Option<String>,
    /// Draws the time the enroll code has left again every second, while a gateway is being added.
    ticking: Option<Task<()>>,
    _subscription: Subscription,
}

impl GatewayHeader {
    pub(super) fn new(store: Entity<Store>, caption: &'static str, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, caption, removal: None, ticking: None, _subscription: subscription }
    }

    fn opens_add(&self) -> impl Fn(&mut Window, &mut App) + 'static {
        let store = self.store.clone();
        move |_, cx| {
            store.update(cx, |store, cx| {
                store.open_add_gateway();
                cx.notify();
            })
        }
    }

    fn row(&self, view: Option<&View>, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = view.filter(|view| view.exists) else {
            let open = self.opens_add();
            let add =
                large("add-gateway", "Add a Gateway").symbol("plus").on_click(move |_, window, cx| open(window, cx));
            let leading = label(
                "No gateway",
                Some("Runs on a machine that is always on".into()),
                Some("waypoints"),
                Cut::Wraps,
                cx,
            );
            return row(vec![leading], vec![add.into_any_element()]).into_any_element();
        };
        let c = colors(cx);
        let name = view.name.clone().unwrap_or_else(|| "Gateway".into());
        let description = match &view.version {
            Some(version) => format!("{} · version {version}", view.status_text),
            None => view.status_text.clone(),
        };
        let error = view
            .error
            .clone()
            .map(|error| div().text_size(px(11.5)).text_color(c.warning).child(error).into_any_element());
        let (header, open) = (cx.weak_entity(), Rc::new(self.opens_add()));
        let removed = name.clone();
        let menu = more("gateway-menu", format!("What to do with {name}"), move |_, _| {
            let (header, open, removed) = (header.clone(), open.clone(), removed.clone());
            Menu::new()
                .symbol_item("Replace with a New Gateway", "refresh-cw", move |window, cx| open(window, cx))
                .separator()
                .danger_symbol_item("Remove Gateway", "trash-2", move |_, cx| {
                    let _ = header.update(cx, |header, cx| {
                        header.removal = Some(removed.clone());
                        cx.notify();
                    });
                })
        });
        let leading = footed_label(name, Some(description), Some("waypoints"), Cut::Wraps, error, cx);
        row(vec![leading], vec![menu.into_any_element()]).into_any_element()
    }

    /// The modal that adds a gateway, while one is being added and while it fades out after, drawn
    /// again every second for the time its code has left.
    fn add_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let adding = self.store.read(cx).gateway.adding.as_ref().map(|adding| adding.enroll.clone());
        match (adding.is_some(), self.ticking.is_some()) {
            (true, false) => {
                self.ticking = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    }
                }));
            }
            (false, true) => self.ticking = None,
            _ => {}
        }
        let (enroll, leaving) = presented("add-gateway", adding, window, cx)?;
        let replaces = self.store.read(cx).gateway.exists();
        Some(add_sheet(&self.store, enroll, replaces, cx).leaving(leaving).into_any_element())
    }

    fn removal_confirmation(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (name, leaving) = presented("gateway-removal", self.removal.clone(), window, cx)?;
        let store = self.store.clone();
        let shown = Confirmation::new("Remove Your Gateway?")
            .message(format!("“{name}” leaves your account, and your servers stop using it. Its threads move to your default accounts."))
            .answer(
                Answer::new("Remove", move |_, cx| {
                    store.update(cx, |store, cx| {
                        store.remove_gateway();
                        cx.notify();
                    })
                })
                .danger(),
            );
        let header = cx.weak_entity();
        let close = move |_: &mut Window, cx: &mut App| {
            let _ = header.update(cx, |header, cx| {
                header.removal = None;
                cx.notify();
            });
        };
        Some(confirmation("gateway-removal", shown, close, cx).leaving(leaving).into_any_element())
    }
}

impl Render for GatewayHeader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.store.read(cx).gateway.view.clone();
        let rows = vec![self.row(view.as_ref(), cx)];
        let add = self.add_sheet(window, cx);
        let removal = self.removal_confirmation(window, cx);
        div().child(group("gateway", "Gateway", Some(self.caption), true, rows, cx)).children(add).children(removal)
    }
}

/// How to run the gateway and the code it joins the account with, until it has.
fn add_sheet(store: &Entity<Store>, enroll: Loaded<Enroll>, replaces: bool, cx: &App) -> crate::ui::modal::Modal {
    let c = colors(cx);
    let close = {
        let store = store.clone();
        move |_: &mut Window, cx: &mut App| {
            store.update(cx, |store, cx| {
                store.close_add_gateway();
                cx.notify();
            })
        }
    };
    let title = if replaces { "Replace Your Gateway" } else { "Add a Gateway" };
    let body: AnyElement = match enroll {
        Loaded::Loading => div()
            .py(px(40.))
            .flex()
            .justify_center()
            .text_color(c.muted_foreground)
            .child(Spinner::regular().render(cx))
            .into_any_element(),
        Loaded::Failed(error) => div().text_size(px(13.)).text_color(c.destructive).child(error).into_any_element(),
        Loaded::Ready(enroll) => enroll_steps(store, &enroll, replaces, cx),
    };
    let done = ActionButton::new("add-gateway-close", "Cancel").on_click({
        let close = close.clone();
        move |_, window, cx| close(window, cx)
    });
    let content = div()
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .text_color(c.foreground)
        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
        .child(body)
        .child(div().flex().justify_end().child(done));
    modal("add-gateway", div().id("add-gateway-scroll").overflow_y_scroll().child(content), close).width(540.)
}

fn enroll_steps(store: &Entity<Store>, enroll: &Enroll, replaces: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    let steps = enroll.steps.iter().enumerate().map(|(index, step)| {
        div()
            .flex()
            .gap(px(8.))
            .text_size(px(13.))
            .line_height(px(18.))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(16.))
                    .text_color(c.muted_stronger_foreground)
                    .child(format!("{}.", index + 1)),
            )
            .child(div().flex_1().min_w_0().text_color(c.muted_foreground).child(step.clone()))
    });
    let at = now();
    let expired = enroll.expires_at <= at;
    let seconds = (enroll.expires_at - at).ceil().max(0.) as u64;
    let status = if expired {
        let store = store.clone();
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().text_size(px(13.)).text_color(c.muted_foreground).child("The code has expired"))
            .child(ActionButton::new("new-enroll-code", "New Code").on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.open_add_gateway();
                    cx.notify();
                })
            }))
    } else {
        let waiting = if replaces { "Waiting for your new gateway" } else { "Waiting for your gateway" };
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_color(c.muted_foreground)
            .child(Spinner::regular().render(cx))
            .child(div().text_size(px(13.)).child(waiting))
            .child(
                div()
                    .text_size(px(12.))
                    .font_features(crate::ui::tabular_numbers())
                    .text_color(c.muted_stronger_foreground)
                    .child(format!("{}:{:02}", seconds / 60, seconds % 60)),
            )
    };
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(div().flex().flex_col().gap(px(6.)).children(steps))
        .child(CommandBox::new("gateway-command", Some(enroll.command.clone())).spans(enroll.spans.clone()))
        .child(status)
        .into_any_element()
}

/// A settings control: outline, large, on the card.
pub(super) fn outline(button: ActionButton) -> ActionButton {
    button.variant(Variant::Outline).size(ControlSize::Large).surface(Surface::Card)
}

pub(super) fn large(id: impl Into<ElementId>, title: impl Into<SharedString>) -> ActionButton {
    outline(ActionButton::new(id, title))
}

/// Signs an account in on the gateway: Claude's page shows a code to paste here, Codex's takes one
/// shown here.
pub struct SignInSheet {
    store: Entity<Store>,
    code: Entity<InputState>,
    pub leaving: bool,
    _subscription: Subscription,
}

impl SignInSheet {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Paste the code"));
        let subscription = cx.subscribe(&code, |_, _, _: &InputEvent, cx| cx.notify());
        code.update(cx, |code, cx| code.focus(window, cx));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, code, leaving: false, _subscription: subscription }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.cancel_gateway_sign_in();
            cx.notify();
        });
    }

    fn finish(&self, cx: &mut App) {
        let code = self.code.read(cx).value().trim().to_string();
        let waiting =
            self.store.read(cx).gateway_sign_in().is_none_or(|sign_in| sign_in.waiting || !sign_in.needs_code);
        if code.is_empty() || waiting {
            return;
        }
        self.store.update(cx, |store, cx| {
            store.finish_gateway_sign_in(code);
            cx.notify();
        });
    }
}

impl Render for SignInSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let (agent, again, local_error) =
            store.gateway.signing_in.as_ref().map_or((Agent::Claude, None, None), |signing_in| {
                (signing_in.agent, signing_in.again.clone(), signing_in.error.clone())
            });
        let sign_in = store.gateway_sign_in().cloned();
        let title = match (&sign_in, &again) {
            (Some(sign_in), _) => sign_in.title.clone(),
            (None, Some(_)) => format!("Sign In to {} Again", agent_name(agent)),
            (None, None) => format!("Sign In to {}", agent_name(agent)),
        };
        let account = sign_in.as_ref().and_then(|sign_in| sign_in.account.clone());
        let error = sign_in.as_ref().and_then(|sign_in| sign_in.error.clone()).or(local_error);
        let mut body: Vec<AnyElement> = Vec::new();
        match &sign_in {
            None if error.is_none() => body.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(c.muted_foreground)
                    .child(Spinner::regular().render(cx))
                    .child("Starting the sign-in on your gateway")
                    .into_any_element(),
            ),
            None => {}
            Some(sign_in) => {
                body.push(
                    div()
                        .text_size(px(13.))
                        .text_color(c.muted_foreground)
                        .child(sign_in.text.clone())
                        .into_any_element(),
                );
                body.push(
                    CommandBox::new("gateway-sign-in-link", Some(sign_in.url.clone()))
                        .link(sign_in.url.clone())
                        .into_any_element(),
                );
                if let Some(user_code) = sign_in.user_code.clone() {
                    let copied = user_code.clone();
                    body.push(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(10.))
                            .py(px(6.))
                            .child(
                                div()
                                    .text_size(px(CODE_SIZE))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .font_family(crate::theme::MONO_FONT)
                                    .child(user_code),
                            )
                            .child(CopyButton::new("copy-user-code", "Copy the code", move |_, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                            }))
                            .into_any_element(),
                    );
                }
                if sign_in.needs_code {
                    let can_finish = !self.code.read(cx).value().trim().is_empty() && !sign_in.waiting;
                    let finish = ActionButton::new("gateway-finish-sign-in", "Sign In")
                        .primary()
                        .pending(sign_in.waiting)
                        .disabled(!can_finish && !sign_in.waiting)
                        .on_click(cx.listener(|this, _, _, cx| this.finish(cx)));
                    body.push(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().flex_1().min_w_0().child(InputField::new(&self.code).monospaced(true)))
                            .child(finish)
                            .into_any_element(),
                    );
                } else if sign_in.waiting && error.is_none() {
                    body.push(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(13.))
                            .text_color(c.muted_foreground)
                            .child(Spinner::regular().render(cx))
                            .child("Waiting for you to approve it")
                            .into_any_element(),
                    );
                }
            }
        }
        let start_again = error.is_some() && sign_in.as_ref().is_none_or(|sign_in| !sign_in.needs_code);
        if let Some(error) = error {
            body.push(div().text_size(px(12.)).text_color(c.destructive).child(error).into_any_element());
        }
        let store = self.store.clone();
        let footer = div()
            .flex()
            .justify_end()
            .child(
                ActionButton::new("gateway-sign-in-cancel", "Cancel")
                    .margin(beside(0., if start_again { 4. } else { 0. }))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .when(start_again, |footer| {
                footer.child(
                    ActionButton::new("gateway-sign-in-again", "Start Again")
                        .primary()
                        .margin(beside(4., 0.))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                store.start_gateway_sign_in(agent, again.clone());
                                cx.notify();
                            })
                        }),
                )
            });
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .children(
                        account.map(|account| div().text_size(px(13.)).text_color(c.muted_foreground).child(account)),
                    ),
            )
            .children(body)
            .child(footer);
        let (close, finish) = (cx.weak_entity(), cx.weak_entity());
        modal("gateway-sign-in", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            finish.update(cx, |this, cx| this.finish(cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}

/// An account's new name.
pub struct RenameSheet {
    store: Entity<Store>,
    page: WeakEntity<AgentGatewayPage>,
    account: String,
    name: Entity<InputState>,
    pub leaving: bool,
    _subscription: Subscription,
}

impl RenameSheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<AgentGatewayPage>,
        account: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Work").default_value(name));
        let subscription = cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify());
        name.update(cx, |name, cx| name.focus(window, cx));
        Self { store, page, account, name, leaving: false, _subscription: subscription }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.rename = None;
            cx.notify();
        });
    }

    fn save(&self, cx: &mut App) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let account = self.account.clone();
        self.store.update(cx, |store, cx| {
            store.update_gateway_account(account, AccountChange::Name { name });
            cx.notify();
        });
        self.close(cx);
    }
}

impl Render for RenameSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let can_save = !self.name.read(cx).value().trim().is_empty();
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Rename Account"))
            .child(InputField::new(&self.name))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(
                        ActionButton::new("rename-cancel", "Cancel")
                            .margin(beside(0., 4.))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        ActionButton::new("rename-save", "Save")
                            .primary()
                            .disabled(!can_save)
                            .margin(beside(4., 0.))
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("gateway-rename", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(380.)
        .leaving(self.leaving)
    }
}

/// A captured request in full: its request or its response, each its headers then its body,
/// drawn a screen of lines at a time.
pub struct CapturedSheet {
    store: Entity<Store>,
    entry: i64,
    response: bool,
    request_view: Entity<CodeView>,
    response_view: Entity<CodeView>,
    pub leaving: bool,
}

impl CapturedSheet {
    fn new(store: Entity<Store>, entry: i64, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            entry,
            response: false,
            request_view: cx.new(CodeView::new),
            response_view: cx.new(CodeView::new),
            leaving: false,
        }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.close_gateway_captured();
            cx.notify();
        });
    }

    fn document(&self, document: &Arc<CodeDocument>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = if self.response { self.response_view.clone() } else { self.request_view.clone() };
        let (collapsed, marks, media) = (Rc::new(HashSet::new()), CodeMarks::default(), HashMap::new());
        view.update(cx, |view, cx| {
            view.element(
                CodeSnapshot { document, collapsed: &collapsed, reveal: None, marks: &marks, media: &media },
                CodeActions::default(),
                window,
                cx,
            )
        })
    }

    fn summary(row: &LogRow, cx: &App) -> Div {
        let c = colors(cx);
        let parts: Vec<String> = [
            Some(row.time.clone()),
            Some(row.server.clone()),
            Some(row.thread.clone()),
            Some(format!("{} {}", row.status, row.path)),
            Some(row.latency.clone()),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .text_size(px(12.))
            .text_color(c.muted_foreground)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(crate::ui::control::dotted(parts.join(" · "), c))
    }
}

impl Render for CapturedSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let size = crate::window_frame::content_size(window);
        let height = (f32::from(size.height) - 260.).clamp(200., 640.);
        let captured = self.store.read(cx).gateway.captured.as_ref().map(|(_, captured)| match captured {
            Loaded::Loading => Loaded::Loading,
            Loaded::Failed(error) => Loaded::Failed(error.clone()),
            Loaded::Ready(None) => Loaded::Ready(None),
            Loaded::Ready(Some(Captured { row, request, response })) => {
                Loaded::Ready(Some((row.clone(), request.clone(), response.clone())))
            }
        });
        let message = |text: String, color: Hsla| {
            div()
                .h(px(height))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(color)
                .child(text)
                .into_any_element()
        };
        let (summary, body) = match captured {
            None | Some(Loaded::Loading) => (
                None,
                div()
                    .h(px(height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c.muted_foreground)
                    .child(Spinner::regular().render(cx))
                    .into_any_element(),
            ),
            Some(Loaded::Failed(error)) => (None, message(error, c.destructive)),
            Some(Loaded::Ready(None)) => {
                (None, message("Your gateway no longer has this request".into(), c.muted_foreground))
            }
            Some(Loaded::Ready(Some((row, request, response)))) => {
                let document = if self.response { response } else { request };
                let code = self.document(&document, window, cx);
                let body = div()
                    .h(px(height))
                    .rounded(px(crate::theme::Radius::MD))
                    .border_1()
                    .border_color(c.border)
                    .overflow_hidden()
                    .child(code)
                    .into_any_element();
                (Some(Self::summary(&row, cx)), body)
            }
        };
        let sheet = cx.weak_entity();
        let tabs =
            Segmented::new("captured-part", vec!["Request".into(), "Response".into()], usize::from(self.response))
                .on_select(move |index, _, cx| {
                    let _ = sheet.update(cx, |this, cx| {
                        this.response = index == 1;
                        cx.notify();
                    });
                });
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_color(c.foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Captured Request"),
                    )
                    .child(tabs),
            )
            .children(summary)
            .child(body)
            .child(
                div().flex().justify_end().child(
                    ActionButton::new("captured-done", "Done")
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                ),
            );
        let close = cx.weak_entity();
        modal("gateway-captured", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .width(820.)
        .leaving(self.leaving)
    }
}
