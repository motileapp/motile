//! Text with styled stretches that the user can select and copy, and links that open with a
//! click. It takes part in the window's one selection, like GPUI Kit's `SelectableText`.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::{TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};
use gpui_kit::*;

type OnLink = Rc<dyn Fn(&str, &mut Window, &mut App)>;

pub struct RichText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    links: Rc<Vec<(Range<usize>, String)>>,
    on_link: Option<OnLink>,
    /// Stretches drawn on a rounded tint as tall as their glyphs, as the Mac draws inline code.
    tinted: Vec<Range<usize>>,
    tint: Hsla,
}

impl RichText {
    /// Text in runs of fonts and colours that cover all of it.
    pub fn with_runs(id: impl Into<ElementId>, text: impl Into<SharedString>, runs: Vec<TextRun>) -> Self {
        let text = text.into();
        Self {
            id: id.into(),
            styled: StyledText::new(text.clone()).with_runs(runs),
            text,
            links: Rc::new(Vec::new()),
            on_link: None,
            tinted: Vec::new(),
            tint: Hsla::transparent_black(),
        }
    }

    pub fn tinted(mut self, ranges: Vec<Range<usize>>, tint: Hsla) -> Self {
        self.tinted = ranges;
        self.tint = tint;
        self
    }

    /// The tint behind a stretch, a rounded box on each line it reaches.
    fn paint_tint(layout: &TextLayout, range: Range<usize>, height: Pixels, color: Hsla, window: &mut Window) {
        let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
        else {
            return;
        };
        let bounds = layout.bounds();
        let line_height = layout.line_height();
        let inset = (line_height - height) / 2.;
        let pad = px(2.);
        let mut boxes = Vec::new();
        let mut top = start.y;
        while top <= end.y {
            let left = if top == start.y { start.x } else { bounds.left() };
            let right = if top == end.y { end.x } else { bounds.right() };
            if right > left {
                boxes.push(Bounds::from_corners(
                    point(left - pad, top + inset),
                    point(right + pad, top + inset + height),
                ));
            }
            top += line_height;
        }
        for tint in boxes {
            window.paint_quad(fill(tint, color).corner_radii(px(4.)));
        }
    }

    pub fn links(
        mut self,
        links: Vec<(Range<usize>, String)>,
        on_link: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.links = Rc::new(links);
        self.on_link = Some(Rc::new(on_link));
        self
    }

    fn paint_selection(layout: &TextLayout, range: Range<usize>, color: Hsla, window: &mut Window) {
        let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
        else {
            return;
        };
        let bounds = layout.bounds();
        let line_height = layout.line_height();
        let mut quads = Vec::new();
        if start.y == end.y {
            quads.push(Bounds::from_corners(start, point(end.x, end.y + line_height)));
        } else {
            quads.push(Bounds::from_corners(start, point(bounds.right(), start.y + line_height)));
            if end.y > start.y + line_height {
                quads.push(Bounds::from_corners(
                    point(bounds.left(), start.y + line_height),
                    point(bounds.right(), end.y),
                ));
            }
            quads.push(Bounds::from_corners(point(bounds.left(), end.y), point(end.x, end.y + line_height)));
        }
        for quad in quads {
            window.paint_quad(fill(quad, color));
        }
    }
}

impl IntoElement for RichText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RichText {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let handle = window.with_element_state(
            global_id.expect("RichText has an id"),
            |retained: Option<TextSelectionHandle>, _| {
                let handle = retained.unwrap_or_else(|| TextSelectionHandle::new(self.text.clone(), cx));
                (handle.clone(), handle)
            },
        );
        handle.set_fallback_copy_text(self.text.to_string(), cx);
        let (layout_id, ()) = self.styled.request_layout(global_id, inspector_id, window, cx);
        (layout_id, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled.prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout().clone();
        let selected_before = TextSelection::selected_text(window, cx);
        let projection = handle.update_runs(&[TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)], cx);
        if selected_before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        let glyphs = window.text_style().font_size.to_pixels(window.rem_size()) * 1.22;
        for range in self.tinted.clone() {
            Self::paint_tint(&layout, range, glyphs, self.tint, window);
        }
        let color = gpui_kit::base::Theme::global(cx).tokens.colors.selection;
        for range in projection.ranges().iter().flatten().cloned() {
            Self::paint_selection(&layout, range, color, window);
        }
        self.styled.paint(global_id, inspector_id, bounds, &mut (), &mut (), window, cx);

        if self.links.is_empty() {
            return;
        }
        let link_at = {
            let links = self.links.clone();
            let layout = layout.clone();
            move |position: Point<Pixels>| -> Option<String> {
                let index = layout.index_for_position(position).ok()?;
                links.iter().find(|(range, _)| range.contains(&index)).map(|(_, url)| url.clone())
            }
        };
        if hitbox.is_hovered(window) && link_at(window.mouse_position()).is_some() {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        let Some(on_link) = self.on_link.clone() else { return };
        let hitbox = hitbox.clone();
        let pressed: Rc<std::cell::Cell<Option<Point<Pixels>>>> = Rc::default();
        let down = pressed.clone();
        let down_link = link_at.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
            if phase.bubble()
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
                && down_link(event.position).is_some()
            {
                down.set(Some(event.position));
            }
        });
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            let Some(start) = pressed.take() else { return };
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
            // A drag selects; only a click opens the link.
            let moved = (event.position.x - start.x).abs() + (event.position.y - start.y).abs();
            if moved > px(4.) {
                return;
            }
            if let Some(url) = link_at(event.position) {
                on_link(&url, window, cx);
            }
        });
    }
}
