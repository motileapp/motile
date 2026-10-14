//! The keyboard shortcuts, as the Mac app's `ShortcutMonitor` runs them: every key press meets
//! the user's rules before the menus and the focused view see it, the threads' keys show in the
//! sidebar while the modifiers that open them are held, and Esc does what nothing focused claimed.

use std::cell::Cell;
use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::panel::state::PanelTab;
use crate::store::Store;
use crate::store::shortcuts::KeyPress;

/// The commands that run again while their keys are held.
const REPEATING: [&str; 4] = ["thread.previous", "thread.next", "rightPanel.nextTab", "rightPanel.previousTab"];

thread_local! {
    /// The key being handled was sent by the dev driver, not the system.
    static SYNTHETIC: Cell<bool> = const { Cell::new(false) };
}

pub fn install(store: &Entity<Store>, cx: &mut App) {
    let monitor = store.clone();
    cx.intercept_keystrokes(move |event, window, cx| {
        if handle(&monitor, event, window, cx) {
            cx.stop_propagation();
        }
    })
    .detach();
    let escaping = store.clone();
    cx.observe_keystrokes(move |event, _, cx| {
        let keystroke = &event.keystroke;
        if event.action.is_some() || keystroke.key != "escape" || keystroke.modifiers.modified() {
            return;
        }
        escaping.update(cx, |store, cx| {
            if store.escape() {
                cx.notify();
            }
        });
    })
    .detach();
    #[cfg(target_os = "macos")]
    watch_modifiers(store, cx);
}

/// Runs the command the press has, when one does. Whether it was taken.
fn handle(store: &Entity<Store>, event: &KeystrokeEvent, window: &mut Window, cx: &mut App) -> bool {
    if store.read(cx).recording_shortcut.is_some() {
        record(store, &event.keystroke, cx);
        return true;
    }
    let state = store.read(cx);
    if state.viewing.is_some() || crate::ui::modal::is_open(cx) || crate::ui::menu::is_open(cx) {
        return false;
    }
    let Some(press) = press_of(&event.keystroke) else { return false };
    let editing = event.context_stack.iter().any(|context| context.contains("Input"));
    let composing = editing && event.context_stack.iter().any(|context| context.contains("Composer"));
    let context = state.shortcut_context(composing, editing);
    let Some(command) = state.shortcuts.command_for(&press, &context).map(String::from) else {
        // ⌘+, ⌘−, ⌘0 and ⌘9 zoom the image file the side panel shows.
        let image_shown = state.side_panel.is_open && matches!(state.panel_tabs().active, Some(PanelTab::File(_)));
        return image_shown && crate::media::zoom::ZoomableImage::key(&event.keystroke, cx);
    };
    // The command panel takes its own keys, the digits of its rows among them.
    if state.panel.is_some() && command != "commandPalette.toggle" && command != "threadPicker.toggle" {
        return false;
    }
    if is_repeat() && !REPEATING.contains(&command.as_str()) {
        return true;
    }
    perform(store, &command, window, cx)
}

/// The key a shortcut field records: the press with its modifiers. Esc alone stops it.
fn record(store: &Entity<Store>, keystroke: &Keystroke, cx: &mut App) {
    let key = press_of(keystroke).filter(|press| !press.is_escape()).map(|press| press.rule());
    store.update(cx, |store, cx| {
        if let Some(field) = store.recording_shortcut.take() {
            store.recorded_shortcut = Some((field, key));
        }
        cx.notify();
    });
}

/// Runs the command: what the window does itself here, the rest in the store. Whether there was
/// anything to do.
pub fn perform(store: &Entity<Store>, command: &str, window: &mut Window, cx: &mut App) -> bool {
    match command {
        "rightPanel.close" => {
            close(store, window.window_handle(), cx);
            true
        }
        "modelPicker.toggle" | "composer.effort" | "composer.access" | "composer.workspace" => {
            !store.read(cx).composer_away() && press(command, window, cx)
        }
        _ => store.update(cx, |store, cx| {
            let done = store.perform(command, cx);
            if done {
                cx.notify();
            }
            done
        }),
    }
}

/// Leaves a route, else closes the side panel's tab, else the window, which ends the app as
/// on the Mac. The window is busy with the key until it has been handled.
fn close(store: &Entity<Store>, window: AnyWindowHandle, cx: &mut App) {
    let store = store.clone();
    cx.defer(move |cx| {
        if !crate::settings::is_main_window(window, cx) {
            let _ = window.update(cx, |_, window, _| window.remove_window());
            return;
        }
        let closed = store.update(cx, |store, cx| {
            cx.notify();
            if store.settings.section.is_some() || store.shows_usage {
                store.close_route();
                return true;
            }
            store.close_active_tab()
        });
        if !closed {
            cx.quit();
        }
    });
}

/// Runs `dispatch` as keys the dev driver sends, which never repeat.
pub fn synthetic<R>(dispatch: impl FnOnce() -> R) -> R {
    SYNTHETIC.set(true);
    let result = dispatch();
    SYNTHETIC.set(false);
    result
}

/// The key the system is handing over, unless the dev driver sent it.
#[cfg(target_os = "macos")]
fn key_down() -> Option<objc2::rc::Retained<objc2_app_kit::NSEvent>> {
    use objc2_app_kit::{NSApplication, NSEventType};
    if SYNTHETIC.get() {
        return None;
    }
    let main = objc2::MainThreadMarker::new()?;
    let event = NSApplication::sharedApplication(main).currentEvent()?;
    (event.r#type() == NSEventType::KeyDown).then_some(event)
}

/// The press as the rules name it: by the key's place and the layout's letters, as the Mac app's
/// `KeyNames` does, when the system's event is at hand; else as GPUI names it.
fn press_of(keystroke: &Keystroke) -> Option<KeyPress> {
    #[cfg(target_os = "macos")]
    if let Some(event) = key_down() {
        use objc2_app_kit::NSEventModifierFlags;
        let typed = event.charactersIgnoringModifiers().map(|typed| typed.to_string());
        let flags = event.modifierFlags();
        let key = crate::store::shortcuts::TypedKey { code: event.keyCode(), typed: typed.as_deref() };
        return KeyPress::of_key(
            key,
            flags.contains(NSEventModifierFlags::Command),
            flags.contains(NSEventModifierFlags::Control),
            flags.contains(NSEventModifierFlags::Option),
            flags.contains(NSEventModifierFlags::Shift),
        );
    }
    KeyPress::of(keystroke)
}

/// Whether the key being handled is the system repeating a held key.
fn is_repeat() -> bool {
    #[cfg(target_os = "macos")]
    return key_down().is_some_and(|event| event.isARepeat());
    #[cfg(not(target_os = "macos"))]
    false
}

/// Tells the store which modifiers are held, for the threads' keys in the sidebar: GPUI says so
/// off the Mac, as the window draws.
#[cfg(not(target_os = "macos"))]
pub fn watch_modifiers_in(store: &Entity<Store>, window: &mut Window) {
    let store = store.clone();
    window.on_modifiers_changed(move |event, _, cx| {
        let held = Modifiers {
            platform: event.modifiers.platform,
            control: event.modifiers.control,
            alt: event.modifiers.alt,
            shift: event.modifiers.shift,
            function: false,
        };
        store.update(cx, |store, cx| {
            store.hold_modifiers(held, cx);
            cx.notify();
        });
    });
}

/// The Mac's own monitor of the modifiers says which are held.
#[cfg(target_os = "macos")]
pub fn watch_modifiers_in(_: &Entity<Store>, _: &mut Window) {}

/// Tells the store which modifiers are held, for the threads' keys in the sidebar.
#[cfg(target_os = "macos")]
fn watch_modifiers(store: &Entity<Store>, cx: &mut App) {
    use std::ptr::NonNull;

    use futures::StreamExt;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};

    let (sender, mut held) = futures::channel::mpsc::unbounded::<Modifiers>();
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor an event that lives while it runs.
        let flags = unsafe { event.as_ref() }.modifierFlags();
        let _ = sender.unbounded_send(Modifiers {
            platform: flags.contains(NSEventModifierFlags::Command),
            control: flags.contains(NSEventModifierFlags::Control),
            alt: flags.contains(NSEventModifierFlags::Option),
            shift: flags.contains(NSEventModifierFlags::Shift),
            function: false,
        });
        event.as_ptr()
    });
    // SAFETY: the block hands back the event it was given.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::FlagsChanged, &block) };
    // The monitor watches for as long as the app runs.
    std::mem::forget(monitor);
    let store = store.downgrade();
    cx.spawn(async move |cx| {
        while let Some(held) = held.next().await {
            let _ = store.update(cx, |store, cx| {
                store.hold_modifiers(held, cx);
                cx.notify();
            });
        }
    })
    .detach();
}

/// The controls a shortcut presses as a click would: the composer's menus, where they were drawn
/// in the last frame.
#[derive(Default)]
struct Targets {
    drawing: HashMap<&'static str, Bounds<Pixels>>,
    drawn: HashMap<&'static str, Bounds<Pixels>>,
}

impl Global for Targets {}

/// Fills its parent, which makes the parent the control the command's shortcut presses.
pub fn target(command: &'static str) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            cx.default_global::<Targets>().drawing.insert(command, bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// Forgets the controls that weren't drawn this frame. The window's root runs it last.
pub fn end_frame(cx: &mut App) {
    let targets = cx.default_global::<Targets>();
    targets.drawn = std::mem::take(&mut targets.drawing);
}

/// Clicks the control of the command, if it is on screen, and puts the pointer back.
fn press(command: &str, window: &mut Window, cx: &mut App) -> bool {
    let bounds = cx.try_global::<Targets>().and_then(|targets| targets.drawn.get(command).copied());
    let Some(bounds) = bounds.filter(|bounds| bounds.size.width > px(0.)) else { return false };
    let center = bounds.center();
    window.defer(cx, move |window, cx| {
        let back = window.mouse_position();
        let modifiers = Modifiers::default();
        for event in [
            PlatformInput::MouseMove(MouseMoveEvent { position: center, pressed_button: None, modifiers }),
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: center,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: center,
                modifiers,
                click_count: 1,
            }),
            PlatformInput::MouseMove(MouseMoveEvent { position: back, pressed_button: None, modifiers }),
        ] {
            window.dispatch_event(event, cx);
        }
    });
    true
}
