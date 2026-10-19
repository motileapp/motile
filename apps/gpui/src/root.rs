//! The window's content: nothing until the core has said what it remembers, then signing in,
//! connecting a first server, or the sidebar and the thread. Modals lie over it.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::main_view::MainView;
use crate::onboarding::{ConnectServer, SignIn};
use crate::store::Store;
use crate::theme::{self, Appearance, colors};
use crate::ui::modal::{Answer, Confirmation, confirmation, error_alert, modal};

pub struct Root {
    store: Entity<Store>,
    sign_in: Option<Entity<SignIn>>,
    /// The page for connecting a first server, or the sheet for adding another.
    connect: Option<Entity<ConnectServer>>,
    main: Option<Entity<MainView>>,
    viewer: Option<Entity<crate::media::viewer::MediaViewer>>,
    panel: Option<Entity<crate::command_panel::CommandPanel>>,
    settings: Option<Entity<crate::settings::SettingsRoute>>,
    appearance: Appearance,
    /// The window is visible: the core has said what it remembers, or something went wrong.
    revealed: bool,
    /// A press in the top bar of a page without one of its own, which moves the window once it
    /// drags.
    moving: std::rc::Rc<std::cell::Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

impl Root {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::transcript::warm_up(window);
        let subscriptions = vec![
            cx.observe_in(&store, window, |this, _, window, cx| {
                this.follow_store(window, cx);
                cx.notify();
            }),
            cx.observe_window_appearance(window, |this, window, cx| this.apply_appearance(window, cx)),
            cx.observe_window_activation(window, |this, window, cx| {
                let active = window.is_window_active();
                this.store.update(cx, |store, cx| store.set_app_active(active, cx));
            }),
            cx.observe_window_bounds(window, |this, window, cx| {
                let full_screen = window.is_fullscreen();
                this.store.update(cx, |store, cx| {
                    crate::app_window::save_frame(&mut store.prefs, window);
                    if store.full_screen != full_screen {
                        store.full_screen = full_screen;
                        cx.notify();
                    }
                });
            }),
        ];
        let mut root = Self {
            store,
            sign_in: None,
            connect: None,
            main: None,
            viewer: None,
            panel: None,
            settings: None,
            appearance: Appearance::System,
            revealed: false,
            moving: Default::default(),
            _subscriptions: subscriptions,
        };
        root.apply_appearance(window, cx);
        crate::app_window::hide(window, colors(cx).background);
        window.activate_window();
        root.follow_store(window, cx);
        root
    }

    pub fn apply_appearance(&mut self, window: &mut Window, cx: &mut App) {
        let appearance = self.store.read(cx).prefs.get::<Appearance>("appearance").unwrap_or_default();
        self.appearance = appearance;
        theme::set_app_appearance(appearance);
        let dark = match appearance {
            Appearance::System => theme::appearance_is_dark(window.appearance()),
            Appearance::Light => false,
            Appearance::Dark => true,
        };
        theme::apply(dark, window, cx);
        crate::app_window::set_background(window, colors(cx).background);
        crate::app_window::set_title_bar_dark(window, dark);
    }

    /// Makes the views the store's state calls for, and lets go of the ones it no longer does.
    fn follow_store(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        if !self.revealed && (store.ready || store.error_message.is_some()) {
            self.revealed = true;
            crate::app_window::reveal(window);
        }
        let signed_in = store.ready && store.account.signed_in;
        let wants_sign_in = store.ready && !store.account.signed_in;
        let first_server = signed_in && store.servers.is_empty();
        let wants_connect = first_server || (signed_in && store.shows_add_server);
        if wants_sign_in && self.sign_in.is_none() {
            let store = self.store.clone();
            self.sign_in = Some(cx.new(|cx| SignIn::new(store, cx)));
        } else if !wants_sign_in {
            self.sign_in = None;
        }
        if self.store.read(cx).prefs.get::<Appearance>("appearance").unwrap_or_default() != self.appearance {
            self.apply_appearance(window, cx);
        }
        // A page asked for while the panel is open starts it again there.
        let page = self.store.read(cx).panel.clone();
        let shown = self.panel.as_ref().map(|panel| panel.read(cx).start.clone());
        if page != shown {
            let store = self.store.clone();
            if page.is_some() {
                crate::ui::overlay_opened(cx);
            }
            self.panel = page.map(|page| cx.new(|cx| crate::command_panel::CommandPanel::new(store, page, window, cx)));
        }
        let wants_settings = self.store.read(cx).settings.section.is_some();
        if wants_settings && self.settings.is_none() {
            let store = self.store.clone();
            self.settings = Some(cx.new(|cx| crate::settings::SettingsRoute::new(store, window, cx)));
        } else if !wants_settings {
            self.settings = None;
        }
        let wants_viewer = self.store.read(cx).viewing.is_some();
        crate::ui::popover_shown("viewer", wants_viewer, cx);
        if wants_viewer && self.viewer.is_none() {
            let store = self.store.clone();
            self.viewer = Some(cx.new(|cx| crate::media::viewer::MediaViewer::new(store, window, cx)));
        } else if !wants_viewer {
            self.viewer = None;
        }
        let store = self.store.read(cx);
        let signed_in = store.ready && store.account.signed_in;
        let first_server = signed_in && store.servers.is_empty();
        let wants_main = signed_in && !first_server;
        if wants_main && self.main.is_none() {
            let store = self.store.clone();
            self.main = Some(cx.new(|cx| MainView::new(store, window, cx)));
        } else if !wants_main {
            self.main = None;
        }
        let shown_first = self.connect.as_ref().map(|connect| connect.read(cx).is_first());
        if wants_connect && shown_first != Some(first_server) {
            let store = self.store.clone();
            self.connect = Some(cx.new(|cx| ConnectServer::new(store, first_server, window, cx)));
            self.store.update(cx, |store, _| store.prepare_to_add_server());
        } else if !wants_connect && self.connect.take().is_some() {
            self.store.update(cx, |store, _| store.stop_adding_server());
        }
    }
}

/// How long the command panel's scrim takes to come and to go.
const PANEL_SCRIM: std::time::Duration = std::time::Duration::from_millis(200);

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let ready = store.ready;
        let first_server = store.account.signed_in && store.servers.is_empty();
        let error = store.error_message.clone();
        let stopping = store.stopping.clone();
        let ui_confirm = store.ui_confirm.clone();
        let error = crate::ui::modal::presented("error", error, window, cx);
        let stopping = crate::ui::modal::presented("stop", stopping, window, cx);
        let ui_confirm = crate::ui::modal::presented("ui-confirm", ui_confirm, window, cx);
        let adding_server = self.connect.clone().filter(|_| !first_server);
        let adding_server = crate::ui::modal::presented("add-server", adding_server, window, cx);
        // One scrim for every page of the command panel, which fades as it opens and closes.
        let panel_scrim = crate::ui::motion::eased_on(
            "command-panel-scrim",
            0.,
            if self.panel.is_some() { 1. } else { 0. },
            PANEL_SCRIM,
            crate::ui::motion::ease_in_out,
            window,
            cx,
        );
        let page: AnyElement = if !ready {
            div().into_any_element()
        } else if let Some(sign_in) = &self.sign_in {
            sign_in.clone().into_any_element()
        } else if let (true, Some(connect)) = (first_server, &self.connect) {
            connect.clone().into_any_element()
        } else if let Some(main) = &self.main {
            main.clone().into_any_element()
        } else {
            div().into_any_element()
        };
        let own_top_bar = self.main.is_some() && self.sign_in.is_none() && !first_server;
        let store_handle = self.store.clone();
        let held = self.store.clone();
        div()
            .size_full()
            .relative()
            .track_focus(&crate::ui::rich_text::selection_focus(cx))
            // Before any row says a press fell in it.
            .capture_any_mouse_down(|event, _, cx| {
                if event.button == MouseButton::Left {
                    crate::ui::rich_text::press_row(None, cx);
                }
            })
            .bg(c.background)
            .text_color(c.foreground)
            .font_family(theme::UI_FONT)
            .text_size(px(13.))
            .line_height(relative(1.21))
            .when(!own_top_bar, |root| {
                root.child(crate::ui::window_drag_strip("page-top-bar", theme::TOP_BAR, &self.moving))
            })
            .child(page)
            .children(self.settings.clone())
            .children(crate::window_frame::buttons(window, cx))
            .when_some(adding_server, |root, (connect, leaving)| {
                let store = self.store.clone();
                root.child(
                    modal("add-server", connect, move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.shows_add_server = false;
                            cx.notify();
                        })
                    })
                    .width(620.)
                    .leaving(leaving),
                )
            })
            .when(panel_scrim > 0., |root| {
                let size = crate::window_frame::content_size(window);
                let scrim = div().w(size.width).h(size.height).bg(c.scrim).opacity(panel_scrim);
                let origin = crate::window_frame::content_origin(window);
                root.child(deferred(anchored().position(origin).child(scrim)).with_priority(1))
            })
            .children(self.panel.clone())
            .children(self.viewer.clone())
            .children(crate::ui::menu::layer(window, cx))
            .when_some(error, |root, (error, leaving)| {
                let (title, message) = error_alert(&error);
                let shown = Confirmation::new(title).message(message).dismissal("OK");
                root.child(
                    confirmation(
                        "error",
                        shown,
                        move |_, cx| {
                            store_handle.update(cx, |store, cx| {
                                store.error_message = None;
                                cx.notify();
                            })
                        },
                        cx,
                    )
                    .leaving(leaving),
                )
            })
            .when_some(stopping, |root, (stopping, leaving)| {
                let (stops, closes) = (self.store.clone(), self.store.clone());
                let shown = Confirmation::new(stopping.question)
                    .message("What they did so far stays in their threads.")
                    .answer(
                        Answer::new("Stop", move |_, cx| {
                            stops.update(cx, |store, _| store.stop_now(&stopping.thread_id));
                        })
                        .danger(),
                    );
                let close = move |_: &mut Window, cx: &mut App| {
                    closes.update(cx, |store, cx| {
                        store.stopping = None;
                        cx.notify();
                    })
                };
                root.child(confirmation("stop", shown, close, cx).leaving(leaving))
            })
            .when_some(ui_confirm, |root, (asked, leaving)| {
                let (allows, refuses) = (self.store.clone(), self.store.clone());
                let shown = Confirmation::new("Run this command?")
                    .message(asked.command)
                    .dismissal("Don't Run")
                    .answer(Answer::new("Run", move |_, cx| {
                        allows.update(cx, |store, cx| {
                            store.answer_ui_confirm(true);
                            cx.notify();
                        })
                    }));
                let close = move |_: &mut Window, cx: &mut App| {
                    refuses.update(cx, |store, cx| {
                        store.answer_ui_confirm(false);
                        cx.notify();
                    })
                };
                root.child(confirmation("ui-confirm", shown, close, cx).leaving(leaving))
            })
            .child(crate::ui::modal::Scrim)
            .child(
                canvas(
                    |_, window, cx| {
                        crate::ui::modal::end_frame(window, cx);
                        crate::keyboard::end_frame(cx);
                    },
                    // Every press, wherever it lands, for the tooltips.
                    move |_, _, window, _| {
                        window.on_mouse_event(|_: &MouseDownEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                crate::ui::mouse_pressed(cx);
                            }
                        });
                        crate::keyboard::watch_modifiers_in(&held, window);
                    },
                )
                .absolute()
                .size_0(),
            )
    }
}
