//! Browses the folders of a server to pick an image as a project's icon.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::models::{Project, RemoteFolder};
use crate::store::Store;
use crate::theme::{Surface, colors};
use crate::ui::sheet::sheet;
use crate::ui::{ActionButton, InputField, Variant, icons};

const ROW_HEIGHT: f32 = 24.;

pub struct FolderPicker {
    store: Entity<Store>,
    project: Project,
    folder: Option<RemoteFolder>,
    path: Entity<InputState>,
    selected: Option<String>,
    error: Option<String>,
    /// The field is to say the path of the folder that was just listed.
    shows_path: bool,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl FolderPicker {
    pub fn new(store: Entity<Store>, project: Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("Path on the server"));
        let subscription = cx.subscribe(&path, |this, path, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let typed = path.read(cx).value().to_string();
                this.load(Some(typed), cx);
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut picker = Self {
            store,
            project,
            folder: None,
            path,
            selected: None,
            error: None,
            shows_path: false,
            focus,
            _subscription: subscription,
        };
        let start = picker.project.path.clone();
        picker.load(Some(start), cx);
        picker
    }

    pub fn project_id(&self) -> &str {
        &self.project.id
    }

    fn load(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        let picker = cx.entity().downgrade();
        let server = self.project.server_id.clone();
        self.store.update(cx, |store, _| {
            store.list_folder(&server, path, true, move |_, result, cx| {
                let _ = picker.update(cx, |this, cx| {
                    match result {
                        Ok(folder) => {
                            this.selected = None;
                            this.error = None;
                            this.shows_path = true;
                            this.folder = Some(folder);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                });
            });
        });
    }

    fn child(&self, name: &str) -> String {
        let base = self.folder.as_ref().map(|folder| folder.path.clone()).unwrap_or_default();
        if base.ends_with('/') { format!("{base}{name}") } else { format!("{base}/{name}") }
    }

    /// What would be chosen: the selected folder or image, or the folder being browsed.
    fn target(&self, cx: &App) -> String {
        match &self.selected {
            Some(selected) => self.child(selected),
            None => self
                .folder
                .as_ref()
                .map(|folder| folder.path.clone())
                .unwrap_or_else(|| self.path.read(cx).value().to_string()),
        }
    }

    fn selected_image(&self) -> Option<String> {
        let selected = self.selected.clone()?;
        self.folder.as_ref()?.files.contains(&selected).then_some(selected)
    }

    fn use_as_icon(&mut self, name: &str, cx: &mut Context<Self>) {
        let (project, path) = (self.project.clone(), self.child(name));
        self.store.update(cx, |store, cx| {
            store.set_icon(&project, Some(path));
            store.icon_project = None;
            cx.notify();
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.icon_project = None;
            cx.notify();
        });
    }

    /// A folder or an image in the list. The rows alternate in colour, and the one picked lies
    /// a layer further.
    fn row(&self, index: usize, name: &str, image: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let selected = self.selected.as_deref() == Some(name);
        let striped = index % 2 == 1;
        let (picked, name_text) = (name.to_string(), name.to_string());
        div()
            .id(SharedString::from(format!("entry-{name}")))
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .mx(px(10.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(13.))
            .when(striped && !selected, |row| row.bg(Surface::Popover.next().color(c).opacity(0.5)))
            .when(!selected, |row| row.hover(|row| row.bg(Surface::Popover.next().color(c))))
            .when(selected, |row| row.bg(Surface::Popover.further().color(c)))
            .child(icons::symbol(if image { "image" } else { "folder" }, 13.).text_color(c.secondary))
            .child(div().truncate().child(name_text))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    if image {
                        this.use_as_icon(&picked, cx);
                    } else {
                        let path = this.child(&picked);
                        this.load(Some(path), cx);
                    }
                    return;
                }
                this.selected = Some(picked.clone());
                cx.notify();
            }))
            .into_any_element()
    }
}

impl Render for FolderPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        if std::mem::take(&mut self.shows_path) {
            let shown = self.folder.as_ref().map(|folder| folder.path.clone()).unwrap_or_default();
            self.path.update(cx, |path, cx| path.set_value(shown, window, cx));
        }
        let parent = self.folder.as_ref().and_then(|folder| folder.parent.clone());
        let mut rows = Vec::new();
        if let Some(folder) = self.folder.clone() {
            for (index, name) in folder.folders.iter().enumerate() {
                rows.push(self.row(index, name, false, cx));
            }
            for (index, name) in folder.files.iter().enumerate() {
                rows.push(self.row(folder.folders.len() + index, name, true, cx));
            }
        }
        let empty = self.folder.as_ref().is_some_and(|folder| folder.folders.is_empty() && folder.files.is_empty());
        let overlay = match (&self.error, empty) {
            (Some(error), _) => Some(div().text_size(px(12.)).text_color(c.danger).p(px(16.)).child(error.clone())),
            (None, true) => Some(div().text_size(px(12.)).text_color(c.tertiary).child("No folders or images in here")),
            _ => None,
        };
        let image = self.selected_image();
        let content = div()
            .id("folder-picker")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss(cx);
                }
            }))
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(18.))
                    .pt(px(18.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Choose an icon for {}", self.project.name)),
            )
            .child(
                div()
                    .px(px(18.))
                    .pt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        ActionButton::icon("enclosing", "chevron-up", "Enclosing folder")
                            .variant(Variant::Secondary)
                            .surface(Surface::Popover)
                            .disabled(parent.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| this.load(parent.clone(), cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(InputField::new(&self.path).monospaced(true).surface(Surface::Popover)),
                    ),
            )
            .child(
                div()
                    .id("entries")
                    .relative()
                    .mt(px(12.))
                    .h(px(300.))
                    .py(px(6.))
                    .overflow_y_scroll()
                    .children(rows)
                    .children(overlay.map(|overlay| {
                        div().absolute().inset_0().flex().items_center().justify_center().child(overlay)
                    })),
            )
            .child(div().h(px(1.)).w_full().flex_shrink_0().bg(c.border))
            .child(
                div()
                    .p(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::theme::MONO_FONT)
                            .text_size(px(12.))
                            .text_color(c.secondary)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_start()
                            .child(self.target(cx)),
                    )
                    .child(
                        ActionButton::new("picker-cancel", "Cancel")
                            .surface(Surface::Popover)
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    )
                    .child(
                        ActionButton::new("use-as-icon", "Use as Icon")
                            .primary()
                            .surface(Surface::Popover)
                            .disabled(image.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(image) = image.clone() {
                                    this.use_as_icon(&image, cx);
                                }
                            })),
                    ),
            );
        sheet("folder-picker-sheet", 520., content, cx)
    }
}
