//! Settings → MCP Gateway: the MCP servers the account's gateway signs in to once for every agent, a
//! card each with its tools, and the calls agents made with them, with the sheets that add an MCP
//! server, rename one, change its key, try a tool and show a call.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_core::gateway::{Reach, View};
use motile_core::mcp_servers::{Known, McpServerRow, McpServerState, ToolGroup, ToolRow, agent_name, is_address};
use motile_protocol::gateway_wire::{McpAuth, McpChange, ToolKind, ToolPermission, mcp_slug};
use serde_json::{Map, Value};

use super::agent_gateway::{GatewayHeader, MCP_CAPTION, large};
use super::layout::{Cut, INSET, TRAILING_GAP, group, label, more, row};
use crate::panel::code_view::{CodeActions, CodeSnapshot, CodeView};
use crate::panel::state::{CodeDocument, CodeMarks, Loaded};
use crate::sign_in::Session;
use crate::store::Store;
use crate::store::mcp_servers::{Answer, CallRow, Field, FieldKind, MCP_REDIRECT};
use crate::theme::{ControlSize, IconSize, Layer, Opacity, Radius, Surface, TextSize, colors};
use crate::ui::menu::{Menu, MenuIcon};
use crate::ui::modal::{Answer as Choice, Confirmation, confirmation, modal, present_sheet, presented};
use crate::ui::parts::FadingScroll;
use crate::ui::{
    ActionButton, ActionMenu, Chip, Highlight, InputField, Segmented, Spinner, Switch, TextArea, Variant, beside,
    card_default, divider_on, edges, icons, text_area_state,
};

const NONE_DETAIL: &str = "Add Linear, GitHub, Notion or any MCP by its address.";
const TILE: f32 = 28.;
const SMALL_TILE: f32 = 16.;
/// How tall the activity grows before it scrolls.
const ACTIVITY_HEIGHT: f32 = 420.;
const GROUP_HEIGHT: f32 = 44.;
/// What agents may do with a tool, as its dropdown and its row name it.
const PERMISSIONS: [(ToolPermission, &str, &str); 3] = [
    (ToolPermission::Allow, "Always allow", "circle-check"),
    (ToolPermission::Ask, "Needs approval", "hand"),
    (ToolPermission::Block, "Blocked", "ban"),
];
/// How far a modal stands from the window's edges.
const MODAL_MARGIN: f32 = 32.;

fn named(prefix: &str, key: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{prefix}-{key}").into())
}

/// An MCP server's letter on its colour's wash.
fn tile(monogram: &str, tint: &str, side: f32, cx: &App) -> Div {
    let c = colors(cx);
    let color = c.named(tint).unwrap_or(c.muted_foreground);
    let radius = if side >= TILE { Radius::SM } else { Radius::XS };
    div()
        .flex_shrink_0()
        .size(px(side))
        .rounded(px(radius))
        .bg(c.wash(color, false))
        .flex()
        .items_center()
        .justify_center()
        .text_size(TextSize::nearest(side * 0.46))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(monogram.to_string())
}

/// What the page shows of the account's MCP servers, or why it can't.
fn listed(view: Option<&View>) -> Option<&Vec<McpServerRow>> {
    let view = view.filter(|view| view.exists && view.connection == Reach::Connected)?;
    view.mcp_servers.as_ref()
}

fn find<'a>(view: Option<&'a View>, id: &str) -> Option<&'a McpServerRow> {
    listed(view)?.iter().find(|mcp_server| mcp_server.id == id)
}

/// The tool its agents' names are shown with: one that writes, or the first.
fn example_tool(mcp_server: &McpServerRow) -> String {
    let writes = mcp_server.tools.iter().find(|tool| tool.kind == ToolKind::Write);
    writes.or(mcp_server.tools.first()).map_or_else(|| "tool".to_string(), |tool| tool.name.clone())
}

/// A permission's label and symbol; none is the group's tools differing, "Custom".
fn permission_look(permission: Option<ToolPermission>) -> (&'static str, &'static str) {
    let found = PERMISSIONS.iter().find(|(each, _, _)| Some(*each) == permission);
    found.map_or(("Custom", "ellipsis"), |(_, title, symbol)| (*title, *symbol))
}

fn update(store: &Entity<Store>, id: &str, change: McpChange, cx: &mut App) {
    let id = id.to_string();
    store.update(cx, |store, cx| {
        store.update_mcp_server(id, change);
        cx.notify();
    })
}

/// "Linear" for `https://mcp.linear.app/mcp`, until the gateway names it; an address by itself.
fn host_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority.rsplit_once(':').map_or(authority, |(host, _)| host);
    if !host.chars().any(char::is_alphabetic) || host == "localhost" {
        return host.to_string();
    }
    let labels: Vec<&str> = host.split('.').collect();
    let named = labels[..labels.len().saturating_sub(1)]
        .iter()
        .find(|label| !["mcp", "api", "www", "server", "app"].contains(label))
        .or(labels.first())
        .copied()
        .unwrap_or(host);
    let mut letters = named.chars();
    letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
}

fn text_of(input: &Entity<InputState>, cx: &App) -> String {
    input.read(cx).value().trim().to_string()
}

fn sheet_title(title: impl Into<SharedString>) -> Div {
    div().text_size(TextSize::Lg).font_weight(FontWeight::SEMIBOLD).child(title.into())
}

fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_size(TextSize::Sm).text_color(colors(cx).muted_foreground).child(text.into())
}

fn field_label(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_size(TextSize::Xs).text_color(colors(cx).muted_foreground).child(text.into())
}

fn warning_line(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_size(TextSize::Xs).text_color(colors(cx).warning).child(text.into())
}

/// A name field, and under it how agents will name the MCP server's tools once it is called that.
fn name_field(name: &Entity<InputState>, tool: Option<&str>, fallback: Option<&str>, cx: &App) -> Div {
    let typed = text_of(name, cx);
    let slug = if typed.is_empty() { fallback.map(String::from) } else { Some(mcp_slug(&typed)) };
    div().flex().flex_col().gap(px(6.)).child(field_label("Name", cx)).child(InputField::new(name)).when_some(
        slug,
        |field, slug| {
            field.child(
                div()
                    .text_size(TextSize::Xs)
                    .font_family(crate::theme::MONO_FONT)
                    .text_color(colors(cx).muted_foreground)
                    .truncate()
                    .child(agent_name(&slug, tool.unwrap_or("…"))),
            )
        },
    )
}

/// A box of a document's lines, which can be selected and scrolled.
fn code_box(
    view: &Entity<CodeView>,
    document: &Arc<CodeDocument>,
    height: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (collapsed, marks, media) = (Rc::new(HashSet::new()), CodeMarks::default(), HashMap::new());
    let code = view.update(cx, |view, cx| {
        view.element(
            CodeSnapshot { document, collapsed: &collapsed, reveal: None, marks: &marks, media: &media },
            CodeActions::default(),
            window,
            cx,
        )
    });
    div()
        .h(px(height))
        .rounded(px(Radius::MD))
        .border_1()
        .border_color(colors(cx).border)
        .overflow_hidden()
        .child(code)
        .into_any_element()
}

fn spinner_box(height: f32, cx: &App) -> AnyElement {
    div()
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .text_color(colors(cx).muted_foreground)
        .child(Spinner::regular().render(cx))
        .into_any_element()
}

/// The page's content in a scroll as tall as the window lets a modal be.
fn scrolling(id: &'static str, content: Div, window: &Window) -> Stateful<Div> {
    let height = f32::from(crate::window_frame::content_size(window).height) - 2. * MODAL_MARGIN;
    div().id(id).max_h(px(height.max(200.))).overflow_y_scroll().child(content)
}

/// A tool as its row shows it: its title, and what it does under it.
fn tool_label(tool: &ToolRow, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().min_w_0().truncate().text_size(TextSize::Sm).child(tool.title.clone()))
        .when_some(tool.summary.clone(), |label, summary| {
            label.child(div().text_size(TextSize::Xs).text_color(c.muted_foreground).truncate().child(summary))
        })
        .into_any_element()
}

/// The dropdown that sets what agents may do with every tool of `group`, which shows "Custom"
/// while they differ.
fn group_permission(store: &Entity<Store>, mcp_server: &str, group: &ToolGroup) -> ActionMenu {
    let (title, symbol) = permission_look(group.permission);
    let (store, id, kind, current) = (store.clone(), mcp_server.to_string(), group.kind, group.permission);
    let key = format!("{id}-{kind:?}");
    ActionMenu::new(named("mcp-server-group-permission", key), title, move |_, _| {
        PERMISSIONS.iter().fold(Menu::new(), |menu, (permission, title, symbol)| {
            let (store, id, permission) = (store.clone(), id.clone(), *permission);
            menu.choice(current == Some(permission), *title, Some(MenuIcon::Symbol(symbol)), None, move |_, cx| {
                update(&store, &id, McpChange::GroupPermission { kind, permission }, cx)
            })
        })
    })
    .button(|button| button.outline().symbol(symbol))
}

/// A group's row: its title, how many tools it has and its dropdown, which unfolds its tools when
/// it is given `fold`, whether it is open.
fn group_row(
    store: &Entity<Store>,
    mcp_server: &str,
    group: &ToolGroup,
    fold: Option<bool>,
    surface: Surface,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let name = SharedString::from(format!("mcp-server-group-{mcp_server}-{:?}", group.kind));
    let dropdown = div().block_mouse_except_scroll().child(group_permission(store, mcp_server, group));
    let row = div()
        .id(ElementId::Name(name.clone()))
        .group(name.clone())
        .relative()
        .px(px(INSET))
        .h(px(GROUP_HEIGHT))
        .flex()
        .items_center()
        .gap(px(8.))
        .when_some(fold, |row, open| {
            row.cursor_default()
                .child(
                    Highlight::new().surface(surface).radius(Radius::MD).inset(edges(3., 4., 3., 4.)).render(name, cx),
                )
                .child(
                    div()
                        .text_color(c.muted_foreground)
                        .child(icons::symbol("chevron-right", IconSize::Md).rotate(if open { 90. } else { 0. })),
                )
        })
        .child(div().text_size(TextSize::Sm).child(group.title.clone()))
        .child(Chip::new(group.tools.len().to_string()).surface(surface).muted(true))
        .child(div().flex_1())
        .child(dropdown);
    if fold.is_none() {
        return row.into_any_element();
    }
    let (store, id, kind) = (store.clone(), mcp_server.to_string(), group.kind);
    row.on_click(move |_, _, cx| {
        store.update(cx, |store, cx| {
            store.toggle_mcp_server_tools(&id, kind);
            cx.notify();
        })
    })
    .into_any_element()
}

/// How far a group's tools stand in, under its title past its chevron.
const TOOL_INDENT: f32 = 24.;

/// A tool's row, with the three things agents may do with it to pick from.
fn tool_row(store: &Entity<Store>, mcp_server: &str, tool: &ToolRow, cx: &App) -> AnyElement {
    let options =
        PERMISSIONS.iter().map(|(_, title, symbol)| (SharedString::from(*title), SharedString::from(*symbol)));
    let selected = PERMISSIONS.iter().position(|(permission, _, _)| *permission == tool.permission).unwrap_or(0);
    let (store, id, name) = (store.clone(), mcp_server.to_string(), tool.name.clone());
    let picker =
        Segmented::symbols(named(&format!("mcp-server-tool-{mcp_server}"), &tool.name), options.collect(), selected)
            .size(ControlSize::Small)
            .on_select(move |index, _, cx| {
                let Some((permission, _, _)) = PERMISSIONS.get(index) else { return };
                update(&store, &id, McpChange::ToolPermission { name: name.clone(), permission: *permission }, cx)
            });
    row(vec![tool_label(tool, cx)], vec![picker.into_any_element()]).pl(px(INSET + TOOL_INDENT)).into_any_element()
}

/// What the page takes away once the user says so.
#[derive(Clone)]
struct Removal {
    id: String,
    name: String,
}

pub struct McpGatewayPage {
    store: Entity<Store>,
    header: Entity<GatewayHeader>,
    removal: Option<Removal>,
    add: Option<Entity<AddSheet>>,
    rename: Option<Entity<RenameSheet>>,
    key: Option<Entity<KeySheet>>,
    trying: Option<Entity<TrySheet>>,
    call: Option<Entity<CallSheet>>,
    _subscription: Subscription,
}

impl McpGatewayPage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        let header = cx.new(|cx| GatewayHeader::new(store.clone(), MCP_CAPTION, cx));
        Self {
            store,
            header,
            removal: None,
            add: None,
            rename: None,
            key: None,
            trying: None,
            call: None,
            _subscription: subscription,
        }
    }

    /// The gateway's MCP servers, or why it can't list them: it isn't connected, or is too old.
    fn mcp_servers(&self, view: &View, cx: &mut Context<Self>) -> AnyElement {
        let Some(mcp_servers) = listed(Some(view)) else {
            let (title, description) = match view.connection {
                Reach::Connected => (
                    "Update your gateway to use MCP servers".to_string(),
                    view.version.as_ref().map(|version| format!("Your gateway runs version {version}")),
                ),
                _ => (view.status_text.clone(), view.error.clone()),
            };
            let leading = label(title, description, Some("waypoints"), Cut::Wraps, cx);
            let spinner = (view.connection == Reach::Connecting).then(|| {
                div().text_color(colors(cx).muted_foreground).child(Spinner::regular().render(cx)).into_any_element()
            });
            let rows = vec![row(vec![leading], spinner.into_iter().collect()).into_any_element()];
            return group("mcp-servers", "MCP Servers", None, true, rows, cx);
        };
        let page = cx.weak_entity();
        let add = large("add-mcp-server", "Add MCP Server").symbol("plus").on_click(move |_, window, cx| {
            let _ = page.update(cx, |page, cx| page.open_add(window, cx));
        });
        let mut cards = vec![div().flex().child(add).into_any_element()];
        if mcp_servers.is_empty() {
            let leading = label("No MCP servers yet", Some(NONE_DETAIL.into()), Some("plug"), Cut::Wraps, cx);
            cards.push(card_default(row(vec![leading], Vec::new()), cx).into_any_element());
        }
        cards.extend(mcp_servers.iter().map(|mcp_server| self.card(mcp_server, cx)));
        group("mcp-servers", "MCP Servers", None, false, cards, cx)
    }

    fn card(&self, mcp_server: &McpServerRow, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let off = !mcp_server.enabled;
        let dim = move |part: Div| part.when(off, |part| part.opacity(c.opacity(Opacity::Disabled)));
        let id = mcp_server.id.clone();
        let store = self.store.clone();
        let signs_in_again = mcp_server.state == McpServerState::SignInAgain;
        let sign_in_again = signs_in_again.then(|| {
            let (store, id) = (self.store.clone(), id.clone());
            let starting = self.store.read(cx).mcp_servers.starting_sign_in.as_ref() == Some(&mcp_server.id);
            large(named("mcp-server-sign-in", &id), "Sign In Again")
                .pending(starting)
                .margin(beside(0., TRAILING_GAP / 2.))
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    store.update(cx, |store, cx| {
                        store.sign_mcp_server_in_again(id);
                        cx.notify();
                    })
                })
        });
        let switch = Switch::new(named("mcp-server-enabled", &id), mcp_server.enabled)
            .margin(beside(if signs_in_again { TRAILING_GAP / 2. } else { 0. }, TRAILING_GAP / 2.))
            .on_change({
                let id = id.clone();
                move |enabled, _, cx| {
                    let id = id.clone();
                    store.update(cx, |store, cx| {
                        store.update_mcp_server(id, McpChange::Enabled { enabled });
                        cx.notify();
                    })
                }
            });
        let menu = self.menu(mcp_server, cx).margin(beside(TRAILING_GAP / 2., 0.));
        let trailing = div().flex().items_center().children(sign_in_again).child(switch).child(menu);
        let warns = matches!(mcp_server.state, McpServerState::SignInAgain | McpServerState::Failing);
        let label = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(4.))
            .child(div().text_size(TextSize::Sm).font_weight(FontWeight::MEDIUM).child(mcp_server.name.clone()))
            .child(
                div()
                    .text_size(TextSize::Xs)
                    .text_color(if warns { c.warning } else { c.muted_foreground })
                    .child(mcp_server.status_text.clone()),
            );
        let leading = vec![
            dim(tile(&mcp_server.monogram, &mcp_server.tint, TILE, cx)).into_any_element(),
            dim(label).into_any_element(),
        ];
        let mut rows = vec![row(leading, vec![trailing.into_any_element()]).into_any_element()];
        let unfolded = &self.store.read(cx).mcp_servers.unfolded;
        for tools in &mcp_server.groups {
            let open = unfolded.contains(&(mcp_server.id.clone(), tools.kind));
            let mut part = div().flex().flex_col().child(divider_on(Surface::Card, cx)).child(group_row(
                &self.store,
                &id,
                tools,
                Some(open),
                Surface::Card,
                cx,
            ));
            if open {
                for tool in &tools.tools {
                    part = part.child(divider_on(Surface::Card, cx)).child(tool_row(&self.store, &id, tool, cx));
                }
            }
            rows.push(dim(part).into_any_element());
        }
        card_default(div().flex().flex_col().overflow_hidden(), cx).children(rows).into_any_element()
    }

    fn menu(&self, mcp_server: &McpServerRow, cx: &mut Context<Self>) -> ActionMenu {
        let (page, store) = (cx.weak_entity(), self.store.clone());
        let mcp_server = mcp_server.clone();
        more(named("mcp-server-menu", &mcp_server.id), format!("What to do with {}", mcp_server.name), move |_, _| {
            let opens = |open: fn(&mut McpGatewayPage, &McpServerRow, &mut Window, &mut Context<McpGatewayPage>)| {
                let (page, mcp_server) = (page.clone(), mcp_server.clone());
                move |window: &mut Window, cx: &mut App| {
                    let _ = page.update(cx, |page, cx| open(page, &mcp_server, window, cx));
                }
            };
            let tells = |command: fn(&mut Store, String)| {
                let (store, id) = (store.clone(), mcp_server.id.clone());
                move |_: &mut Window, cx: &mut App| {
                    store.update(cx, |store, cx| {
                        command(store, id.clone());
                        cx.notify();
                    })
                }
            };
            let address = mcp_server.url.clone();
            let remove = {
                let (page, removal) =
                    (page.clone(), Removal { id: mcp_server.id.clone(), name: mcp_server.name.clone() });
                move |_: &mut Window, cx: &mut App| {
                    let removal = removal.clone();
                    let _ = page.update(cx, |page, cx| {
                        page.removal = Some(removal);
                        cx.notify();
                    });
                }
            };
            Menu::new()
                .when(!mcp_server.tools.is_empty(), |menu| {
                    menu.symbol_item("Try a Tool", "play", opens(McpGatewayPage::open_try))
                })
                .symbol_item("Rename", "pencil", opens(McpGatewayPage::open_rename))
                .when(mcp_server.auth == McpAuth::SignIn, |menu| {
                    menu.symbol_item("Sign In Again", "log-in", tells(Store::sign_mcp_server_in_again))
                })
                .when(mcp_server.auth == McpAuth::Key, |menu| {
                    menu.symbol_item("Change Key", "key", opens(McpGatewayPage::open_key))
                })
                .symbol_item("Ask for Its Tools Again", "refresh-cw", tells(Store::refresh_mcp_server))
                .symbol_item("Copy Address", "copy", move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(address.clone()))
                })
                .separator()
                .danger_symbol_item("Remove MCP Server", "trash-2", remove)
        })
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.add = Some(cx.new(|cx| AddSheet::new(store, page, window, cx)));
        cx.notify();
    }

    fn open_rename(&mut self, mcp_server: &McpServerRow, window: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.rename = Some(cx.new(|cx| RenameSheet::new(store, page, mcp_server, window, cx)));
        cx.notify();
    }

    fn open_key(&mut self, mcp_server: &McpServerRow, window: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.key = Some(cx.new(|cx| KeySheet::new(store, page, mcp_server, window, cx)));
        cx.notify();
    }

    fn open_try(&mut self, mcp_server: &McpServerRow, _: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.trying = Some(cx.new(|cx| TrySheet::new(store, page, mcp_server, cx)));
        cx.notify();
    }

    /// The calls agents made, newest first, all of them or the ones that failed.
    fn activity(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let calls = &self.store.read(cx).mcp_servers.calls;
        let (rows, more_rows, loading, error, errors_only) =
            (calls.rows.clone(), calls.more, calls.loading, calls.error.clone(), calls.errors_only);
        let store = self.store.clone();
        let filter =
            Segmented::new("mcp-server-calls-filter", vec!["All".into(), "Errors".into()], usize::from(errors_only))
                .on_select(move |index, _, cx| {
                    store.update(cx, |store, cx| {
                        store.show_mcp_server_errors_only(index == 1);
                        cx.notify();
                    })
                });
        let empty = match (&error, errors_only) {
            (Some(error), _) if rows.is_empty() => Some(error.clone()),
            _ if !rows.is_empty() || loading => None,
            (_, true) => Some("No errors".to_string()),
            (_, false) => Some("No calls yet".to_string()),
        };
        let mut list: Vec<AnyElement> = Vec::new();
        if let Some(empty) = empty {
            list.push(
                div()
                    .px(px(INSET))
                    .py(px(14.))
                    .text_size(TextSize::Xs)
                    .text_color(c.muted_foreground)
                    .child(empty)
                    .into_any_element(),
            );
        }
        if rows.is_empty() && loading {
            list.push(spinner_box(44., cx));
        }
        for (index, call) in rows.iter().enumerate() {
            if index > 0 {
                list.push(divider_on(Surface::Card, cx).into_any_element());
            }
            list.push(self.call_row(index, call, cx));
        }
        if more_rows {
            list.push(divider_on(Surface::Card, cx).into_any_element());
            list.push(self.more_rows(loading, cx));
        }
        let scroll =
            FadingScroll::new("mcp-server-calls", ACTIVITY_HEIGHT).holds_wheel(true).fill(c.card).children(list);
        let content = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().flex().child(filter))
            .child(card_default(div().flex().flex_col().overflow_hidden(), cx).child(scroll));
        group("mcp-activity", "Activity", None, false, vec![content.into_any_element()], cx)
    }

    fn call_row(&self, index: usize, call: &CallRow, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let store = self.store.clone();
        let id = call.id;
        div()
            .id(("mcp-server-call", index))
            .px(px(INSET))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .cursor_default()
            .hover(|row| row.bg(Surface::Card.layer(Layer::Row, c)))
            .child(call_line(call, cx))
            .when_some(call.error.clone(), |row, error| {
                row.child(
                    div().pl(px(SMALL_TILE + 8.)).text_size(TextSize::Xs).text_color(c.warning).truncate().child(error),
                )
            })
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.open_mcp_server_call(id);
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
                        store.load_mcp_server_calls(false);
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
        let (removal, leaving) = presented("mcp-server-removal", self.removal.clone(), window, cx)?;
        let store = self.store.clone();
        let id = removal.id.clone();
        let shown = Confirmation::new(format!("Remove the “{}” MCP Server?", removal.name))
            .message("Agents won't get its tools anymore, and your gateway forgets its login.")
            .answer(
                Choice::new("Remove", move |_, cx| {
                    let id = id.clone();
                    store.update(cx, |store, cx| {
                        store.remove_mcp_server(id);
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
        Some(confirmation("mcp-server-removal", shown, close, cx).leaving(leaving).into_any_element())
    }

    /// Keeps the call sheet open while the store has a call open.
    fn follow_call(&mut self, cx: &mut Context<Self>) {
        let call = self.store.read(cx).mcp_servers.call.as_ref().map(|(call, _)| *call);
        let shown = self.call.as_ref().map(|sheet| sheet.read(cx).call);
        if call == shown {
            return;
        }
        let store = self.store.clone();
        self.call = call.map(|call| cx.new(|cx| CallSheet::new(store, call, cx)));
    }
}

impl Render for McpGatewayPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_call(cx);
        let view = self.store.read(cx).gateway.view.clone();
        let mut groups = vec![self.header.clone().into_any_element()];
        if let Some(view) = view.as_ref().filter(|view| view.exists) {
            groups.push(self.mcp_servers(view, cx));
        }
        if listed(view.as_ref()).is_some_and(|mcp_servers| !mcp_servers.is_empty()) {
            groups.push(self.activity(cx));
        }
        let removal = self.removal_confirmation(window, cx);
        let add =
            present_sheet("mcp-server-add", self.add.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        let rename = present_sheet(
            "mcp-server-rename",
            self.rename.clone(),
            |sheet, leaving| sheet.leaving = leaving,
            window,
            cx,
        );
        let key =
            present_sheet("mcp-server-key", self.key.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        let trying =
            present_sheet("mcp-server-try", self.trying.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        let call =
            present_sheet("mcp-server-call", self.call.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(super::layout::GROUP_GAP))
            .children(groups)
            .children(removal)
            .children(add)
            .children(rename)
            .children(key)
            .children(trying)
            .children(call)
    }
}

/// A call's tile, `MCP server · tool`, where it came from, and how long it took and when.
fn call_line(call: &CallRow, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(tile(&call.monogram, &call.tint, SMALL_TILE, cx))
        .child(
            div()
                .flex_shrink_0()
                .max_w(px(260.))
                .flex()
                .items_center()
                .gap(px(4.))
                .overflow_hidden()
                .child(div().text_size(TextSize::Sm).font_weight(FontWeight::MEDIUM).child(call.mcp_server.clone()))
                .child(div().text_size(TextSize::Sm).text_color(c.muted_foreground).child("·"))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::Xs)
                        .font_family(crate::theme::MONO_FONT)
                        .child(call.tool.clone()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(TextSize::Xs)
                .text_color(c.muted_foreground)
                .child(call.source.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(TextSize::Xs)
                .font_features(crate::ui::tabular_numbers())
                .text_color(c.muted_foreground)
                .child(crate::ui::control::dotted(format!("{} · {}", call.latency, call.time), c)),
        )
        .when(!call.ok, |line| {
            line.child(div().flex_shrink_0().child(icons::symbol("triangle-alert", IconSize::Sm).text_color(c.warning)))
        })
}

/// Where the add sheet is.
enum Step {
    Pick,
    /// The browser signs it in; the session is gone once it came back and is being finished.
    SigningIn {
        name: String,
        session: Option<Session>,
    },
    Key {
        url: String,
        name: String,
    },
    Connected(String),
}

/// Adds an MCP server: picked or pasted, signed in or given a key, then named.
pub struct AddSheet {
    store: Entity<Store>,
    page: WeakEntity<McpGatewayPage>,
    step: Step,
    search: Entity<InputState>,
    key: Entity<InputState>,
    header: Entity<InputState>,
    /// The name of an MCP server added by its address.
    name: Entity<InputState>,
    /// The address and name being added.
    adding: Option<(String, Option<String>)>,
    /// The gateway hasn't answered yet.
    waiting: bool,
    error: Option<String>,
    /// Counts the sign-ins, so that only the last one's return is taken.
    attempt: u64,
    focus_key: bool,
    pub leaving: bool,
    _subscriptions: Vec<Subscription>,
}

impl AddSheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<McpGatewayPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search, or paste an MCP address"));
        let key = cx.new(|cx| InputState::new(window, cx).placeholder("Key"));
        let header = cx.new(|cx| InputState::new(window, cx).placeholder("Authorization"));
        let name = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |this, search, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.continue_with_address(cx),
                _ => {
                    let placeholder = host_name(&search.read(cx).value());
                    this.name.update(cx, |name, cx| name.set_placeholder(placeholder, window, cx));
                    cx.notify();
                }
            }),
            cx.subscribe(&key, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => this.continue_with_address(cx),
                _ => cx.notify(),
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        search.update(cx, |search, cx| search.focus(window, cx));
        Self {
            store,
            page,
            step: Step::Pick,
            search,
            key,
            header,
            name,
            adding: None,
            waiting: false,
            error: None,
            attempt: 0,
            focus_key: false,
            leaving: false,
            _subscriptions: subscriptions,
        }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.add = None;
            cx.notify();
        });
    }

    fn continue_with_address(&mut self, cx: &mut Context<Self>) {
        let text = text_of(&self.search, cx);
        if !matches!(self.step, Step::Pick) || !is_address(&text) {
            return;
        }
        let name = Some(text_of(&self.name, cx)).filter(|name| !name.is_empty());
        self.add(text, name, cx);
    }

    /// Sends the address to the gateway, with the key the key step holds.
    fn add(&mut self, url: String, name: Option<String>, cx: &mut Context<Self>) {
        if self.waiting {
            return;
        }
        let (header, key) = match self.step {
            Step::Key { .. } => (text_of(&self.header, cx), text_of(&self.key, cx)),
            _ => (String::new(), String::new()),
        };
        self.adding = Some((url.clone(), name.clone()));
        self.waiting = true;
        self.error = None;
        let command = Command::McpServerAdd { url, name, header, key };
        self.send(command, cx);
    }

    fn send(&mut self, command: Command, cx: &mut Context<Self>) {
        let sheet = cx.weak_entity();
        self.store.update(cx, |store, _| {
            store.ask_mcp_servers(command, move |result, cx| {
                let _ = sheet.update(cx, |sheet, cx| sheet.answered(result, cx));
            })
        });
        cx.notify();
    }

    fn answered(&mut self, result: Result<Value, String>, cx: &mut Context<Self>) {
        self.waiting = false;
        cx.notify();
        let answer = match result {
            Ok(answer) => answer,
            Err(error) => {
                self.error = Some(error);
                if matches!(self.step, Step::SigningIn { .. }) {
                    self.step = Step::Pick;
                }
                return;
            }
        };
        if let Some(id) = answer["added"].as_str() {
            self.step = Step::Connected(id.to_string());
            return;
        }
        if let Some(url) = answer["sign_in"]["url"].as_str() {
            return self.open_browser(url.to_string(), cx);
        }
        if answer["needs_key"].as_bool() != Some(true) {
            return;
        }
        if matches!(self.step, Step::Key { .. }) {
            self.error = Some("It didn't take that key.".into());
            return;
        }
        let Some((url, name)) = self.adding.clone() else { return };
        let name = name.unwrap_or_else(|| host_name(&url));
        self.step = Step::Key { url, name };
        self.focus_key = true;
    }

    fn open_browser(&mut self, url: String, cx: &mut Context<Self>) {
        let Some((session, sent_back)) = crate::sign_in::start(&url, MCP_REDIRECT, cx) else {
            self.error = Some("The browser couldn't be opened.".into());
            self.step = Step::Pick;
            return;
        };
        self.attempt += 1;
        let attempt = self.attempt;
        let name = match &self.adding {
            Some((_, Some(name))) => name.clone(),
            Some((url, None)) => host_name(url),
            None => String::new(),
        };
        self.step = Step::SigningIn { name, session: Some(session) };
        cx.spawn(async move |this, cx| {
            let address = sent_back.await.ok().flatten();
            let _ = this.update(cx, |sheet, cx| sheet.came_back(attempt, address, cx));
        })
        .detach();
    }

    /// The browser came back with the address the sign-in ended at, or was given up.
    fn came_back(&mut self, attempt: u64, address: Option<String>, cx: &mut Context<Self>) {
        if attempt != self.attempt || !matches!(self.step, Step::SigningIn { session: Some(_), .. }) {
            return;
        }
        cx.notify();
        let Some(address) = address else {
            self.step = Step::Pick;
            return;
        };
        if let Step::SigningIn { session, .. } = &mut self.step {
            *session = None;
        }
        self.waiting = true;
        self.send(Command::McpServerFinishSignIn { address }, cx);
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Pick;
        self.waiting = false;
        self.error = None;
        cx.notify();
    }

    fn submit_key(&mut self, cx: &mut Context<Self>) {
        let Step::Key { url, .. } = &self.step else { return };
        if text_of(&self.key, cx).is_empty() {
            return;
        }
        let name = self.adding.as_ref().and_then(|(_, name)| name.clone());
        self.add(url.clone(), name, cx);
    }

    fn on_return(&mut self, cx: &mut Context<Self>) {
        match self.step {
            Step::Pick => self.continue_with_address(cx),
            Step::Key { .. } => self.submit_key(cx),
            Step::Connected(_) => self.close(cx),
            Step::SigningIn { .. } => {}
        }
    }

    fn pick(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let text = text_of(&self.search, cx);
        let pending = self.adding.as_ref().filter(|_| self.waiting).map(|(url, _)| url.clone());
        let icon = if is_address(&text) { "link" } else { "search" };
        let mut body = vec![InputField::new(&self.search).icon(icon).into_any_element()];
        if let Some(error) = &self.error {
            body.push(warning_line(error.clone(), cx).into_any_element());
        }
        if is_address(&text) {
            body.push(name_field(&self.name, None, None, cx).into_any_element());
            body.push(
                caption("Your gateway reaches it there, and signs in or asks you for a key if it needs one.", cx)
                    .into_any_element(),
            );
            return body;
        }
        let view = self.store.read(cx).gateway.view.clone();
        let known = view.map(|view| view.known).unwrap_or_default();
        let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        let found: Vec<&Known> = known
            .iter()
            .filter(|known| {
                let about = format!("{} {} {}", known.name, known.keywords, known.host).to_lowercase();
                words.iter().all(|word| about.contains(word))
            })
            .collect();
        if found.is_empty() {
            body.push(caption("Nothing known by that name. Paste its address instead.", cx).into_any_element());
            return body;
        }
        let rows = found.chunks(2).enumerate().map(|(place, pair)| {
            let tiles = pair.iter().enumerate().map(|(index, known)| {
                let adding = pending.as_deref() == Some(known.url.as_str());
                self.known_tile(place * 2 + index, known, adding, cx)
            });
            div()
                .flex()
                .gap(px(4.))
                .children(tiles)
                .when(pair.len() == 1, |row| row.child(div().flex_1()))
                .into_any_element()
        });
        body.push(div().flex().flex_col().gap(px(4.)).children(rows).into_any_element());
        body
    }

    fn known_tile(&self, index: usize, known: &Known, pending: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let group = SharedString::from(format!("known-mcp-server-{index}"));
        let (url, name) = (known.url.clone(), known.name.clone());
        div()
            .id(("known-mcp-server", index))
            .group(group.clone())
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(52.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_default()
            .child(Highlight::new().radius(Radius::MD).render(group, cx))
            .child(tile(&known.monogram, &known.tint, TILE, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(TextSize::Sm)
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(known.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Xs)
                            .text_color(c.muted_foreground)
                            .truncate()
                            .child(known.host.clone()),
                    ),
            )
            .when(pending, |tile| tile.child(div().text_color(c.muted_foreground).child(Spinner::regular().render(cx))))
            .on_click(cx.listener(move |this, _, _, cx| this.add(url.clone(), Some(name.clone()), cx)))
            .into_any_element()
    }

    fn signing_in(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = colors(cx);
        let waiting =
            if self.waiting { "Finishing the sign-in on your gateway" } else { "Waiting for you to approve it" };
        vec![
            caption("Approve Motile in the browser.", cx).into_any_element(),
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(TextSize::Sm)
                .text_color(c.muted_foreground)
                .child(Spinner::regular().render(cx))
                .child(waiting)
                .into_any_element(),
        ]
    }

    fn key_step(&self, name: &str, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.focus_key {
            // Once the field is in the modal, which would take the keys back otherwise.
            let key = self.key.clone();
            window.on_next_frame(move |window, cx| key.update(cx, |key, cx| key.focus(window, cx)));
        }
        let mut body = vec![caption(format!("Paste a key from {name}'s settings."), cx).into_any_element()];
        body.extend(key_fields(&self.key, &self.header, cx));
        if let Some(error) = &self.error {
            body.push(warning_line(error.clone(), cx).into_any_element());
        }
        body
    }

    fn connected(&mut self, id: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let view = self.store.read(cx).gateway.view.clone();
        let Some(mcp_server) = find(view.as_ref(), id).cloned() else { return vec![spinner_box(80., cx)] };
        let mut body = vec![caption(mcp_server.tools_text.clone(), cx).into_any_element()];
        if !mcp_server.groups.is_empty() {
            let groups = mcp_server.groups.iter().enumerate().fold(
                card_default(div().flex().flex_col(), cx),
                |card, (index, tools)| {
                    card.when(index > 0, |card| card.child(divider_on(Surface::Card, cx))).child(group_row(
                        &self.store,
                        &mcp_server.id,
                        tools,
                        None,
                        Surface::Card,
                        cx,
                    ))
                },
            );
            body.push(field_label("Tool permissions", cx).into_any_element());
            body.push(groups.into_any_element());
        }
        body
    }
}

/// The key, and the header it goes in.
fn key_fields(key: &Entity<InputState>, header: &Entity<InputState>, cx: &App) -> Vec<AnyElement> {
    vec![
        InputField::new(key).secure(true).into_any_element(),
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(field_label("Header", cx))
            .child(InputField::new(header))
            .into_any_element(),
    ]
}

impl Render for AddSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let view = self.store.read(cx).gateway.view.clone();
        let title = match &self.step {
            Step::Pick => "Add an MCP Server".to_string(),
            Step::SigningIn { name, .. } => format!("Sign In to {name}"),
            Step::Key { name, .. } => format!("{name} Takes a Key"),
            Step::Connected(id) => {
                let name =
                    find(view.as_ref(), id).map_or_else(|| "It".to_string(), |mcp_server| mcp_server.name.clone());
                format!("Connected to {name}")
            }
        };
        let (body, footer): (Vec<AnyElement>, Vec<AnyElement>) = match &self.step {
            Step::Pick => {
                let address = is_address(&text_of(&self.search, cx));
                let cancel = ActionButton::new("mcp-server-add-cancel", "Cancel")
                    .margin(beside(0., if address { 4. } else { 0. }))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
                let continues = address.then(|| {
                    ActionButton::new("mcp-server-continue", "Continue")
                        .primary()
                        .pending(self.waiting)
                        .margin(beside(4., 0.))
                        .on_click(cx.listener(|this, _, _, cx| this.continue_with_address(cx)))
                        .into_any_element()
                });
                (self.pick(cx), std::iter::once(cancel.into_any_element()).chain(continues).collect())
            }
            Step::SigningIn { .. } => {
                let cancel = ActionButton::new("mcp-server-sign-in-cancel", "Cancel")
                    .disabled(self.waiting)
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx)));
                (self.signing_in(cx), vec![cancel.into_any_element()])
            }
            Step::Key { name, .. } => {
                let name = name.clone();
                let can_add = !text_of(&self.key, cx).is_empty();
                let back = ActionButton::new("mcp-server-key-back", "Back")
                    .margin(beside(0., 4.))
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx)));
                let add = ActionButton::new("mcp-server-key-add", "Add")
                    .primary()
                    .pending(self.waiting)
                    .disabled(!can_add && !self.waiting)
                    .margin(beside(4., 0.))
                    .on_click(cx.listener(|this, _, _, cx| this.submit_key(cx)));
                (self.key_step(&name, window, cx), vec![back.into_any_element(), add.into_any_element()])
            }
            Step::Connected(id) => {
                let id = id.clone();
                let done = ActionButton::new("mcp-server-added-done", "Done")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
                (self.connected(&id, cx), vec![done.into_any_element()])
            }
        };
        self.focus_key = false;
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(sheet_title(title))
            .children(body)
            .child(div().flex().justify_end().children(footer));
        let (close, submit) = (cx.weak_entity(), cx.weak_entity());
        modal("mcp-server-add", scrolling("mcp-server-add-scroll", content, window), move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            submit.update(cx, |this, cx| this.on_return(cx)).ok();
        })
        .width(540.)
        .leaving(self.leaving)
    }
}

/// An MCP server's new name, and the name agents call it by.
pub struct RenameSheet {
    store: Entity<Store>,
    page: WeakEntity<McpGatewayPage>,
    mcp_server: McpServerRow,
    name: Entity<InputState>,
    pub leaving: bool,
    _subscriptions: Vec<Subscription>,
}

impl RenameSheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<McpGatewayPage>,
        mcp_server: &McpServerRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name =
            cx.new(|cx| InputState::new(window, cx).placeholder("Linear").default_value(mcp_server.name.clone()));
        let subscriptions = vec![cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify())];
        name.update(cx, |name, cx| name.focus(window, cx));
        Self { store, page, mcp_server: mcp_server.clone(), name, leaving: false, _subscriptions: subscriptions }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.rename = None;
            cx.notify();
        });
    }

    fn save(&self, cx: &mut App) {
        let name = text_of(&self.name, cx);
        if name.is_empty() {
            return;
        }
        if name != self.mcp_server.name {
            let id = self.mcp_server.id.clone();
            self.store.update(cx, |store, cx| {
                store.update_mcp_server(id, McpChange::Name { name });
                cx.notify();
            });
        }
        self.close(cx);
    }
}

impl Render for RenameSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let can_save = !text_of(&self.name, cx).is_empty();
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(sheet_title(format!("Rename {}", self.mcp_server.name)))
            .child(name_field(&self.name, Some(&example_tool(&self.mcp_server)), Some(&self.mcp_server.slug), cx))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(
                        ActionButton::new("mcp-server-rename-cancel", "Cancel")
                            .margin(beside(0., 4.))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        ActionButton::new("mcp-server-rename-save", "Save")
                            .primary()
                            .disabled(!can_save)
                            .margin(beside(4., 0.))
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("mcp-server-rename", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(420.)
        .leaving(self.leaving)
    }
}

/// An MCP server's new key.
pub struct KeySheet {
    store: Entity<Store>,
    page: WeakEntity<McpGatewayPage>,
    mcp_server: McpServerRow,
    key: Entity<InputState>,
    header: Entity<InputState>,
    waiting: bool,
    error: Option<String>,
    pub leaving: bool,
    _subscription: Subscription,
}

impl KeySheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<McpGatewayPage>,
        mcp_server: &McpServerRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let key = cx.new(|cx| InputState::new(window, cx).placeholder("Key"));
        let header = cx.new(|cx| InputState::new(window, cx).placeholder("Authorization"));
        let subscription = cx.subscribe(&key, |_, _, _: &InputEvent, cx| cx.notify());
        key.update(cx, |key, cx| key.focus(window, cx));
        Self {
            store,
            page,
            mcp_server: mcp_server.clone(),
            key,
            header,
            waiting: false,
            error: None,
            leaving: false,
            _subscription: subscription,
        }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.key = None;
            cx.notify();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let key = text_of(&self.key, cx);
        if key.is_empty() || self.waiting {
            return;
        }
        self.waiting = true;
        self.error = None;
        let command =
            Command::McpServerKey { mcp_server: self.mcp_server.id.clone(), header: text_of(&self.header, cx), key };
        let sheet = cx.weak_entity();
        self.store.update(cx, |store, _| {
            store.ask_mcp_servers(command, move |result, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    sheet.waiting = false;
                    match result {
                        Ok(_) => sheet.close(cx),
                        Err(error) => sheet.error = Some(error),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }
}

impl Render for KeySheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let can_save = !text_of(&self.key, cx).is_empty();
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(sheet_title(format!("New Key for {}", self.mcp_server.name)))
            .child(caption(format!("Paste a new key from {}'s settings.", self.mcp_server.name), cx))
            .children(key_fields(&self.key, &self.header, cx))
            .children(self.error.clone().map(|error| warning_line(error, cx)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(
                        ActionButton::new("mcp-server-key-cancel", "Cancel")
                            .margin(beside(0., 4.))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        ActionButton::new("mcp-server-key-save", "Save")
                            .primary()
                            .pending(self.waiting)
                            .disabled(!can_save && !self.waiting)
                            .margin(beside(4., 0.))
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("mcp-server-key", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(420.)
        .leaving(self.leaving)
    }
}

/// What a field of the form holds.
enum Input {
    Line(Entity<InputState>),
    Area(Entity<TextareaState>),
    Switch(bool),
    Choice(String),
}

struct FormField {
    field: Field,
    input: Input,
}

impl FormField {
    fn value(&self, cx: &App) -> String {
        match &self.input {
            Input::Line(input) => input.read(cx).value().to_string(),
            Input::Area(area) => area.read(cx).value().to_string(),
            Input::Switch(on) => on.to_string(),
            Input::Choice(choice) => choice.clone(),
        }
    }
}

/// Calls one of an MCP server's tools with what a form of its fields holds, and shows the answer.
pub struct TrySheet {
    store: Entity<Store>,
    page: WeakEntity<McpGatewayPage>,
    mcp_server: String,
    tool: Option<String>,
    /// The fields as the core said them, until they are drawn as a form.
    said: Option<Vec<Field>>,
    form: Loaded<Vec<FormField>>,
    /// Counts what was asked, so that only the last answer is kept.
    asked: u64,
    running: bool,
    answer: Option<Result<Answer, String>>,
    json: bool,
    text_view: Entity<CodeView>,
    json_view: Entity<CodeView>,
    pub leaving: bool,
    _subscription: Subscription,
}

impl TrySheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<McpGatewayPage>,
        mcp_server: &McpServerRow,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        let mut sheet = Self {
            store,
            page,
            mcp_server: mcp_server.id.clone(),
            tool: None,
            said: None,
            form: Loaded::Loading,
            asked: 0,
            running: false,
            answer: None,
            json: false,
            text_view: cx.new(CodeView::new),
            json_view: cx.new(CodeView::new),
            leaving: false,
            _subscription: subscription,
        };
        if let Some(tool) =
            mcp_server.tools.iter().find(|tool| tool.permission != ToolPermission::Block).or(mcp_server.tools.first())
        {
            sheet.choose(tool.name.clone(), cx);
        }
        sheet
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.trying = None;
            cx.notify();
        });
    }

    fn choose(&mut self, tool: String, cx: &mut Context<Self>) {
        self.tool = Some(tool.clone());
        self.said = None;
        self.form = Loaded::Loading;
        self.answer = None;
        self.asked += 1;
        let asked = self.asked;
        let sheet = cx.weak_entity();
        let command = Command::McpServerFields { mcp_server: self.mcp_server.clone(), tool };
        self.store.update(cx, |store, _| {
            store.ask_mcp_servers(command, move |result, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    if sheet.asked != asked {
                        return;
                    }
                    let fields = result.and_then(|answer| {
                        serde_json::from_value::<Vec<Field>>(answer["fields"].clone())
                            .map_err(|error| error.to_string())
                    });
                    match fields {
                        Ok(fields) => sheet.said = Some(fields),
                        Err(error) => sheet.form = Loaded::Failed(error),
                    }
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    /// Makes the form of the fields the core said, which takes the window.
    fn build_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fields) = self.said.take() else { return };
        let form = fields
            .into_iter()
            .map(|field| {
                let value = field.value.clone();
                let input = match field.kind {
                    FieldKind::Text | FieldKind::Number => {
                        Input::Line(cx.new(|cx| InputState::new(window, cx).default_value(value)))
                    }
                    FieldKind::Json => {
                        let area = text_area_state("", 3, 10, window, cx);
                        area.update(cx, |area, cx| area.set_value(value, window, cx));
                        Input::Area(area)
                    }
                    FieldKind::Switch => Input::Switch(value == "true"),
                    FieldKind::Choice => Input::Choice(value),
                };
                FormField { field, input }
            })
            .collect();
        self.form = Loaded::Ready(form);
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let (Some(tool), Loaded::Ready(form)) = (&self.tool, &self.form) else { return };
        if self.running {
            return;
        }
        let values: Map<String, Value> =
            form.iter().map(|field| (field.field.name.clone(), Value::String(field.value(cx)))).collect();
        self.running = true;
        self.asked += 1;
        let asked = self.asked;
        let sheet = cx.weak_entity();
        let command = Command::McpServerTry { mcp_server: self.mcp_server.clone(), tool: tool.clone(), values };
        self.store.update(cx, |store, _| {
            store.try_mcp_server_tool(command, asked, move |answer, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    if sheet.asked != asked {
                        return;
                    }
                    sheet.running = false;
                    sheet.answer = Some(answer);
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    fn set_input(&mut self, index: usize, input: Input, cx: &mut Context<Self>) {
        let Loaded::Ready(form) = &mut self.form else { return };
        let Some(field) = form.get_mut(index) else { return };
        field.input = input;
        cx.notify();
    }

    fn picker(&self, mcp_server: &McpServerRow, cx: &mut Context<Self>) -> ActionMenu {
        let tool = mcp_server.tools.iter().find(|tool| Some(&tool.name) == self.tool.as_ref());
        let title = tool.map_or_else(|| "Pick a Tool".to_string(), |tool| tool.title.clone());
        let (sheet, tools) = (cx.weak_entity(), mcp_server.tools.clone());
        let chosen = self.tool.clone();
        ActionMenu::new("mcp-server-try-tool", title, move |_, _| {
            tools.iter().fold(Menu::new(), |menu, tool| {
                let (sheet, name) = (sheet.clone(), tool.name.clone());
                menu.checked(chosen.as_ref() == Some(&tool.name), tool.title.clone(), move |_, cx| {
                    let name = name.clone();
                    let _ = sheet.update(cx, |sheet, cx| sheet.choose(name, cx));
                })
            })
        })
        .button(|button| button.variant(Variant::Outline))
    }

    fn form(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = colors(cx);
        let form = match &self.form {
            Loaded::Loading => return vec![spinner_box(60., cx)],
            Loaded::Failed(error) => return vec![warning_line(error.clone(), cx).into_any_element()],
            Loaded::Ready(form) => form,
        };
        if form.is_empty() {
            return vec![caption("It takes nothing.", cx).into_any_element()];
        }
        form.iter()
            .enumerate()
            .map(|(index, field)| {
                let name = div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div().text_size(TextSize::Xs).font_weight(FontWeight::MEDIUM).child(field.field.name.clone()),
                    )
                    .when(field.field.required, |name| {
                        name.child(
                            div().text_size(TextSize::Xs).text_color(c.muted_stronger_foreground).child("required"),
                        )
                    });
                let description =
                    field.field.description.clone().map(|description| {
                        div().text_size(TextSize::Xs).text_color(c.muted_foreground).child(description)
                    });
                let sheet = cx.weak_entity();
                let control: Option<AnyElement> = match &field.input {
                    Input::Line(input) => Some(
                        InputField::new(input).monospaced(field.field.kind == FieldKind::Number).into_any_element(),
                    ),
                    Input::Area(area) => Some(TextArea::new(area).monospaced(true).into_any_element()),
                    Input::Choice(chosen) => {
                        let (choices, chosen, sheet) = (field.field.choices.clone(), chosen.clone(), sheet.clone());
                        let title = if chosen.is_empty() { "Pick One".to_string() } else { chosen.clone() };
                        let menu = ActionMenu::new(("mcp-server-try-choice", index), title, move |_, _| {
                            choices.iter().fold(Menu::new(), |menu, choice| {
                                let (sheet, picked) = (sheet.clone(), choice.clone());
                                menu.checked(*choice == chosen, choice.clone(), move |_, cx| {
                                    let picked = picked.clone();
                                    let _ =
                                        sheet.update(cx, |sheet, cx| sheet.set_input(index, Input::Choice(picked), cx));
                                })
                            })
                        })
                        .button(|button| button.variant(Variant::Outline));
                        Some(div().flex().child(menu).into_any_element())
                    }
                    Input::Switch(_) => None,
                };
                let name = match &field.input {
                    Input::Switch(on) => {
                        let switch = Switch::new(("mcp-server-try-switch", index), *on).on_change(move |on, _, cx| {
                            let _ = sheet.update(cx, |sheet, cx| sheet.set_input(index, Input::Switch(on), cx));
                        });
                        div().flex().items_center().gap(px(10.)).child(switch).child(name)
                    }
                    _ => name,
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().flex().flex_col().gap(px(2.)).child(name).children(description))
                    .children(control)
                    .into_any_element()
            })
            .collect()
    }

    fn answer(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(answer) = &self.answer else { return Vec::new() };
        let answer = match answer {
            Ok(answer) => answer,
            Err(error) => return vec![warning_line(error.clone(), cx).into_any_element()],
        };
        let sheet = cx.weak_entity();
        let tabs = Segmented::new("mcp-server-try-answer", vec!["Text".into(), "JSON".into()], usize::from(self.json))
            .on_select(move |index, _, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    sheet.json = index == 1;
                    cx.notify();
                });
            });
        let (view, document) =
            if self.json { (&self.json_view, &answer.json) } else { (&self.text_view, &answer.text) };
        let mut parts: Vec<AnyElement> = Vec::new();
        if let Some(error) = &answer.error {
            parts.push(warning_line(error.clone(), cx).into_any_element());
        }
        parts.push(div().flex().child(tabs).into_any_element());
        parts.push(code_box(view, document, 240., window, cx));
        parts
    }
}

impl Render for TrySheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        self.build_form(window, cx);
        let view = self.store.read(cx).gateway.view.clone();
        let mut body: Vec<AnyElement> = Vec::new();
        match find(view.as_ref(), &self.mcp_server).cloned() {
            None => body.push(caption("This MCP server is gone.", cx).into_any_element()),
            Some(mcp_server) => {
                let tool = mcp_server.tools.iter().find(|tool| Some(&tool.name) == self.tool.as_ref());
                body.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(tile(&mcp_server.monogram, &mcp_server.tint, TILE, cx))
                        .child(
                            div().flex_1().min_w_0().truncate().text_size(TextSize::Sm).child(mcp_server.name.clone()),
                        )
                        .child(self.picker(&mcp_server, cx))
                        .into_any_element(),
                );
                if let Some(summary) = tool.and_then(|tool| tool.summary.clone()) {
                    body.push(caption(summary, cx).into_any_element());
                }
                body.extend(self.form(cx));
                let can_run = matches!(self.form, Loaded::Ready(_));
                let writes = tool.is_some_and(|tool| tool.kind != ToolKind::Read);
                body.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().flex_1().min_w_0().flex().items_center().gap(px(6.)).text_color(c.warning).when(
                            writes,
                            |line| {
                                line.child(icons::symbol("triangle-alert", IconSize::Sm)).child(
                                    div()
                                        .text_size(TextSize::Xs)
                                        .child("It changes things for real, as an agent's call would."),
                                )
                            },
                        ))
                        .child(
                            ActionButton::new("mcp-server-try-run", "Run")
                                .primary()
                                .pending(self.running)
                                .disabled(!can_run && !self.running)
                                .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                        )
                        .into_any_element(),
                );
                body.extend(self.answer(window, cx));
            }
        }
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(sheet_title("Try a Tool"))
            .children(body)
            .child(div().flex().justify_end().child(
                ActionButton::new("mcp-server-try-done", "Done").on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ));
        let close = cx.weak_entity();
        modal("mcp-server-try", scrolling("mcp-server-try-scroll", content, window), move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .width(600.)
        .leaving(self.leaving)
    }
}

/// A call in full: what it was given, and what it answered or why it failed.
pub struct CallSheet {
    store: Entity<Store>,
    call: i64,
    arguments_view: Entity<CodeView>,
    result_view: Entity<CodeView>,
    pub leaving: bool,
}

impl CallSheet {
    fn new(store: Entity<Store>, call: i64, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, call, arguments_view: cx.new(CodeView::new), result_view: cx.new(CodeView::new), leaving: false }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.close_mcp_server_call();
            cx.notify();
        });
    }
}

impl Render for CallSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let loaded = self.store.read(cx).mcp_servers.call.as_ref().map(|(_, loaded)| match loaded {
            Loaded::Loading => Loaded::Loading,
            Loaded::Failed(error) => Loaded::Failed(error.clone()),
            Loaded::Ready(None) => Loaded::Ready(None),
            Loaded::Ready(Some(call)) => Loaded::Ready(Some((
                call.row.clone(),
                call.arguments.clone(),
                call.result.clone(),
                call.error.is_some(),
            ))),
        });
        let mut body: Vec<AnyElement> = Vec::new();
        match loaded {
            None | Some(Loaded::Loading) => body.push(spinner_box(200., cx)),
            Some(Loaded::Failed(error)) => body.push(warning_line(error, cx).into_any_element()),
            Some(Loaded::Ready(None)) => {
                body.push(caption("Your gateway no longer has this call", cx).into_any_element())
            }
            Some(Loaded::Ready(Some((row, arguments, result, failed)))) => {
                body.push(call_line(&row, cx).into_any_element());
                body.push(field_label("Arguments", cx).into_any_element());
                body.push(code_box(&self.arguments_view, &arguments, 160., window, cx));
                body.push(field_label(if failed { "Error" } else { "Result" }, cx).into_any_element());
                body.push(code_box(&self.result_view, &result, 260., window, cx));
            }
        }
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_color(c.foreground)
            .child(sheet_title("Tool Call"))
            .children(body)
            .child(
                div().flex().justify_end().child(
                    ActionButton::new("mcp-server-call-done", "Done")
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                ),
            );
        let close = cx.weak_entity();
        modal("mcp-server-call", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .width(680.)
        .leaving(self.leaving)
    }
}
