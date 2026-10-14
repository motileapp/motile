//! The window's frame where the desktop draws none, as GNOME does under Wayland. GPUI Kit draws
//! its edges, which resize the window, its line and its shadow; the top bar is the title bar,
//! with the buttons that minimize, zoom and close the window at the ends the desktop puts them.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, ControlSize};
use crate::ui::{ActionButton, TOOLBAR_MARGIN, even};

/// A window button, with the room around it that the buttons beside it share.
const BUTTON_WIDTH: f32 = ControlSize::Small.height() + 2. * TOOLBAR_MARGIN;
/// How far the buttons at the start of the top bar are from the window's edge: three of them
/// fit before the sidebar's button, where the Mac's window buttons are.
const LEADING_EDGE: f32 = 6.;
/// How far the buttons at the end are from the window's edge, as the right panel's are.
const TRAILING_EDGE: f32 = 10.;
/// The room between the buttons at the end and the top bar's own.
const TRAILING_GAP: f32 = 4.;
/// What the Mac's window buttons take at the start of the top bar.
const MAC_WINDOW_BUTTONS: f32 = 90.;
/// Where the desktop doesn't say: all three at the end, as most Linux desktops have them.
const DEFAULT_TRAILING: [Option<WindowButton>; 3] =
    [Some(WindowButton::Minimize), Some(WindowButton::Maximize), Some(WindowButton::Close)];

/// Whether the app draws the window's frame.
pub fn draws_frame(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

/// What the frame takes of the window on each side: its shadow and its line.
fn frame_insets(window: &Window) -> Edges<Pixels> {
    let Decorations::Client { tiling } = window.window_decorations() else { return Edges::default() };
    let paddings = component::window_paddings(window);
    let line = |tiled: bool| if tiled { px(0.) } else { px(1.) };
    Edges {
        top: paddings.top + line(tiling.top),
        right: paddings.right + line(tiling.right),
        bottom: paddings.bottom + line(tiling.bottom),
        left: paddings.left + line(tiling.left),
    }
}

/// Where the window's content lies: all of the window, or what the frame leaves where the app
/// draws one.
pub fn content_bounds(window: &Window) -> Bounds<Pixels> {
    let insets = frame_insets(window);
    let whole = window.viewport_size();
    Bounds {
        origin: point(insets.left, insets.top),
        size: size(whole.width - insets.left - insets.right, whole.height - insets.top - insets.bottom),
    }
}

pub fn content_size(window: &Window) -> Size<Pixels> {
    content_bounds(window).size
}

pub fn content_origin(window: &Window) -> Point<Pixels> {
    content_bounds(window).origin
}

/// The buttons the desktop puts at the start and at the end of the title bar, of those the
/// window has.
fn layout(window: &Window, cx: &App) -> (Vec<WindowButton>, Vec<WindowButton>) {
    let controls = window.window_controls();
    let has = |button: &WindowButton| match button {
        WindowButton::Minimize => controls.minimize,
        WindowButton::Maximize => controls.maximize,
        WindowButton::Close => true,
    };
    let kept = |side: &[Option<WindowButton>]| side.iter().flatten().copied().filter(has).collect();
    match cx.button_layout() {
        Some(layout) => (kept(&layout.left), kept(&layout.right)),
        None => (Vec::new(), kept(&DEFAULT_TRAILING)),
    }
}

/// How much of the top bar's end the window's buttons take, which what lies there keeps clear.
pub fn trailing_room(window: &Window, cx: &App) -> f32 {
    if !draws_frame(window) {
        return 0.;
    }
    let (_, trailing) = layout(window, cx);
    if trailing.is_empty() { 0. } else { trailing.len() as f32 * BUTTON_WIDTH + TRAILING_GAP }
}

/// Where the top bar's own buttons start: past the Mac's window buttons, past those the app draws
/// at the start, or as far from the edge as the right panel's are from the other.
pub fn leading_room(window: &Window, cx: &App) -> f32 {
    if cfg!(target_os = "macos") {
        return MAC_WINDOW_BUTTONS;
    }
    let leading = if draws_frame(window) { layout(window, cx).0.len() } else { 0 };
    if leading == 0 { TRAILING_EDGE } else { LEADING_EDGE + leading as f32 * BUTTON_WIDTH + TRAILING_GAP }
}

/// The window's buttons at the ends of the top bar, where the app draws the window's frame.
pub fn buttons(window: &Window, cx: &App) -> Option<Div> {
    if !draws_frame(window) {
        return None;
    }
    let (leading, trailing) = layout(window, cx);
    let maximized = window.is_maximized();
    let group = |buttons: Vec<WindowButton>| {
        div()
            .absolute()
            .top_0()
            .h(px(theme::TOP_BAR))
            .flex()
            .items_center()
            .children(buttons.into_iter().map(|button| window_button(button, maximized)))
    };
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::TOP_BAR))
            .child(group(leading).left(px(LEADING_EDGE)))
            .child(group(trailing).right(px(TRAILING_EDGE))),
    )
}

fn window_button(button: WindowButton, maximized: bool) -> ActionButton {
    let (id, symbol, help) = match button {
        WindowButton::Minimize => ("window-minimize", "minus", "Minimize"),
        WindowButton::Maximize if maximized => ("window-maximize", "copy", "Restore"),
        WindowButton::Maximize => ("window-maximize", "square", "Maximize"),
        WindowButton::Close => ("window-close", "x", "Close"),
    };
    let press = move |_: &ClickEvent, window: &mut Window, _: &mut App| match button {
        WindowButton::Minimize => window.minimize_window(),
        WindowButton::Maximize => window.zoom_window(),
        WindowButton::Close => window.remove_window(),
    };
    ActionButton::icon(id, symbol, help).small().margin(even(TOOLBAR_MARGIN)).on_click(press)
}
