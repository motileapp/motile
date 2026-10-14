//! The window's frame across launches, and the window kept invisible on the background colour
//! until it has something to show, so that the first thing seen is the finished layout.

use gpui_kit::*;

use crate::prefs::Prefs;

const FRAME_KEY: &str = "window.frame";
const DEFAULT_SIZE: (f32, f32) = (1180., 780.);

/// Where the window was when the app last closed, while that is still on a display; otherwise
/// centred at its first size.
pub fn bounds(prefs: &Prefs, cx: &App) -> WindowBounds {
    let saved = prefs
        .get::<[f32; 4]>(FRAME_KEY)
        .map(|[x, y, width, height]| Bounds { origin: point(px(x), px(y)), size: size(px(width), px(height)) });
    let on_display = saved.filter(|frame| cx.displays().iter().any(|display| display.bounds().intersects(frame)));
    WindowBounds::Windowed(
        on_display.unwrap_or_else(|| Bounds::centered(None, size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)), cx)),
    )
}

/// Keeps the window's frame, as it is when not full screen or zoomed.
pub fn save_frame(prefs: &mut Prefs, window: &Window) {
    let frame = window.window_bounds().get_bounds();
    let frame = [frame.origin.x, frame.origin.y, frame.size.width, frame.size.height].map(f32::from);
    prefs.set(FRAME_KEY, frame);
}

/// Makes the window invisible, its background the colour, until `reveal`.
pub fn hide(window: &Window, background: Hsla) {
    set_background(window, background);
    set_alpha(window, 0.);
}

pub fn reveal(window: &Window) {
    set_alpha(window, 1.);
}

#[cfg(target_os = "macos")]
fn set_alpha(window: &Window, alpha: f64) {
    let Some(native) = native(window) else { return };
    native.setAlphaValue(alpha);
}

/// What shows of the window where nothing is drawn yet, as when it is resized.
#[cfg(target_os = "macos")]
pub fn set_background(window: &Window, color: Hsla) {
    let Some(native) = native(window) else { return };
    let color = Rgba::from(color);
    let color = objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(
        color.r.into(),
        color.g.into(),
        color.b.into(),
        color.a.into(),
    );
    native.setBackgroundColor(Some(&color));
}

#[cfg(target_os = "macos")]
fn native(window: &Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
    let view: &objc2_app_kit::NSView = unsafe { handle.ns_view.cast().as_ref() };
    view.window()
}

#[cfg(not(target_os = "macos"))]
fn set_alpha(_: &Window, _: f64) {}

#[cfg(not(target_os = "macos"))]
pub fn set_background(_: &Window, _: Hsla) {}

/// Has the title bar Windows draws take the app's appearance rather than the system's.
#[cfg(windows)]
pub fn set_title_bar_dark(window: &Window, dark: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};

    let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return };
    let dark = i32::from(dark);
    // SAFETY: the handle is the window's, alive while it is borrowed, and the attribute is a BOOL.
    unsafe {
        DwmSetWindowAttribute(
            handle.hwnd.get() as _,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&raw const dark).cast(),
            size_of::<i32>() as u32,
        );
    }
}

#[cfg(not(windows))]
pub fn set_title_bar_dark(_: &Window, _: bool) {}
