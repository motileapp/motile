//! The window's content: nothing until the core has said what it remembers, then signing in,
//! connecting a first server, or the sidebar and the thread. Sheets and alerts lie over it.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::main_view::MainView;
use crate::onboarding::{ConnectServer, SignIn};
use crate::store::Store;
use crate::theme::{self, Appearance, colors};
use crate::ui::alert::{AlertButton, alert};
use crate::ui::sheet::sheet;

pub struct Root {
    store: Entity<Store>,
    sign_in: Option<Entity<SignIn>>,
    /// The page for connecting a first server, or the sheet for adding another.
    connect: Option<Entity<ConnectServer>>,
    main: Option<Entity<MainView>>,
    viewer: Option<Entity<crate::media::viewer::MediaViewer>>,
    panel: Option<Entity<crate::command_panel::CommandPanel>>,
    icon_picker: Option<Entity<crate::folder_picker::FolderPicker>>,
    settings: Option<Entity<crate::settings::SettingsRoute>>,
    appearance: Appearance,
    _subscriptions: Vec<Subscription>,
}

impl Root {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        ];
        let mut root = Self {
            store,
            sign_in: None,
            connect: None,
            main: None,
            viewer: None,
            panel: None,
            icon_picker: None,
            settings: None,
            appearance: Appearance::System,
            _subscriptions: subscriptions,
        };
        root.apply_appearance(window, cx);
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
    }

    /// Makes the views the store's state calls for, and lets go of the ones it no longer does.
    fn follow_store(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
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
        let icon_project = self.store.read(cx).icon_project.clone();
        let picking = self.icon_picker.as_ref().map(|picker| picker.read(cx).project_id().to_string());
        if icon_project.as_ref().map(|project| project.id.clone()) != picking {
            let store = self.store.clone();
            self.icon_picker = icon_project
                .map(|project| cx.new(|cx| crate::folder_picker::FolderPicker::new(store, project, window, cx)));
        }
        // A page asked for while the panel is open starts it again there.
        let page = self.store.read(cx).panel.clone();
        let shown = self.panel.as_ref().map(|panel| panel.read(cx).start.clone());
        if page != shown {
            let store = self.store.clone();
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

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let ready = store.ready;
        let first_server = store.account.signed_in && store.servers.is_empty();
        let error = store.error_message.clone();
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
        let store_handle = self.store.clone();
        div()
            .size_full()
            .relative()
            .bg(c.background)
            .text_color(c.text)
            .font_family(theme::UI_FONT)
            .text_size(px(13.))
            .line_height(relative(1.21))
            .child(page)
            .children(self.settings.clone())
            .when_some(self.connect.clone().filter(|_| !first_server), |root, connect| {
                root.child(sheet("add-server", 620., connect, cx))
            })
            .children(self.icon_picker.clone())
            .children(self.panel.clone())
            .children(self.viewer.clone())
            .children(crate::ui::menu::layer(_window, cx))
            .when_some(error, |root, error| {
                let ok = AlertButton::new("OK", move |_, cx| {
                    store_handle.update(cx, |store, cx| {
                        store.error_message = None;
                        cx.notify();
                    })
                })
                .prominent();
                root.child(alert("error", "Something went wrong", Some(error.into()), None, vec![ok], cx))
            })
    }
}
