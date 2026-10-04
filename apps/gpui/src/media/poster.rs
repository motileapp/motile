//! The first frame of a video, as an image that stands for it until it plays.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Writes the video's first frame to a JPEG or PNG file, at most `max_pixels` on its longer
/// side. `None` when no tool on this device can read the video.
pub fn first_frame(video: &Path, max_pixels: u32) -> Option<PathBuf> {
    let folder = std::env::temp_dir().join(format!("motile-poster-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).ok()?;
    let poster = folder.join("poster.jpg");
    let scale = format!("scale='if(gt(iw,ih),min({max_pixels},iw),-2)':'if(gt(iw,ih),-2,min({max_pixels},ih))'");
    let made = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf", &scale, "-q:v", "3"])
        .arg(&poster)
        .status()
        .is_ok_and(|status| status.success());
    if made && poster.exists() {
        return Some(poster);
    }
    quick_look(video, &folder, max_pixels)
}

/// macOS draws a thumbnail of any video it can play.
#[cfg(target_os = "macos")]
fn quick_look(video: &Path, folder: &Path, max_pixels: u32) -> Option<PathBuf> {
    let made = Command::new("qlmanage")
        .args(["-t", "-s", &max_pixels.to_string(), "-o"])
        .arg(folder)
        .arg(video)
        .output()
        .is_ok_and(|output| output.status.success());
    if !made {
        return None;
    }
    let name = format!("{}.png", video.file_name()?.to_string_lossy());
    let thumbnail = folder.join(name);
    thumbnail.exists().then_some(thumbnail)
}

#[cfg(not(target_os = "macos"))]
fn quick_look(_video: &Path, _folder: &Path, _max_pixels: u32) -> Option<PathBuf> {
    None
}
