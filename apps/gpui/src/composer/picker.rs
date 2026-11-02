//! The composer's picker: the agents down its side, the models of each account of the one picked
//! under the account's name and the limits it has used, the chosen model's efforts and context
//! windows beside them, and under all of them the access, which opens a menu of its own. A search
//! goes through every agent's models, and finds each of its words in a model's name or its
//! account's, in any order.

use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveLeft, MoveRight, MoveUp};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::limits;
use motile_core::model_search;
use motile_protocol::wire::{Access, Agent, AgentAccount, ModelContext, ModelInfo};

use super::{access_symbol, effort_label, effort_symbol};
use crate::models::{ACCESSES, AGENTS, ComposerPickerPart, ModelInfoExt, access_detail, access_label, agent_name};
use crate::store::Store;
use crate::theme::{ControlSize, IconSize, Layer, Opacity, Radius, Surface, colors};
use crate::ui::menu::Anchor;
use crate::ui::{ActionButton, Chip, Highlight, KeyCaps, Segmented, divider_on, edges, icons, logos, tooltip};

const RAIL_WIDTH: f32 = 44.;
const LIST_WIDTH: f32 = 232.;
const SETTINGS_WIDTH: f32 = 196.;
const HEIGHT: f32 = 348.;
const SHORTEST: f32 = 180.;
const ROW_HEIGHT: f32 = 28.;
const PADDING: f32 = 4.;
const SEARCH_HEIGHT: f32 = 38.;
/// The picker and its lines, as wide as its columns make it.
pub const WIDTH: f32 = RAIL_WIDTH + LIST_WIDTH + SETTINGS_WIDTH + 4.;
/// The room between the picker and the button it opens from.
pub const GAP: f32 = 4.;
const FOOTER_HEIGHT: f32 = ControlSize::Small.height() + 2. * PADDING;
const ACCESS_MENU_WIDTH: f32 = 288.;
const ACCESS_MENU_PADDING: f32 = 10.;
/// Over the columns, just above the footer's button, as a menu opens above what opened it.
const ACCESS_MENU_LIFT: f32 = FOOTER_HEIGHT + 4.;

struct Group {
    title: String,
    account: AgentAccount,
    windows: Vec<limits::Row>,
    rows: Vec<ModelRow>,
}

#[derive(Clone)]
struct ModelRow {
    account: AgentAccount,
    model: ModelInfo,
    index: usize,
}

/// The effort and context window rows of the column beside the models, in order.
#[derive(Clone, PartialEq)]
enum Setting {
    Effort(String),
    Context(ModelContext),
}

pub struct ComposerPicker {
    store: Entity<Store>,
    query: Entity<InputState>,
    /// The agent whose models are listed while nothing is searched.
    agent: Option<Agent>,
    part: ComposerPickerPart,
    column: ComposerPickerPart,
    lit: Option<usize>,
    /// The access the pointer is on in the access menu, which it names.
    shown_access: Option<Access>,
    access_open: bool,
    list: ScrollHandle,
    /// The button it opens from, whose presses are the button's to close it.
    button: Anchor,
    _subscriptions: Vec<Subscription>,
}

impl ComposerPicker {
    pub fn new(store: Entity<Store>, button: Anchor, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search models"));
        query.update(cx, |query, cx| query.focus(window, cx));
        let subscriptions = vec![
            cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                this.column = ComposerPickerPart::Model;
                this.lit = (!this.rows(cx).is_empty()).then_some(0);
                cx.notify();
            }),
            cx.observe(&store, |this, _, cx| this.follow_store(cx)),
        ];
        let state = store.read(cx);
        let part = state.composer_picker.unwrap_or(ComposerPickerPart::Model);
        let agent = state.composer_model().map(|model| model.agent);
        let mut picker = Self {
            store,
            query,
            agent,
            part,
            column: part,
            lit: None,
            shown_access: None,
            access_open: false,
            list: ScrollHandle::new(),
            button,
            _subscriptions: subscriptions,
        };
        picker.follow_store(cx);
        picker.lit = picker.chosen_index(part, cx);
        picker.scroll_to_lit(cx);
        picker
    }

    /// Follows the keys that open it on another part, or its access menu, and takes the row ⌘1
    /// to ⌘9 asked for.
    fn follow_store(&mut self, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let (part, access_open, jump) = (store.composer_picker, store.composer_access_open, store.composer_picker_jump);
        if let Some(part) = part.filter(|part| *part != self.part) {
            self.part = part;
            self.column = part;
            self.lit = self.chosen_index(part, cx);
        }
        if access_open != self.access_open {
            self.access_open = access_open;
            self.shown_access = None;
            if access_open {
                self.column = ComposerPickerPart::Access;
                self.lit = self.chosen_index(ComposerPickerPart::Access, cx);
            }
        }
        if let Some(jump) = jump {
            self.store.update(cx, |store, _| store.composer_picker_jump = None);
            if let Some(row) = self.rows(cx).get(jump).cloned() {
                self.choose(&row, cx);
            }
        }
        cx.notify();
    }

    fn words(&self, cx: &App) -> Vec<String> {
        model_search::words(&self.query.read(cx).value())
    }

    fn groups(&self, cx: &App) -> Vec<Group> {
        let store = self.store.read(cx);
        let words = self.words(cx);
        let limits = store.composer_server().map(|server| server.account_limits.clone()).unwrap_or_default();
        let mut index = 0;
        store
            .composer_choices()
            .into_iter()
            .filter(|(_, account, _)| !words.is_empty() || self.agent.is_none_or(|agent| account.agent == agent))
            .filter_map(|(title, account, models)| {
                let rows: Vec<ModelRow> = models
                    .into_iter()
                    .filter(|model| {
                        let texts = [
                            model.name.as_str(),
                            &self.short_name(model, cx),
                            &account.name,
                            agent_name(account.agent),
                        ];
                        model_search::matches(&words, &texts)
                    })
                    .map(|model| {
                        index += 1;
                        ModelRow { account: account.clone(), model, index: index - 1 }
                    })
                    .collect();
                if rows.is_empty() {
                    return None;
                }
                let windows = limits.get(&account.id).cloned().unwrap_or_default();
                Some(Group { title, account, windows, rows })
            })
            .collect()
    }

    fn rows(&self, cx: &App) -> Vec<ModelRow> {
        self.groups(cx).into_iter().flat_map(|group| group.rows).collect()
    }

    /// The agents with models here, Claude Code's first.
    fn agents(&self, cx: &App) -> Vec<Agent> {
        let choices = self.store.read(cx).composer_choices();
        AGENTS.into_iter().filter(|agent| choices.iter().any(|(_, account, _)| account.agent == *agent)).collect()
    }

    fn settings(&self, cx: &App) -> Vec<Setting> {
        let Some(model) = self.store.read(cx).composer_model() else { return Vec::new() };
        let efforts = model.effort_choices().into_iter().map(Setting::Effort);
        let contexts = model.contexts.iter().filter(|_| model.contexts.len() > 1).cloned().map(Setting::Context);
        efforts.chain(contexts).collect()
    }

    fn short_name(&self, model: &ModelInfo, cx: &App) -> String {
        let store = self.store.read(cx);
        store.composer_server().map_or(model.name.clone(), |server| server.short_name(model).to_string())
    }

    fn count(&self, column: ComposerPickerPart, cx: &App) -> usize {
        match column {
            ComposerPickerPart::Model => self.rows(cx).len(),
            ComposerPickerPart::Effort => self.settings(cx).len(),
            ComposerPickerPart::Access if self.access_open => ACCESSES.len(),
            ComposerPickerPart::Access => 1,
        }
    }

    fn chosen_index(&self, column: ComposerPickerPart, cx: &App) -> Option<usize> {
        let store = self.store.read(cx);
        match column {
            ComposerPickerPart::Model => {
                let (model, account) = (store.composer_model(), store.composer_account());
                self.rows(cx).iter().position(|row| {
                    Some(&row.model.id) == model.as_ref().map(|model| &model.id)
                        && Some(&row.account.id) == account.as_ref().map(|account| &account.id)
                })
            }
            ComposerPickerPart::Effort => {
                let settings = self.settings(cx);
                let effort = Setting::Effort(store.composer_effort().unwrap_or_default());
                settings.iter().position(|setting| *setting == effort).or((!settings.is_empty()).then_some(0))
            }
            ComposerPickerPart::Access if self.access_open => {
                ACCESSES.iter().position(|access| *access == store.composer_access())
            }
            ComposerPickerPart::Access => Some(0),
        }
    }

    fn close(&self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.composer_picker = None;
            store.composer_access_open = false;
            store.composer_focus += 1;
            cx.notify();
        });
    }

    fn choose(&self, row: &ModelRow, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| store.set_model(&row.model, &row.account));
        self.close(cx);
    }

    fn pick(&self, setting: &Setting, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| match setting {
            Setting::Effort(effort) => store.set_effort(effort),
            Setting::Context(context) => store.set_context(context),
        });
        self.close(cx);
    }

    fn open_access(&mut self, open: bool, cx: &mut Context<Self>) {
        self.access_open = open;
        self.store.update(cx, |store, cx| {
            store.composer_access_open = open;
            cx.notify();
        });
        self.column = ComposerPickerPart::Access;
        self.lit = if open { self.chosen_index(ComposerPickerPart::Access, cx) } else { Some(0) };
        self.shown_access = None;
        cx.notify();
    }

    fn set_access(&self, access: Access, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| store.set_access(access));
        self.close(cx);
    }

    fn pick_agent(&mut self, agent: Agent, window: &mut Window, cx: &mut Context<Self>) {
        self.agent = Some(agent);
        self.query.update(cx, |query, cx| {
            query.set_value("", window, cx);
            query.focus(window, cx);
        });
        self.column = ComposerPickerPart::Model;
        self.lit = Some(0);
        self.scroll_to_lit(cx);
        cx.notify();
    }

    /// Keeps the model the arrow keys are on in sight.
    fn scroll_to_lit(&self, cx: &App) {
        let Some(lit) = self.lit.filter(|_| self.column == ComposerPickerPart::Model) else { return };
        let mut child = 0;
        for (number, group) in self.groups(cx).iter().enumerate() {
            child += if number == 0 { 1 } else { 2 };
            if let Some(at) = group.rows.iter().position(|row| row.index == lit) {
                return self.list.scroll_to_item(child + at);
            }
            child += group.rows.len();
        }
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        if self.access_open {
            return;
        }
        let count = self.count(self.column, cx);
        if count == 0 {
            return;
        }
        let from = self.lit.map_or(-by, |lit| lit as isize);
        self.lit = Some((from + by).clamp(0, count as isize - 1) as usize);
        self.scroll_to_lit(cx);
        cx.notify();
    }

    /// Left and right go between the columns, while there is no search whose text they move in.
    /// Whether they did.
    fn move_column(&mut self, by: isize, cx: &mut Context<Self>) -> bool {
        if self.access_open {
            let at = self.lit.unwrap_or(0) as isize + by;
            let lit = at.clamp(0, ACCESSES.len() as isize - 1) as usize;
            self.lit = Some(lit);
            self.shown_access = Some(ACCESSES[lit]);
            cx.notify();
            return true;
        }
        if !self.query.read(cx).value().is_empty() {
            return false;
        }
        let columns: Vec<ComposerPickerPart> =
            [ComposerPickerPart::Model, ComposerPickerPart::Effort, ComposerPickerPart::Access]
                .into_iter()
                .filter(|column| self.count(*column, cx) > 0)
                .collect();
        let Some(at) = columns.iter().position(|column| *column == self.column) else { return true };
        let next = columns[(at as isize + by).clamp(0, columns.len() as isize - 1) as usize];
        if next != self.column {
            self.column = next;
            self.lit = self.chosen_index(next, cx).or(Some(0));
            self.scroll_to_lit(cx);
            cx.notify();
        }
        true
    }

    fn pick_lit(&mut self, cx: &mut Context<Self>) {
        let Some(lit) = self.lit else { return };
        match self.column {
            ComposerPickerPart::Model => {
                let Some(row) = self.rows(cx).get(lit).cloned() else { return };
                self.choose(&row, cx);
            }
            ComposerPickerPart::Effort => {
                let Some(setting) = self.settings(cx).get(lit).cloned() else { return };
                self.pick(&setting, cx);
            }
            ComposerPickerPart::Access if !self.access_open => self.open_access(true, cx),
            ComposerPickerPart::Access => self.set_access(ACCESSES[lit.min(ACCESSES.len() - 1)], cx),
        }
    }

    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.access_open {
            return self.open_access(false, cx);
        }
        self.close(cx);
    }

    /// How tall the columns are: as tall as they go, in the room over the button.
    fn columns_height(&self, window: &Window) -> f32 {
        let content = crate::window_frame::content_bounds(window);
        let room = f32::from(self.button.bounds().top() - content.top()) - 3. * GAP;
        (room - FOOTER_HEIGHT - 3.).clamp(SHORTEST, HEIGHT)
    }

    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.query.read(cx).value().is_empty();
        let side = RAIL_WIDTH - 2. * PADDING;
        div().w(px(RAIL_WIDTH)).flex_shrink_0().p(px(PADDING)).flex().flex_col().gap(px(4.)).children(
            self.agents(cx).into_iter().map(|agent| {
                let group = SharedString::from(format!("picker-agent-{agent:?}"));
                let selected = !searching && self.agent == Some(agent);
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .relative()
                    .size(px(side))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Highlight::new()
                            .radius(Radius::SM)
                            .surface(Surface::Popover)
                            .selected(selected)
                            .render(group, cx),
                    )
                    .child(div().relative().flex().child(logos::agent_icon(agent, IconSize::Lg, cx)))
                    .tooltip(tooltip(agent_name(agent)))
                    .on_click(cx.listener(move |this, _, window, cx| this.pick_agent(agent, window, cx)))
            }),
        )
    }

    fn search(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let query = self.query.clone();
        div()
            .id("picker-search")
            .h(px(SEARCH_HEIGHT))
            .flex_shrink_0()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_text()
            .on_click(move |_, window, cx| query.update(cx, |query, cx| query.focus(window, cx)))
            .child(icons::symbol("search", IconSize::Md).text_color(c.muted_stronger_foreground))
            .child(div().flex_1().child(Input::new(&self.query).appearance(false).px_0().py_0().text_size(px(13.))))
    }

    fn list(&self, groups: &[Group], cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let mut children: Vec<AnyElement> = Vec::new();
        for (number, group) in groups.iter().enumerate() {
            if number > 0 {
                children.push(section_divider(cx).into_any_element());
            }
            children.push(header(group, cx).into_any_element());
            for row in &group.rows {
                children.push(self.model_row(row, cx).into_any_element());
            }
        }
        if groups.is_empty() {
            children.push(
                div()
                    .h(px(ROW_HEIGHT))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .text_color(c.muted_foreground)
                    .child("No model matches")
                    .into_any_element(),
            );
        }
        div()
            .id("picker-models")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.list)
            .p(px(PADDING))
            .flex()
            .flex_col()
            .children(children)
    }

    fn model_row(&self, row: &ModelRow, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let chosen = store.composer_model().is_some_and(|model| model.id == row.model.id)
            && store.composer_account().is_some_and(|account| account.id == row.account.id);
        let key = (row.index < 9).then(|| store.shortcuts.label(&format!("thread.jump.{}", row.index + 1))).flatten();
        let lit = self.column == ComposerPickerPart::Model && self.lit == Some(row.index);
        let group = SharedString::from(format!("picker-model-{}", row.index));
        let index = row.index;
        let chosen_row = row.clone();
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .text_color(c.foreground)
            .child(row_light(group, lit, cx))
            .child(div().relative().flex().child(logos::agent_icon(row.model.agent, IconSize::Md, cx)))
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(self.short_name(&row.model, cx)),
            )
            .child(div().flex_1().min_w(px(8.)))
            .when_some(key, |row, key| {
                row.child(div().relative().text_color(c.muted_foreground).child(KeyCaps::new([key])))
            })
            .child(check(chosen))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.column = ComposerPickerPart::Model;
                    this.lit = Some(index);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.choose(&chosen_row, cx)))
    }

    fn settings_column(&self, settings: &[Setting], cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let model = self.store.read(cx).composer_model();
        let mut children: Vec<AnyElement> = Vec::new();
        for (index, setting) in settings.iter().enumerate() {
            if index == 0 && matches!(setting, Setting::Effort(_)) {
                let short = model.as_ref().map(|model| self.short_name(model, cx));
                children.push(section_title("Reasoning", short, cx).into_any_element());
            }
            let first_context = matches!(setting, Setting::Context(_))
                && (index == 0 || !matches!(settings[index - 1], Setting::Context(_)));
            if first_context {
                if index > 0 {
                    children.push(section_divider(cx).into_any_element());
                }
                children.push(section_title("Context window", None, cx).into_any_element());
            }
            children.push(self.setting_row(setting, index, model.as_ref(), cx).into_any_element());
        }
        if settings.is_empty() {
            let name = model.as_ref().map_or("This model".to_string(), |model| model.name.clone());
            children.push(
                div()
                    .p(px(8.))
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .child(format!("{name} has no settings to choose"))
                    .into_any_element(),
            );
        }
        div()
            .id("picker-settings")
            .w(px(SETTINGS_WIDTH))
            .flex_shrink_0()
            .overflow_y_scroll()
            .p(px(PADDING))
            .flex()
            .flex_col()
            .children(children)
    }

    fn setting_row(
        &self,
        setting: &Setting,
        index: usize,
        model: Option<&ModelInfo>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let (title, symbol, default, chosen) = match setting {
            Setting::Effort(effort) => (
                effort_label(effort),
                Some(effort_symbol(effort)),
                model.and_then(|model| model.default_effort.as_ref()) == Some(effort),
                store.composer_effort().as_ref() == Some(effort),
            ),
            Setting::Context(context) => {
                (context.label.clone(), None, context.default, store.composer_context().as_ref() == Some(context))
            }
        };
        let lit = self.column == ComposerPickerPart::Effort && self.lit == Some(index);
        let group = SharedString::from(format!("picker-setting-{index}"));
        let picked = setting.clone();
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(13.))
            .text_color(c.foreground)
            .child(row_light(group, lit, cx))
            .when_some(symbol, |row, symbol| {
                row.child(
                    div().relative().flex().child(icons::symbol(symbol, IconSize::Md).text_color(c.muted_foreground)),
                )
            })
            .child(div().relative().whitespace_nowrap().child(title))
            .when(default, |row| row.child(div().relative().child(Chip::new("Default").muted(true))))
            .child(div().flex_1().min_w(px(6.)))
            .child(check(chosen))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.column = ComposerPickerPart::Effort;
                    this.lit = Some(index);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.pick(&picked, cx)))
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let current = self.store.read(cx).composer_access();
        let open = self.access_open;
        div().flex_shrink_0().p(px(PADDING)).flex().justify_end().child(
            ActionButton::new("picker-access", access_label(current))
                .symbol(access_symbol(current))
                .help(access_detail(current))
                .ghost()
                .small()
                .selected(open || self.column == ComposerPickerPart::Access)
                .opens(true)
                .open(open)
                .tint(Some(c.foreground))
                .surface(Surface::Popover)
                .on_click(cx.listener(move |this, _, _, cx| this.open_access(!open, cx))),
        )
    }

    /// The access modes as one row of their symbols, the one under the pointer or the arrow keys
    /// named and told about under it.
    fn access_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let current = self.store.read(cx).composer_access();
        let shown = self.shown_access.unwrap_or(current);
        let selected = ACCESSES.iter().position(|access| *access == current).unwrap_or(0);
        let this = cx.entity().downgrade();
        let options =
            ACCESSES.iter().map(|access| (access_label(*access).into(), access_symbol(*access).into())).collect();
        let switcher = Segmented::symbols("picker-access-modes", options, selected)
            .fills(true)
            .compact(true)
            .lit(self.lit.filter(|_| self.column == ComposerPickerPart::Access))
            .on_select({
                let this = this.clone();
                move |index, _, cx| {
                    let _ = this.update(cx, |this, cx| this.set_access(ACCESSES[index], cx));
                }
            })
            .on_hover(move |index, inside, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    let access = ACCESSES[index];
                    if inside {
                        this.shown_access = Some(access);
                    } else if this.shown_access == Some(access) {
                        this.shown_access = None;
                    }
                    cx.notify();
                });
            });
        crate::ui::dropdown_card(div(), cx)
            .id("picker-access-menu")
            .absolute()
            .right(px(PADDING))
            .bottom(px(ACCESS_MENU_LIFT))
            .w(px(ACCESS_MENU_WIDTH))
            .p(px(ACCESS_MENU_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.muted_foreground)
                    .child("Access"),
            )
            .child(switcher)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).text_color(c.foreground).child(access_label(shown)))
                    .child(
                        div()
                            .h(px(2. * DETAIL_LINE))
                            .text_size(px(12.))
                            .line_height(px(DETAIL_LINE))
                            .line_clamp(2)
                            .text_color(c.muted_foreground)
                            .child(access_detail(shown)),
                    ),
            )
    }
}

/// The line height of the access menu's description, which keeps room for two lines.
const DETAIL_LINE: f32 = 15.;

/// The light of a row, drawn a point in from its top and bottom so that two lit rows look apart
/// while their clicks meet.
fn row_light(group: SharedString, lit: bool, cx: &App) -> Div {
    Highlight::new()
        .radius(Radius::SM)
        .surface(Surface::Popover)
        .inset(edges(1., 0., 1., 0.))
        .lit(lit)
        .render(group, cx)
}

fn check(chosen: bool) -> Div {
    div().relative().flex().when(!chosen, |mark| mark.opacity(0.)).child(icons::symbol("check", IconSize::Sm))
}

/// Between two sections, as far from the last row's words as from the next title's, which has
/// room of its own above it.
fn section_divider(cx: &App) -> Div {
    div().flex_shrink_0().px(px(8.)).pt(px(6.)).pb(px(2.)).child(divider_on(Surface::Popover, cx))
}

/// A section's title, and at its end the model it is about.
fn section_title(title: &'static str, model: Option<String>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_shrink_0()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(4.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .child(div().text_color(c.muted_foreground).child(title))
        .child(div().flex_1())
        .when_some(model, |row, model| {
            row.child(
                div()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(c.muted_stronger_foreground)
                    .child(model),
            )
        })
}

fn header(group: &Group, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_shrink_0()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(4.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_foreground)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(group.title.clone()),
        )
        .when(!group.windows.is_empty(), |header| header.child(limit_pills(&group.account.id, &group.windows, cx)))
}

/// An account's windows as thin pills side by side, each filled as far as it was used.
fn limit_pills(account: &str, windows: &[limits::Row], cx: &App) -> Div {
    let c = colors(cx);
    let track = Surface::Popover.layer(Layer::Control, c);
    div().flex().gap(px(12.)).children(windows.iter().enumerate().map(|(index, window)| {
        let tone = match window.tone {
            limits::Tone::Success => c.success,
            limits::Tone::Pending => c.pending,
            limits::Tone::Warning => c.warning,
        };
        let said = [
            Some(window.label.clone()),
            Some(format!("{} used", window.used)),
            window.resets_in.as_ref().map(|resets| format!("resets in {resets}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        div()
            .id(SharedString::from(format!("pill-{account}-{index}")))
            .flex_1()
            .h(px(4.))
            .rounded_full()
            .overflow_hidden()
            .bg(track)
            .tooltip(tooltip(said))
            .child(
                div()
                    .h_full()
                    .w(relative((window.used_percent / 100.).clamp(0., 1.) as f32))
                    .rounded_full()
                    .bg(c.at(tone, Opacity::ColorTintChartStrongest)),
            )
    }))
}

impl Render for ComposerPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let groups = self.groups(cx);
        let settings = self.settings(cx);
        let height = self.columns_height(window);
        let line = || div().w(px(1.)).flex_shrink_0().bg(c.border_popover);
        let button = self.button.clone();
        crate::ui::dropdown_card(div(), cx)
            .id("composer-picker")
            .relative()
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if this.access_open {
                    return this.open_access(false, cx);
                }
                if !button.bounds().contains(&event.position) {
                    this.close(cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.step(-1, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.step(1, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveLeft, _, cx| {
                if this.move_column(-1, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &MoveRight, _, cx| {
                if this.move_column(1, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                cx.stop_propagation();
                this.pick_lit(cx);
            }))
            .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                cx.stop_propagation();
                this.escape(cx);
            }))
            .child(
                div()
                    .h(px(height))
                    .flex()
                    .child(self.rail(cx))
                    .child(line())
                    .child(
                        div()
                            .w(px(LIST_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .child(self.search(cx))
                            .child(divider_on(Surface::Popover, cx))
                            .child(self.list(&groups, cx)),
                    )
                    .child(line())
                    .child(self.settings_column(&settings, cx)),
            )
            .child(divider_on(Surface::Popover, cx))
            .child(self.footer(cx))
            .when(self.access_open, |picker| picker.child(self.access_menu(cx)))
    }
}

/// Lays the picker out over the button it opens from, its trailing edge on the button's.
pub fn placed(picker: Entity<ComposerPicker>, button: &Anchor) -> impl IntoElement {
    let bounds = button.bounds();
    deferred(
        anchored()
            .anchor(gpui_kit::Anchor::BottomRight)
            .position(point(bounds.right(), bounds.top() - px(GAP)))
            .snap_to_window_with_margin(px(8.))
            .child(picker),
    )
    .with_priority(1)
}
