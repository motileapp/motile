//! Playing a video in the window: AVFoundation decodes it and plays its sound, and each frame
//! it hands over is drawn as a surface, with play and pause, the time and a bar to seek on.

use std::path::Path;
use std::ptr::NonNull;
use std::time::Duration;

use core_foundation::base::TCFType;
use gpui_kit::prelude::*;
use gpui_kit::*;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_av_foundation::{AVPlayer, AVPlayerItem, AVPlayerItemOutput, AVPlayerItemVideoOutput};
use objc2_core_media::CMTime;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::theme::colors;
use crate::ui::icons;

pub struct VideoPlayer {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    frame: Option<core_video::pixel_buffer::CVPixelBuffer>,
    playing: bool,
    /// Where the video is and how long it is, in seconds.
    position: f64,
    length: f64,
    hovered: bool,
    _frames: Task<()>,
}

impl VideoPlayer {
    pub fn new(file: &Path, cx: &mut Context<Self>) -> Self {
        let main = MainThreadMarker::new().expect("GPUI draws on the main thread");
        let path = NSString::from_str(&file.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        let item = unsafe { AVPlayerItem::playerItemWithURL(&url, main) };
        let format = NSNumber::new_u32(objc2_core_video::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange);
        let metal = NSNumber::new_bool(true);
        let keys: [&NSString; 2] = unsafe {
            [
                &*(objc2_core_video::kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString),
                &*(objc2_core_video::kCVPixelBufferMetalCompatibilityKey as *const _ as *const NSString),
            ]
        };
        let values: [&AnyObject; 2] = [&format, &metal];
        let attributes = NSDictionary::from_slices(&keys, &values);
        let output = unsafe {
            AVPlayerItemVideoOutput::initWithPixelBufferAttributes(AVPlayerItemVideoOutput::alloc(), Some(&attributes))
        };
        unsafe { item.addOutput(&output as &AVPlayerItemOutput) };
        let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), main) };
        unsafe { player.play() };
        // Frames are taken as the display would show them.
        let frames = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                if this.update(cx, |this, cx| this.take_frame(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            player,
            item,
            output,
            frame: None,
            playing: true,
            position: 0.,
            length: 0.,
            hovered: false,
            _frames: frames,
        }
    }

    fn take_frame(&mut self, cx: &mut Context<Self>) {
        let time = unsafe { self.item.currentTime() };
        let position = unsafe { time.seconds() };
        let length = unsafe { self.item.duration().seconds() };
        let playing = unsafe { self.player.rate() } > 0.;
        let mut changed = playing != self.playing || (position - self.position).abs() > 0.2;
        self.playing = playing;
        self.position = if position.is_finite() { position } else { 0. };
        self.length = if length.is_finite() { length } else { 0. };
        if unsafe { self.output.hasNewPixelBufferForItemTime(time) } {
            let mut shown = time;
            let buffer = unsafe { self.output.copyPixelBufferForItemTime_itemTimeForDisplay(time, &mut shown) };
            if let Some(buffer) = buffer {
                let raw = Retained::into_raw(buffer) as *mut _ as core_video::pixel_buffer::CVPixelBufferRef;
                if let Some(raw) = NonNull::new(raw) {
                    // The copy is ours to release, which the wrapper does when it goes.
                    self.frame =
                        Some(unsafe { core_video::pixel_buffer::CVPixelBuffer::wrap_under_create_rule(raw.as_ptr()) });
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        unsafe {
            if self.playing {
                self.player.pause();
            } else {
                if self.length > 0. && self.position >= self.length - 0.05 {
                    self.player.seekToTime(CMTime::with_seconds(0., 600));
                }
                self.player.play();
            }
        }
        self.playing = !self.playing;
        cx.notify();
    }

    pub fn pause(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
    }

    fn seek(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let seconds = self.length * fraction.clamp(0., 1.) as f64;
        unsafe { self.player.seekToTime(CMTime::with_seconds(seconds, 600)) };
        self.position = seconds;
        cx.notify();
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        unsafe { self.player.pause() };
    }
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl Render for VideoPlayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let fraction = if self.length > 0. { (self.position / self.length) as f32 } else { 0. };
        let bar = Rc::new(std::cell::Cell::new(Bounds::default()));
        let measured = bar.clone();
        div()
            .id("video")
            .size_full()
            .relative()
            .bg(black())
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                cx.notify();
            }))
            .children(self.frame.clone().map(|frame| surface(frame).size_full().object_fit(ObjectFit::Contain)))
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)))
            .when(self.hovered || !self.playing, |video| {
                video.child(
                    div()
                        .absolute()
                        .left(px(10.))
                        .right(px(10.))
                        .bottom(px(10.))
                        .h(px(30.))
                        .px(px(10.))
                        .rounded(px(8.))
                        .bg(hsla(0., 0., 0., 0.55))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .text_size(px(11.))
                        .text_color(white())
                        .child(
                            div()
                                .id("play-pause")
                                .child(
                                    icons::symbol(if self.playing { "pause.fill" } else { "play.fill" }, 12.)
                                        .text_color(white()),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle(cx)
                                })),
                        )
                        .child(div().child(clock(self.position)))
                        .child(
                            div()
                                .id("seek")
                                .flex_1()
                                .h(px(14.))
                                .flex()
                                .items_center()
                                .child(
                                    canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                                        .absolute()
                                        .size_full(),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(4.))
                                        .rounded_full()
                                        .bg(hsla(0., 0., 1., 0.3))
                                        .child(div().h_full().rounded_full().bg(white()).w(relative(fraction))),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        let bounds = bar.get();
                                        if bounds.size.width > px(0.) {
                                            this.seek((event.position.x - bounds.origin.x) / bounds.size.width, cx);
                                        }
                                    }),
                                )
                                .on_click(|_, _, cx| cx.stop_propagation()),
                        )
                        .child(div().text_color(c.tertiary.opacity(0.)).child(""))
                        .child(div().child(clock(self.length))),
                )
            })
    }
}

use std::rc::Rc;
