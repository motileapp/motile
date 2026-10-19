//! Input from a script, for the dev app. With `MOTILE_DEV_DRIVE=1` the app reads lines from the
//! `drive` pipe in its data folder and does each to its window as the user's mouse and keyboard
//! would, then writes the token of a `done` line to `drive.done`. Once the pipe is there it writes
//! the app's pid to `drive.ready`, which `scripts/do.sh` waits for before it writes:
//!
//!   click x y | rclick x y | move x y | scroll x y dy   points from the window's top left
//!   wheel x y lines                                     a mouse wheel's notches, not a trackpad
//!   attach /path/to/file                                attaches a file to what is being written
//!   key cmd-k | type some text | mods cmd-shift | mods  keys as GPUI writes them
//!   perform thread.next | wait 0.5 | done <token>
//!   down x y | drag x y | up x y                        a press that moves, and its release
//!   updater available 9.9.9                             shows the update row in that state
//!   active on | active off                              as if the window came to the front or left it
//!   slide <label> <share> | press <label>               drags the latest program's slider with that label
//!                                                       to a share of its track, or presses its button

use std::io::BufRead;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use gpui_kit::*;

use crate::store::Store;

/// The time each line leaves the window to draw what it did before the next one.
const PAUSE: Duration = Duration::from_millis(150);

pub fn start(store: &Entity<Store>, cx: &mut App) {
    if std::env::var("MOTILE_DEV_DRIVE").as_deref() != Ok("1") {
        return;
    }
    let dir = store.read(cx).data_dir.clone();
    let pipe = dir.join("drive");
    let _ = std::fs::remove_file(&pipe);
    let made = std::process::Command::new("mkfifo").arg(&pipe).status();
    if !made.is_ok_and(|status| status.success()) {
        tracing::error!("the dev driver's pipe couldn't be made at {}", pipe.display());
        return;
    }
    let _ = std::fs::write(dir.join("drive.ready"), std::process::id().to_string());
    let (sender, mut lines) = futures::channel::mpsc::unbounded::<String>();
    std::thread::spawn(move || {
        loop {
            // Opening waits for a script to write; its end is the script closing it.
            let Ok(file) = std::fs::File::open(&pipe) else { return };
            for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
                if sender.unbounded_send(line).is_err() {
                    return;
                }
            }
        }
    });
    let store = store.clone();
    let done = dir.join("drive.done");
    cx.spawn(async move |cx| {
        while let Some(line) = lines.next().await {
            if let Some(("slide" | "press", _)) = line.trim().split_once(' ') {
                touch_program(&line, &store, &done, cx).await;
                continue;
            }
            let wait = run(&line, &store, &done, cx);
            cx.background_executor().timer(wait.unwrap_or(PAUSE)).await;
        }
    })
    .detach();
}

/// Drags or presses a node of the open thread's latest program, by its label, where the next frame
/// draws it: a press down, moves along the track, and up.
async fn touch_program(line: &str, store: &Entity<Store>, done: &PathBuf, cx: &mut AsyncApp) {
    let (verb, rest) = line.trim().split_once(' ').unwrap_or_default();
    let (label, share) = match (verb, rest.rsplit_once(' ')) {
        ("slide", Some((label, share))) => (label, share.parse::<f32>().unwrap_or(0.5)),
        _ => (rest, 0.5),
    };
    let Some(id) = cx.update(|cx| labelled(store, label, cx)) else {
        return tracing::warn!("the latest program has no {label}");
    };
    cx.update(|cx| {
        crate::transcript::openui::record_frames();
        if let Some(window) = cx.windows().into_iter().next() {
            let _ = window.update(cx, |_, window, _| window.refresh());
        }
    });
    cx.background_executor().timer(PAUSE).await;
    let frames = crate::transcript::openui::recorded_frames();
    let Some((_, bounds)) = frames.iter().rev().find(|(node, _)| *node == id) else {
        return tracing::warn!("{label} isn't on screen");
    };
    let (left, width) = (f32::from(bounds.left()), f32::from(bounds.size.width));
    let (from, to, y, steps) = match verb {
        "slide" => (left + width / 2., left + width * share, f32::from(bounds.bottom()) - 8., 10),
        _ => (left + width / 2., left + width / 2., f32::from(bounds.center().y), 0),
    };
    let moves = (1..=steps).map(|step| format!("drag {} {y}", from + (to - from) * step as f32 / steps as f32));
    for line in std::iter::once(format!("down {from} {y}")).chain(moves).chain([format!("up {to} {y}")]) {
        run(&line, store, done, cx);
        cx.background_executor().timer(Duration::from_millis(16)).await;
    }
}

/// The id of the latest program's slider or button read by the label.
fn labelled(store: &Entity<Store>, label: &str, cx: &App) -> Option<String> {
    use motile_core::render::rows::RowKind;
    fn find(node: &serde_json::Value, label: &str) -> Option<String> {
        match node {
            serde_json::Value::Object(map) => {
                let named = map.get("a11y").and_then(|a11y| a11y["label"].as_str()) == Some(label);
                let control = matches!(map.get("component").and_then(|kind| kind.as_str()), Some("Slider" | "Button"));
                if named && control {
                    return map.get("id").and_then(|id| id.as_str()).map(str::to_string);
                }
                map.values().find_map(|value| find(value, label))
            }
            serde_json::Value::Array(list) => list.iter().find_map(|value| find(value, label)),
            _ => None,
        }
    }
    let transcript = store.read(cx).transcript.read(cx);
    let tree = transcript.rows.iter().rev().find_map(|model| match &model.row.kind {
        RowKind::Ui { ui } => ui.tree.as_deref(),
        _ => None,
    })?;
    find(&serde_json::to_value(tree).ok()?, label)
}

/// Does what the line says. How long to wait after it, when it says.
fn run(line: &str, store: &Entity<Store>, done: &PathBuf, cx: &mut AsyncApp) -> Option<Duration> {
    let (verb, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    match verb {
        "done" => {
            let _ = std::fs::write(done, rest);
            Some(Duration::ZERO)
        }
        "wait" => rest.parse::<f32>().ok().map(Duration::from_secs_f32),
        _ => {
            cx.update(|cx| {
                let Some(window) = cx.windows().into_iter().next() else { return };
                let _ = window.update(cx, |_, window, cx| act(verb, rest, store, window, cx));
            });
            None
        }
    }
}

fn act(verb: &str, rest: &str, store: &Entity<Store>, window: &mut Window, cx: &mut App) {
    let numbers: Vec<f32> = rest.split_whitespace().filter_map(|number| number.parse().ok()).collect();
    let at = |index: usize| {
        point(px(numbers.get(index).copied().unwrap_or(0.)), px(numbers.get(index + 1).copied().unwrap_or(0.)))
    };
    let modifiers = window.modifiers();
    let click = |button: MouseButton, window: &mut Window, cx: &mut App| {
        let position = at(0);
        for event in [
            PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: None, modifiers }),
            PlatformInput::MouseDown(MouseDownEvent {
                button,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::MouseUp(MouseUpEvent { button, position, modifiers, click_count: 1 }),
        ] {
            window.dispatch_event(event, cx);
        }
    };
    match verb {
        "click" => click(MouseButton::Left, window, cx),
        "rclick" => click(MouseButton::Right, window, cx),
        "move" => {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent { position: at(0), pressed_button: None, modifiers }),
                cx,
            );
        }
        "scroll" => {
            let delta = ScrollDelta::Pixels(point(px(0.), px(numbers.get(2).copied().unwrap_or(0.))));
            let event = ScrollWheelEvent { position: at(0), delta, modifiers, touch_phase: TouchPhase::Moved };
            window.dispatch_event(PlatformInput::ScrollWheel(event), cx);
        }
        // A trackpad's gesture: fingers down, moving by `dy` in steps, and up.
        "swipe" => {
            let steps = 8;
            let step = numbers.get(2).copied().unwrap_or(0.) / steps as f32;
            let phases = std::iter::once((TouchPhase::Started, 0.))
                .chain((0..steps).map(|_| (TouchPhase::Moved, step)))
                .chain(std::iter::once((TouchPhase::Ended, 0.)));
            for (touch_phase, dy) in phases {
                let delta = ScrollDelta::Pixels(point(px(0.), px(dy)));
                let event = ScrollWheelEvent { position: at(0), delta, modifiers, touch_phase };
                window.dispatch_event(PlatformInput::ScrollWheel(event), cx);
            }
        }
        "wheel" => {
            let delta = ScrollDelta::Lines(point(0., numbers.get(2).copied().unwrap_or(0.)));
            let event = ScrollWheelEvent { position: at(0), delta, modifiers, touch_phase: TouchPhase::Moved };
            window.dispatch_event(PlatformInput::ScrollWheel(event), cx);
        }
        "key" => {
            let Ok(keystroke) = Keystroke::parse(rest) else {
                return tracing::warn!("the dev driver can't read the keys {rest}");
            };
            crate::keyboard::synthetic(|| window.dispatch_keystroke(keystroke, cx));
        }
        "type" => {
            for typed in rest.chars() {
                let key = if typed == ' ' { "space".to_string() } else { typed.to_lowercase().to_string() };
                let keystroke = Keystroke { modifiers: Modifiers::default(), key, key_char: Some(typed.to_string()) };
                crate::keyboard::synthetic(|| window.dispatch_keystroke(keystroke, cx));
            }
        }
        "mods" => {
            let held = Keystroke::parse(&format!("{rest}-a")).map(|keystroke| keystroke.modifiers).unwrap_or_default();
            let event = ModifiersChangedEvent { modifiers: held, capslock: Default::default() };
            window.dispatch_event(PlatformInput::ModifiersChanged(event), cx);
            store.update(cx, |store, cx| {
                store.hold_modifiers(held, cx);
                cx.notify();
            });
        }
        "attach" => {
            let file = std::path::PathBuf::from(rest.trim());
            store.update(cx, |store, cx| {
                store.attach(vec![file]);
                cx.notify();
            });
        }
        "perform" => {
            crate::keyboard::perform(store, rest.trim(), window, cx);
        }
        "down" => {
            let event = MouseDownEvent {
                button: MouseButton::Left,
                position: at(0),
                modifiers,
                click_count: 1,
                first_mouse: false,
            };
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent { position: at(0), pressed_button: None, modifiers }),
                cx,
            );
            window.dispatch_event(PlatformInput::MouseDown(event), cx);
        }
        "drag" => {
            let event = MouseMoveEvent { position: at(0), pressed_button: Some(MouseButton::Left), modifiers };
            window.dispatch_event(PlatformInput::MouseMove(event), cx);
        }
        "up" => {
            let event = MouseUpEvent { button: MouseButton::Left, position: at(0), modifiers, click_count: 1 };
            window.dispatch_event(PlatformInput::MouseUp(event), cx);
        }
        "active" => {
            store.update(cx, |store, cx| {
                store.set_app_active(rest.trim() == "on", cx);
                cx.notify();
            });
        }
        "fileenter" => {
            let files = rest.split_whitespace().skip(2).map(std::path::PathBuf::from).collect();
            let event = FileDropEvent::Entered { position: at(0), paths: ExternalPaths(files) };
            window.dispatch_event(PlatformInput::FileDrop(event), cx);
        }
        "filemove" => {
            window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Pending { position: at(0) }), cx);
        }
        "filedrop" => {
            window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Submit { position: at(0) }), cx);
        }
        "fileexit" => {
            window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Exited), cx);
        }
        "signin" => {
            let email = rest.trim().to_string();
            store.update(cx, |store, cx| {
                store.dev_sign_in(email);
                cx.notify();
            });
        }
        "error" => {
            let message = rest.trim().to_string();
            store.update(cx, |store, cx| {
                store.error_message = Some(message);
                cx.notify();
            });
        }
        "updater" => {
            let mut words = rest.split_whitespace();
            let (state, version) = (words.next().unwrap_or_default(), words.next().unwrap_or("9.9.9").to_string());
            let state = match state {
                "checking" => crate::updater::UpdateState::Checking,
                "uptodate" => crate::updater::UpdateState::UpToDate,
                "available" => crate::updater::UpdateState::Available(version.clone()),
                "downloading" => crate::updater::UpdateState::Downloading(version.clone(), 0.42),
                "installing" => crate::updater::UpdateState::Installing(version.clone()),
                "ready" => crate::updater::UpdateState::Ready(version.clone()),
                "failed" => crate::updater::UpdateState::Failed("The new version couldn’t be downloaded (404).".into()),
                _ => crate::updater::UpdateState::Idle,
            };
            store.update(cx, |store, cx| {
                store.updater.show(state, Some(version));
                cx.notify();
            });
        }
        _ => tracing::warn!("the dev driver doesn't know {verb}"),
    }
}
