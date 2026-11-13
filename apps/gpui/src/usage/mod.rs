//! What was done, what the agents spent and what is left of their plans, in place of the thread:
//! the title and the choices in the top bar over it, after the way back.

mod activity;
pub mod limits;
mod spending;

use gpui_kit::prelude::*;
use gpui_kit::*;

use self::activity::ActivityView;
use self::spending::SpendingView;
use crate::panel::state::Loaded;
use crate::store::Store;
use crate::store::usage::{UsagePeriod, UsageTab};
use crate::theme::{self, ControlSize, Layer, Radius, Surface, TextSize, colors};
use crate::ui::menu::Menu;
use crate::ui::motion::pulsing;
use crate::ui::parts::{Check, CheckMenu};
use crate::ui::{ActionButton, ActionMenu, Segmented, Spinner, Variant, beside, text_width};

const MARGIN: f32 = 24.;
const TOP: f32 = 36.;
const TRAILING: f32 = 14.;
/// The room between the controls' lights, which a button beside them takes as its own.
const CONTROL_GAP: f32 = 8.;

pub struct UsageRoute {
    store: Entity<Store>,
    spending: Entity<SpendingView>,
    activity: Entity<ActivityView>,
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    pub title_inset: f32,
    /// How wide the pane is.
    pub width: f32,
    /// What the window's buttons take of the top bar's end, where the app draws them.
    pub trailing_room: f32,
    /// A press in the top bar that moves the window once it drags.
    moving: std::rc::Rc<std::cell::Cell<bool>>,
    _subscription: Subscription,
}

impl UsageRoute {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let spending = cx.new(|cx| SpendingView::new(store.clone(), cx));
        let activity = cx.new(|cx| ActivityView::new(store.clone(), cx));
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            store,
            spending,
            activity,
            title_inset: 20.,
            width: 0.,
            trailing_room: 0.,
            moving: Default::default(),
            _subscription: subscription,
        }
    }

    /// "Usage", then the servers counted, which picks them.
    fn title(&self, cx: &App) -> Div {
        let c = colors(cx);
        let size = ControlSize::Regular;
        let store = self.store.read(cx);
        let checks = store
            .servers
            .iter()
            .map(|server| {
                let (handle, id) = (self.store.clone(), server.id.clone());
                Check::new(server.name.clone(), store.usage_server_picked(&server.id), move |_, cx| {
                    handle.update(cx, |store, cx| {
                        store.toggle_usage_server(&id);
                        cx.notify();
                    })
                })
            })
            .collect();
        div()
            .flex()
            .items_center()
            .gap(px(size.padding()))
            .text_size(size.text_size())
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .child(div().text_color(c.muted_foreground).child("Usage"))
            .child(div().text_color(c.muted_stronger_foreground).child("/"))
            .child(
                div().ml(px(-size.padding())).child(
                    CheckMenu::new("usage-servers", store.usage_servers_label(), "The servers counted", checks)
                        .size(size)
                        .tint(Some(c.foreground)),
                ),
            )
    }

    /// The tabs, the period of the activity, the cost and the tokens, and the way to read it all again, in a
    /// row: the period as a menu where they don't fit, and the tabs too where even that doesn't.
    fn controls(&self, room: f32, window: &mut Window, cx: &App) -> Div {
        let store = self.store.read(cx);
        let (tab, period) = (store.usage_tab(), store.usage_period());
        let handle = self.store.clone();
        let segmented_tabs = Segmented::new(
            "usage-tabs",
            UsageTab::ALL.iter().map(|tab| SharedString::from(tab.label())).collect(),
            UsageTab::ALL.iter().position(|shown| *shown == tab).unwrap_or(0),
        )
        .on_select(move |index, _, cx| {
            handle.update(cx, |store, cx| {
                store.set_usage_tab(UsageTab::ALL[index]);
                cx.notify();
            })
        });
        let periods: Vec<&str> = UsagePeriod::ALL.iter().map(|period| period.label()).collect();
        let tabs_width = segmented_width(&UsageTab::ALL.map(UsageTab::label), window);
        let refresh_width = ControlSize::Regular.height() + 2. * CONTROL_GAP;
        let fits = room >= tabs_width + segmented_width(&periods, window) + refresh_width;
        let size = ControlSize::Regular;
        let period_menu_width = text_width(period.label(), size.text_size(), FontWeight::MEDIUM, window)
            + 2. * size.padding()
            + size.height();
        let tabs = if fits || room >= tabs_width + period_menu_width + refresh_width {
            segmented_tabs.into_any_element()
        } else {
            let handle = self.store.clone();
            ActionMenu::new("usage-tabs-menu", tab.label(), move |_, _| {
                UsageTab::ALL.iter().fold(Menu::new(), |menu, choice| {
                    let (handle, choice) = (handle.clone(), *choice);
                    menu.checked(choice == tab, choice.label(), move |_, cx| {
                        handle.update(cx, |store, cx| {
                            store.set_usage_tab(choice);
                            cx.notify();
                        })
                    })
                })
            })
            .button(|button| button.variant(Variant::Outline))
            .into_any_element()
        };
        let picker = if fits {
            let handle = self.store.clone();
            let picker = Segmented::new(
                "usage-period",
                periods.iter().map(|label| SharedString::from(*label)).collect(),
                UsagePeriod::ALL.iter().position(|shown| *shown == period).unwrap_or(0),
            )
            .disabled(tab == UsageTab::Limits)
            .on_select(move |index, _, cx| {
                handle.update(cx, |store, cx| {
                    store.set_usage_period(UsagePeriod::ALL[index]);
                    cx.notify();
                })
            });
            div().pl(px(CONTROL_GAP)).child(picker).into_any_element()
        } else {
            let handle = self.store.clone();
            ActionMenu::new("usage-period-menu", period.label(), move |_, _| {
                UsagePeriod::ALL.iter().fold(Menu::new(), |menu, choice| {
                    let (handle, choice) = (handle.clone(), *choice);
                    menu.checked(choice == period, choice.label(), move |_, cx| {
                        handle.update(cx, |store, cx| {
                            store.set_usage_period(choice);
                            cx.notify();
                        })
                    })
                })
            })
            .margin(beside(CONTROL_GAP, 0.))
            .button(|button| button.variant(Variant::Outline).disabled(tab == UsageTab::Limits))
            .into_any_element()
        };
        let handle = self.store.clone();
        let refresh = ActionButton::icon("usage-refresh", "refresh-cw", "Refresh")
            .pending(store.usage.refreshing)
            .margin(beside(CONTROL_GAP, 0.))
            .on_click(move |_, _, cx| {
                handle.update(cx, |store, cx| {
                    store.refresh_usage();
                    cx.notify();
                })
            });
        div().flex().items_center().child(tabs).child(picker).child(refresh)
    }

    fn content(&self, content_width: f32, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.read(cx);
        let loading = || {
            div()
                .w_full()
                .min_h(px(240.))
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors(cx).muted_foreground)
                .child(Spinner::regular().render(cx))
                .into_any_element()
        };
        if store.usage_tab() == UsageTab::Limits {
            return match &store.usage.limits {
                Loaded::Loading => loading(),
                Loaded::Failed(message) => note(message.clone(), cx),
                Loaded::Ready(report) => limits::limits(report, content_width, cx),
            };
        }
        if store.usage_tab() == UsageTab::Activity {
            return match &store.usage.activity {
                Loaded::Loading => loading(),
                Loaded::Failed(message) => note(message.clone(), cx),
                Loaded::Ready(_) => {
                    self.activity.update(cx, |activity, _| activity.width = content_width);
                    self.activity.clone().into_any_element()
                }
            };
        }
        match &store.usage.spending {
            Loaded::Loading => loading(),
            Loaded::Failed(message) => note(message.clone(), cx),
            Loaded::Ready(_) => {
                self.spending.update(cx, |spending, _| spending.width = content_width);
                self.spending.clone().into_any_element()
            }
        }
    }
}

impl Render for UsageRoute {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let content_width = (self.width - 2. * MARGIN).clamp(0., theme::CONTENT_WIDTH);
        let size = ControlSize::Regular;
        let store = self.store.clone();
        let back = ActionButton::new("usage-back", "Back")
            .symbol("arrow-left")
            .ghost()
            .help("Back to the threads (Esc)")
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.close_route();
                    cx.notify();
                })
            });
        let label = self.store.read(cx).usage_servers_label();
        let title_width = text_width("Usage", size.text_size(), FontWeight::MEDIUM, window)
            + text_width("/", size.text_size(), FontWeight::MEDIUM, window)
            + text_width(&label, size.text_size(), FontWeight::MEDIUM, window)
            + 3. * size.padding()
            + 12.;
        let back_width = text_width("Back", size.text_size(), FontWeight::MEDIUM, window) + 2. * size.padding() + 20.;
        let trailing = TRAILING + self.trailing_room;
        let room = self.width - self.title_inset - trailing - back_width - title_width - 2. * size.padding() - 24.;
        let content = self.content(content_width, cx);
        let top_bar = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::TOP_BAR))
            .pl(px(self.title_inset))
            .pr(px(trailing))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(size.padding()))
                    .child(div().ml(px(size.symbol_outset() - size.padding())).mr(px(-size.padding())).child(back))
                    .child(div().w(px(1.)).h(px(14.)).bg(c.border))
                    .child(self.title(cx)),
            )
            .child(div().flex_1().min_w(px(12.)))
            .child(self.controls(room, window, cx));
        div()
            .id("usage-route")
            .occlude()
            .size_full()
            .relative()
            .bg(c.background)
            .text_color(c.foreground)
            .child(
                div().absolute().top(px(theme::TOP_BAR)).left_0().right_0().bottom_0().child(
                    div().id("usage-scroll").size_full().overflow_y_scroll().child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px(px(MARGIN))
                            .pt(px(TOP))
                            .pb(px(80.))
                            .child(div().w_full().max_w(px(theme::CONTENT_WIDTH)).child(content)),
                    ),
                ),
            )
            .child(crate::settings::top_fade(c.background))
            .child(crate::ui::window_drag_strip("usage-top-bar", theme::TOP_BAR, &self.moving))
            .child(top_bar)
    }
}

/// How wide a `Segmented` of these choices is drawn.
fn segmented_width(options: &[&str], window: &mut Window) -> f32 {
    let size = ControlSize::Regular;
    let words: f32 = options
        .iter()
        .map(|option| text_width(option, size.text_size(), FontWeight::MEDIUM, window) + 2. * size.padding())
        .sum();
    words + Segmented::INSET * (options.len() as f32 + 1.) + 2. * Segmented::BORDER
}

/// Why a tab has nothing to show.
pub fn note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .w_full()
        .min_h(px(240.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(TextSize::Xs)
        .text_color(colors(cx).muted_foreground)
        .text_center()
        .child(text.into())
        .into_any_element()
}

/// A bar where a line of `size` will be, in its line and as tall as its capitals, fading in and out.
fn placeholder_bar(id: impl Into<ElementId>, width: f32, size: TextSize, surface: Surface, cx: &App) -> Div {
    let system = cx.text_system();
    let font = system.resolve_font(&crate::transcript::rows::font(theme::UI_FONT, FontWeight::NORMAL, false));
    let cap = f32::from(system.cap_height(font, Pixels::from(size))).round();
    let bar = div().w_full().h(px(cap)).rounded(px(Radius::XS)).bg(surface.layer(Layer::Control, colors(cx)));
    div()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .text_size(size)
        .child(" ")
        .child(div().absolute().inset_0().flex().items_center().child(pulsing(id, bar, cx)))
}
