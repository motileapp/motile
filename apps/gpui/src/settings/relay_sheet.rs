//! The account's own iroh relay to set or change, which every device uses in place of iroh's.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::account_sheet::failing_field;
use super::page::SettingsPage;
use crate::store::Store;
use crate::theme::{TextSize, colors};
use crate::ui::modal::modal;
use crate::ui::{ActionButton, InputField, beside};

const CAPTION: &str = "Every device of your account uses this iroh relay in place of iroh's public ones. Leave it empty to go back to them.";

pub struct RelaySheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    address: Entity<InputState>,
    saving: bool,
    error: Option<String>,
    /// It has closed and fades out.
    pub leaving: bool,
    _subscription: Subscription,
}

impl RelaySheet {
    pub fn new(
        store: Entity<Store>,
        page: WeakEntity<SettingsPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kept = store.read(cx).account.relay.clone().unwrap_or_default();
        let address =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://relay.example.com").default_value(kept));
        let subscription = cx.subscribe(&address, |_, _, _: &InputEvent, cx| cx.notify());
        address.update(cx, |address, cx| address.focus(window, cx));
        Self { store, page, address, saving: false, error: None, leaving: false, _subscription: subscription }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| page.close_relay_sheet(cx));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let address = self.address.read(cx).value().trim().to_string();
        let url = (!address.is_empty()).then_some(address);
        self.saving = true;
        self.error = None;
        cx.notify();
        let sheet = cx.weak_entity();
        self.store.update(cx, |store, _| {
            store.set_relay(url, move |error, cx| {
                let _ = sheet.update(cx, |sheet, cx| {
                    sheet.saving = false;
                    cx.notify();
                    let Some(error) = error else { return sheet.close(cx) };
                    sheet.error = Some(error);
                });
            })
        });
    }
}

impl Render for RelaySheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let cancel = ActionButton::new("relay-cancel", "Cancel")
            .margin(beside(0., 4.))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
        let save = ActionButton::new("relay-save", "Save")
            .primary()
            .pending(self.saving)
            .margin(beside(4., 0.))
            .on_click(cx.listener(|this, _, _, cx| this.save(cx)));
        let address = InputField::new(&self.address).monospaced(true);
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(div().text_size(TextSize::Lg).font_weight(FontWeight::SEMIBOLD).child("Relay"))
            .child(failing_field("", Some(CAPTION.to_string()), self.error.clone(), address, cx))
            .child(div().flex().justify_end().child(cancel).child(save));
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("relay", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}
