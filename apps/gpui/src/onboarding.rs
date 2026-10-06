//! Signing in, and connecting a server: the screens before the main window has anything to
//! show.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::Store;
use crate::theme::{self, Radius, Surface, colors};
use crate::ui::{ActionButton, Spinner, Variant, card_default, icons, logos};

pub struct SignIn {
    store: Entity<Store>,
}

impl SignIn {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store }
    }
}

impl Render for SignIn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let signing_in = store.signing_in;
        let error = store.sign_in_error.clone();
        // An auth server with the dev login lets this client sign in as the address named.
        let dev_login = std::env::var("MOTILE_DEV_LOGIN").ok().filter(|email| !email.is_empty());
        let store_handle = self.store.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .child(div().flex_1())
            .child(
                div()
                    .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.18), 6., 14.))
                    .rounded(px(72. * 0.225))
                    .child(logos::app_logo(72.)),
            )
            .child(div().mt(px(22.)).text_size(px(30.)).font_weight(FontWeight::SEMIBOLD).child("Motile"))
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(15.))
                    .text_color(c.secondary)
                    .child("The command center for coding agents."),
            )
            .child(
                div().mt(px(34.)).w(px(250.)).child(
                    ActionButton::new("google", "Continue with Google")
                        .picture(logos::google_mark(16.))
                        .large()
                        .pending(signing_in)
                        .fills(true)
                        .on_click(move |_, _, cx| {
                            store_handle.update(cx, |store, cx| match &dev_login {
                                Some(email) => store.dev_sign_in(email.clone()),
                                None => store.sign_in(cx),
                            });
                        }),
                ),
            )
            .when(signing_in, |page| {
                page.child(div().mt(px(10.)).text_size(px(12.)).text_color(c.tertiary).child("Waiting for the browser"))
            })
            .when_some(error, |page, error| {
                page.child(
                    div()
                        .mt(px(14.))
                        .max_w(px(360.))
                        .text_size(px(13.))
                        .text_color(c.danger)
                        .text_center()
                        .child(error),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .mb(px(24.))
                    .px(px(30.))
                    .text_size(px(12.))
                    .text_color(c.tertiary)
                    .text_center()
                    .child("Signing in links this Mac to your account. Your threads stay on your own machines."),
            )
    }
}

/// The one command that turns a machine into a server. Shown when the account has no server yet,
/// and as a sheet when adding another.
pub struct ConnectServer {
    store: Entity<Store>,
    is_first: bool,
    copied: bool,
    focus: FocusHandle,
}

impl ConnectServer {
    pub fn new(store: Entity<Store>, is_first: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        if !is_first {
            focus.focus(window, cx);
        }
        Self { store, is_first, copied: false, focus }
    }

    pub fn is_first(&self) -> bool {
        self.is_first
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let Some(command) = self.store.read(cx).enroll_token.as_ref().map(|token| token.command.clone()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(command));
        self.copied = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.shows_add_server = false;
            cx.notify();
        });
    }

    /// Lets the command wrap between any two characters, as CSS's `break-all` does. Not
    /// selectable, since a copy would carry the zero-width spaces into the shell.
    fn breaking_anywhere(text: &str) -> String {
        text.chars().map(|character| character.to_string()).collect::<Vec<_>>().join("\u{200B}")
    }

    fn command_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let command = self.store.read(cx).enroll_token.as_ref().map(|token| token.command.clone());
        let ready = command.is_some();
        let surface = Surface::Background.next();
        card_default(div(), cx)
            .w_full()
            .max_w(px(560.))
            .pl(px(14.))
            .pr(px(10.))
            .py(px(10.))
            .flex()
            .items_start()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family(theme::MONO_FONT)
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(if ready { c.text } else { c.tertiary })
                    .child(match command {
                        Some(command) => Self::breaking_anywhere(&command),
                        None => "Preparing the command…".into(),
                    }),
            )
            .child(
                ActionButton::icon("copy", if self.copied { "check" } else { "copy" }, "Copy the command")
                    .symbol_size(12.)
                    .surface(surface)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
    }
}

impl Render for ConnectServer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let is_first = self.is_first;
        let email = self.store.read(cx).account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let store = self.store.clone();
        div()
            .id("connect-server")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if !is_first && event.keystroke.key == "escape" {
                    this.dismiss(cx);
                }
            }))
            .px(px(30.))
            .w_full()
            .when(is_first, |page| page.size_full())
            .flex()
            .flex_col()
            .items_center()
            .when(is_first, |page| page.child(div().flex_1()))
            .child(
                div()
                    .when(!is_first, |icon| icon.mt(px(32.)))
                    .size(px(56.))
                    .rounded(px(Radius::SHEET))
                    .bg(c.primary.opacity(0.1))
                    .text_color(c.primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol("server", 24.)),
            )
            .child(div().mt(px(18.)).text_size(px(24.)).font_weight(FontWeight::SEMIBOLD).child(if is_first {
                "Connect your first server"
            } else {
                "Add a server"
            }))
            .child(
                div()
                    .mt(px(8.))
                    .max_w(px(480.))
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(c.secondary)
                    .text_center()
                    .child("Run this on the Linux machine or Mac that will run your agents."),
            )
            .child(div().mt(px(26.)).w_full().flex().justify_center().child(self.command_box(cx)))
            .child(
                div()
                    .mt(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(c.secondary)
                    .child(Spinner::regular().render(cx))
                    .child(div().text_size(px(13.)).child("Waiting for your server")),
            )
            .child(
                div()
                    .mt(px(14.))
                    .max_w(px(440.))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(c.tertiary)
                    .text_center()
                    .child("The command works once, for an hour."),
            )
            .when(is_first, |page| {
                page.child(div().flex_1()).child(
                    div()
                        .mb(px(20.))
                        .flex()
                        .items_center()
                        .text_size(px(12.))
                        .text_color(c.tertiary)
                        .child(format!("Signed in as {email}"))
                        .child(ActionButton::new("sign-out", "Sign out").variant(Variant::Link).small().on_click(
                            move |_, _, cx| {
                                store.update(cx, |store, cx| store.sign_out(cx));
                            },
                        )),
                )
            })
            .when(!is_first, |page| {
                page.child(
                    div().py(px(26.)).child(
                        ActionButton::new("done", "Done")
                            .surface(Surface::Popover)
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    ),
                )
            })
    }
}
