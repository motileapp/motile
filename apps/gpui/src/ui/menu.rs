//! Menus, drawn by the app: a popover card of rows under the control that opened it, for context
//! menus and the buttons that open a menu. Each item runs a closure once the menu has closed.
//! One menu is open at a time; the window's root draws it over everything else.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, IconSize, Opacity, Radius, Surface, colors};
use crate::ui::icons;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// The image in front of an item: a symbol, or an image file on this device.
pub enum MenuIcon {
    Symbol(&'static str),
    File(SharedString),
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

/// What opened a menu under its button: a press, which may go on to be let go on a row, or the
/// keys, which light its first row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Press,
    Keys,
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
        self.open(position, position.y, false, None, window, cx);
    }

    /// Opens the menu `gap` under the element the anchor tracks, its left edge on the element's,
    /// or as far above it when there is no room under it.
    pub fn show_under(self, anchor: &Anchor, gap: f32, opened: Opened, window: &mut Window, cx: &mut App) {
        let bounds = anchor.bounds();
        let position = point(bounds.origin.x, bounds.bottom() + px(gap));
        self.open(position, bounds.origin.y - px(gap), false, Some((bounds, opened)), window, cx);
    }

    /// Opens the menu `gap` under the element the anchor tracks, its right edge on the element's,
    /// or as far above it when there is no room under it.
    pub fn show_under_right(self, anchor: &Anchor, gap: f32, opened: Opened, window: &mut Window, cx: &mut App) {
        let bounds = anchor.bounds();
        let position = point(bounds.right(), bounds.bottom() + px(gap));
        self.open(position, bounds.origin.y - px(gap), true, Some((bounds, opened)), window, cx);
    }

    fn open(
        self,
        position: Point<Pixels>,
        above: Pixels,
        right_aligned: bool,
        button: Option<(Bounds<Pixels>, Opened)>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let returns = window.focused(cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        crate::ui::overlay_opened(cx);
        let opened = button.map(|(_, opened)| opened);
        let highlighted = if opened == Some(Opened::Keys) { self.runnable().first().copied() } else { None };
        let open = Open {
            menu: Rc::new(self),
            position,
            above,
            right_aligned,
            focus,
            returns,
            button: button.map(|(bounds, _)| bounds),
            held: (opened == Some(Opened::Press)).then(Instant::now),
            highlighted,
            submenu: None,
            sub_highlighted: None,
            sub_card: Anchor::default(),
            leaving: None,
            crossing: None,
            moved_at: Instant::now(),
            typed: String::new(),
            typed_at: None,
            typed_from: None,
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

    /// The first runnable row after `after`, going round, whose label starts with `typed`.
    fn starting_with(&self, typed: &str, after: Option<usize>) -> Option<usize> {
        let runnable = self.runnable();
        let start = after.and_then(|after| runnable.iter().position(|&index| index == after)).map_or(0, |at| at + 1);
        let (before, from) = runnable.split_at(start.min(runnable.len()));
        from.iter().chain(before).copied().find(|&index| match &self.entries[index] {
            Entry::Item { label, .. } | Entry::Submenu { label, .. } => label.to_lowercase().starts_with(typed),
            _ => false,
        })
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
    /// What had the keys before the menu opened, which gets them back.
    returns: Option<FocusHandle>,
    /// The button it opened from, where letting go of the press that opened it leaves it open.
    button: Option<Bounds<Pixels>>,
    /// When the press on its button that opened it went down, while it is held.
    held: Option<Instant>,
    highlighted: Option<usize>,
    /// The row whose menu is open beside this one, and the row of that menu that is lit.
    submenu: Option<usize>,
    sub_highlighted: Option<usize>,
    /// Where the open submenu is drawn.
    sub_card: Anchor,
    /// Where the pointer left the row of the open submenu, while it heads for the submenu.
    leaving: Option<Point<Pixels>>,
    /// The row the pointer crosses on its way to the submenu, which takes over once the pointer
    /// turns away or rests.
    crossing: Option<usize>,
    moved_at: Instant,
    /// What was typed to find a row, when last, and the row lit when the typing began.
    typed: String,
    typed_at: Option<Instant>,
    typed_from: Option<usize>,
}

impl Open {
    fn submenu(&self) -> Option<&Rc<Menu>> {
        match self.menu.entries.get(self.submenu?) {
            Some(Entry::Submenu { menu, .. }) => Some(menu),
            _ => None,
        }
    }

    fn opens_submenu(&self, index: usize) -> bool {
        matches!(self.menu.entries.get(index), Some(Entry::Submenu { .. }))
    }

    /// Whether the pointer, on another row, is still on its way from the open submenu's row to
    /// the submenu: inside the triangle between where it left the row and the submenu's near edge.
    fn heads_for_submenu(&self, index: usize, pointer: Point<Pixels>) -> bool {
        let (Some(open), Some(apex)) = (self.submenu, self.leaving) else { return false };
        let card = self.sub_card.bounds();
        let edge = if card.left() >= apex.x { card.left() } else { card.right() };
        open != index && inside_triangle(pointer, apex, point(edge, card.top()), point(edge, card.bottom()))
    }

    /// Lights a row of the top menu, closing a submenu of another row.
    fn light(&mut self, index: usize) {
        self.highlighted = Some(index);
        self.leaving = None;
        self.crossing = None;
        if self.submenu != Some(index) {
            self.submenu = None;
            self.sub_highlighted = None;
        }
    }
}

fn inside_triangle(p: Point<Pixels>, a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>) -> bool {
    let side = |from: Point<Pixels>, to: Point<Pixels>| {
        f32::from(to.x - from.x) * f32::from(p.y - from.y) - f32::from(to.y - from.y) * f32::from(p.x - from.x)
    };
    let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
    (ab >= 0. && bc >= 0. && ca >= 0.) || (ab <= 0. && bc <= 0. && ca <= 0.)
}

#[derive(Default)]
struct Layer(Option<Open>);

impl Global for Layer {}

/// Whether a menu is open over the window.
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<Layer>().is_some_and(|layer| layer.0.is_some())
}

/// Closes the menu that is open, if one is, and gives the keys back to what had them.
pub fn close(window: &mut Window, cx: &mut App) {
    if !is_open(cx) {
        return;
    }
    let open = cx.global_mut::<Layer>().0.take();
    give_back(open.as_ref(), window, cx);
    window.refresh();
}

fn give_back(open: Option<&Open>, window: &mut Window, cx: &mut App) {
    let Some(returns) = open.and_then(|open| open.returns.as_ref()) else { return };
    window.focus(returns, cx);
}

fn open_mut(cx: &mut App) -> Option<&mut Open> {
    if !cx.has_global::<Layer>() {
        return None;
    }
    cx.global_mut::<Layer>().0.as_mut()
}

/// Esc closes the submenu that is open, or else the menu.
fn back(window: &mut Window, cx: &mut App) {
    let Some(open) = open_mut(cx).filter(|open| open.submenu.is_some()) else { return close(window, cx) };
    open.submenu = None;
    open.sub_highlighted = None;
    open.leaving = None;
    open.crossing = None;
}

/// Whether the press that opened the menu was held long enough for letting go on a row to pick
/// it, so that a click on the button only opens the menu.
fn held_to_pick(cx: &App) -> bool {
    let held = cx.try_global::<Layer>().and_then(|layer| layer.0.as_ref()).and_then(|open| open.held);
    held.is_some_and(|at| at.elapsed() >= HOLD_TO_PICK)
}

/// Letting go of the press that opened the menu away from it and from its button closes it.
fn let_go(position: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let Some(open) = open_mut(cx) else { return };
    let held = open.held.take().is_some();
    if !held || open.button.is_some_and(|button| button.contains(&position)) {
        return;
    }
    close(window, cx);
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
        Some(Entry::Item { run, enabled: true, .. }) => {
            give_back(Some(&open), window, cx);
            run(window, cx)
        }
        Some(Entry::Submenu { menu, .. }) => {
            let first = menu.runnable().first().copied();
            let mut open = open;
            open.submenu = Some(index);
            open.sub_highlighted = first;
            cx.set_global(Layer(Some(open)));
        }
        _ => give_back(Some(&open), window, cx),
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

/// How long typed letters keep adding up to one search.
const TYPING_PAUSE: Duration = Duration::from_millis(500);
/// How long the press that opened a menu is held before letting go on a row picks it.
const HOLD_TO_PICK: Duration = Duration::from_millis(200);
/// How long the pointer stays on a row before its submenu opens, and rests on its way to an open
/// submenu before the row under it takes over.
const SUBMENU_DELAY: Duration = Duration::from_millis(100);

/// Lights the row of the menu in front that starts with what was typed, from the one after the
/// row lit when the typing began. The same letter typed again goes on to the next row with it.
fn type_select(typed: &str, cx: &mut App) {
    let Some(open) = cx.global_mut::<Layer>().0.as_mut() else { return };
    let in_submenu = open.submenu.is_some() && open.sub_highlighted.is_some();
    let lit = if in_submenu { open.sub_highlighted } else { open.highlighted };
    let fresh = open.typed_at.is_none_or(|at| at.elapsed() > TYPING_PAUSE);
    if fresh {
        open.typed.clear();
        open.typed_from = lit;
    }
    open.typed.push_str(&typed.to_lowercase());
    open.typed_at = Some(Instant::now());
    let menu = if in_submenu { open.submenu().cloned() } else { Some(open.menu.clone()) };
    let Some(menu) = menu else { return };
    let mut letters = open.typed.chars();
    let first = letters.next();
    let repeated = first.filter(|first| letters.all(|letter| letter == *first)).map(String::from);
    let found = menu
        .starting_with(&open.typed, open.typed_from)
        .or_else(|| repeated.and_then(|letter| menu.starting_with(&letter, lit)));
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
    let (menu, position, right_aligned, focus, highlighted, submenu, sub_highlighted, sub_card, max_height) = {
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
            open.sub_card.clone(),
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
        let card = card(&sub, Level::Sub, sub_highlighted, None, cx).child(sub_card.track());
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
                "escape" => back(window, cx),
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
        .on_mouse_up(MouseButton::Left, |event, window, cx| let_go(event.position, window, cx))
        .on_mouse_move(|_, window, cx| moved(window, cx))
        .child(placed)
        .children(beside);
    Some(deferred(overlay).with_priority(PRIORITY).into_any_element())
}

/// Lights the row, or lets go of it, as the pointer goes over it or leaves it. A row that opens a
/// submenu opens it once the pointer has stayed on it a moment, and the submenu stays open while
/// the pointer heads for it across other rows.
fn hover(level: Level, index: usize, inside: bool, window: &mut Window, cx: &mut App) {
    let pointer = window.mouse_position();
    let Some(open) = open_mut(cx) else { return };
    let mut opens_soon = false;
    let mut crossing = false;
    match (level, inside) {
        (Level::Sub, true) => {
            open.sub_highlighted = Some(index);
            open.leaving = None;
            open.crossing = None;
        }
        (Level::Sub, false) if open.sub_highlighted == Some(index) => open.sub_highlighted = None,
        (Level::Sub, false) => {}
        (Level::Top, true) if open.heads_for_submenu(index, pointer) => {
            crossing = open.crossing.is_none();
            open.crossing = Some(index);
            open.moved_at = Instant::now();
        }
        (Level::Top, true) => {
            open.light(index);
            opens_soon = open.opens_submenu(index) && open.submenu != Some(index);
        }
        (Level::Top, false) => {
            if open.crossing == Some(index) {
                open.crossing = None;
            }
            if open.submenu == Some(index) {
                open.leaving = Some(pointer);
            } else if open.highlighted == Some(index) {
                open.highlighted = None;
            }
        }
    }
    window.refresh();
    if opens_soon {
        open_submenu_soon(index, window, cx);
    }
    if crossing {
        watch_crossing(window, cx);
    }
}

fn open_submenu_soon(index: usize, window: &mut Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(SUBMENU_DELAY).await;
            let _ = cx.update(|window, cx| {
                let Some(open) = open_mut(cx) else { return };
                if open.highlighted != Some(index) || open.submenu == Some(index) || !open.opens_submenu(index) {
                    return;
                }
                open.submenu = Some(index);
                open.sub_highlighted = None;
                window.refresh();
            });
        })
        .detach();
}

/// The pointer moved over the menus: once it turns away from the submenu it was heading for, the
/// row it is on takes over.
fn moved(window: &mut Window, cx: &mut App) {
    let pointer = window.mouse_position();
    let Some(open) = open_mut(cx) else { return };
    let Some(row) = open.crossing else { return };
    if open.heads_for_submenu(row, pointer) {
        open.moved_at = Instant::now();
        return;
    }
    take_over(row, window, cx);
}

/// Once the pointer rests on its way to the submenu, the row it is on takes over.
fn watch_crossing(window: &mut Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            loop {
                cx.background_executor().timer(SUBMENU_DELAY).await;
                let watching = cx
                    .update(|window, cx| {
                        let Some(open) = open_mut(cx) else { return false };
                        let Some(row) = open.crossing else { return false };
                        if open.moved_at.elapsed() < SUBMENU_DELAY {
                            return true;
                        }
                        take_over(row, window, cx);
                        false
                    })
                    .unwrap_or(false);
                if !watching {
                    break;
                }
            }
        })
        .detach();
}

fn take_over(row: usize, window: &mut Window, cx: &mut App) {
    let Some(open) = open_mut(cx) else { return };
    open.light(row);
    let opens_soon = open.opens_submenu(row);
    window.refresh();
    if opens_soon {
        open_submenu_soon(row, window, cx);
    }
}

/// Letting go on a row of the press that opened the menu picks it.
fn released(level: Level, index: usize, window: &mut Window, cx: &mut App) {
    if !held_to_pick(cx) {
        return;
    }
    cx.stop_propagation();
    pick(level, index, window, cx);
}

/// Letting go on the menu but off its rows leaves it open, and a press on it starts anew.
fn let_go_inside(cx: &mut App) {
    let Some(open) = open_mut(cx) else { return };
    open.held = None;
}

/// The card of a menu's rows.
fn card(menu: &Menu, level: Level, highlighted: Option<usize>, open_submenu: Option<usize>, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let any_checked = menu.entries.iter().any(|entry| matches!(entry, Entry::Item { checked: true, .. }));
    let check_column = |checked: bool| {
        div()
            .size(px(IconSize::Sm.side()))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .when(checked, |mark| mark.child(icons::symbol("check", IconSize::Sm)))
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
            .child(icons::symbol("chevron-right", IconSize::Sm).text_color(c.muted_foreground))
            .on_hover(move |inside, window, cx| hover(level, index, *inside, window, cx))
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
                                MenuIcon::Symbol(name) => icons::symbol(*name, IconSize::Sm).into_any_element(),
                                MenuIcon::File(path) => crate::ui::logos::icon_file(path.to_string(), IconSize::Sm, cx),
                            }),
                    )
                })
                .child(
                    div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(label.clone()),
                )
                .when_some(tooltip.clone(), |row, tooltip| row.tooltip(crate::ui::tooltip(tooltip)))
                .when(*enabled, |row| {
                    row.on_hover(move |inside, window, cx| hover(level, index, *inside, window, cx))
                        .on_mouse_up(MouseButton::Left, move |_, window, cx| released(level, index, window, cx))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            pick(level, index, window, cx)
                        })
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
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
            let_go_inside(cx);
        })
        .on_mouse_up(MouseButton::Left, |_, _, cx| let_go_inside(cx))
        .on_mouse_move(|_, window, cx| moved(window, cx))
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
