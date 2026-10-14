//! Signing in, and connecting a server: the screens before the main window has anything to
//! show.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::models::now;
use crate::store::Store;
use crate::theme::{ControlSize, Radius, colors};
use crate::ui::{ActionButton, CommandBox, Spinner, Variant, icons, logos};

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
        let store_handle = self.store.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .child(div().flex_1())
            .child(logos::motile_mark(48., cx))
            .child(div().mt(px(14.)).text_size(px(30.)).font_weight(FontWeight::SEMIBOLD).child("Motile"))
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(15.))
                    .text_color(c.muted_foreground)
                    .child("The command center for coding agents."),
            )
            .child(
                div().mt(px(34.)).w(px(250.)).child(
                    ActionButton::new("google", "Continue with Google")
                        .picture(logos::google_mark(ControlSize::Large.symbol_side(), cx))
                        .large()
                        .pending(signing_in)
                        .fills(true)
                        .on_click(move |_, _, cx| store_handle.update(cx, |store, cx| store.sign_in(cx))),
                ),
            )
            .when(signing_in, |page| {
                page.child(
                    div()
                        .mt(px(10.))
                        .text_size(px(12.))
                        .text_color(c.muted_stronger_foreground)
                        .child("Waiting for the browser"),
                )
            })
            .when(signing_in && !cfg!(target_os = "macos"), |page| {
                let store = self.store.clone();
                page.child(div().mt(px(6.)).child(
                    ActionButton::new("cancel-sign-in", "Cancel").ghost().small().on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.cancel_sign_in();
                            cx.notify();
                        })
                    }),
                ))
            })
            .when_some(error, |page, error| {
                page.child(
                    div()
                        .mt(px(14.))
                        .max_w(px(360.))
                        .text_size(px(13.))
                        .text_color(c.destructive)
                        .text_center()
                        .child(error),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .mb(px(24.))
                    .px(px(20.))
                    .text_size(px(12.))
                    .text_color(c.muted_stronger_foreground)
                    .text_center()
                    .child(format!(
                        "Signing in links {} to your account. Your threads stay on your own machines.",
                        crate::models::THIS_DEVICE
                    )),
            )
    }
}

/// The one command that turns a machine into a server. Shown when the account has no server yet,
/// and as a sheet when adding another.
pub struct ConnectServer {
    store: Entity<Store>,
    is_first: bool,
    focus: FocusHandle,
    /// Draws the time the command has left again every second.
    _ticking: Task<()>,
}

impl ConnectServer {
    pub fn new(store: Entity<Store>, is_first: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        if !is_first {
            focus.focus(window, cx);
        }
        let ticking = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self { store, is_first, focus, _ticking: ticking }
    }

    pub fn is_first(&self) -> bool {
        self.is_first
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.shows_add_server = false;
            cx.notify();
        });
    }

    fn command_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let token = self.store.read(cx).enroll_token.clone();
        div().w_full().max_w(px(560.)).child(
            CommandBox::new("install-command", token.as_ref().map(|token| token.command.clone()))
                .spans(token.map(|token| token.spans).unwrap_or_default()),
        )
    }

    /// The spinner and the time the command has left, or that it has run out and a way to a new one.
    fn status(&self, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let store = self.store.read(cx);
        let at = now();
        let status = div().flex().flex_col().items_center().gap(px(14.));
        if store.enroll_token.as_ref().is_some_and(|token| token.is_expired(at)) {
            let regenerate = self.store.clone();
            return status
                .child(div().text_size(px(13.)).text_color(c.muted_foreground).child("The command has expired"))
                .child(ActionButton::new("regenerate", "Regenerate").pending(store.enroll_token_pending).on_click(
                    move |_, _, cx| {
                        regenerate.update(cx, |store, cx| {
                            store.regenerate_enroll_token();
                            cx.notify();
                        })
                    },
                ));
        }
        let left = store.enroll_token.as_ref().map_or("15:00".into(), |token| token.time_left(at));
        status
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(c.muted_foreground)
                    .child(Spinner::regular().render(cx))
                    .child(div().text_size(px(13.)).child("Waiting for your server")),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .font_features(crate::ui::tabular_numbers())
                    .text_color(c.muted_stronger_foreground)
                    .child(left),
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
            .px(px(16.))
            .w_full()
            .when(is_first, |page| page.size_full().bg(c.background))
            .flex()
            .flex_col()
            .items_center()
            .when(is_first, |page| page.child(div().flex_1()))
            .child(
                div()
                    .when(!is_first, |icon| icon.mt(px(32.)))
                    .size(px(56.))
                    .flex_shrink_0()
                    .rounded(px(Radius::LG))
                    .bg(c.wash(c.primary, false))
                    .text_color(c.primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol("server", 24.)),
            )
            .child(
                div()
                    .mt(px(18.))
                    .px(px(24.))
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .child(if is_first { "Connect your first server" } else { "Run this on your server" }),
            )
            .child(
                div()
                    .mt(px(8.))
                    .px(px(24.))
                    .max_w(px(480.))
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(c.muted_foreground)
                    .text_center()
                    .child("Run the command below on the server that will run your agents"),
            )
            .child(div().mt(px(26.)).w_full().flex().justify_center().child(self.command_box(cx)))
            .child(div().mt(px(22.)).child(self.status(cx)))
            .when(is_first, |page| {
                page.child(div().flex_1())
                    // Raises the centred content a little above the middle.
                    .child(div().flex_shrink_0().h(relative(0.08)))
                    .child(
                        div()
                            .mb(px(20.))
                            .flex()
                            .items_center()
                            .text_size(px(12.))
                            .text_color(c.muted_stronger_foreground)
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
                        ActionButton::new("done", "Done").on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    ),
                )
            })
    }
}
