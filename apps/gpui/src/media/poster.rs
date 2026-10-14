//! The first frame of a video, which stands for it until it plays. AVFoundation reads it off the
//! main thread, as the Mac's `Pictures.firstFrame` does.

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use futures::channel::oneshot;
use image::RgbaImage;

/// The video's first frame, no larger than `max_pixels` on a side. `None` for a video this
/// device can't read.
pub async fn first_frame(video: &Path, max_pixels: u32) -> Option<RgbaImage> {
    let (sender, receiver) = oneshot::channel();
    generate(video, max_pixels, sender);
    receiver.await.ok().flatten()
}

/// The first frame written to a JPEG file, which is what a video's poster is sent as.
pub async fn first_frame_jpeg(video: &Path, max_pixels: u32) -> Option<PathBuf> {
    let frame = first_frame(video, max_pixels).await?;
    let file = std::env::temp_dir().join(format!("motile-poster-{}.jpg", uuid::Uuid::new_v4()));
    let writer = BufWriter::new(std::fs::File::create(&file).ok()?);
    let rgb = image::DynamicImage::ImageRgba8(frame).into_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(writer, 80).encode_image(&rgb).ok()?;
    Some(file)
}

#[cfg(target_os = "macos")]
fn generate(video: &Path, max_pixels: u32, sender: oneshot::Sender<Option<RgbaImage>>) {
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
    use objc2_core_foundation::CGSize;
    use objc2_core_graphics::CGImage;
    use objc2_core_media::{CMTime, kCMTimeZero};
    use objc2_foundation::{NSError, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&video.to_string_lossy()));
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
    let frames = unsafe { AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset) };
    let side = f64::from(max_pixels);
    unsafe {
        frames.setAppliesPreferredTrackTransform(true);
        frames.setMaximumSize(CGSize::new(side, side));
    }
    // The generator is kept until it answers.
    let waiting = Mutex::new(Some((sender, frames.clone())));
    let done = RcBlock::new(move |image: *mut CGImage, _: CMTime, _: *mut NSError| {
        let Some((sender, _frames)) = waiting.lock().ok().and_then(|mut waiting| waiting.take()) else { return };
        let _ = sender.send(unsafe { image.as_ref() }.and_then(pixels));
    });
    unsafe { frames.generateCGImageAsynchronouslyForTime_completionHandler(kCMTimeZero, &done) };
}

/// The image's pixels, drawn into RGBA in sRGB, its colours turned from its own profile's, with
/// their alpha multiplied in.
#[cfg(target_os = "macos")]
pub fn pixels(image: &objc2_core_graphics::CGImage) -> Option<RgbaImage> {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
        kCGColorSpaceSRGB,
    };

    let (width, height) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if width == 0 || height == 0 {
        return None;
    }
    let mut bytes = vec![0u8; width * height * 4];
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let context =
        unsafe { CGBitmapContextCreate(bytes.as_mut_ptr().cast(), width, height, 8, width * 4, Some(&space), info) }?;
    let rect = CGRect::new(CGPoint::new(0., 0.), CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    RgbaImage::from_raw(width as u32, height as u32, bytes)
}

#[cfg(not(target_os = "macos"))]
fn generate(_video: &Path, _max_pixels: u32, sender: oneshot::Sender<Option<RgbaImage>>) {
    let _ = sender.send(None);
}
