//! The settings' sections under a search for one of them. While something is typed, the groups
//! found take the sections' place.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::sidebar::SidebarFooter;
use crate::store::Store;
use crate::store::settings::{SettingsEntry, SettingsSection};
use crate::theme::{self, IconSize, Radius, colors};
use crate::ui::{Highlight, InputField, InputVariant, edges, hover_state, icons, track_hover};

const ROW_INSET: f32 = 10.;
const SECTION_HEIGHT: f32 = 32.;
const RESULT_HEIGHT: f32 = 30.;
/// Where the sidebar's text starts: past the rows' inset and their padding.
const SIDEBAR_EDGE: f32 = ROW_INSET + 8.;

pub struct SettingsSidebar {
    store: Entity<Store>,
    search: Entity<InputState>,
    footer: Entity<SidebarFooter>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsSidebar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        let footer = cx.new(|cx| SidebarFooter::new(store.clone(), cx));
        Self { store, search, footer, _subscriptions: subscriptions }
    }

    fn choose(&self, section: SettingsSection, target: Option<String>, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            let target = target.or_else(|| store.settings.target.clone());
            store.open_settings(section, target);
            cx.notify();
        });
    }

    /// A row of the list: its light, and what it shows faded until it lights up.
    fn row(
        &self,
        id: ElementId,
        selected: bool,
        content: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hovered = hover_state(&id, window, cx);
        let group = SharedString::from(format!("settings-row-{id:?}"));
        let light = Highlight::new()
            .radius(Radius::MD)
            .inset(edges(1., ROW_INSET, 1., ROW_INSET))
            .selected(selected)
            .lit(*hovered.read(cx));
        div()
            .id(id)
            .group(group.clone())
            .relative()
            .px(px(ROW_INSET))
            .py(px(1.))
            .child(light.render(group, cx))
            .child(light.faded(content.relative(), cx))
            .on_hover(track_hover(&hovered))
    }

    fn sections(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let selected = self.store.read(cx).settings.section;
        SettingsSection::ALL
            .into_iter()
            .enumerate()
            .map(|(index, section)| {
                let content = div()
                    .h(px(SECTION_HEIGHT))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol(section.symbol(), IconSize::Md))
                    .child(div().text_size(px(13.)).child(section.title()));
                self.row(
                    ElementId::NamedInteger("settings-section".into(), index as u64),
                    selected == Some(section),
                    content,
                    window,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.choose(section, None, cx)))
                .into_any_element()
            })
            .collect()
    }

    fn results(&self, query: &str, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = colors(cx);
        let found = SettingsEntry::matching(query, &self.store.read(cx).shortcuts);
        if found.is_empty() {
            return vec![
                div()
                    .px(px(SIDEBAR_EDGE))
                    .py(px(10.))
                    .text_size(px(13.))
                    .text_color(c.muted_stronger_foreground)
                    .child("No settings found")
                    .into_any_element(),
            ];
        }
        found
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let content = div()
                    .min_h(px(RESULT_HEIGHT))
                    .px(px(8.))
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icons::symbol(entry.section.symbol(), IconSize::Md))
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
                                    .child(entry.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(c.muted_stronger_foreground)
                                    .whitespace_nowrap()
                                    .child(entry.section.title()),
                            ),
                    );
                let (section, target) = (entry.section, entry.id);
                self.row(ElementId::NamedInteger("settings-entry".into(), index as u64), false, content, window, cx)
                    .on_click(cx.listener(move |this, _, _, cx| this.choose(section, Some(target.clone()), cx)))
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for SettingsSidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        let rows = if query.is_empty() { self.sections(window, cx) } else { self.results(&query, window, cx) };
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
            .child(crate::ui::divider(cx))
            .child(self.footer.clone())
    }
}
