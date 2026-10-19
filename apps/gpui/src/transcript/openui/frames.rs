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
    use std::rc::Rc;

    struct Dumped {
        tree: motile_core::render::openui::tree::UiNode,
        width: f32,
    }

    impl Render for Dumped {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let ui_cx = super::UiCx {
                row: "dump".into(),
                key: 0,
                column: self.width,
                width: self.width,
                window_height: 900.,
                text_size: crate::theme::PROSE_SIZE,
                surface: crate::theme::Surface::Background,
                ground: None,
                sink: Rc::new(|_, _, _| {}),
                store: None,
                transcript: None,
            };
            div().w(px(self.width)).child(super::root(&self.tree, &ui_cx, window, cx))
        }
    }

    let window =
        cx.open_window(size(px(width), px(4000.)), |_, cx| cx.new(|_| Dumped { tree, width })).expect("a window");
    let draw = |cx: &mut HeadlessAppContext| {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("a frame");
    };
    draw(cx);
    cx.run_until_parked();
    RECORDED.with_borrow_mut(|recorded| *recorded = Some(Vec::new()));
    draw(cx);
    let recorded = RECORDED.with_borrow_mut(|recorded| recorded.take()).unwrap_or_default();
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

    /// Lays out every golden tree at both widths; with `MOTILE_FRAME_DUMPS` set to a folder, writes
    /// each dump there as `<name>-<width>.json` for the test that compares the clients.
    #[test]
    fn every_golden_tree_lays_out() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        let out = std::env::var_os("MOTILE_FRAME_DUMPS").map(std::path::PathBuf::from);
        let goldens = format!("{}/../../crates/core/fixtures/openui", env!("CARGO_MANIFEST_DIR"));
        let mut names: Vec<String> = std::fs::read_dir(goldens)
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_stem()?.to_str().map(str::to_string))
            .collect();
        names.sort();
        for name in names {
            for width in [390., 720.] {
                let frames = frame_dump(&mut cx, golden(&name), width);
                assert!(frame(&frames, "")[3] > 0., "{name} at {width} has no height");
                if let Some(out) = &out {
                    let path = out.join(format!("{name}-{width}.json"));
                    std::fs::write(path, serde_json::to_string_pretty(&frames).unwrap()).unwrap();
                }
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
    fn a_cards_content_is_inset_by_its_padding_which_narrows_in_a_narrow_column() {
        let Some(mut cx) = headless() else {
            eprintln!("skipped: GPUI has no text system here");
            return;
        };
        for (width, inset) in [(720., 21.), (390., 17.)] {
            let frames = frame_dump(&mut cx, golden("text"), width);
            let [x, y, content, _] = frame(&frames, "children/0");
            assert_eq!((x, y, content), (inset, inset, width - 2. * inset));
        }
    }
}
