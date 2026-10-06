//! Menus, drawn by the app: a popover card of rows under the control that opened it, for context
//! menus and the buttons that open a menu. Each item runs a closure once the menu has closed.
//! One menu is open at a time; the window's root draws it over everything else.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{Radius, colors};
use crate::ui::icons;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// The image in front of an item: a symbol, or an image file on this device.
pub enum MenuIcon {
    Symbol(&'static str),
    File(SharedString),
    /// An agent's logo, in its colour.
    Logo(motile_protocol::wire::Agent),
}

enum Entry {
    Item {
        label: SharedString,
        enabled: bool,
        checked: bool,
        danger: bool,
        icon: Option<MenuIcon>,
        tooltip: Option<SharedString>,
        run: Run,
    },
    Note(SharedString),
    Separator,
}

#[derive(Default)]
pub struct Menu {
    entries: Vec<Entry>,
}

impl Menu {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(mut self, entry: Entry) -> Self {
        self.entries.push(entry);
        self
    }

    fn entry(label: impl Into<SharedString>, enabled: bool, run: impl Fn(&mut Window, &mut App) + 'static) -> Entry {
        Entry::Item {
            label: label.into(),
            enabled,
            checked: false,
            danger: false,
            icon: None,
            tooltip: None,
            run: Rc::new(run),
        }
    }

    pub fn item(self, label: impl Into<SharedString>, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.push(Self::entry(label, true, run))
    }

    pub fn item_if(
        self,
        enabled: bool,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Self::entry(label, enabled, run))
    }

    pub fn checked(
        self,
        checked: bool,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let Entry::Item { label, enabled, icon, tooltip, run, .. } = Self::entry(label, true, run) else {
            unreachable!()
        };
        self.push(Entry::Item { label, enabled, checked, danger: false, icon, tooltip, run })
    }

    /// An item for what can't be taken back, in the danger colour.
    pub fn danger_item(self, label: impl Into<SharedString>, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        let Entry::Item { label, enabled, icon, tooltip, run, .. } = Self::entry(label, true, run) else {
            unreachable!()
        };
        self.push(Entry::Item { label, enabled, checked: false, danger: true, icon, tooltip, run })
    }

    /// An item with an image in front, and what the pointer resting on it says.
    pub fn icon_item(
        self,
        label: impl Into<SharedString>,
        icon: MenuIcon,
        enabled: bool,
        tooltip: Option<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Entry::Item {
            label: label.into(),
            enabled,
            checked: false,
            danger: false,
            icon: Some(icon),
            tooltip,
            run: Rc::new(run),
        })
    }

    /// One of the things to choose from, with a check mark when it is the one in use.
    pub fn choice(
        self,
        checked: bool,
        label: impl Into<SharedString>,
        icon: Option<MenuIcon>,
        tooltip: Option<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Entry::Item {
            label: label.into(),
            enabled: true,
            checked,
            danger: false,
            icon,
            tooltip,
            run: Rc::new(run),
        })
    }

    /// An item with a symbol in front.
    pub fn symbol_item(
        self,
        label: impl Into<SharedString>,
        symbol: &'static str,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.icon_item(label, MenuIcon::Symbol(symbol), true, None, run)
    }

    /// A line that only says something.
    pub fn note(self, label: impl Into<SharedString>) -> Self {
        self.push(Entry::Note(label.into()))
    }

    pub fn separator(self) -> Self {
        self.push(Entry::Separator)
    }

    pub fn when(self, condition: bool, build: impl FnOnce(Menu) -> Menu) -> Self {
        if condition { build(self) } else { self }
    }

    pub fn when_some<T>(self, value: Option<T>, build: impl FnOnce(Menu, T) -> Menu) -> Self {
        match value {
            Some(value) => build(self, value),
            None => self,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Opens the menu with its top left corner at `position`, in the window.
    pub fn show(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.open(position, false, window, cx);
    }

    /// Opens the menu with its top right corner at `position`.
    pub fn show_right_aligned(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.open(position, true, window, cx);
    }

    fn open(self, position: Point<Pixels>, right_aligned: bool, window: &mut Window, cx: &mut App) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        cx.set_global(Layer(Some(Open { menu: Rc::new(self), position, right_aligned, focus, highlighted: None })));
        window.refresh();
    }

    fn runnable(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry, Entry::Item { enabled: true, .. }))
            .map(|(index, _)| index)
            .collect()
    }
}

struct Open {
    menu: Rc<Menu>,
    position: Point<Pixels>,
    right_aligned: bool,
    focus: FocusHandle,
    highlighted: Option<usize>,
}

#[derive(Default)]
struct Layer(Option<Open>);

impl Global for Layer {}

/// Closes the menu that is open, if one is.
pub fn close(window: &mut Window, cx: &mut App) {
    if cx.try_global::<Layer>().is_some_and(|layer| layer.0.is_some()) {
        cx.global_mut::<Layer>().0 = None;
        window.refresh();
    }
}

fn pick(index: usize, window: &mut Window, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.take() else { return };
    window.refresh();
    let Some(Entry::Item { run, enabled: true, .. }) = open.menu.entries.get(index) else { return };
    run(window, cx);
}

fn step(forward: bool, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    let runnable = open.menu.runnable();
    if runnable.is_empty() {
        return;
    }
    let at = open.highlighted.and_then(|index| runnable.iter().position(|&candidate| candidate == index));
    let next = match (at, forward) {
        (None, true) => 0,
        (None, false) => runnable.len() - 1,
        (Some(at), true) => (at + 1) % runnable.len(),
        (Some(at), false) => (at + runnable.len() - 1) % runnable.len(),
    };
    open.highlighted = Some(runnable[next]);
}

const MENU_WIDTH: (f32, f32) = (180., 340.);
const ROW_HEIGHT: f32 = 28.;

/// The open menu over the window, for the root to draw last.
pub fn layer(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (menu, position, right_aligned, focus, highlighted) = {
        let open = cx.try_global::<Layer>()?.0.as_ref()?;
        (open.menu.clone(), open.position, open.right_aligned, open.focus.clone(), open.highlighted)
    };
    if !focus.is_focused(window) {
        window.focus(&focus, cx);
    }
    let c = colors(cx);
    let any_checked = menu.entries.iter().any(|entry| matches!(entry, Entry::Item { checked: true, .. }));
    let rows = menu.entries.iter().enumerate().map(|(index, entry)| match entry {
        Entry::Separator => div().h(px(1.)).my(px(4.)).mx(px(-4.)).bg(c.border_secondary).into_any_element(),
        Entry::Note(label) => div()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .text_color(c.tertiary)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(label.clone())
            .into_any_element(),
        Entry::Item { label, enabled, checked, danger, icon, tooltip, .. } => {
            let lit = highlighted == Some(index) && *enabled;
            div()
                .id(("menu-item", index))
                .h(px(ROW_HEIGHT))
                .px(px(8.))
                .rounded(px(Radius::SMALL))
                .flex()
                .items_center()
                .gap(px(7.))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if *danger { c.danger } else { c.text })
                .when(!*enabled, |row| row.opacity(0.45))
                .when(lit, |row| row.bg(c.popover_secondary))
                .when(any_checked, |row| {
                    row.child(
                        div()
                            .size(px(14.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(*checked, |mark| mark.child(icons::symbol("check", 12.))),
                    )
                })
                .when_some(icon.as_ref(), |row, icon| {
                    row.child(
                        div().flex_shrink_0().flex().items_center().text_color(c.secondary).child(match icon {
                            MenuIcon::Symbol(name) => icons::symbol(*name, 13.).into_any_element(),
                            MenuIcon::File(path) => img(std::path::PathBuf::from(path.as_ref()))
                                .size(px(16.))
                                .rounded(px(3.))
                                .into_any_element(),
                            MenuIcon::Logo(agent) => crate::ui::logos::agent_icon(*agent, 16., cx).into_any_element(),
                        }),
                    )
                })
                .child(
                    div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(label.clone()),
                )
                .when_some(tooltip.clone(), |row, tooltip| row.tooltip(crate::ui::tooltip(tooltip)))
                .when(*enabled, |row| {
                    row.on_hover(move |inside, window, cx| {
                        if let Some(open) = cx.global_mut::<Layer>().0.as_mut() {
                            if *inside {
                                open.highlighted = Some(index);
                            } else if open.highlighted == Some(index) {
                                open.highlighted = None;
                            }
                            window.refresh();
                        }
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        pick(index, window, cx)
                    })
                })
                .into_any_element()
        }
    });
    let card = div()
        .min_w(px(MENU_WIDTH.0))
        .max_w(px(MENU_WIDTH.1))
        .p(px(4.))
        .bg(c.popover)
        .border_1()
        .border_color(c.border_secondary)
        .rounded(px(Radius::CARD))
        .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.22), 8., 24.))
        .flex()
        .flex_col()
        .children(rows);
    let anchored = anchored()
        .position(position)
        .anchor(if right_aligned { gpui_kit::Anchor::TopRight } else { gpui_kit::Anchor::TopLeft })
        .snap_to_window_with_margin(px(8.))
        .child(card);
    let overlay = div()
        .id("menu-layer")
        .absolute()
        .inset_0()
        .occlude()
        .track_focus(&focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            match event.keystroke.key.as_str() {
                "escape" => close(window, cx),
                "up" => {
                    step(false, cx);
                    window.refresh();
                }
                "down" => {
                    step(true, cx);
                    window.refresh();
                }
                "enter" => {
                    let picked =
                        cx.try_global::<Layer>().and_then(|layer| layer.0.as_ref()).and_then(|open| open.highlighted);
                    if let Some(index) = picked {
                        pick(index, window, cx);
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            cx.stop_propagation();
            close(window, cx);
        })
        .on_mouse_down(MouseButton::Right, |_, window, cx| {
            cx.stop_propagation();
            close(window, cx);
        })
        .child(anchored);
    Some(deferred(overlay).with_priority(2).into_any_element())
}

/// Where a button that opens a menu was last drawn, so the menu opens under it.
#[derive(Clone, Default)]
pub struct Anchor(Rc<std::cell::Cell<Bounds<Pixels>>>);

impl Anchor {
    /// An element that fills its parent and keeps the parent's bounds here.
    pub fn track(&self) -> impl IntoElement {
        let cell = self.0.clone();
        canvas(move |bounds, _, _| cell.set(bounds), |_, _, _, _| {}).absolute().size_full()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.0.get()
    }

    /// Under the element and centred on it, where a popover `width` wide opens.
    pub fn popover_origin(&self, width: f32) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.center().x - px(width / 2.), bounds.origin.y + bounds.size.height + px(6.))
    }

    /// Under the element, where a pull-down menu opens.
    pub fn below(&self) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.origin.x, bounds.origin.y + bounds.size.height + px(4.))
    }

    /// Under the element's right edge, where a menu that ends with it opens.
    pub fn below_right(&self, gap: f32) -> Point<Pixels> {
        let bounds = self.0.get();
        point(bounds.origin.x + bounds.size.width, bounds.origin.y + bounds.size.height + px(gap))
    }
}
