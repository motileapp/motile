//! Images decoded off the main thread at the size they are shown, and the latest ones kept, as the
//! Mac's `Pictures` does. Each is kept under a key: the image's own name for a row, `tile:` and
//! the name for a tile, `view:` and the file for the viewer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::*;

use super::MediaFiles;
use crate::store::Store;

/// What the decoded images may take before the least recently shown ones go.
const KEPT_BYTES: usize = 256 * 1024 * 1024;

/// Why an image isn't shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Failure {
    /// Its file couldn't be had.
    Fetch,
    /// Its file isn't an image this app can read.
    Decode,
}

#[derive(Clone)]
pub enum Picture {
    Loading,
    Ready(Shown),
    Failed(Failure),
}

/// A decoded image and its size in pixels.
#[derive(Clone)]
pub struct Shown {
    pub image: Arc<RenderImage>,
    pub width: u32,
    pub height: u32,
}

struct Kept {
    shown: Shown,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
pub struct Pictures {
    kept: HashMap<String, Kept>,
    loading: HashMap<String, u64>,
    failed: HashMap<String, Failure>,
    bytes: usize,
    clock: u64,
}

impl Global for Pictures {}

impl Pictures {
    /// The image of the file named `media`, kept under `key` and no larger than `max_pixels` on a
    /// side. It is fetched and decoded the first time it is asked for, and the windows are drawn
    /// again once it is there.
    pub fn load(store: &Entity<Store>, key: &str, media: &str, max_pixels: u32, cx: &mut App) -> Picture {
        if let Some(known) = Self::known(key, cx) {
            return known;
        }
        let generation = Self::start(key, cx);
        let key = key.to_string();
        MediaFiles::fetch(store, media, cx, move |result, cx| match result {
            Ok(file) => Self::decode(key, generation, file, max_pixels, cx),
            Err(_) => Self::finish(&key, generation, Err(Failure::Fetch), cx),
        });
        Picture::Loading
    }

    /// The image of a file on this device, kept under `key`.
    pub fn load_file(key: &str, file: &Path, max_pixels: u32, cx: &mut App) -> Picture {
        if let Some(known) = Self::known(key, cx) {
            return known;
        }
        let generation = Self::start(key, cx);
        Self::decode(key.to_string(), generation, file.to_path_buf(), max_pixels, cx);
        Picture::Loading
    }

    /// The first frame of a video on this device, kept under `key`.
    pub fn load_video_file(key: &str, file: &Path, max_pixels: u32, cx: &mut App) -> Picture {
        if let Some(known) = Self::known(key, cx) {
            return known;
        }
        let generation = Self::start(key, cx);
        let (key, file) = (key.to_string(), file.to_path_buf());
        let reading = cx.background_executor().spawn(async move {
            let frame = super::poster::first_frame(&file, max_pixels).await?;
            Some(shown(frame))
        });
        cx.spawn(async move |cx| {
            let read = reading.await;
            cx.update(|cx| Self::finish(&key, generation, read.ok_or(Failure::Decode), cx));
        })
        .detach();
        Picture::Loading
    }

    /// Forgets that the image failed, so that the next `load` tries again.
    pub fn retry(key: &str, cx: &mut App) {
        cx.default_global::<Pictures>().failed.remove(key);
    }

    fn known(key: &str, cx: &mut App) -> Option<Picture> {
        let pictures = cx.default_global::<Pictures>();
        pictures.clock += 1;
        let clock = pictures.clock;
        if let Some(kept) = pictures.kept.get_mut(key) {
            kept.used = clock;
            return Some(Picture::Ready(kept.shown.clone()));
        }
        if let Some(failure) = pictures.failed.get(key) {
            return Some(Picture::Failed(*failure));
        }
        pictures.loading.contains_key(key).then_some(Picture::Loading)
    }

    fn start(key: &str, cx: &mut App) -> u64 {
        let pictures = cx.default_global::<Pictures>();
        pictures.clock += 1;
        pictures.loading.insert(key.to_string(), pictures.clock);
        pictures.clock
    }

    fn decode(key: String, generation: u64, file: PathBuf, max_pixels: u32, cx: &mut App) {
        let decoding = cx.background_executor().spawn(async move { decode(&file, max_pixels) });
        cx.spawn(async move |cx| {
            let decoded = decoding.await;
            cx.update(|cx| Self::finish(&key, generation, decoded.ok_or(Failure::Decode), cx));
        })
        .detach();
    }

    fn finish(key: &str, generation: u64, result: Result<Shown, Failure>, cx: &mut App) {
        let pictures = cx.default_global::<Pictures>();
        if pictures.loading.get(key) != Some(&generation) {
            return;
        }
        pictures.loading.remove(key);
        let dropped = match result {
            Ok(shown) => pictures.keep(key, shown),
            Err(failure) => {
                pictures.failed.insert(key.to_string(), failure);
                Vec::new()
            }
        };
        for image in dropped {
            cx.drop_image(image, None);
        }
        cx.refresh_windows();
    }

    /// Keeps the image, and hands back the ones let go to make room for it.
    fn keep(&mut self, key: &str, shown: Shown) -> Vec<Arc<RenderImage>> {
        let bytes = shown.width as usize * shown.height as usize * 4;
        self.clock += 1;
        self.bytes += bytes;
        if let Some(old) = self.kept.insert(key.to_string(), Kept { shown, bytes, used: self.clock }) {
            self.bytes -= old.bytes;
        }
        let mut dropped = Vec::new();
        while self.bytes > KEPT_BYTES && self.kept.len() > 1 {
            let Some(oldest) = self.kept.iter().min_by_key(|(_, kept)| kept.used).map(|(key, _)| key.clone()) else {
                break;
            };
            let Some(kept) = self.kept.remove(&oldest) else { break };
            self.bytes -= kept.bytes;
            dropped.push(kept.shown.image);
        }
        dropped
    }
}

/// Reads the image, no larger than `max_pixels` on a side, the right way round, in the BGRA GPUI
/// draws. The system's ImageIO reads it, as it does for the Mac app: every format it knows, HEIC
/// and AVIF among them, with the colours of the profile it carries.
#[cfg(target_os = "macos")]
fn decode(file: &Path, max_pixels: u32) -> Option<Shown> {
    let mut pixels = image_io::thumbnail(file, max_pixels)?;
    // GPUI takes colours whose alpha isn't multiplied in.
    for pixel in pixels.pixels_mut().filter(|pixel| pixel.0[3] > 0 && pixel.0[3] < 255) {
        let alpha = u32::from(pixel.0[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    Some(shown(pixels))
}

/// The image re-encoded as a PNG, as the Mac app turns a pasted TIFF into one: by ImageIO, with
/// its colour profile and resolution.
#[cfg(target_os = "macos")]
pub fn png(data: &[u8]) -> Option<Vec<u8>> {
    image_io::png(data)
}

#[cfg(not(target_os = "macos"))]
pub fn png(data: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(data).ok()?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

#[cfg(target_os = "macos")]
mod image_io {
    use std::ffi::c_void;
    use std::path::Path;
    use std::ptr::NonNull;

    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::data::{CFData, CFDataRef};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::url::{CFURL, CFURLRef};
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::CGImage;

    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        fn CGImageSourceCreateWithURL(url: CFURLRef, options: CFDictionaryRef) -> *const c_void;
        fn CGImageSourceCreateThumbnailAtIndex(
            source: *const c_void,
            index: usize,
            options: CFDictionaryRef,
        ) -> *mut CGImage;
        static kCGImageSourceCreateThumbnailFromImageAlways: CFStringRef;
        static kCGImageSourceCreateThumbnailWithTransform: CFStringRef;
        static kCGImageSourceShouldCacheImmediately: CFStringRef;
        static kCGImageSourceThumbnailMaxPixelSize: CFStringRef;
        fn CGImageSourceCreateWithData(data: CFDataRef, options: CFDictionaryRef) -> *const c_void;
        fn CGImageDestinationCreateWithData(
            data: *const c_void,
            kind: CFStringRef,
            count: usize,
            options: CFDictionaryRef,
        ) -> *const c_void;
        fn CGImageDestinationAddImageFromSource(
            destination: *const c_void,
            source: *const c_void,
            index: usize,
            properties: CFDictionaryRef,
        );
        fn CGImageDestinationFinalize(destination: *const c_void) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const c_void);
        fn CFDataCreateMutable(allocator: *const c_void, capacity: isize) -> *const c_void;
        fn CFDataGetLength(data: *const c_void) -> isize;
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    }

    /// The first image of `data`, written as a PNG.
    pub fn png(data: &[u8]) -> Option<Vec<u8>> {
        let data = CFData::from_buffer(data);
        let source = unsafe { CGImageSourceCreateWithData(data.as_concrete_TypeRef(), std::ptr::null()) };
        if source.is_null() {
            return None;
        }
        let png = unsafe { CFDataCreateMutable(std::ptr::null(), 0) };
        let kind = CFString::from_static_string("public.png");
        let destination =
            unsafe { CGImageDestinationCreateWithData(png, kind.as_concrete_TypeRef(), 1, std::ptr::null()) };
        let written = !destination.is_null()
            && unsafe {
                CGImageDestinationAddImageFromSource(destination, source, 0, std::ptr::null());
                CGImageDestinationFinalize(destination)
            };
        let bytes = written.then(|| unsafe {
            std::slice::from_raw_parts(CFDataGetBytePtr(png), CFDataGetLength(png) as usize).to_vec()
        });
        unsafe {
            if !destination.is_null() {
                CFRelease(destination);
            }
            CFRelease(png);
            CFRelease(source);
        }
        bytes
    }

    /// The image of the file, no larger than `max_pixels` on a side and turned the way it says.
    pub fn thumbnail(file: &Path, max_pixels: u32) -> Option<image::RgbaImage> {
        let url = CFURL::from_path(file, false)?;
        let key = |key: CFStringRef| unsafe { CFString::wrap_under_get_rule(key) };
        let yes = CFBoolean::true_value().as_CFType();
        let options: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::from_CFType_pairs(&[
                (key(kCGImageSourceCreateThumbnailFromImageAlways), yes.clone()),
                (key(kCGImageSourceCreateThumbnailWithTransform), yes.clone()),
                (key(kCGImageSourceShouldCacheImmediately), yes),
                (key(kCGImageSourceThumbnailMaxPixelSize), CFNumber::from(i64::from(max_pixels)).as_CFType()),
            ])
        };
        let source = unsafe { CGImageSourceCreateWithURL(url.as_concrete_TypeRef(), std::ptr::null()) };
        if source.is_null() {
            return None;
        }
        let image = unsafe { CGImageSourceCreateThumbnailAtIndex(source, 0, options.as_concrete_TypeRef()) };
        unsafe { CFRelease(source) };
        let image = unsafe { CFRetained::from_raw(NonNull::new(image)?) };
        super::super::poster::pixels(&image)
    }
}

#[cfg(not(target_os = "macos"))]
fn decode(file: &Path, max_pixels: u32) -> Option<Shown> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(file).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    let image = if image.width().max(image.height()) > max_pixels {
        image.resize(max_pixels, max_pixels, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    Some(shown(image.into_rgba8()))
}

/// The pixels in the BGRA GPUI draws.
fn shown(mut pixels: image::RgbaImage) -> Shown {
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let (width, height) = pixels.dimensions();
    let frame = image::Frame::new(pixels);
    Shown { image: Arc::new(RenderImage::new(vec![frame])), width, height }
}
