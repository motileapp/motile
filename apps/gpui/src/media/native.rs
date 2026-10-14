//! What is done with a file outside the app, as the Mac's `MediaFiles` does it: the save panel as
//! a sheet on the window, the file itself on the pasteboard, and the Dock told of a download.
//! Elsewhere the system's save dialog, and the file's path on the clipboard.

use std::path::{Path, PathBuf};

use gpui_kit::App;

/// Asks where to save the file, in a sheet on the key window, and copies it there.
#[cfg(target_os = "macos")]
pub fn save(file: PathBuf, name: &str, _: &mut App) {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSModalResponseOK, NSSavePanel};
    use objc2_foundation::NSString;

    let Some(main) = MainThreadMarker::new() else { return };
    let panel = NSSavePanel::savePanel(main);
    panel.setNameFieldStringValue(&NSString::from_str(name));
    let chosen = panel.clone();
    let handler = RcBlock::new(move |response| {
        if response != NSModalResponseOK {
            return;
        }
        let Some(destination) = chosen.URL().and_then(|url| url.path()).map(|path| PathBuf::from(path.to_string()))
        else {
            return;
        };
        let file = file.clone();
        std::thread::spawn(move || {
            let _ = std::fs::remove_file(&destination);
            let _ = std::fs::copy(&file, &destination);
        });
    });
    let app = NSApplication::sharedApplication(main);
    match app.keyWindow().or_else(|| app.mainWindow()) {
        Some(window) => panel.beginSheetModalForWindow_completionHandler(&window, &handler),
        None => panel.beginWithCompletionHandler(&handler),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn save(file: PathBuf, name: &str, cx: &mut App) {
    let folder = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let downloads = folder.map(|home| home.join("Downloads")).filter(|downloads| downloads.is_dir());
    let chosen = cx.prompt_for_new_path(&downloads.unwrap_or_else(std::env::temp_dir), Some(name));
    cx.background_executor()
        .spawn(async move {
            let Ok(Ok(Some(destination))) = chosen.await else { return };
            let _ = std::fs::remove_file(&destination);
            let _ = std::fs::copy(&file, &destination);
        })
        .detach();
}

/// Puts the file itself on the pasteboard, as Finder copies one.
#[cfg(target_os = "macos")]
pub fn copy_file(file: &Path, _: &mut App) {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    let object: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*url);
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.writeObjects(&NSArray::from_slice(&[object]));
}

/// Puts the file's path on the clipboard, which takes only text and images.
#[cfg(not(target_os = "macos"))]
pub fn copy_file(file: &Path, cx: &mut App) {
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(file.to_string_lossy().into_owned()));
}

/// Has the Downloads stack in the Dock bounce, as it does for a browser's download.
#[cfg(target_os = "macos")]
pub fn download_finished(file: &Path) {
    use objc2_foundation::{NSDistributedNotificationCenter, NSString};

    let name = NSString::from_str("com.apple.DownloadFileFinished");
    let path = NSString::from_str(&file.to_string_lossy());
    unsafe { NSDistributedNotificationCenter::defaultCenter().postNotificationName_object(&name, Some(&path)) };
}

#[cfg(not(target_os = "macos"))]
pub fn download_finished(_: &Path) {}

/// Whether the system asks for less motion.
#[cfg(target_os = "macos")]
pub fn reduces_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// Whether the desktop's animations are off, as GNOME's setting says, read once.
#[cfg(target_os = "linux")]
pub fn reduces_motion() -> bool {
    static REDUCES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REDUCES.get_or_init(|| {
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "false")
    })
}

/// Whether Windows' "Animation effects" are off, read once.
#[cfg(windows)]
pub fn reduces_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};

    static REDUCES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REDUCES.get_or_init(|| {
        let mut animates: i32 = 1;
        // SAFETY: the setting is a BOOL, written into the one it is given.
        let read = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut animates).cast(), 0) };
        read != 0 && animates == 0
    })
}
