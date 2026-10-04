//! The push button of the Mac: a raised, lightly bordered surface with its title, in three
//! sizes, or filled with the accent colour for the button that goes ahead.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{colors, is_dark};
use crate::ui::{OnClick, icons};

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonSize {
    Small,
    Regular,
    Large,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonStyle {
    Bordered,
    /// The default action, in the accent colour.
    Prominent,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    title: SharedString,
    symbol: Option<&'static str>,
    size: ButtonSize,
    style: ButtonStyle,
    disabled: bool,
    help: Option<SharedString>,
    on_click: Option<OnClick>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            symbol: None,
            size: ButtonSize::Regular,
            style: ButtonStyle::Bordered,
            disabled: false,
            help: None,
            on_click: None,
        }
    }

    pub fn symbol(mut self, symbol: &'static str) -> Self {
        self.symbol = Some(symbol);
        self
    }

    pub fn small(mut self) -> Self {
        self.size = ButtonSize::Small;
        self
    }

    pub fn large(mut self) -> Self {
        self.size = ButtonSize::Large;
        self
    }

    pub fn prominent(mut self) -> Self {
        self.style = ButtonStyle::Prominent;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn help(mut self, help: impl Into<SharedString>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let dark = is_dark(cx);
        let (height, text_size, padding, radius) = match self.size {
            ButtonSize::Small => (20., 11.5, 8., 5.),
            ButtonSize::Regular => (22., 13., 9., 6.),
            ButtonSize::Large => (30., 13., 14., 8.),
        };
        let prominent = self.style == ButtonStyle::Prominent;
        let (background, hover, foreground) = if prominent {
            (c.accent, c.accent.opacity(0.88), white())
        } else if dark {
            (hsla(0., 0., 1., 0.1), hsla(0., 0., 1., 0.14), c.text)
        } else {
            (white(), hsla(0., 0., 0.97, 1.), c.text)
        };
        let disabled = self.disabled;
        div()
            .id(self.id)
            .flex_shrink_0()
            .h(px(height))
            .px(px(padding))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .rounded(px(radius))
            .bg(background)
            .when(!prominent, |button| button.border_1().border_color(if dark { c.border } else { c.strong_border }))
            .shadow(crate::ui::shadow(hsla(0., 0., 0., if dark { 0.25 } else { 0.08 }), 0.5, 1.))
            .text_size(px(text_size))
            .font_weight(if prominent { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
            .text_color(foreground)
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button.hover(move |button| button.bg(hover)).active(|button| button.opacity(0.85))
            })
            .when_some(self.symbol, |button, symbol| button.child(icons::symbol(symbol, text_size)))
            .child(self.title)
            .when_some(self.help, |button, help| button.tooltip(crate::ui::tooltip(help)))
            .when_some(self.on_click.filter(|_| !disabled), |button, on_click| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}
