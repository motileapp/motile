//! Motile, the command center for coding agents, drawn with GPUI.
// Without a console window on Windows, but while debugging.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_menu;
mod app_window;
mod assets;
mod bridge;
mod command_panel;
mod composer;
mod dev_drive;
mod fonts;
mod format;
mod git;
#[cfg(not(target_os = "macos"))]
mod instance;
mod keyboard;
mod lifecycle;
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
mod tokens;
mod transcript;
mod ui;
mod updater;
mod usage;
mod window_frame;

use futures::StreamExt;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::root::Root;
use crate::store::Store;

fn main() {
    bridge::init_logging();
    #[cfg(target_os = "linux")]
    if std::env::args().skip(1).any(|arg| arg == instance::REGISTER) {
        instance::register_scheme();
        return;
    }
    #[cfg(not(target_os = "macos"))]
    if !updater::relaunched() && instance::hand_over(&instance::opened_urls()) {
        return;
    }
    updater::remove_replaced();
    let app = gpui_kit::application().with_assets(assets::Assets);
    let (url_sender, mut opened_urls) = futures::channel::mpsc::unbounded::<Vec<String>>();
    #[cfg(not(target_os = "macos"))]
    {
        instance::listen(url_sender.clone());
        let opened = instance::opened_urls();
        if !opened.is_empty() {
            let _ = url_sender.unbounded_send(opened);
        }
        if option_env!("MOTILE_PACKAGED").is_none() {
            std::thread::spawn(instance::register_scheme);
        }
    }
    app.on_open_urls(move |urls| {
        let _ = url_sender.unbounded_send(urls);
    });
    app.run(move |cx| {
        gpui_kit::init(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        let fonts = assets::FONTS.iter().map(|bytes| std::borrow::Cow::Borrowed(*bytes)).collect();
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            tracing::error!("the fonts couldn't be loaded: {error:#}");
        }
        fonts::load();
        panel::code_view::bind_keys(cx);

        let store = cx.new(|cx| {
            let mut store = Store::new(cx);
            store.start(cx);
            store
        });
        app_menu::install(&store, cx);
        keyboard::install(&store, cx);
        dev_drive::start(&store, cx);
        lifecycle::start(&store, cx);
        // The browser sends a sign-in back to `motile://auth`, Linear's to `motile://linear` and
        // Ricrac's to `motile://ricrac`, which the system hands to the app. A launch with nothing to hand over brings it forward.
        let signing_in = store.downgrade();
        cx.spawn(async move |cx| {
            while let Some(urls) = opened_urls.next().await {
                cx.update(|cx| {
                    cx.activate(true);
                    if let Some(window) = cx.active_window().or_else(|| cx.windows().into_iter().next()) {
                        let _ = window.update(cx, |_, window, _| window.activate_window());
                    }
                });
                for url in urls {
                    if sign_in::deliver(&url) || !url.starts_with(motile_protocol::APP_REDIRECT) {
                        continue;
                    }
                    let _ = signing_in.update(cx, |store, cx| {
                        store.complete_sign_in(url);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(app_window::bounds(&store.read(cx).prefs, cx)),
            // Shown by the root once it is invisible, which it stays until it has something to show.
            show: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Motile".into()),
                // Elsewhere the system's title bar has the window's buttons, which the app doesn't draw.
                appears_transparent: cfg!(target_os = "macos"),
                traffic_light_position: Some(point(px(19.), px(19.))),
            }),
            window_min_size: Some(size(px(780.), px(500.))),
            // The top bar is drawn here and moves the window itself; AppKit would otherwise hold
            // clicks in it back while it waits to see whether they are double clicks.
            app_owns_titlebar_drag: true,
            app_id: Some("app.motile.motile".into()),
            // Where the desktop draws no frame, as GNOME under Wayland, the app draws its own.
            window_decorations: Some(WindowDecorations::Server),
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
