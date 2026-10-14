//! Playing a video in the window: AVFoundation decodes it and plays its sound, and each frame
//! it hands over is drawn as a surface, under controls as the system's player floats them: the
//! sound and its volume, ten seconds back and on, play and pause, filling the screen, and a bar
//! to scrub along. The controls go once the pointer rests on a playing video.

use std::cell::Cell;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use gpui_kit::prelude::*;
use gpui_kit::*;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_av_foundation::{AVPlayer, AVPlayerItem, AVPlayerItemOutput, AVPlayerItemVideoOutput};
use objc2_core_media::{CMTime, kCMTimeZero};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::theme::{Radius, colors};
use crate::ui::icons;

/// How long the controls stay once the pointer rests on a playing video.
const CONTROLS_STAY: Duration = Duration::from_secs(3);
/// How far the buttons that skip go.
const SKIP: f64 = 10.;

/// What the pointer drags along: the time or the volume.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slider {
    Time,
    Volume,
}

pub struct VideoPlayer {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    frame: Option<core_video::pixel_buffer::CVPixelBuffer>,
    playing: bool,
    /// Where the video is and how long it is, in seconds.
    position: f64,
    length: f64,
    volume: f32,
    muted: bool,
    hovered: bool,
    /// When the pointer last moved over the video.
    moved: Instant,
    controls: bool,
    dragging: Option<Slider>,
    /// Where the bars of the time and the volume were drawn.
    time_bar: Rc<Cell<Bounds<Pixels>>>,
    volume_bar: Rc<Cell<Bounds<Pixels>>>,
    /// The window was made full screen for the video.
    full_screen: bool,
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
        let (volume, muted) = unsafe { (player.volume(), player.isMuted()) };
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
            volume,
            muted,
            hovered: false,
            moved: Instant::now(),
            controls: true,
            dragging: None,
            time_bar: Rc::default(),
            volume_bar: Rc::default(),
            full_screen: false,
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
        if self.dragging != Some(Slider::Time) {
            self.position = if position.is_finite() { position } else { 0. };
        }
        self.length = if length.is_finite() { length } else { 0. };
        let controls = self.wants_controls();
        changed |= controls != self.controls;
        self.controls = controls;
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

    /// The controls show while the video waits, while they are used, and a while after the
    /// pointer moved over it.
    fn wants_controls(&self) -> bool {
        !self.playing || self.dragging.is_some() || (self.hovered && self.moved.elapsed() < CONTROLS_STAY)
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

    /// Whether its first frame is there to show.
    pub fn ready(&self) -> bool {
        self.frame.is_some()
    }

    pub fn full_screen(&self) -> bool {
        self.full_screen
    }

    /// Has the video fill the screen, making the window full screen for it, or leaves it.
    pub fn set_full_screen(&mut self, full_screen: bool, window: &mut Window, cx: &mut Context<Self>) {
        if full_screen == self.full_screen {
            return;
        }
        if full_screen != window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.full_screen = full_screen;
        cx.notify();
    }

    pub fn pause(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
    }

    fn seek_to(&mut self, seconds: f64, cx: &mut Context<Self>) {
        let seconds = seconds.clamp(0., self.length.max(0.));
        unsafe {
            let time = CMTime::with_seconds(seconds, 600);
            self.player.seekToTime_toleranceBefore_toleranceAfter(time, kCMTimeZero, kCMTimeZero);
        }
        self.position = seconds;
        cx.notify();
    }

    /// Skips back or on by `seconds`.
    pub fn skip(&mut self, seconds: f64, cx: &mut Context<Self>) {
        self.seek_to(self.position + seconds, cx);
        self.shown(cx);
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.volume = volume.clamp(0., 1.);
        self.muted = self.volume == 0.;
        unsafe {
            self.player.setVolume(self.volume);
            self.player.setMuted(self.muted);
        }
        cx.notify();
    }

    /// Turns the sound up or down by `step`.
    pub fn change_volume(&mut self, step: f32, cx: &mut Context<Self>) {
        let from = if self.muted { 0. } else { self.volume };
        self.set_volume(from + step, cx);
        self.shown(cx);
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.muted = !self.muted;
        if !self.muted && self.volume == 0. {
            self.volume = 1.;
            unsafe { self.player.setVolume(1.) };
        }
        unsafe { self.player.setMuted(self.muted) };
        cx.notify();
    }

    /// The controls show a while, as when the pointer moves.
    fn shown(&mut self, cx: &mut Context<Self>) {
        self.moved = Instant::now();
        if !self.controls {
            self.controls = true;
            cx.notify();
        }
    }

    /// Follows the pointer along the bar it drags.
    fn slide(&mut self, slider: Slider, x: Pixels, cx: &mut Context<Self>) {
        let bar = match slider {
            Slider::Time => self.time_bar.get(),
            Slider::Volume => self.volume_bar.get(),
        };
        if bar.size.width <= px(0.) {
            return;
        }
        let fraction = ((x - bar.origin.x) / bar.size.width).clamp(0., 1.);
        match slider {
            Slider::Time => self.seek_to(self.length * fraction as f64, cx),
            Slider::Volume => self.set_volume(fraction, cx),
        }
    }

    /// The keys of a video that fills the screen, as the system's player has them: space and K
    /// play and pause, ← and J go back, → and L on, ↑ and ↓ turn the sound up and down. Whether
    /// the key was one of them.
    pub fn key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        match key {
            "space" | "k" => self.toggle(cx),
            "left" | "j" => self.skip(-SKIP, cx),
            "right" | "l" => self.skip(SKIP, cx),
            "up" => self.change_volume(0.1, cx),
            "down" => self.change_volume(-0.1, cx),
            _ => return false,
        }
        true
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

/// A bar that fills to `fraction`, which a press and a drag move along.
fn slider_bar(
    id: &'static str,
    fraction: f32,
    measured: Rc<Cell<Bounds<Pixels>>>,
    track: Hsla,
    fill: Hsla,
    on_press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(16.))
        .flex()
        .items_center()
        .child(canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {}).absolute().size_full())
        .child(
            div()
                .w_full()
                .h(px(4.))
                .rounded_full()
                .bg(track)
                .child(div().h_full().rounded_full().bg(fill).w(relative(fraction.clamp(0., 1.)))),
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            cx.stop_propagation();
            on_press(event, window, cx);
        })
        .on_click(|_, _, cx| cx.stop_propagation())
}

/// A symbol that is a button over the video.
fn control(
    id: &'static str,
    symbol: &'static str,
    size: f32,
    help: &'static str,
    color: Hsla,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(size + 12.))
        .flex()
        .items_center()
        .justify_center()
        .child(icons::symbol(symbol, size).text_color(color))
        .tooltip(crate::ui::tooltip(help))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
}

impl Render for VideoPlayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let (bright, quiet) = (c.foreground_over_picture, c.muted_foreground_over_picture);
        let fraction = if self.length > 0. { (self.position / self.length) as f32 } else { 0. };
        let view = cx.entity().downgrade();
        let act = move |run: fn(&mut VideoPlayer, &mut Window, &mut Context<VideoPlayer>)| {
            let view = view.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |player, cx| run(player, window, cx));
            }
        };
        let pressed = cx.entity().downgrade();
        let press = move |slider: Slider| {
            let pressed = pressed.clone();
            move |event: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                let _ = pressed.update(cx, |player, cx| {
                    player.dragging = Some(slider);
                    player.slide(slider, event.position.x, cx);
                });
            }
        };
        let volume_symbol = match (self.muted, self.volume) {
            (true, _) | (_, 0.) => "volume-x",
            (_, volume) if volume < 0.5 => "volume-1",
            _ => "volume-2",
        };
        let shown_volume = if self.muted { 0. } else { self.volume };
        let controls = div()
            .w_full()
            .max_w(px(460.))
            .px(px(10.))
            .pt(px(4.))
            .pb(px(8.))
            .rounded(px(Radius::LG))
            .bg(c.control_over_picture)
            .text_size(px(11.))
            .text_color(bright)
            .font_features(crate::ui::tabular_numbers())
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(control(
                                "mute",
                                volume_symbol,
                                14.,
                                "Mute",
                                bright,
                                act(|player, _, cx| player.toggle_mute(cx)),
                            ))
                            .child(
                                slider_bar(
                                    "volume",
                                    shown_volume,
                                    self.volume_bar.clone(),
                                    c.control_lit_over_picture,
                                    bright,
                                    press(Slider::Volume),
                                )
                                .w(px(64.)),
                            ),
                    )
                    .child(control(
                        "back",
                        "rotate-ccw",
                        14.,
                        "Skip Back 10 Seconds",
                        bright,
                        act(|player, _, cx| player.skip(-SKIP, cx)),
                    ))
                    .child(control(
                        "play-pause",
                        if self.playing { "pause" } else { "play" },
                        18.,
                        if self.playing { "Pause" } else { "Play" },
                        bright,
                        act(|player, _, cx| player.toggle(cx)),
                    ))
                    .child(control(
                        "forward",
                        "rotate-cw",
                        14.,
                        "Skip Forward 10 Seconds",
                        bright,
                        act(|player, _, cx| player.skip(SKIP, cx)),
                    ))
                    .child(div().flex_1().flex().justify_end().child(control(
                        "full-screen",
                        if self.full_screen { "minimize-2" } else { "maximize-2" },
                        13.,
                        if self.full_screen { "Exit Full Screen" } else { "Enter Full Screen" },
                        bright,
                        act(|player, window, cx| player.set_full_screen(!player.full_screen, window, cx)),
                    ))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(36.)).text_color(quiet).child(clock(self.position)))
                    .child(
                        slider_bar(
                            "seek",
                            fraction,
                            self.time_bar.clone(),
                            c.control_lit_over_picture,
                            bright,
                            press(Slider::Time),
                        )
                        .flex_1(),
                    )
                    .child(
                        div()
                            .w(px(40.))
                            .text_right()
                            .text_color(quiet)
                            .child(format!("-{}", clock(self.length - self.position))),
                    ),
            );
        let dragging = self.dragging;
        div()
            .id("video")
            .size_full()
            .relative()
            .bg(c.overlay)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                this.moved = Instant::now();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| this.shown(cx)))
            .children(self.frame.clone().map(|frame| surface(frame).size_full().object_fit(ObjectFit::Contain)))
            // A click plays and pauses; a second one fills the screen, as the system's player does.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.toggle(cx);
                    if event.click_count == 2 {
                        this.set_full_screen(!this.full_screen, window, cx);
                    }
                }),
            )
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(self.controls, |video| {
                video.child(
                    div()
                        .absolute()
                        .left(px(10.))
                        .right(px(10.))
                        .bottom(px(10.))
                        .flex()
                        .justify_center()
                        .child(controls),
                )
            })
            // A drag along a bar follows the pointer wherever it goes until the button is let go.
            .when_some(dragging, |video, slider| {
                let (moved, released) = (cx.entity().downgrade(), cx.entity().downgrade());
                video.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let moved = moved.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase.bubble() && event.pressed_button == Some(MouseButton::Left) {
                                    let _ = moved.update(cx, |player, cx| player.slide(slider, event.position.x, cx));
                                }
                            });
                            let released = released.clone();
                            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                if phase.bubble() {
                                    let _ = released.update(cx, |player, cx| {
                                        player.dragging = None;
                                        cx.notify();
                                    });
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}
