//! The settings' sections under a search for one of them. While something is typed, the groups
//! found take the sections' place.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::Store;
use crate::store::settings::{SettingsEntry, SettingsSection};
use crate::theme::{self, Surface, colors};
use crate::ui::{InputField, InputVariant, icons};

const ROW_INSET: f32 = 10.;
const ROW_RADIUS: f32 = 8.;
const ROW_HEIGHT: f32 = 30.;

pub struct SettingsSidebar {
    store: Entity<Store>,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsSidebar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self { store, search, _subscriptions: subscriptions }
    }

    fn choose(&self, section: SettingsSection, target: Option<String>, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            let target = target.or_else(|| store.settings.target.clone());
            store.open_settings(section, target);
            cx.notify();
        });
    }

    fn section_row(
        &self,
        index: usize,
        section: SettingsSection,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let group: SharedString = format!("settings-section-{index}").into();
        div()
            .id(("settings-section", index))
            .group(group.clone())
            .relative()
            .px(px(ROW_INSET))
            .py(px(1.))
            .child(highlight(group.clone(), selected, cx))
            .child(
                div()
                    .relative()
                    .h(px(ROW_HEIGHT))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(if selected { c.text } else { c.secondary })
                    .group_hover(group, move |row| row.text_color(c.text))
                    .child(icons::symbol(section.symbol(), 14.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(section.title())),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.choose(section, None, cx)))
            .into_any_element()
    }

    fn results(&self, query: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = colors(cx);
        let found = SettingsEntry::matching(query);
        if found.is_empty() {
            return vec![
                div()
                    .px(px(ROW_INSET + 8.))
                    .py(px(10.))
                    .text_size(px(13.))
                    .text_color(c.tertiary)
                    .child("No settings found")
                    .into_any_element(),
            ];
        }
        found
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let group: SharedString = format!("settings-entry-{index}").into();
                div()
                    .id(("settings-entry", index))
                    .group(group.clone())
                    .relative()
                    .px(px(ROW_INSET))
                    .py(px(1.))
                    .child(highlight(group.clone(), false, cx))
                    .child(
                        div()
                            .relative()
                            .min_h(px(ROW_HEIGHT))
                            .px(px(8.))
                            .py(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_color(c.secondary)
                            .group_hover(group, move |row| row.text_color(c.text))
                            .child(icons::symbol(entry.section.symbol(), 14.))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(entry.title),
                                    )
                                    .child(
                                        div().text_size(px(11.)).text_color(c.tertiary).child(entry.section.title()),
                                    ),
                            ),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.choose(entry.section, Some(entry.id.to_string()), cx)),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

/// The light under the row the pointer is over, and under the open section.
fn highlight(group: SharedString, selected: bool, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .absolute()
        .top(px(1.))
        .bottom(px(1.))
        .left(px(ROW_INSET))
        .right(px(ROW_INSET))
        .rounded(px(ROW_RADIUS))
        .when(selected, |light| light.bg(Surface::Background.further().color(c)))
        .when(!selected, |light| light.group_hover(group, move |light| light.bg(Surface::Background.next().color(c))))
}

impl Render for SettingsSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        let selected = self.store.read(cx).settings.section;
        let rows: Vec<AnyElement> = if query.trim().is_empty() {
            SettingsSection::ALL
                .into_iter()
                .enumerate()
                .map(|(index, section)| self.section_row(index, section, selected == Some(section), cx))
                .collect()
        } else {
            self.results(&query, cx)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .pt(px(theme::TOP_BAR))
            .child(
                div()
                    .px(px(10.))
                    .pt(px(2.))
                    .pb(px(6.))
                    .child(InputField::new(&self.search).icon("search").variant(InputVariant::Filled).clearable(true)),
            )
            .child(
                div()
                    .id("settings-sections")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(div().flex().flex_col().pb(px(12.)).children(rows)),
            )
    }
}
