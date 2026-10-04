//! A sheet: a card over the window, which waits until it is done with. The window behind it is
//! dimmed and takes no clicks. It is drawn over the whole window from wherever it is put.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::colors;

#[derive(IntoElement)]
pub struct Sheet {
    id: ElementId,
    width: f32,
    radius: f32,
    content: AnyElement,
}

pub fn sheet(id: impl Into<ElementId>, width: f32, content: impl IntoElement, _cx: &App) -> Sheet {
    sheet_with(id, width, 18., content)
}

pub fn sheet_with(id: impl Into<ElementId>, width: f32, radius: f32, content: impl IntoElement) -> Sheet {
    Sheet { id: id.into(), width, radius, content: content.into_any_element() }
}

impl RenderOnce for Sheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let size = window.viewport_size();
        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                div()
                    .id(self.id)
                    .w(size.width)
                    .h(size.height)
                    .bg(c.scrim)
                    .flex()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("sheet-card")
                            .w(px(self.width))
                            .max_h(size.height * 0.9)
                            .bg(c.background)
                            .rounded(px(self.radius))
                            .border_1()
                            .border_color(c.border)
                            .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.22), 12., 40.))
                            .overflow_hidden()
                            .child(self.content),
                    ),
            ),
        )
        .with_priority(1)
    }
}
