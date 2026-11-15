//! Tabs as words on a line across the whole width, the one on underlined: `UnderlineTabs.swift`.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Layer, Surface, colors};
use crate::ui::TakesPress;
use crate::ui::control::OnSelect;

const GAP: f32 = 4.;
/// Between a tab's words and its sides.
const PADDING: f32 = 16.;
const UNDERLINE: f32 = 2.;
/// Between a tab's light and the line under it.
const LIGHT_GAP: f32 = 4.;

/// The tabs touch, so no click falls between them; their lights and lines are drawn `GAP` apart,
/// and one lights as a ghost button does above the line.
#[derive(IntoElement)]
pub struct UnderlineTabs {
    id: ElementId,
    options: Vec<SharedString>,
    selected: usize,
    size: ControlSize,
    on_select: Option<OnSelect>,
}

impl UnderlineTabs {
    pub fn new(id: impl Into<ElementId>, options: Vec<SharedString>, selected: usize) -> Self {
        Self { id: id.into(), options, selected, size: ControlSize::Regular, on_select: None }
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for UnderlineTabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hovers: Vec<Entity<bool>> = (0..self.options.len())
            .map(|index| crate::ui::hover_state(&(self.id.clone(), format!("tab-{index}")).into(), window, cx))
            .collect();
        let c = colors(cx);
        let size = self.size;
        let count = self.options.len();
        let on_select = self.on_select.filter(|_| !crate::ui::inert(cx));
        let (light, pressed) =
            (Surface::Background.layer(Layer::Control, c), Surface::Background.layer(Layer::ControlLit, c));
        let foreground = c.foreground;
        div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .flex()
            .cursor_default()
            .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(c.border))
            .children(self.options.into_iter().enumerate().map(|(index, title)| {
                let selected = index == self.selected;
                let on_select = on_select.clone();
                let group = SharedString::from(format!("underline-tab-{:?}-{index}", self.id));
                let lit = div()
                    .id("light")
                    .absolute()
                    .inset_0()
                    .rounded(px(size.radius()))
                    .group_hover(group.clone(), move |lit| lit.bg(light))
                    .group_active(group.clone(), move |lit| lit.bg(pressed));
                let words = div()
                    .relative()
                    .h(px(size.height()))
                    .px(px(PADDING))
                    .flex()
                    .items_center()
                    .text_size(size.text_size())
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .text_color(if selected || *hovers[index].read(cx) { c.foreground } else { c.muted_foreground })
                    .child(lit)
                    .child(div().relative().child(title));
                let underline = div().h(px(UNDERLINE)).rounded_full().when(selected, |line| line.bg(foreground));
                let hovered = hovers[index].clone();
                div()
                    .id(("tab", index))
                    .group(group)
                    .on_hover(crate::ui::track_hover(&hovered))
                    .flex()
                    .flex_col()
                    .gap(px(LIGHT_GAP))
                    .pl(px(if index == 0 { 0. } else { GAP / 2. }))
                    .pr(px(if index == count - 1 { 0. } else { GAP / 2. }))
                    .child(words)
                    .child(underline)
                    .when_some(on_select, |tab, on_select| {
                        tab.takes_press().on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            on_select(index, window, cx)
                        })
                    })
            }))
    }
}
