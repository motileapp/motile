//! Menus, drawn by the app: a popover card of rows under the control that opened it, for context
//! menus and the buttons that open a menu. Each item runs a closure once the menu has closed.
//! One menu is open at a time; the window's root draws it over everything else.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Opacity, Radius, Surface, colors};
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
    /// A row that opens another menu beside it.
    Submenu {
        label: SharedString,
        menu: Rc<Menu>,
        anchor: Anchor,
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

    /// An item with a symbol in front, for what can't be taken back, in the danger colour.
    pub fn danger_symbol_item(
        self,
        label: impl Into<SharedString>,
        symbol: &'static str,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Entry::Item {
            label: label.into(),
            enabled: true,
            checked: false,
            danger: true,
            icon: Some(MenuIcon::Symbol(symbol)),
            tooltip: None,
            run: Rc::new(run),
        })
    }

    /// A row that opens `menu` beside it, as the Mac's submenus do.
    pub fn submenu(self, label: impl Into<SharedString>, menu: Menu) -> Self {
        self.push(Entry::Submenu { label: label.into(), menu: Rc::new(menu), anchor: Anchor::default() })
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

    /// Opens the menu with its top left corner at `position`, in the window, as a context menu
    /// does: above the point when there is no room under it.
    pub fn show(self, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.open(position, position.y, false, window, cx);
    }

    /// Opens the menu `gap` under the element the anchor tracks, its left edge on the element's,
    /// or as far above it when there is no room under it.
    pub fn show_under(self, anchor: &Anchor, gap: f32, window: &mut Window, cx: &mut App) {
        let bounds = anchor.bounds();
        let position = point(bounds.origin.x, bounds.bottom() + px(gap));
        self.open(position, bounds.origin.y - px(gap), false, window, cx);
    }

    /// Opens the menu `gap` under the element the anchor tracks, its right edge on the element's,
    /// or as far above it when there is no room under it.
    pub fn show_under_right(self, anchor: &Anchor, gap: f32, window: &mut Window, cx: &mut App) {
        let bounds = anchor.bounds();
        let position = point(bounds.right(), bounds.bottom() + px(gap));
        self.open(position, bounds.origin.y - px(gap), true, window, cx);
    }

    fn open(self, position: Point<Pixels>, above: Pixels, right_aligned: bool, window: &mut Window, cx: &mut App) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        crate::ui::overlay_opened(cx);
        let open = Open {
            menu: Rc::new(self),
            position,
            above,
            right_aligned,
            focus,
            highlighted: None,
            submenu: None,
            sub_highlighted: None,
            typed: String::new(),
            typed_at: None,
        };
        cx.set_global(Layer(Some(open)));
        window.refresh();
    }

    /// How tall the menu is drawn: its rows, its padding and its border.
    fn height(&self) -> f32 {
        let rows: f32 = self
            .entries
            .iter()
            .map(|entry| if matches!(entry, Entry::Separator) { SEPARATOR_HEIGHT } else { ROW_HEIGHT })
            .sum();
        rows + 2. * PADDING + 2.
    }

    fn runnable(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry, Entry::Item { enabled: true, .. } | Entry::Submenu { .. }))
            .map(|(index, _)| index)
            .collect()
    }
}

struct Open {
    menu: Rc<Menu>,
    position: Point<Pixels>,
    /// Where the menu's bottom goes when it opens upwards.
    above: Pixels,
    right_aligned: bool,
    focus: FocusHandle,
    highlighted: Option<usize>,
    /// The row whose menu is open beside this one, and the row of that menu that is lit.
    submenu: Option<usize>,
    sub_highlighted: Option<usize>,
    /// What was typed to find a row, and when last.
    typed: String,
    typed_at: Option<std::time::Instant>,
}

impl Open {
    fn submenu(&self) -> Option<&Rc<Menu>> {
        match self.menu.entries.get(self.submenu?) {
            Some(Entry::Submenu { menu, .. }) => Some(menu),
            _ => None,
        }
    }
}

#[derive(Default)]
struct Layer(Option<Open>);

impl Global for Layer {}

/// Whether a menu is open over the window.
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<Layer>().is_some_and(|layer| layer.0.is_some())
}

/// Closes the menu that is open, if one is.
pub fn close(window: &mut Window, cx: &mut App) {
    if cx.try_global::<Layer>().is_some_and(|layer| layer.0.is_some()) {
        cx.global_mut::<Layer>().0 = None;
        window.refresh();
    }
}

/// Which of the open menus a row is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Top,
    Sub,
}

fn pick(level: Level, index: usize, window: &mut Window, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.take() else { return };
    window.refresh();
    let menu = match level {
        Level::Top => &open.menu,
        Level::Sub => match open.submenu() {
            Some(menu) => menu,
            None => return,
        },
    };
    match menu.entries.get(index) {
        Some(Entry::Item { run, enabled: true, .. }) => run(window, cx),
        Some(Entry::Submenu { menu, .. }) => {
            let first = menu.runnable().first().copied();
            let mut open = open;
            open.submenu = Some(index);
            open.sub_highlighted = first;
            cx.set_global(Layer(Some(open)));
        }
        _ => {}
    }
}

fn next(runnable: &[usize], at: Option<usize>, forward: bool) -> Option<usize> {
    if runnable.is_empty() {
        return None;
    }
    let at = at.and_then(|index| runnable.iter().position(|&candidate| candidate == index));
    let next = match (at, forward) {
        (None, true) => 0,
        (None, false) => runnable.len() - 1,
        (Some(at), true) => (at + 1) % runnable.len(),
        (Some(at), false) => (at + runnable.len() - 1) % runnable.len(),
    };
    Some(runnable[next])
}

fn step(forward: bool, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    if let Some(menu) = open.submenu().filter(|_| open.sub_highlighted.is_some()) {
        open.sub_highlighted = next(&menu.runnable(), open.sub_highlighted, forward);
        return;
    }
    open.highlighted = next(&open.menu.runnable(), open.highlighted, forward);
    open.submenu = None;
}

/// How long typed letters keep adding up to one search, as the system's menus have it.
const TYPING_PAUSE: std::time::Duration = std::time::Duration::from_secs(1);

/// Lights the first row of the menu in front that starts with what was typed, as the system's
/// menus do.
fn type_select(typed: &str, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    let fresh = open.typed_at.is_none_or(|at| at.elapsed() > TYPING_PAUSE);
    if fresh {
        open.typed.clear();
    }
    open.typed.push_str(&typed.to_lowercase());
    open.typed_at = Some(std::time::Instant::now());
    let in_submenu = open.submenu.is_some() && open.sub_highlighted.is_some();
    let menu = if in_submenu { open.submenu().cloned() } else { Some(open.menu.clone()) };
    let Some(menu) = menu else { return };
    let found = menu.runnable().into_iter().find(|&index| {
        let label = match &menu.entries[index] {
            Entry::Item { label, .. } | Entry::Submenu { label, .. } => label,
            _ => return false,
        };
        label.to_lowercase().starts_with(&open.typed)
    });
    let Some(found) = found else { return };
    if in_submenu {
        open.sub_highlighted = Some(found);
    } else {
        open.highlighted = Some(found);
        open.submenu = None;
    }
}

/// → opens the lit row's menu, ← goes back from it.
fn step_level(into: bool, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    if !into {
        open.sub_highlighted = None;
        open.submenu = None;
        return;
    }
    let Some(index) = open.highlighted else { return };
    if !matches!(open.menu.entries.get(index), Some(Entry::Submenu { .. })) {
        return;
    }
    open.submenu = Some(index);
    open.sub_highlighted = open.submenu().and_then(|menu| menu.runnable().first().copied());
}

const MENU_WIDTH: (f32, f32) = (180., 340.);
/// The deferred priority of what drops from a control: over the modals too.
pub const PRIORITY: usize = 100;
const ROW_HEIGHT: f32 = 28.;
const SEPARATOR_HEIGHT: f32 = 9.;
const PADDING: f32 = 4.;
/// How far a menu keeps from the window's edges.
const WINDOW_MARGIN: f32 = 8.;

/// Where the menu's top goes, and how tall it may be: under the point when it fits, else above
/// where it opens upwards when that fits, else wherever in the window it fits best, scrolling
/// when the window is too short for it.
fn placement(open: &Open, height: f32, window: &Window) -> (Pixels, Pixels) {
    let bottom = crate::window_frame::content_bounds(window).bottom() - px(WINDOW_MARGIN);
    let height = px(height);
    if open.position.y + height <= bottom {
        return (open.position.y, height);
    }
    if open.above - height >= px(WINDOW_MARGIN) {
        return (open.above - height, height);
    }
    let room = bottom - px(WINDOW_MARGIN);
    let height = height.min(room);
    ((bottom - height).max(px(WINDOW_MARGIN)), height)
}

/// The open menu over the window, for the root to draw last.
pub fn layer(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (menu, position, right_aligned, focus, highlighted, submenu, sub_highlighted, max_height) = {
        let open = cx.try_global::<Layer>()?.0.as_ref()?;
        let (top, max_height) = placement(open, open.menu.height(), window);
        let submenu = open.submenu.zip(open.submenu().cloned());
        (
            open.menu.clone(),
            point(open.position.x, top),
            open.right_aligned,
            open.focus.clone(),
            open.highlighted,
            submenu,
            open.sub_highlighted,
            max_height,
        )
    };
    if !focus.is_focused(window) {
        window.focus(&focus, cx);
    }
    let top = card(&menu, Level::Top, highlighted, submenu.as_ref().map(|(index, _)| *index), cx).max_h(max_height);
    let placed = anchored()
        .position(position)
        .anchor(if right_aligned { gpui_kit::Anchor::TopRight } else { gpui_kit::Anchor::TopLeft })
        .snap_to_window_with_margin(px(WINDOW_MARGIN))
        .child(top);
    let beside = submenu.and_then(|(index, sub)| {
        let Some(Entry::Submenu { anchor, .. }) = menu.entries.get(index) else { return None };
        let row = anchor.bounds();
        let card = card(&sub, Level::Sub, sub_highlighted, None, cx);
        Some(
            anchored()
                .position(point(row.right() + px(PADDING + 2.), row.origin.y - px(PADDING + 1.)))
                .snap_to_window_with_margin(px(WINDOW_MARGIN))
                .child(card),
        )
    });
    let overlay = div()
        .id("menu-layer")
        .absolute()
        .inset_0()
        .occlude()
        .track_focus(&focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            match event.keystroke.key.as_str() {
                "escape" => close(window, cx),
                "up" => step(false, cx),
                "down" => step(true, cx),
                "right" => step_level(true, cx),
                "left" => step_level(false, cx),
                "enter" => {
                    let picked =
                        cx.try_global::<Layer>().and_then(|layer| layer.0.as_ref()).and_then(|open| {
                            match open.sub_highlighted.filter(|_| open.submenu.is_some()) {
                                Some(index) => Some((Level::Sub, index)),
                                None => open.highlighted.map(|index| (Level::Top, index)),
                            }
                        });
                    if let Some((level, index)) = picked {
                        pick(level, index, window, cx);
                    }
                }
                _ => {
                    let modifiers = event.keystroke.modifiers;
                    let typed = event.keystroke.key_char.clone().filter(|_| !modifiers.platform && !modifiers.control);
                    let Some(typed) = typed.filter(|typed| !typed.trim().is_empty()) else { return };
                    type_select(&typed, cx);
                }
            }
            window.refresh();
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
        .child(placed)
        .children(beside);
    Some(deferred(overlay).with_priority(PRIORITY).into_any_element())
}

/// Lights the row, or lets go of it, as the pointer goes over it or leaves it. Going over a row
/// of the menu opens the one it opens, or closes the one open beside it.
fn hover(level: Level, index: usize, inside: bool, submenu: bool, window: &mut Window, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    let lit = match level {
        Level::Top => &mut open.highlighted,
        Level::Sub => &mut open.sub_highlighted,
    };
    if inside {
        *lit = Some(index);
        if level == Level::Top {
            open.submenu = submenu.then_some(index);
            open.sub_highlighted = None;
        }
    } else if *lit == Some(index) && !submenu {
        *lit = None;
    }
    window.refresh();
}

/// The card of a menu's rows.
fn card(menu: &Menu, level: Level, highlighted: Option<usize>, open_submenu: Option<usize>, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let any_checked = menu.entries.iter().any(|entry| matches!(entry, Entry::Item { checked: true, .. }));
    let check_column = |checked: bool| {
        div()
            .size(px(14.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .when(checked, |mark| mark.child(icons::symbol("check", 12.)))
    };
    let row = |index: usize, lit: bool| {
        div()
            .id(("menu-item", index))
            .relative()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .rounded(px(Radius::SM))
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .when(lit, |row| row.bg(Surface::Popover.layer(theme::Layer::Row, c)))
    };
    let rows = menu.entries.iter().enumerate().map(|(index, entry)| match entry {
        Entry::Separator => div()
            .flex_shrink_0()
            .h(px(1.))
            .my(px(4.))
            .mx(px(-PADDING))
            .bg(Surface::Popover.border(c))
            .into_any_element(),
        Entry::Note(label) => div()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .text_color(c.muted_stronger_foreground)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(label.clone())
            .into_any_element(),
        Entry::Submenu { label, anchor, .. } => row(index, highlighted == Some(index) || open_submenu == Some(index))
            .text_color(c.foreground)
            .child(anchor.track())
            .when(any_checked, |row| row.child(check_column(false)))
            .child(div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(label.clone()))
            .child(icons::symbol("chevron-right", 11.).text_color(c.muted_foreground))
            .on_hover(move |inside, window, cx| hover(level, index, *inside, true, window, cx))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                pick(level, index, window, cx)
            })
            .into_any_element(),
        Entry::Item { label, enabled, checked, danger, icon, tooltip, .. } => {
            row(index, highlighted == Some(index) && *enabled)
                .text_color(if *danger { c.destructive } else { c.foreground })
                .when(!*enabled, |row| row.opacity(c.opacity(Opacity::Disabled)))
                .when(any_checked, |row| row.child(check_column(*checked)))
                .when_some(icon.as_ref(), |row, icon| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .text_color(if *danger { c.destructive } else { c.muted_foreground })
                            .child(match icon {
                                MenuIcon::Symbol(name) => icons::symbol(*name, 13.).into_any_element(),
                                MenuIcon::File(path) => img(std::path::PathBuf::from(path.as_ref()))
                                    .size(px(16.))
                                    .rounded(px(3.))
                                    .into_any_element(),
                                MenuIcon::Logo(agent) => {
                                    crate::ui::logos::agent_icon(*agent, 16., cx).into_any_element()
                                }
                            }),
                    )
                })
                .child(
                    div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(label.clone()),
                )
                .when_some(tooltip.clone(), |row, tooltip| row.tooltip(crate::ui::tooltip(tooltip)))
                .when(*enabled, |row| {
                    row.on_hover(move |inside, window, cx| hover(level, index, *inside, false, window, cx)).on_click(
                        move |_, window, cx| {
                            cx.stop_propagation();
                            pick(level, index, window, cx)
                        },
                    )
                })
                .into_any_element()
        }
    });
    crate::ui::dropdown_card(div(), cx)
        .id(if level == Level::Top { "menu-card" } else { "submenu-card" })
        .min_w(px(MENU_WIDTH.0))
        .max_w(px(MENU_WIDTH.1))
        .overflow_y_scroll()
        .p(px(PADDING))
        .flex()
        .flex_col()
        // A press inside the menu is its rows', not one beside it that closes it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(rows)
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
}
