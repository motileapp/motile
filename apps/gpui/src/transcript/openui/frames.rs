//! Where each node of a program was laid out, for the test that compares the clients: a node's box
//! is wrapped in [`Framed`], which takes no room of its own and notes the box's bounds while a dump
//! is being recorded.

use std::cell::RefCell;

use gpui_kit::*;

/// Each node's id and bounds, in the order they were laid out.
type Frames = Vec<(String, Bounds<Pixels>)>;

thread_local! {
    static RECORDED: RefCell<Option<Frames>> = const { RefCell::new(None) };
}

/// Notes the bounds in the window of every node drawn from now, until [`recorded`] is asked.
pub fn record() {
    RECORDED.with_borrow_mut(|recorded| *recorded = Some(Vec::new()));
}

/// What was drawn since [`record`], each node by its id in the order it was laid out.
pub fn recorded() -> Vec<(String, Bounds<Pixels>)> {
    RECORDED.with_borrow_mut(|recorded| recorded.take()).unwrap_or_default()
}

pub struct Framed {
    id: String,
    child: AnyElement,
}

impl Framed {
    pub fn new(id: &str, child: impl IntoElement) -> Self {
        Self { id: id.to_string(), child: child.into_any_element() }
    }
}

impl IntoElement for Framed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Framed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        RECORDED.with_borrow_mut(|recorded| {
            if let Some(recorded) = recorded {
                recorded.push((self.id.clone(), bounds));
            }
        });
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// How wide the window is that the trees are drawn in, as the Apple clients' frames have it.
#[cfg(test)]
const WINDOW_WIDTH: f32 = 1200.;

/// The tree drawn `width` wide in a headless window of `cx` `height` tall, dark on the transcript's
/// background, as the transcript draws it, once what it laid out off the main thread has arrived.
#[cfg(test)]
fn drawn(
    cx: &mut HeadlessAppContext,
    tree: motile_core::render::openui::tree::UiNode,
    width: f32,
    height: f32,
) -> AnyWindowHandle {
    use std::rc::Rc;

    struct Drawn {
        tree: motile_core::render::openui::tree::UiNode,
        width: f32,
    }

    impl Render for Drawn {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let ui_cx = super::UiCx {
                row: "dump".into(),
                key: 0,
                column: self.width,
                width: self.width,
                window_height: 800.,
                text_size: crate::theme::PROSE_SIZE,
                surface: crate::theme::Surface::Background,
                streaming: false,
                ground: None,
                sink: Rc::new(|_, _, _| {}),
                store: None,
                transcript: None,
            };
            let c = crate::theme::colors(cx);
            div()
                .w(px(self.width))
                .bg(c.background)
                .text_color(c.foreground)
                .font_family(crate::theme::UI_FONT)
                .child(super::root(&self.tree, &ui_cx, window, cx))
        }
    }

    let window = cx
        .open_window(size(px(WINDOW_WIDTH.max(width)), px(height)), |window, cx| {
            crate::theme::apply(true, window, cx);
            cx.new(|_| Drawn { tree, width })
        })
        .expect("a window");
    draw(cx, window.into());
    cx.run_until_parked();
    window.into()
}

#[cfg(test)]
fn draw(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("a frame");
}

/// Closes the window, so that what it drew, a map's animation above all, no longer runs.
#[cfg(test)]
fn close(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.remove_window()).expect("a window");
    cx.run_until_parked();
}

/// Every node's frame `[x, y, width, height]` in points from the root's top left corner, by its
/// id, for the tree laid out `width` wide: `{"children/0": [0, 0, 720, 24], …}`. It draws the
/// tree in a headless window of `cx`, as the transcript draws it; a node drawn twice (a carousel's
/// copy, a popover's panel) keeps its first frame.
#[cfg(test)]
pub fn frame_dump(
    cx: &mut HeadlessAppContext,
    tree: motile_core::render::openui::tree::UiNode,
    width: f32,
) -> serde_json::Value {
    let window = drawn(cx, tree, width, 4000.);
    record();
    draw(cx, window);
    let recorded = recorded();
    close(cx, window);
    let origin = recorded.iter().find(|(id, _)| id.is_empty()).map_or(Point::default(), |(_, bounds)| bounds.origin);
    let mut frames = serde_json::Map::new();
    for (id, bounds) in recorded {
        if frames.contains_key(&id) {
            continue;
        }
        let at = bounds.origin - origin;
        let frame = [at.x, at.y, bounds.size.width, bounds.size.height].map(f32::from);
        frames.insert(id, serde_json::json!(frame));
    }
    serde_json::Value::Object(frames)
}

/// A picture of the tree drawn `width` wide and as tall as it lays out, for a `cx` with a renderer.
#[cfg(test)]
pub fn picture(
    cx: &mut HeadlessAppContext,
    tree: motile_core::render::openui::tree::UiNode,
    width: f32,
) -> image::RgbaImage {
    let height = frame_dump(cx, tree.clone(), width)[""][3].as_f64().unwrap_or(1.).max(1.).ceil() as f32;
    let window = drawn(cx, tree, width, height);
    draw(cx, window);
    let shot = cx.capture_screenshot(window).expect("a picture");
    close(cx, window);
    let shown = (shot.width() as f32 * width / WINDOW_WIDTH.max(width)).round() as u32;
    image::imageops::crop_imm(&shot, 0, 0, shown, shot.height()).to_image()
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use motile_core::render::openui::tree::UiNode;
    use serde_json::Value;

    use super::*;

    fn headless() -> Option<HeadlessAppContext> {
        let text = crate::transcript::canvas::tests::platform_text()?;
        let mut cx = HeadlessAppContext::new(text);
        let fonts = crate::assets::FONTS.iter().map(|bytes| std::borrow::Cow::Borrowed(*bytes)).collect();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.text_system().add_fonts(fonts)
        })
        .expect("the fonts");
        crate::fonts::load();
        Some(cx)
    }

    fn golden(name: &str) -> UiNode {
        let path = format!("{}/../../crates/core/fixtures/openui/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn goldens() -> Vec<String> {
        let goldens = format!("{}/../../crates/core/fixtures/openui", env!("CARGO_MANIFEST_DIR"));
        let mut names: Vec<String> = std::fs::read_dir(goldens)
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_stem()?.to_str().map(str::to_string))
            .collect();
        names.sort();
        names
    }

    fn frame(frames: &Value, id: &str) -> [f32; 4] {
        let frame = frames[id].as_array().unwrap_or_else(|| panic!("no frame for {id:?}"));
        [0, 1, 2, 3].map(|index| frame[index].as_f64().unwrap() as f32)
    }

    #[test]
    fn every_node_lies_in_the_root_at_the_width_given() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        for name in ["text", "layout", "lists", "answer"] {
            for width in [390., 720.] {
                let frames = frame_dump(&mut cx, golden(name), width);
                let [x, y, root_width, height] = frame(&frames, "");
                assert_eq!((x, y, root_width), (0., 0., width), "{name} at {width}");
                assert!(height > 0.);
                for (id, _) in frames.as_object().unwrap() {
                    let [x, _, node_width, _] = frame(&frames, id);
                    assert!(x >= -0.5 && x + node_width <= width + 0.5, "{name}: {id} at {width} is outside the root");
                }
            }
        }
    }

    /// Lays out every golden tree at both widths, and writes each dump to build/frames/gpui as
    /// `<name>-<width>.json` for `scripts/compare-frames.sh`.
    #[test]
    fn every_golden_tree_lays_out() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/frames/gpui");
        std::fs::create_dir_all(&out).unwrap();
        for name in goldens() {
            for width in [390., 720.] {
                let frames = frame_dump(&mut cx, golden(&name), width);
                assert!(frame(&frames, "")[3] > 0., "{name} at {width} has no height");
                std::fs::write(out.join(format!("{name}-{width}.json")), frames.to_string()).unwrap();
            }
        }
    }

    /// Every golden tree drawn dark at both widths, as build/programs/gpui/<name>-<width>.png for
    /// `scripts/compare-frames.sh`, where GPUI has a headless renderer.
    #[test]
    fn every_golden_tree_is_pictured() {
        let Some(text) = crate::transcript::canvas::tests::platform_text() else { return };
        if !matches!(gpui_kit::platform::current_headless_renderer(), Ok(Some(_))) {
            eprintln!("skipped: GPUI has no headless renderer here");
            return;
        }
        let mut cx = HeadlessAppContext::with_platform(
            text,
            std::sync::Arc::new(crate::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        let fonts = crate::assets::FONTS.iter().map(|bytes| std::borrow::Cow::Borrowed(*bytes)).collect();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.text_system().add_fonts(fonts)
        })
        .expect("the fonts");
        crate::fonts::load();
        let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/programs/gpui");
        std::fs::create_dir_all(&out).unwrap();
        for name in goldens() {
            for width in [390., 720.] {
                picture(&mut cx, golden(&name), width).save(out.join(format!("{name}-{width}.png"))).unwrap();
            }
        }
    }

    #[test]
    fn a_canvas_is_measured_once_core_laid_it_out() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        let [_, _, _, height] = frame(&frame_dump(&mut cx, golden("bicycle"), 720.), "children/6/children/1");
        assert!(height > 100., "the drawing is {height} tall");
    }

    #[test]
    fn a_map_is_as_tall_as_its_node_asks_in_a_wide_window() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        for width in [390., 720.] {
            let [_, _, map_width, map_height] = frame(&frame_dump(&mut cx, golden("map"), width), "children/2");
            assert_eq!((map_width, map_height), (width, 400.));
        }
    }

    #[test]
    fn a_cards_content_is_inset_by_its_padding_which_narrows_in_a_narrow_column() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        for (width, inset) in [(720., 20.), (390., 16.)] {
            let frames = frame_dump(&mut cx, golden("text"), width);
            let [x, y, content, _] = frame(&frames, "children/0");
            assert_eq!((x, y, content), (inset, inset, width - 2. * inset));
        }
    }
}
