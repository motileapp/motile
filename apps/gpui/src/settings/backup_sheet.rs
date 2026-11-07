//! The account's S3 bucket to set up or change, which every server backs up to and syncs through.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::Bucket;

use super::account_sheet::field;
use super::page::SettingsPage;
use crate::store::Store;
use crate::theme::colors;
use crate::ui::modal::modal;
use crate::ui::{ActionButton, InputField, beside};

const CAPTION: &str = "One of your servers checks the bucket first. Each server then backs up to it by itself and syncs through it. The keys stay on your servers and this computer.";

pub struct BackupSheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    endpoint: Entity<InputState>,
    region: Entity<InputState>,
    bucket: Entity<InputState>,
    access_key: Entity<InputState>,
    secret_key: Entity<InputState>,
    saving: bool,
    error: Option<String>,
    /// It has closed and fades out.
    pub leaving: bool,
    _subscriptions: Vec<Subscription>,
}

impl BackupSheet {
    pub fn new(
        store: Entity<Store>,
        page: WeakEntity<SettingsPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kept = store.read(cx).backup.as_ref().and_then(|backup| backup.bucket.clone()).unwrap_or_default();
        let mut input = |placeholder: &'static str, value: String| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let endpoint = input("https://<account>.r2.cloudflarestorage.com", kept.endpoint);
        let region = input("us-east-1", kept.region);
        let bucket = input("motile-backup", kept.bucket);
        let access_key = input("", kept.access_key);
        let secret_key = input("", kept.secret_key);
        let subscriptions = [&endpoint, &region, &bucket, &access_key, &secret_key]
            .into_iter()
            .map(|field| cx.subscribe(field, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        endpoint.update(cx, |endpoint, cx| endpoint.focus(window, cx));
        Self {
            store,
            page,
            endpoint,
            region,
            bucket,
            access_key,
            secret_key,
            saving: false,
            error: None,
            leaving: false,
            _subscriptions: subscriptions,
        }
    }

    /// The bucket as the fields have it, trimmed.
    fn edited(&self, cx: &App) -> Bucket {
        let value = |field: &Entity<InputState>| field.read(cx).value().trim().to_string();
        Bucket {
            endpoint: value(&self.endpoint),
            region: value(&self.region),
            bucket: value(&self.bucket),
            access_key: value(&self.access_key),
            secret_key: value(&self.secret_key),
        }
    }

    fn can_save(&self, cx: &App) -> bool {
        let edited = self.edited(cx);
        !self.saving && !edited.bucket.is_empty() && !edited.access_key.is_empty() && !edited.secret_key.is_empty()
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| page.close_backup_sheet(cx));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let bucket = self.edited(cx);
        self.saving = true;
        self.error = None;
        cx.notify();
        let sheet = cx.weak_entity();
        self.store.update(cx, |store, _| {
            store.set_backup(Some(bucket), move |error, cx| {
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

impl Render for BackupSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let caption = |text: &str| Some(text.to_string());
        let (tint, footnote) = match &self.error {
            Some(error) => (c.destructive, error.clone()),
            None => (c.muted_stronger_foreground, CAPTION.to_string()),
        };
        let cancel = ActionButton::new("backup-cancel", "Cancel")
            .margin(beside(0., 4.))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
        let save = ActionButton::new("backup-save", "Save")
            .primary()
            .pending(self.saving)
            .disabled(!self.saving && !self.can_save(cx))
            .margin(beside(4., 0.))
            .on_click(cx.listener(|this, _, _, cx| this.save(cx)));
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_color(c.foreground)
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("S3 Backup"))
            .child(field("Endpoint", caption("Empty for AWS."), InputField::new(&self.endpoint), cx))
            .child(field("Region", caption("auto for Cloudflare R2."), InputField::new(&self.region), cx))
            .child(field("Bucket", None, InputField::new(&self.bucket), cx))
            .child(field("Access key", None, InputField::new(&self.access_key).monospaced(true), cx))
            .child(field("Secret key", None, InputField::new(&self.secret_key).monospaced(true).secure(true), cx))
            .child(div().px(px(4.)).text_size(px(11.5)).text_color(tint).child(footnote))
            .child(div().flex().justify_end().child(cancel).child(save));
        let (close, save) = (cx.weak_entity(), cx.weak_entity());
        modal("backup", div().id("backup-scroll").overflow_y_scroll().child(content), move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(460.)
        .leaving(self.leaving)
    }
}
