use core::prelude::v1::test;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::*;
use image::{Rgba as Pixel, RgbaImage};
use motile_core::render::openui::canvas::motion::{celebration, flip};
use motile_core::render::openui::canvas::{
    Color, Gradient, Hit, Item, Legend, LegendEntry, Motion, Paint, Region, Scene, Stop, Tip,
};

use super::legend::LegendText;
use super::*;
use crate::theme::{self, Colors, Surface};

/// A pixel differs when a channel is this far from the reference's, and a picture when more
/// than this share of its pixels do.
const CHANNEL: i32 = 40;
const SHARE: f32 = 0.01;
/// A formula is all small glyphs, whose edges GPUI's four samples a pixel smooth more coarsely
/// than the reference's exact coverage.
const FORMULA_SHARE: f32 = 0.02;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn references() -> PathBuf {
    repo().join("crates/core/fixtures/canvas/reference")
}

fn fixture(name: &str) -> Scene {
    let json = std::fs::read_to_string(references().join(format!("{name}.json"))).expect("the scene");
    serde_json::from_str(&json).expect("a scene")
}

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    hover: Option<usize>,
    scale: f32,
    masks: Vec<[f32; 4]>,
}

/// The scene alone on the surface's colour, as the reference paints it.
struct Shot {
    prepared: Arc<Prepared>,
    palette: Palette,
    hover: Option<usize>,
}

impl Render for Shot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (prepared, palette, hover) = (self.prepared.clone(), self.palette, self.hover);
        div().size_full().bg(palette.surface.color(palette.colors)).child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| paint_scene(&prepared, bounds.origin, &palette, hover, window, cx),
            )
            .size_full(),
        )
    }
}

#[cfg(target_os = "macos")]
fn platform_text() -> Option<Arc<dyn PlatformTextSystem>> {
    Some(Arc::new(gpui_apple::AppleTextSystem::new()))
}

#[cfg(target_os = "linux")]
fn platform_text() -> Option<Arc<dyn PlatformTextSystem>> {
    Some(Arc::new(gpui_wgpu::CosmicTextSystem::new(theme::UI_FONT)))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_text() -> Option<Arc<dyn PlatformTextSystem>> {
    None
}

/// GPUI's headless app with a renderer, where this machine has one.
fn headless() -> Option<HeadlessAppContext> {
    let text = platform_text()?;
    if !matches!(gpui_kit::platform::current_headless_renderer(), Ok(Some(_))) {
        return None;
    }
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(()), gpui_kit::platform::current_headless_renderer);
    let fonts = crate::assets::FONTS.iter().map(|bytes| std::borrow::Cow::Borrowed(*bytes)).collect();
    cx.update(|cx| cx.text_system().add_fonts(fonts)).expect("the fonts");
    Some(cx)
}

fn shot(
    cx: &mut HeadlessAppContext,
    scene: &Scene,
    colors: &'static Colors,
    hover: Option<usize>,
    scale: f32,
) -> RgbaImage {
    let points = |length: f32| px((length * scale).ceil().max(1.) / scale);
    let prepared = Arc::new(Prepared::new(scene.clone()));
    let palette = Palette::new(colors, Surface::Background);
    let window = cx
        .open_window(size(points(scene.width), points(scene.height)), |_, cx| {
            cx.new(|_| Shot { prepared, palette, hover })
        })
        .expect("a window");
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(window.scale_factor(), scale);
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("a frame");
    cx.capture_screenshot(window.into()).expect("a picture")
}

/// The share of pixels that differ, outside the boxes the labels may take in any typeface.
fn differing(ours: &RgbaImage, reference: &RgbaImage, masks: &[[f32; 4]], scale: f32) -> f32 {
    assert_eq!(ours.dimensions(), reference.dimensions());
    let (mut ours, mut reference) = (ours.clone(), reference.clone());
    let (width, height) = ours.dimensions();
    for [x, y, w, h] in masks {
        let span = |from: f32, length: f32, limit: u32| {
            let start = (from * scale).floor().clamp(0., limit as f32) as u32;
            let end = ((from + length) * scale).ceil().clamp(0., limit as f32) as u32;
            start..end
        };
        for row in span(*y, *h, height) {
            for column in span(*x, *w, width) {
                ours.put_pixel(column, row, Pixel([255, 0, 255, 255]));
                reference.put_pixel(column, row, Pixel([255, 0, 255, 255]));
            }
        }
    }
    let differ = ours
        .pixels()
        .zip(reference.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(a, b)| (*a as i32 - *b as i32).abs() > CHANNEL))
        .count();
    differ as f32 / (width * height) as f32
}

#[test]
fn paints_scenes_as_the_reference_does() {
    let Some(mut cx) = headless() else {
        eprintln!("skipped: GPUI has no headless renderer here");
        return;
    };
    let cases: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(references().join("cases.json")).unwrap()).unwrap();
    let out = repo().join("build/canvas/gpui");
    std::fs::create_dir_all(&out).unwrap();
    let mut failed = Vec::new();
    for (name, colors) in [("dark", &*theme::DARK), ("light", &*theme::LIGHT)] {
        let mut worst = (0f32, String::new());
        for case in &cases {
            let scene = fixture(&case.name);
            let ours = shot(&mut cx, &scene, colors, case.hover, case.scale);
            ours.save(out.join(format!("{}-{name}.png", case.name))).unwrap();
            let reference = image::open(references().join(format!("{}-{name}.png", case.name))).unwrap().to_rgba8();
            let share = differing(&ours, &reference, &case.masks, case.scale);
            println!("{}-{name}: {:.3}% differ", case.name, share * 100.);
            if share > worst.0 {
                worst = (share, case.name.clone());
            }
            let allowed = if case.name.starts_with("math-") { FORMULA_SHARE } else { SHARE };
            if share > allowed {
                failed.push(format!("{}-{name} ({:.2}%)", case.name, share * 100.));
            }
        }
        println!("worst {name}: {} at {:.3}%", worst.1, worst.0 * 100.);
    }
    assert!(failed.is_empty(), "differ from the reference: {failed:?}");
}

#[test]
fn paints_a_gradient_of_three_stops_along_its_line() {
    let Some(mut cx) = headless() else {
        eprintln!("skipped: GPUI has no headless renderer here");
        return;
    };
    let stop = |at: f32, rgba: [u8; 4]| Stop { at, color: Color::Rgba(rgba), opacity: 1. };
    let stops = vec![stop(0., [255, 0, 0, 255]), stop(0.5, [0, 255, 0, 255]), stop(1., [0, 0, 255, 255])];
    let gradient = Gradient { x1: 10., y1: 0., x2: 110., y2: 60., stops: stops.clone() };
    let paint = Paint { gradient: Some(gradient), ..Paint::default() };
    let rect = Item::Rect { x: 0., y: 0., width: 120., height: 60., rx: 0., ry: 0., paint };
    let scene = Scene { width: 120., height: 60., items: vec![rect], ..Scene::default() };
    let picture = shot(&mut cx, &scene, &theme::DARK, None, 2.);
    let (dx, dy) = (100f32, 60f32);
    for (column, row) in [(4, 4), (60, 30), (120, 60), (180, 90), (236, 116), (200, 10), (30, 110)] {
        let (x, y) = ((column as f32 + 0.5) / 2., (row as f32 + 0.5) / 2.);
        let along = (((x - 10.) * dx + y * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
        let (from, to, t) =
            if along < 0.5 { (&stops[0], &stops[1], along * 2.) } else { (&stops[1], &stops[2], along * 2. - 1.) };
        let (Color::Rgba(from), Color::Rgba(to)) = (&from.color, &to.color) else { unreachable!() };
        let expected: Vec<f32> = (0..3).map(|i| from[i] as f32 + (to[i] as f32 - from[i] as f32) * t).collect();
        let got = picture.get_pixel(column, row).0;
        for i in 0..3 {
            assert!((got[i] as f32 - expected[i]).abs() <= 6., "{column},{row}: {got:?} for {expected:?}");
        }
    }
}

/// A scene as a component embeds it, on the transcript's background.
struct Embedded {
    prepared: Arc<Prepared>,
    width: f32,
}

impl Render for Embedded {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = theme::colors(cx).background;
        div().size_full().bg(background).child(SceneView::new("scene", self.prepared.clone(), self.width))
    }
}

fn frame(cx: &mut HeadlessAppContext, window: AnyWindowHandle, moved_to: Option<(f32, f32)>) -> RgbaImage {
    cx.update_window(window, |_, window, cx| {
        if let Some((x, y)) = moved_to {
            let event =
                MouseMoveEvent { position: point(px(x), px(y)), pressed_button: None, modifiers: Modifiers::default() };
            window.dispatch_event(PlatformInput::MouseMove(event), cx);
        }
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("a frame");
    cx.capture_screenshot(window).expect("a picture")
}

#[test]
fn the_pointer_lights_a_band_and_shows_its_tooltip() {
    let Some(mut cx) = headless() else {
        eprintln!("skipped: GPUI has no headless renderer here");
        return;
    };
    let scene = fixture("chart-bar-720");
    let size_of = cx.update(|cx| arrange(&scene, 720., cx.text_system()));
    assert_eq!((size_of.width, size_of.height), (720., 240.));
    let prepared = Arc::new(Prepared::new(scene));
    let handle = cx
        .open_window(size(px(720.), px(240.)), |_, cx| cx.new(|_| Embedded { prepared, width: 720. }))
        .expect("a window");
    let window: AnyWindowHandle = handle.into();
    let plain = frame(&mut cx, window, None);
    let hovered = frame(&mut cx, window, Some((400., 100.)));
    let left = frame(&mut cx, window, Some((5., 230.)));
    let out = repo().join("build/canvas/gpui");
    std::fs::create_dir_all(&out).unwrap();
    hovered.save(out.join("view-hovered-light.png")).unwrap();
    let rgb = |color: Hsla| {
        let color = Rgba::from(color);
        [color.r, color.g, color.b].map(|channel| (channel * 255.).round() as i32)
    };
    let near = |picture: &RgbaImage, (x, y): (u32, u32), color: Hsla| {
        let got = picture.get_pixel(x * 2, y * 2).0;
        rgb(color).iter().zip(got).all(|(want, got)| (want - got as i32).abs() <= 2)
    };
    let c = &*theme::LIGHT;
    let (band, card) = ((380, 150), (440, 14));
    assert!(near(&plain, band, c.background) && near(&plain, card, c.background));
    assert!(near(&hovered, band, c.background_accent_larger), "the band is lit");
    assert!(near(&hovered, card, c.popover), "the tooltip shows");
    assert!(near(&left, band, c.background) && near(&left, card, c.background));
}

fn region_hit(region: Region) -> Hit {
    Hit { region, at: 0, lit: Vec::new(), tip: Tip { x: 0., y: 0., title: String::new(), rows: Vec::new() } }
}

#[test]
fn finds_the_bar_band_under_the_pointer() {
    let scene = fixture("chart-bar-720");
    assert_eq!(hit(&scene, 400., 100.), Some(3));
    assert_eq!(hit(&scene, 373.6, 0.), Some(3));
    assert_eq!(hit(&scene, 400., 241.), None);
}

#[test]
fn finds_a_donut_slice_by_its_angle_past_a_full_turn() {
    let sector = Region::Sector { cx: 100., cy: 100., inner: 60., outer: 100., start: 5., end: 7. };
    let scene = Scene { hits: vec![region_hit(sector)], ..Scene::default() };
    let at = |angle: f32, distance: f32| hit(&scene, 100. + distance * angle.cos(), 100. + distance * angle.sin());
    assert_eq!(at(0.3, 80.), Some(0));
    assert_eq!(at(5.5, 80.), Some(0));
    assert_eq!(at(1., 80.), None);
    assert_eq!(at(0.3, 50.), None);
    assert_eq!(at(0.3, 101.), None);
}

#[test]
fn the_last_hit_under_the_pointer_wins() {
    let rect = |x: f32| Region::Rect { x, y: 0., width: 100., height: 100. };
    let circle = Region::Circle { cx: 50., cy: 50., r: 10. };
    let scene =
        Scene { hits: vec![region_hit(rect(0.)), region_hit(rect(50.)), region_hit(circle)], ..Scene::default() };
    assert_eq!(hit(&scene, 20., 50.), Some(0));
    assert_eq!(hit(&scene, 70., 50.), Some(1));
    assert_eq!(hit(&scene, 52., 50.), Some(2));
}

/// Every character 6 wide in a chart's legend and 7 in a pie's, lines broken at any character.
struct Even;

impl Measure for Even {
    fn width(&self, text: &str, style: LegendText) -> f32 {
        text.chars().count() as f32 * if style == LegendText::Caption { 6. } else { 7. }
    }

    fn lines(&self, text: &str, style: LegendText, width: f32) -> Vec<String> {
        let per_line = ((width / self.width("x", style)) as usize).max(1);
        let chars: Vec<char> = text.chars().collect();
        chars.chunks(per_line).map(|line| line.iter().collect()).collect()
    }
}

fn legend_scene(width: f32, height: f32, beside: bool, entries: &[(&str, Option<&str>)]) -> Scene {
    let entries = entries
        .iter()
        .map(|(label, share)| LegendEntry {
            color: Color::token("chart-1"),
            label: label.to_string(),
            share: share.map(str::to_string),
        })
        .collect();
    Scene { width, height, legend: Some(Legend { beside, entries }), ..Scene::default() }
}

#[test]
fn a_chart_legend_wraps_under_the_plot() {
    let scene = legend_scene(300., 200., false, &[("Revenue", None), ("Costs", None), ("Profit margin", None)]);
    assert_eq!(arrange(&scene, 300., &Even).height, 228.);
    let narrow = arrange(&scene, 150., &Even);
    assert_eq!(narrow.height, 252.);
    assert_eq!((narrow.labels[2].x, narrow.labels[2].top), (15., 234.));
    assert_eq!(arrange(&Scene { legend: None, ..scene }, 300., &Even).height, 200.);
}

#[test]
fn a_pie_legend_lies_beside_its_disc_or_under_it() {
    let entries =
        [("Rent", Some("52.3%")), ("Food", Some("18.8%")), ("Travel", Some("0%")), ("Savings", Some("28.9%"))];
    let beside = arrange(&legend_scene(200., 200., true, &entries), 500., &Even);
    assert_eq!((beside.width, beside.height, beside.scene_y), (500., 200., 0.));
    assert_eq!((beside.dots[0].x, beside.labels[0].top), (224., 46.5));
    let under = arrange(&legend_scene(200., 200., false, &entries), 300., &Even);
    assert_eq!(under.height, 331.);
    let wrapped = arrange(&legend_scene(200., 200., false, &[("A very long label here", Some("52.3%"))]), 120., &Even);
    assert_eq!(wrapped.height, 284.);
    assert_eq!(wrapped.labels.last().map(|share| (share.x, share.top, share.end)), Some((120., 244., true)));
}

#[test]
fn the_flip_narrows_to_nothing_where_the_faces_swap() {
    let Some(Motion::Flip(flip)) = flip().motion else { panic!("a flip") };
    assert_eq!(flip_scale(&flip, 0.), (1., false));
    let (scale, past) = flip_scale(&flip, flip.swap_at * flip.duration);
    assert!(scale.abs() < 1e-3 && past);
    assert_eq!(flip_scale(&flip, flip.duration), (1., true));
}

#[test]
fn a_piece_of_confetti_falls_turns_and_fades() {
    let Some(Motion::Confetti(confetti)) = celebration().motion else { panic!("confetti") };
    let piece = &confetti.pieces[1];
    let at = |seconds: f32| confetti_frames(&confetti, 1000., 800., seconds).swap_remove(1);
    let start = at(0.);
    assert_eq!((start.x, start.y, start.turn, start.opacity), (370., -12., 0., 1.));
    let middle = at(piece.delay + piece.duration / 2.);
    let eased = 0.861_68;
    assert!((middle.x - (370. - 29. * eased)).abs() < 0.01);
    assert!((middle.y - (-12. + 1.05 * 800. * eased)).abs() < 0.05);
    assert!((middle.turn - 97. * eased).abs() < 0.01);
    let end = at(confetti.duration);
    assert!((end.x - 341.).abs() < 1e-3 && (end.y - 828.).abs() < 1e-3);
    assert!((end.turn - 97.).abs() < 1e-3 && (end.opacity - 0.7).abs() < 1e-4);
    assert_eq!((end.width, end.height, end.radius), (8., 12., 2.));
}
