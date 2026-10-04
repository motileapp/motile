//! Motile, the command center for coding agents, drawn with GPUI.
// Removed once every view is ported: until then much of the store has no caller yet.

mod app_menu;
mod assets;
mod bridge;
mod command_panel;
mod composer;
mod folder_picker;
mod git;
mod main_view;
mod media;
mod models;
mod onboarding;
mod panel;
mod prefs;
mod root;
mod settings;
mod sidebar;
mod sign_in;
mod store;
mod theme;
mod thread;
mod transcript;
mod ui;
mod updater;

use futures::StreamExt;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::root::Root;
use crate::store::Store;

pub use app_menu::OpenSettings;

fn main() {
    bridge::init_logging();
    let app = gpui_kit::application().with_assets(assets::Assets);
    let (url_sender, mut opened_urls) = futures::channel::mpsc::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = url_sender.unbounded_send(urls);
    });
    app.run(move |cx| {
        gpui_kit::init(cx);
        panel::code_view::bind_keys(cx);

        let store = cx.new(|cx| {
            let mut store = Store::new(cx);
            store.start(cx);
            store
        });
        app_menu::install(&store, cx);
        // The browser sends a sign-in back to `motile://auth`, which the system hands to the app.
        let signing_in = store.downgrade();
        cx.spawn(async move |cx| {
            while let Some(urls) = opened_urls.next().await {
                let Some(url) = urls.into_iter().find(|url| url.starts_with("motile://auth")) else { continue };
                let _ = signing_in.update(cx, |store, cx| {
                    store.complete_sign_in(url);
                    cx.notify();
                });
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Motile".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(19.), px(19.))),
            }),
            window_min_size: Some(size(px(780.), px(500.))),
            // The top bar is drawn here and moves the window itself; AppKit would otherwise hold
            // clicks in it back while it waits to see whether they are double clicks.
            app_owns_titlebar_drag: true,
            app_id: Some("app.motile.gpui".into()),
            ..Default::default()
        };
        let opened = gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Root::new(store.clone(), window, cx)));
        if let Err(error) = opened {
            tracing::error!("the window couldn't open: {error:#}");
            cx.quit();
        }
        cx.on_app_quit(move |cx| {
            store.read(cx).stop();
            async {}
        })
        .detach();
        cx.activate(true);
    });
}
