//! The symbols the app draws, from Lucide, by the names the Mac app's `Symbol` has for them in
//! kebab case: `git-pull-request`, `square-pen`. The SF Symbol names the first views used still
//! resolve until they are gone.

use gpui_kit::prelude::*;
use gpui_kit::*;

/// The Lucide icon's path for the SF Symbol named.
pub fn path(symbol: &str) -> SharedString {
    if symbol.starts_with("icons/") {
        return SharedString::from(symbol.to_string());
    }
    let lucide = match symbol {
        "arrow.clockwise" => "rotate-cw",
        "arrow.down" => "arrow-down",
        "arrow.down.circle" => "circle-arrow-down",
        "arrow.down.right.and.arrow.up.left" => "minimize-2",
        "arrow.up.left.and.arrow.down.right" => "maximize-2",
        "arrow.left" => "arrow-left",
        "arrow.up" => "arrow-up",
        "arrow.triangle.2.circlepath" => "refresh-cw",
        "arrow.triangle.branch" => "git-branch",
        "arrow.triangle.merge" => "git-pull-request",
        "arrow.turn.left.up" => "corner-left-up",
        "arrow.up.forward.square" | "arrow.up.right.square" => "square-arrow-out-up-right",
        "arrow.uturn.backward" => "undo-2",
        "arrow.uturn.backward.circle" => "rotate-ccw",
        "book.closed" => "book",
        "checkmark" => "check",
        "checkmark.circle" => "circle-check",
        "checkmark.square.fill" => "square-check",
        "checklist" => "list-todo",
        "clock" => "clock",
        "brain" => "brain",
        "square" => "square",
        "circle" => "circle",
        "photo.on.rectangle" => "images",
        "film" => "film",
        "play.rectangle" => "square-play",
        "chevron.left.2" => "chevrons-left",
        "minus.magnifyingglass" => "zoom-out",
        "plus.magnifyingglass" => "zoom-in",
        "arrow.up.and.down.and.arrow.left.and.right" => "move",
        "ellipsis" => "ellipsis",
        "gearshape" => "settings",
        "trash" => "trash-2",
        "link" => "link",
        "key" => "key",
        "externaldrive" => "hard-drive",
        "chevron.down" => "chevron-down",
        "chevron.up.chevron.down" => "chevrons-up-down",
        "chevron.left" => "chevron-left",
        "chevron.right" => "chevron-right",
        "chevron.up" => "chevron-up",
        "chevron.left.forwardslash.chevron.right" => "code",
        "circle.dashed" => "loader-circle",
        "curlybraces" => "braces",
        "doc" => "file",
        "doc.on.doc" => "copy",
        "doc.text" => "file-text",
        "exclamationmark.circle" => "circle-alert",
        "exclamationmark.triangle" => "triangle-alert",
        "eye" => "eye",
        "folder" => "folder",
        "folder.badge.gearshape" => "folder-cog",
        "folder.badge.plus" => "folder-plus",
        "globe" => "globe",
        "icloud.and.arrow.down" => "cloud-download",
        "icloud.and.arrow.up" => "cloud-upload",
        "largecircle.fill.circle" => "circle-dot",
        "list.bullet.clipboard" => "clipboard-list",
        "lock" => "lock",
        "lock.open" => "lock-open",
        "magnifyingglass" => "search",
        "paperclip" => "paperclip",
        "pencil" => "pencil",
        "pencil.line" => "pen-line",
        "person.2" => "users",
        "person.crop.circle" => "circle-user-round",
        "photo" => "image",
        "play.circle.fill" => "circle-play",
        "play.fill" => "play",
        "pause.fill" => "pause",
        "plus" => "plus",
        "plus.square" => "square-plus",
        "plusminus" => "diff",
        "questionmark.bubble" => "message-circle-question-mark",
        "questionmark.circle" => "circle-question-mark",
        "rectangle.compress.vertical" => "fold-vertical",
        "rectangle.expand.vertical" => "unfold-vertical",
        "server.rack" => "server",
        "sidebar.left" => "panel-left",
        "sidebar.right" => "panel-right",
        "smallcircle.filled.circle" => "circle-dot",
        "sparkle" => "sparkle",
        "sparkles" => "sparkles",
        "square.and.pencil" => "square-pen",
        "stop.circle" => "circle-stop",
        "stop.fill" => "square",
        "terminal" => "terminal",
        "text.bubble" => "message-square-text",
        "wrench.and.screwdriver" => "wrench",
        "xmark" => "x",
        "xmark.circle.fill" => "circle-x",
        "github" => "github",
        "google" => "google",
        "claude" => "claude",
        "openai" => "openai",
        other => other,
    };
    SharedString::from(format!("icons/{lucide}.svg"))
}

/// A symbol for text of a size is drawn in a square this much larger, as the Mac app draws it.
const SCALE: f32 = crate::theme::SYMBOL_SCALE;

/// The symbol at `size`, in the colour of the text around it unless it is given one.
pub fn symbol(name: impl AsRef<str>, size: f32) -> Symbol {
    Symbol { path: path(name.as_ref()), size: size * SCALE, color: None, rotation: 0. }
}

#[derive(IntoElement)]
pub struct Symbol {
    path: SharedString,
    size: f32,
    color: Option<Hsla>,
    /// Turned clockwise by this many degrees.
    rotation: f32,
}

impl Symbol {
    pub fn text_color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn rotate(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }
}

impl RenderOnce for Symbol {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(window.text_style().color);
        svg().path(self.path).size(px(self.size)).flex_shrink_0().text_color(color).when(self.rotation != 0., |icon| {
            icon.with_transformation(Transformation::rotate(radians(self.rotation.to_radians())))
        })
    }
}

/// The icon a tool call is drawn with, by the core's name for it.
pub fn tool_symbol(icon: &str) -> &'static str {
    match icon {
        "terminal" => "terminal",
        "file" => "doc.text",
        "edit" => "pencil",
        "search" => "magnifyingglass",
        "web" => "globe",
        "agent" => "person.2",
        "watch" => "eye",
        "question" => "questionmark.bubble",
        "todo" => "checklist",
        _ => "wrench.and.screwdriver",
    }
}
