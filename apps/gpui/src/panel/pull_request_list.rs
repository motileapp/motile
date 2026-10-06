//! The repository's pull requests, the last updated first, to open one in a tab of its own or link
//! it to the thread.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::PullRequestState;

use super::pull_request::{
    label, line_counts, link_field, notice_bar, panel_bar, panel_loading, panel_message, parse_number,
};
use super::state::{Loaded, PanelTarget};
use crate::models::ago;
use crate::store::Store;
use crate::store::pull_requests::{Row, Tone};
use crate::theme::colors;
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, ActionMenu, Chip, InputField, InputVariant, edges, highlight, icons};

const STATES: [(&str, &str, PullRequestState); 4] = [
    ("open", "Open", PullRequestState::Open),
    ("merged", "Merged", PullRequestState::Merged),
    ("closed", "Closed", PullRequestState::Closed),
    ("all", "All", PullRequestState::All),
];

#[derive(Clone, PartialEq, Debug)]
struct Trigger {
    target: PanelTarget,
    state: &'static str,
    version: u64,
    asked: u64,
}

pub struct PullRequestListView {
    store: Entity<Store>,
    /// Which pull requests are listed: "open", "merged", "closed" or "all".
    state: &'static str,
    search: Entity<InputState>,
    linking: Entity<InputState>,
    asked: u64,
    trigger: Option<Trigger>,
}

impl PullRequestListView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let saved = store.read(cx).prefs.string("pullRequests.state").unwrap_or_default();
        let state = STATES.iter().find(|(key, _, _)| *key == saved).map_or("open", |(key, _, _)| key);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let linking = cx.new(|cx| InputState::new(window, cx).placeholder("Link a PR by number or address"));
        cx.subscribe_in(&linking, window, |this, linking, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } => {
                let Some(number) = parse_number(linking.read(cx).value().as_ref()) else { return };
                this.link(number, cx);
                linking.update(cx, |state, cx| state.set_value("", window, cx));
            }
            InputEvent::Change => cx.notify(),
            _ => {}
        })
        .detach();
        Self { store, state, search, linking, asked: 0, trigger: None }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.asked += 1;
        cx.notify();
    }

    fn choose_state(&mut self, state: &'static str, cx: &mut Context<Self>) {
        self.state = state;
        self.store.update(cx, |store, _| store.prefs.set("pullRequests.state", state));
        cx.notify();
    }

    fn link(&mut self, number: u64, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            let Some(thread) = store.selected_thread() else { return };
            let (thread_id, server_id) = (thread.id.clone(), thread.server_id.clone());
            store.link_pull_request(Some(number), thread_id, &server_id);
            cx.notify();
        });
    }

    /// Asks the server for the list when what it shows may have changed.
    fn follow(&mut self, trigger: Trigger, cx: &mut Context<Self>) {
        if self.trigger.as_ref() == Some(&trigger) {
            return;
        }
        self.trigger = Some(trigger.clone());
        let store = self.store.clone();
        let state =
            STATES.iter().find(|(key, _, _)| *key == trigger.state).map_or(PullRequestState::Open, |found| found.2);
        cx.defer(move |cx| {
            store.update(cx, |store, cx| {
                store.load_pull_requests(&trigger.target, state);
                cx.notify();
            });
        });
    }

    fn filtered<'a>(&self, rows: &'a [Row], cx: &App) -> Vec<&'a Row> {
        let words = self.search.read(cx).value().trim().to_lowercase();
        rows.iter()
            .filter(|row| {
                words.is_empty()
                    || format!("{} {} {} {}", row.number, row.title, row.author, row.head)
                        .to_lowercase()
                        .contains(&words)
            })
            .collect()
    }

    fn bar(&self, shows_refresh: bool, cx: &mut Context<Self>) -> Div {
        let current = STATES.iter().find(|(key, _, _)| *key == self.state).map_or("Open", |(_, title, _)| title);
        let this = cx.entity().downgrade();
        let chosen = self.state;
        let refresh = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .w_full()
            .gap(px(4.))
            .child(
                ActionMenu::new("pr-list-state", current, move |_, _| {
                    let mut menu = Menu::new();
                    for (key, title, _) in STATES {
                        let this = this.clone();
                        menu = menu.checked(key == chosen, title, move |_, cx| {
                            let _ = this.update(cx, |view, cx| view.choose_state(key, cx));
                        });
                    }
                    menu
                })
                .button(|button| button.help("Which pull requests to show").margin(edges(0., -8., 0., 0.))),
            )
            .child(
                div().max_w(px(220.)).flex_1().child(
                    InputField::new(&self.search).icon("search").variant(InputVariant::Outlined).clearable(true),
                ),
            )
            .child(div().flex_1().min_w(px(4.)))
            .when(shows_refresh, |bar| {
                bar.child(ActionButton::icon("pr-list-refresh", "rotate-cw", "Read the pull requests again").on_click(
                    move |_, _, cx| {
                        let _ = refresh.update(cx, |view, cx| view.refresh(cx));
                    },
                ))
            })
    }

    fn rows(&self, rows: &[Row], target: &PanelTarget, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let thread = store.selected_thread().map(|thread| (thread.id.clone(), thread.server_id.clone()));
        let linked = store.selected_thread().and_then(|thread| thread.pull_request.as_ref()).map(|found| found.number);
        let shown = self.filtered(rows, cx);
        let search = self.search.read(cx).value().trim().to_string();
        let empty = if search.is_empty() {
            format!("No {}pull requests.", if self.state == "all" { String::new() } else { format!("{} ", self.state) })
        } else {
            format!("None match “{search}”.")
        };
        let this = cx.entity().downgrade();
        div()
            .id("pr-list")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(2.))
                    .when(thread.is_some(), |column| {
                        column.child(div().w_full().max_w(px(380.)).px(px(6.)).pb(px(8.)).child(link_field(
                            "pr-list-link",
                            &self.linking,
                            move |number, _, cx| {
                                let _ = this.update(cx, |view, cx| view.link(number, cx));
                            },
                            cx,
                        )))
                    })
                    .when(shown.is_empty(), |column| {
                        column.child(div().w_full().pt(px(40.)).flex().justify_center().child(label(
                            empty,
                            12.5,
                            c.secondary,
                        )))
                    })
                    .children(
                        shown
                            .into_iter()
                            .map(|row| self.row(row, target, linked == Some(row.number), thread.clone(), cx)),
                    ),
            )
            .into_any_element()
    }

    fn row(
        &self,
        row: &Row,
        target: &PanelTarget,
        linked: bool,
        thread: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = colors(cx);
        let group: SharedString = format!("pr-row-{}", row.number).into();
        let (open, number, open_target) = (self.store.clone(), row.number, target.clone());
        let (menu_store, url) = (self.store.clone(), row.url.clone());
        let signals = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .when_some(row.review.as_ref(), |row_signals, (tone, text)| {
                let symbol = match tone {
                    Tone::Success => "circle-check",
                    Tone::Danger => "circle-alert",
                    _ => "eye",
                };
                row_signals.child(
                    div()
                        .id(("pr-row-review", row.number as usize))
                        .child(icons::symbol(symbol, 11.).text_color(tone.color(c)))
                        .tooltip(crate::ui::tooltip(text.clone())),
                )
            })
            .when_some(row.checks, |row_signals, checks| {
                row_signals.child(
                    div()
                        .id(("pr-row-checks", row.number as usize))
                        .size(px(7.))
                        .rounded_full()
                        .bg(checks.color(c))
                        .tooltip(crate::ui::tooltip(row.checks_label.clone().unwrap_or_default())),
                )
            })
            .children(line_counts(row.additions, row.deletions, cx));
        let subline =
            format!("#{} · {} · {} → {} · {}", row.number, row.author, row.head, row.base, ago(row.updated_at));
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .w_full()
            .px(px(10.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(10.))
            .child(highlight(group, 7., edges(0., 0., 0., 0.), false, cx))
            .child(
                div()
                    .relative()
                    .w(px(16.))
                    .h(px(18.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol(row.state.symbol(), 13.).text_color(row.state.color(c))),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(px(6.))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c.text)
                                    .line_clamp(2)
                                    .child(row.title.clone()),
                            )
                            .when(linked, |title| title.child(Chip::new("This thread").tone(c.link)))
                            .child(div().flex_1().min_w(px(4.)))
                            .child(signals),
                    )
                    .child(div().text_size(px(11.5)).text_color(c.tertiary).truncate().child(subline)),
            )
            .on_click(move |_, _, cx| {
                open.update(cx, |store, cx| {
                    store.show_pull_request_tab(number, &open_target);
                    cx.notify();
                })
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let (link_store, thread) = (menu_store.clone(), thread.clone());
                let (open_url, copy_url) = (url.clone(), url.clone());
                Menu::new()
                    .when_some(thread.filter(|_| !linked), |menu, (thread_id, server_id)| {
                        menu.item("Link to This Thread", move |_, cx| {
                            link_store.update(cx, |store, cx| {
                                store.link_pull_request(Some(number), thread_id.clone(), &server_id);
                                cx.notify();
                            })
                        })
                    })
                    .when(!url.is_empty(), |menu| {
                        menu.item("Open on GitHub", move |_, cx| cx.open_url(&open_url))
                            .item("Copy Link", move |_, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()))
                            })
                    })
                    .show(event.position, window, cx);
            })
    }
}

impl Render for PullRequestListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (target, unavailable, extended, version, notice) = {
            let store = self.store.read(cx);
            (
                store.panel_target(),
                store.pull_requests_unavailable(),
                store.pull_requests_extended(),
                store.workspace_version,
                store.pull_requests.notice.clone(),
            )
        };
        let listable = unavailable.is_none() && extended;
        if let (Some(target), true) = (&target, listable) {
            self.follow(Trigger { target: target.clone(), state: self.state, version, asked: self.asked }, cx);
        }
        let bar = panel_bar(self.bar(listable, cx), cx);
        let body: AnyElement = match (&unavailable, extended, &target) {
            (Some(reason), _, _) => panel_message(reason.clone(), false, cx),
            (None, false, _) => panel_message("Update your server to see its pull requests here.", false, cx),
            (None, true, Some(target)) => {
                let list = match &self.store.read(cx).pull_requests.list {
                    Loaded::Loading => None,
                    Loaded::Failed(message) => Some(Err(message.clone())),
                    Loaded::Ready(rows) => Some(Ok(rows.clone())),
                };
                match list {
                    None => panel_loading(cx),
                    Some(Err(message)) => panel_message(message, true, cx),
                    Some(Ok(rows)) => self.rows(&rows, target, cx),
                }
            }
            _ => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar)
            .when_some(notice, |column, notice| column.child(notice_bar(&notice, &self.store, cx)))
            .child(div().flex_1().min_h_0().child(body))
    }
}
