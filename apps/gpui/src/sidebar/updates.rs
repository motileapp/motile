//! A new version of the client where the sidebar ends, and what stands at the end of a server's
//! line: `UpdateViews.swift`. Settings draws both too, in another variant and size.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::RestartWhen;

use crate::models::{Server, ServerUpdate};
use crate::store::Store;
use crate::theme::{ControlSize, IconSize, Radius, Surface, colors};
use crate::ui::control::turning_symbol;
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, ActionMenu, Spinner, Variant, tabular_numbers, tooltip};
use crate::updater::UpdateState;

/// What an update is at, after its symbol.
#[derive(IntoElement)]
pub struct UpdateLabel {
    text: SharedString,
    symbol: &'static str,
    tint: Option<Hsla>,
    turning: bool,
}

impl UpdateLabel {
    pub fn new(text: impl Into<SharedString>, symbol: &'static str) -> Self {
        Self { text: text.into(), symbol, tint: None, turning: false }
    }

    pub fn up_to_date(cx: &App) -> Self {
        Self::new("Up to date", "circle-check").tint(colors(cx).success)
    }

    pub fn tint(mut self, tint: Hsla) -> Self {
        self.tint = Some(tint);
        self
    }

    fn turning(mut self, turning: bool) -> Self {
        self.turning = turning;
        self
    }
}

impl RenderOnce for UpdateLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tint = self.tint.unwrap_or(colors(cx).muted_foreground);
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(div().flex_shrink_0().text_color(tint).child(turning_symbol(
                self.symbol.into(),
                IconSize::Sm,
                self.turning,
            )))
            .child(div().min_w_0().text_size(px(12.)).font_weight(FontWeight::MEDIUM).truncate().child(self.text))
    }
}

/// A new version of the client: the offer, the download as it goes, and the restart that
/// finishes it.
#[derive(IntoElement)]
pub struct AppUpdateRow {
    store: Entity<Store>,
    variant: Variant,
    size: ControlSize,
    surface: Surface,
}

impl AppUpdateRow {
    pub fn new(store: &Entity<Store>) -> Self {
        Self {
            store: store.clone(),
            variant: Variant::Secondary,
            size: ControlSize::Small,
            surface: Surface::Background,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    fn button(&self, id: &'static str, title: &'static str, run: fn(&mut Store, &mut Context<Store>)) -> ActionButton {
        let store = self.store.clone();
        ActionButton::new(id, title).surface(self.surface).on_click(move |_, _, cx| {
            store.update(cx, |store, cx| {
                run(store, cx);
                cx.notify();
            })
        })
    }
}

/// A line of the row: the label, and what can be done after it.
fn line(label: UpdateLabel, trailing: Vec<AnyElement>) -> Div {
    div()
        .w_full()
        .min_h(px(ControlSize::Small.height()))
        .flex()
        .items_center()
        .gap(px(7.))
        .child(label)
        .child(div().flex_1().min_w(px(4.)))
        .children(trailing)
}

impl RenderOnce for AppUpdateRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let state = self.store.read(cx).updater.state.clone();
        let size = self.size;
        match state {
            UpdateState::Idle => div(),
            UpdateState::Checking => line(UpdateLabel::new("Checking for updates", "refresh-cw").turning(true), vec![]),
            UpdateState::UpToDate => line(UpdateLabel::up_to_date(cx), vec![]),
            UpdateState::Available(version) => line(
                UpdateLabel::new(format!("v{version} is available"), "circle-arrow-down").tint(c.success),
                vec![
                    self.button("app-update", "Update", |store, cx| store.updater.install(cx))
                        .variant(self.variant)
                        .size(size)
                        .into_any_element(),
                ],
            ),
            UpdateState::Downloading(version, fraction) => {
                let store = self.store.clone();
                let cancel = ActionButton::icon("cancel-update", "x", "Cancel the download")
                    .size(size)
                    .surface(self.surface)
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.updater.cancel_download();
                            cx.notify();
                        })
                    });
                let percent = div()
                    .text_size(px(11.))
                    .text_color(c.muted_stronger_foreground)
                    .font_features(tabular_numbers())
                    .child(format!("{}%", (fraction * 100.) as u32));
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(line(
                        UpdateLabel::new(format!("Downloading v{version}"), "circle-arrow-down").tint(c.foreground),
                        vec![percent.into_any_element(), cancel.into_any_element()],
                    ))
                    .child(progress_bar(fraction, self.surface, cx))
            }
            UpdateState::Installing(version) => line(
                UpdateLabel::new(format!("Installing v{version}"), "circle-arrow-down").tint(c.foreground),
                vec![
                    div()
                        .text_color(c.muted_foreground)
                        .child(Spinner::new(ControlSize::Small.symbol()).render(cx))
                        .into_any_element(),
                ],
            ),
            UpdateState::Ready(version) => {
                let restart = ActionButton::new("app-restart", "Restart")
                    .small()
                    .surface(self.surface)
                    .on_click(|_, _, cx| crate::updater::relaunch(cx));
                line(
                    UpdateLabel::new(format!("v{version} is installed"), "circle-check").tint(c.success),
                    vec![restart.into_any_element()],
                )
            }
            UpdateState::Failed(message) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(line(
                    UpdateLabel::new("The update didn’t work", "triangle-alert"),
                    vec![
                        self.button("app-retry", "Try Again", |store, cx| store.updater.retry(cx))
                            .small()
                            .into_any_element(),
                    ],
                ))
                .child(div().text_size(px(11.)).text_color(c.muted_foreground).child(message)),
        }
    }
}

/// How far a download has come, in a thin track.
fn progress_bar(fraction: f64, surface: Surface, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .w_full()
        .h(px(4.))
        .rounded(px(Radius::XS))
        .bg(surface.layer(crate::theme::Layer::Control, c))
        .child(div().h_full().w(relative(fraction.clamp(0., 1.) as f32)).rounded(px(Radius::XS)).bg(c.muted_foreground))
}

/// What stands at the end of a server's line: the offer to update it, the update as it goes, or
/// `otherwise`. While agents work there, the server restarts once they have finished, or at once
/// with their threads going on after.
#[derive(IntoElement)]
pub struct ServerUpdateStatus {
    store: Entity<Store>,
    server: Server,
    variant: Variant,
    size: ControlSize,
    surface: Surface,
    otherwise: Option<AnyElement>,
}

impl ServerUpdateStatus {
    pub fn new(store: &Entity<Store>, server: &Server) -> Self {
        Self {
            store: store.clone(),
            server: server.clone(),
            variant: Variant::Secondary,
            size: ControlSize::Small,
            surface: Surface::Background,
            otherwise: None,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn otherwise(mut self, otherwise: impl IntoElement) -> Self {
        self.otherwise = Some(otherwise.into_any_element());
        self
    }

    fn update(&self, when: RestartWhen) -> impl Fn(&mut Window, &mut App) + 'static {
        let (store, server) = (self.store.clone(), self.server.clone());
        move |_, cx| {
            store.update(cx, |store, cx| {
                store.update_server(&server, when);
                cx.notify();
            })
        }
    }
}

fn progress(update: &ServerUpdate) -> String {
    if update.restarting {
        return "Restarting".into();
    }
    match update.fraction {
        Some(fraction) => format!("Updating {}%", (fraction * 100.) as u32),
        None => "Updating".into(),
    }
}

impl RenderOnce for ServerUpdateStatus {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let server = &self.server;
        let id = |name: &str| SharedString::from(format!("{name}-{}", server.id));
        let update = store.server_update(server).cloned();
        if update.as_ref().is_some_and(|update| update.waiting && !update.restarting) {
            let restart = self.update(RestartWhen::Now);
            return div()
                .id(id("waiting"))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().text_size(px(11.)).text_color(c.muted_foreground).child("Waiting"))
                .child(
                    ActionButton::new(id("restart-now"), "Restart Now")
                        .help(format!(
                            "Stop the agents on {} and restart it. Their threads continue once it is back.",
                            server.name
                        ))
                        .variant(self.variant)
                        .size(self.size)
                        .surface(self.surface)
                        .on_click(move |_, window, cx| restart(window, cx)),
                )
                .tooltip(tooltip(format!("{} restarts once its agents have finished", server.name)))
                .into_any_element();
        }
        if let Some(update) = update {
            return div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(c.muted_foreground)
                .child(div().text_size(px(11.)).font_features(tabular_numbers()).child(progress(&update)))
                .child(Spinner::new(self.size.symbol()).render(cx))
                .into_any_element();
        }
        if !store.is_outdated(server) {
            return self.otherwise.unwrap_or_else(|| div().into_any_element());
        }
        let version = store.updater.latest.clone().unwrap_or_default();
        if store.is_busy(server) && server.can_choose_restart() {
            let (later, now) = (self.update(RestartWhen::Idle), self.update(RestartWhen::Now));
            let later = std::rc::Rc::new(later);
            let now = std::rc::Rc::new(now);
            return ActionMenu::new(id("update-menu"), "Update", move |_, _| {
                let (later, now) = (later.clone(), now.clone());
                Menu::new()
                    .item("Update when agents finish", move |window, cx| later(window, cx))
                    .item("Update now and auto-resume agents", move |window, cx| now(window, cx))
            })
            .button(|button| {
                button
                    .help(format!(
                        "Agents are working on {}. Update it once they finish, or now: they stop and continue once it is back.",
                        server.name
                    ))
                    .variant(self.variant)
                    .size(self.size)
                    .surface(self.surface)
            })
            .into_any_element();
        }
        let help = if server.can_choose_restart() {
            format!("Install version {version} on {}. It restarts once no agent works there.", server.name)
        } else {
            format!("Install version {version} on {}. It restarts, and no agent may be working.", server.name)
        };
        let update = self.update(RestartWhen::Idle);
        ActionButton::new(id("update"), "Update")
            .help(help)
            .variant(self.variant)
            .size(self.size)
            .surface(self.surface)
            .on_click(move |_, window, cx| update(window, cx))
            .into_any_element()
    }
}
