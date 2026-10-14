//! A list that only draws the rows on screen, each as tall as its kind is, so that it goes
//! through thousands of rows as fast as through ten. It fades out at an edge with more rows behind
//! it, shows the scroller while it scrolls, and eases to the row it is asked to show:
//! `RecycledListMac.swift`.

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::ease_out;
use crate::ui::parts::{EDGE_FADE, edge_fade};

const SCROLL_FOR: Duration = Duration::from_millis(150);
/// How far past the edges rows are drawn, so that a row sliding in during a drag is there.
const OVERDRAW: f32 = 80.;

/// Where each row of a list lies in its content, with the room above and below them.
#[derive(Clone)]
pub struct Layout {
    pub tops: Vec<f32>,
    pub heights: Vec<f32>,
    content: f32,
}

impl Layout {
    pub fn new(heights: Vec<f32>, top_inset: f32, bottom_inset: f32) -> Self {
        let mut tops = Vec::with_capacity(heights.len());
        let mut y = top_inset;
        for height in &heights {
            tops.push(y);
            y += height;
        }
        Self { tops, heights, content: y + bottom_inset }
    }

    pub fn content(&self) -> f32 {
        self.content
    }

    pub fn bottom(&self, index: usize) -> f32 {
        self.tops[index] + self.heights[index]
    }
}

/// What a list remembers between frames: where it is scrolled, its rows' ids, and the row it
/// last scrolled to.
pub struct RecycledList {
    pub scroll: ScrollHandle,
    ids: Vec<String>,
    target: Option<String>,
    animation: Option<(f32, f32, Instant)>,
}

impl RecycledList {
    pub fn new() -> Self {
        Self { scroll: ScrollHandle::new(), ids: Vec::new(), target: None, animation: None }
    }

    /// Takes the rows' ids. Whether they are others than last time, which puts away whatever
    /// pointed at the rows that were there.
    pub fn set_ids(&mut self, ids: Vec<String>) -> bool {
        if ids == self.ids {
            return false;
        }
        self.ids = ids;
        true
    }

    /// How far the content is scrolled up.
    pub fn scroll_top(&self) -> f32 {
        (-f32::from(self.scroll.offset().y)).max(0.)
    }

    /// The list's bounds in the window, as last laid out.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.scroll.bounds()
    }

    fn viewport(&self) -> f32 {
        f32::from(self.scroll.bounds().size.height)
    }

    /// Where the row's top is in the window.
    pub fn row_top(&self, layout: &Layout, index: usize) -> f32 {
        f32::from(self.bounds().origin.y) + layout.tops[index] - self.scroll_top()
    }

    /// The content's y at the window's `y`.
    pub fn content_y(&self, y: f32) -> f32 {
        y - f32::from(self.bounds().origin.y) + self.scroll_top()
    }

    /// Brings the target's row into sight once it is in the list: only into sight, or to the
    /// top clear of the fade. A list only just made is laid out first.
    pub fn follow(&mut self, target: Option<&str>, to_top: bool, layout: &Layout, window: &mut Window) {
        if target == self.target.as_deref() {
            return;
        }
        let Some(target) = target else {
            self.target = None;
            return;
        };
        let Some(index) = self.ids.iter().position(|id| id == target) else { return };
        if self.viewport() <= 0. {
            window.request_animation_frame();
            return;
        }
        self.target = Some(target.to_string());
        let (top, bottom) = (layout.tops[index], layout.bottom(index));
        let (current, viewport) = (self.scroll_top(), self.viewport());
        let goal = if to_top {
            top - EDGE_FADE
        } else if top < current {
            top
        } else if bottom > current + viewport {
            bottom - viewport
        } else {
            return;
        };
        let goal = goal.clamp(0., (layout.content() - viewport).max(0.));
        if goal == current {
            return;
        }
        self.animation = Some((current, goal, Instant::now()));
        window.request_animation_frame();
    }

    /// The user scrolled: the list stays where they put it.
    pub fn stop_scrolling(&mut self) {
        self.animation = None;
    }

    /// Moves the scroll on toward its goal, for this frame.
    fn step(&mut self, window: &mut Window) {
        let Some((from, to, started)) = self.animation else { return };
        let progress = (started.elapsed().as_secs_f32() / SCROLL_FOR.as_secs_f32()).min(1.);
        let at = from + (to - from) * ease_out(progress);
        self.scroll.set_offset(point(px(0.), px(-at)));
        if progress < 1. {
            window.request_animation_frame();
        } else {
            self.animation = None;
        }
    }

    /// The rows on screen, or on the screen the list would fill before it was laid out.
    pub fn visible(&mut self, layout: &Layout, window: &mut Window) -> Range<usize> {
        self.step(window);
        let viewport = if self.viewport() > 0. {
            self.viewport()
        } else {
            f32::from(crate::window_frame::content_size(window).height)
        };
        let (top, bottom) = (self.scroll_top() - OVERDRAW, self.scroll_top() + viewport + OVERDRAW);
        let (mut start, mut high) = (0, layout.tops.len());
        while start < high {
            let middle = (start + high) / 2;
            if layout.bottom(middle) <= top { start = middle + 1 } else { high = middle }
        }
        let end = layout.tops.partition_point(|&row| row < bottom);
        start..end.max(start)
    }

    /// The scrolling content with its rows, each placed where `layout` says, with the system's
    /// scroller, fading at an edge with more behind it into `fill`, the colour the list lies on.
    pub fn element(&self, id: impl Into<ElementId>, layout: &Layout, rows: Vec<AnyElement>, fill: Hsla) -> Div {
        let above = self.scroll_top();
        let below = if self.viewport() > 0. { (layout.content() - above - self.viewport()).max(0.) } else { 0. };
        let id = id.into();
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id(id.clone())
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().relative().w_full().h(px(layout.content())).children(rows)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::vertical(&self.scroll).id((id, "scroller")).viewport_from_layout()),
            )
            .when(above > 0., |list| list.child(edge_fade(true, above, fill)))
            .when(below > 0., |list| list.child(edge_fade(false, below, fill)))
    }
}

/// A row at its place in the list's content, `shift` from where it is laid out.
pub fn place(layout: &Layout, index: usize, shift: f32, row: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(layout.tops[index] + shift))
        .h(px(layout.heights[index]))
        .child(row)
        .into_any_element()
}
