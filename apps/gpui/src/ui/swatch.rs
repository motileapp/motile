//! A colour to pick: a dot as tall as a button of its size, ringed in the foreground when it is the
//! one picked.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, colors};
use crate::ui::{OnClick, tooltip};

/// The ring around the one picked.
const RING: f32 = 2.;
/// The room between the dot and the button's edge, of its height.
const INSET: f32 = 0.2;

#[derive(IntoElement)]
pub struct Swatch {
    id: ElementId,
    color: Hsla,
    help: SharedString,
    selected: bool,
    size: ControlSize,
    on_click: Option<OnClick>,
}

impl Swatch {
    pub fn new(id: impl Into<ElementId>, color: Hsla, help: impl Into<SharedString>) -> Self {
        Self { id: id.into(), color, help: help.into(), selected: false, size: ControlSize::Regular, on_click: None }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for Swatch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let side = self.size.height();
        let inset = side * INSET;
        div()
            .id(self.id)
            .flex_none()
            .size(px(side))
            .rounded_full()
            .relative()
            .cursor_default()
            .child(
                div()
                    .absolute()
                    .top(px(inset))
                    .left(px(inset))
                    .size(px(side - 2. * inset))
                    .rounded_full()
                    .bg(self.color)
                    .border_1()
                    .border_color(c.border_accent),
            )
            .when(self.selected, |swatch| {
                swatch.child(div().absolute().inset_0().rounded_full().border(px(RING)).border_color(c.foreground))
            })
            .tooltip(tooltip(self.help))
            .when_some(self.on_click, |swatch, on_click| {
                swatch.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}
