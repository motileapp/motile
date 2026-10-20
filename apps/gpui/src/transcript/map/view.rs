//! The live map: a resolved `Map` node drawn over its tiles, with the camera of
//! open-intelligent-ui's `map.tsx` (core's maths), its gestures and its glass controls, inline
//! or expanded over the window.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::map::{self as camera, Camera, DRAW_MS, FLY_MS, Framing};
use motile_core::render::openui::tree::{MapPin, RouteKind, UiKind};

use super::draw::{Scene, paint_items};
use super::paint::{self, Pin, View};
use super::style::style;
use super::tiles::{self, Slot};
use super::{height, pool};
use crate::theme::{ControlSize, Opacity, Radius, ShadowSize, Surface, colors};
use crate::transcript::kit::paint_corners;
use crate::ui::control::{ActionButton, ActionMenu, Variant};
use crate::ui::menu::Menu;
use crate::ui::motion;

/// The framed key is set this long after the fly starts, as `map.tsx`'s timer does.
const FRAMED_MS: u64 = FLY_MS as u64 + 40;
const ZOOM_MS: u64 = 250;
/// A press that moves less than this is a click.
const CLICK_SLOP: f32 = 4.;
const ERROR: &str = "Map tiles are temporarily unavailable. The places are still shown.";
const ATTRIBUTION: [(&str, Option<&str>); 5] = [
    ("OpenFreeMap", Some("https://openfreemap.org")),
    (" © ", None),
    ("OpenMapTiles", Some("https://www.openmaptiles.org")),
    (" · Data from ", None),
    ("OpenStreetMap", Some("https://www.openstreetmap.org/copyright")),
];

/// What the map draws: core's `UiKind::Map`.
#[derive(Clone, PartialEq, Debug)]
pub struct MapNode {
    pub pins: Vec<MapPin>,
    pub route: RouteKind,
    pub line: Vec<[f64; 2]>,
    pub selected: Option<String>,
    pub height: f32,
    pub categories: Vec<String>,
    pub filter: Option<String>,
    pub note: String,
}

impl MapNode {
    pub fn from_kind(kind: &UiKind) -> Option<MapNode> {
        let UiKind::Map { pins, route, line, selected, height, categories, filter, note } = kind else { return None };
        Some(MapNode {
            pins: pins.clone(),
            route: *route,
            line: line.clone(),
            selected: selected.clone(),
            height: *height,
            categories: categories.clone(),
            filter: filter.clone(),
            note: note.clone(),
        })
    }

    fn visible(&self) -> Vec<Pin> {
        self.pins
            .iter()
            .filter(|pin| pin.visible)
            .map(|pin| Pin {
                key: pin.key.clone(),
                label: pin.label.clone(),
                lat: pin.lat,
                lng: pin.lng,
                icon: pin.icon.clone(),
                number: pin.number,
                selected: pin.selected,
            })
            .collect()
    }

    /// The visible pins' `lng,lat` joined by `;`, which says when they changed.
    fn routing_key(&self) -> String {
        let pins = self.pins.iter().filter(|pin| pin.visible);
        pins.map(|pin| format!("{},{}", pin.lng, pin.lat)).collect::<Vec<_>>().join(";")
    }
}

pub type OnSelect = Rc<dyn Fn(String, &mut Window, &mut App)>;
pub type OnFilter = Rc<dyn Fn(Option<String>, &mut Window, &mut App)>;
pub type OnExpand = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// A camera on its way from one place to another, flown or zoomed.
#[derive(Clone, Copy)]
struct Flight {
    from: Camera,
    to: Camera,
    since: Instant,
    /// A fly, else a plain zoom eased out.
    fly: bool,
}

struct Drag {
    last: Point<Pixels>,
    moved: f32,
}

/// What a scene was made from: when it changes, a new one is made.
#[derive(Clone, PartialEq)]
struct SceneKey {
    view: View,
    dark: bool,
    tiles: Vec<(Slot, Slot)>,
    pins: Vec<Pin>,
}

/// OpenUI's Map. Make it from the node with `new`, give it each new node with `set`, and size
/// its row with `height`. `on_select` is called with a pin's label when it is pressed,
/// `on_filter` with the category chosen (none for every place), `on_expand` when it opens over
/// the window or closes; core answers them with a new node.
pub struct MapView {
    id: u64,
    node: MapNode,
    streaming: bool,
    /// The colour around the map, which its corners are painted in.
    under: Hsla,
    on_select: OnSelect,
    on_filter: OnFilter,
    on_expand: OnExpand,
    camera: Option<Camera>,
    /// The map's size when it was last laid out.
    size: Option<(f64, f64)>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    anchored: bool,
    user_moved: bool,
    fit_signature: Option<String>,
    framed_key: Option<String>,
    framing: Option<(String, Instant)>,
    line_drawn: bool,
    /// The line last given to the route: none until framed.
    line_set: Option<Vec<[f64; 2]>>,
    shown_line: Vec<[f64; 2]>,
    reveal: Option<(Vec<[f64; 2]>, Instant)>,
    flight: Option<Flight>,
    drag: Option<Drag>,
    expanded: bool,
    info_open: bool,
    hovered: bool,
    live: bool,
    scene: Option<Arc<Scene>>,
    scene_key: Option<SceneKey>,
    making: Option<(SceneKey, Task<()>)>,
    focus: FocusHandle,
    /// While expanded, Esc collapses the map and goes no further.
    _escape: Option<Subscription>,
    _tiles: Task<()>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl MapView {
    pub fn new(
        node: MapNode,
        streaming: bool,
        under: Hsla,
        on_select: OnSelect,
        on_filter: OnFilter,
        on_expand: OnExpand,
        cx: &mut Context<Self>,
    ) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut arrivals = tiles::store().listen();
        let tiles = cx.spawn(async move |this, cx| {
            use futures::StreamExt;
            while arrivals.next().await.is_some() {
                let Ok(live) = this.update(cx, |map, _| map.live) else { return };
                if live {
                    let _ = this.update(cx, |_, cx| cx.notify());
                }
            }
        });
        cx.on_release(move |_, cx| {
            pool::forget(id, cx);
            tiles::store().release(id);
        })
        .detach();
        Self {
            id,
            node,
            streaming,
            under,
            on_select,
            on_filter,
            on_expand,
            camera: None,
            size: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            anchored: false,
            user_moved: false,
            fit_signature: None,
            framed_key: None,
            framing: None,
            line_drawn: false,
            line_set: None,
            shown_line: Vec::new(),
            reveal: None,
            flight: None,
            drag: None,
            expanded: false,
            info_open: false,
            hovered: false,
            live: false,
            scene: None,
            scene_key: None,
            making: None,
            focus: cx.focus_handle(),
            _escape: None,
            _tiles: tiles,
        }
    }

    /// The node core sent last, whether its program still streams, and the colour around it.
    pub fn set(&mut self, node: MapNode, streaming: bool, under: Hsla, cx: &mut Context<Self>) {
        if self.node == node && self.streaming == streaming && self.under == under {
            return;
        }
        self.node = node;
        self.streaming = streaming;
        self.under = under;
        cx.notify();
    }

    /// Gives the live state up: its tiles go, its paths go and its animations end where they
    /// were going. Drawn again, it goes live again.
    pub fn give_up(&mut self) {
        self.live = false;
        tiles::store().release(self.id);
        self.scene = None;
        self.scene_key = None;
        self.making = None;
        if let Some(flight) = self.flight.take() {
            self.camera = Some(flight.to);
        }
        if let Some((key, _)) = self.framing.take() {
            self.framed_key = Some(key);
        }
        if let Some((line, _)) = self.reveal.take() {
            self.shown_line = line;
            self.line_drawn = true;
        }
    }

    fn view(&self, narrow: bool) -> Option<View> {
        let (width, height) = self.size?;
        Some(View { camera: self.camera?, width, height, narrow })
    }

    /// Follows `map.tsx`'s effects: the first pin's jump, the fit once the program ends, and
    /// the route's draw once the fit is done. Animations move on to now.
    fn sync(&mut self, window: &mut Window) {
        let Some((width, height)) = self.size else { return };
        let reduced = crate::media::native::reduces_motion();
        let pins = self.node.visible();
        let points: Vec<[f64; 2]> = pins.iter().map(|pin| [pin.lat, pin.lng]).collect();
        let routing_key = self.node.routing_key();
        if self.camera.is_none() {
            self.camera = Some(Camera { lat: 20., lng: 0., zoom: 1. });
        }
        if !points.is_empty() && !self.anchored {
            self.camera = camera::frame(&points, width, height, Framing::First).or(self.camera);
            self.anchored = true;
        }
        if !self.streaming && !points.is_empty() {
            let signature = format!("{:?}|{routing_key}", self.node.filter);
            if self.fit_signature.as_ref() != Some(&signature) {
                self.fit_signature = Some(signature);
                self.shown_line.clear();
                self.reveal = None;
                self.line_set = None;
                let fit = camera::frame(&points, width, height, Framing::Fit);
                if self.user_moved || reduced {
                    if !self.user_moved {
                        self.camera = fit.or(self.camera);
                    }
                    self.framed_key = Some(routing_key.clone());
                    self.framing = None;
                } else if let (Some(from), Some(to)) = (self.camera, fit) {
                    self.flight = Some(Flight { from, to, since: Instant::now(), fly: true });
                    self.framing = Some((routing_key.clone(), Instant::now()));
                }
            }
        }
        if let Some((key, since)) = &self.framing
            && since.elapsed() >= Duration::from_millis(FRAMED_MS)
        {
            self.framed_key = Some(key.clone());
            self.framing = None;
        }
        if !self.streaming && self.framed_key.as_ref() == Some(&routing_key) {
            let positions = if self.node.route == RouteKind::Off { Vec::new() } else { self.node.line.clone() };
            if self.line_set.as_ref() != Some(&positions) {
                self.line_set = Some(positions.clone());
                if positions.len() < 2 || self.line_drawn || reduced {
                    self.line_drawn = positions.len() >= 2;
                    self.shown_line = if self.line_drawn { positions } else { Vec::new() };
                    self.reveal = None;
                } else {
                    self.reveal = Some((positions, Instant::now()));
                }
            }
        }
        if let Some(flight) = self.flight {
            let elapsed = flight.since.elapsed().as_millis() as f64;
            let (t, now) = match flight.fly {
                true => {
                    let t = elapsed / f64::from(FLY_MS);
                    (t, camera::fly(flight.from, flight.to, width, height, t))
                }
                false => {
                    let t = (elapsed / ZOOM_MS as f64).min(1.);
                    let eased = f64::from(crate::ui::ease_out(t as f32));
                    let zoom = flight.from.zoom + (flight.to.zoom - flight.from.zoom) * eased;
                    (t, Camera { zoom, ..flight.from })
                }
            };
            self.camera = Some(now);
            if t >= 1. {
                self.camera = Some(flight.to);
                self.flight = None;
            }
        }
        if let Some((line, since)) = &self.reveal {
            let t = since.elapsed().as_millis() as f64 / f64::from(DRAW_MS);
            self.shown_line = camera::reveal(line, t);
            if t >= 1. {
                self.shown_line = line.clone();
                self.line_drawn = true;
                self.reveal = None;
            }
        }
        if self.flight.is_some() || self.framing.is_some() || self.reveal.is_some() {
            window.request_animation_frame();
        }
    }

    /// Asks for the tiles on screen and has the scene made again off the main thread when what
    /// it shows changed.
    fn refresh_scene(&mut self, view: View, dark: bool, cx: &mut Context<Self>) {
        let slots = tiles::cover(view.camera, view.width, view.height);
        let store = tiles::store();
        store.want(self.id, slots.iter().map(|slot| slot.id()).collect());
        let picks = tiles::stand_ins(&slots, |id| store.has(id));
        let key = SceneKey { view, dark, tiles: picks.clone(), pins: self.node.visible() };
        if self.scene_key.as_ref() == Some(&key) || self.making.as_ref().is_some_and(|(making, _)| *making == key) {
            return;
        }
        if self.making.is_some() {
            return;
        }
        let placed: Vec<paint::Placed> = picks
            .iter()
            .filter_map(|(slot, source)| {
                Some(paint::Placed { slot: *slot, source: *source, tile: store.tile(source.id())? })
            })
            .collect();
        let pins = key.pins.clone();
        let made =
            cx.background_spawn(async move { Scene::new(paint::base(style(dark), &placed, &view, &pins), view) });
        let making_key = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let scene = made.await;
            let _ = this.update(cx, |map, cx| {
                map.scene = Some(Arc::new(scene));
                map.scene_key = Some(making_key);
                map.making = None;
                cx.notify();
            });
        });
        self.making = Some((key, task));
    }

    fn visible_points(&self) -> Vec<[f64; 2]> {
        self.node.pins.iter().filter(|pin| pin.visible).map(|pin| [pin.lat, pin.lng]).collect()
    }

    fn reduced() -> bool {
        crate::media::native::reduces_motion()
    }

    fn recenter(&mut self, cx: &mut Context<Self>) {
        let Some((width, height)) = self.size else { return };
        let Some(to) = camera::frame(&self.visible_points(), width, height, Framing::Recenter) else { return };
        match (self.camera, Self::reduced()) {
            (Some(from), false) => self.flight = Some(Flight { from, to, since: Instant::now(), fly: true }),
            _ => self.camera = Some(to),
        }
        cx.notify();
    }

    fn zoom_by(&mut self, by: f64, about: Option<[f64; 2]>, animated: bool, cx: &mut Context<Self>) {
        let (Some(now), Some((width, height))) = (self.camera, self.size) else { return };
        let about = about.unwrap_or([width / 2., height / 2.]);
        let to = camera::zoom_about(now, width, height, by, about);
        self.user_moved = true;
        self.flight = None;
        match animated && !Self::reduced() && about == [width / 2., height / 2.] {
            true => self.flight = Some(Flight { from: now, to, since: Instant::now(), fly: false }),
            false => self.camera = Some(to),
        }
        cx.notify();
    }

    fn local(&self, position: Point<Pixels>) -> [f64; 2] {
        let origin = self.bounds.get().origin;
        [f64::from(f32::from(position.x - origin.x)), f64::from(f32::from(position.y - origin.y))]
    }

    fn pressed(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.drag = Some(Drag { last: event.position, moved: 0. });
        cx.stop_propagation();
    }

    fn dragged(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let position = event.position;
        let Some(drag) = &mut self.drag else { return };
        let delta = position - drag.last;
        drag.last = position;
        drag.moved += f32::from(delta.x).abs() + f32::from(delta.y).abs();
        if drag.moved < CLICK_SLOP {
            return;
        }
        let Some(now) = self.camera else { return };
        self.camera = Some(camera::pan(now, f64::from(f32::from(delta.x)), f64::from(f32::from(delta.y))));
        self.user_moved = true;
        self.flight = None;
        cx.notify();
    }

    fn released(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else { return };
        if drag.moved >= CLICK_SLOP {
            return;
        }
        let Some(view) = self.view(narrow(window)) else { return };
        let [x, y] = self.local(position);
        let pins = self.node.visible();
        let Some(index) = paint::hit(style(colors(cx).dark), &view, &pins, [x as f32, y as f32]) else { return };
        let label = pins[index].label.clone();
        self.set_expanded(false, window, cx);
        (self.on_select)(label, window, cx);
    }

    fn scrolled(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if !event.modifiers.control {
            return;
        }
        let delta = event.delta.pixel_delta(px(20.));
        let by = (-f64::from(f32::from(delta.y)) / 200.).clamp(-1., 1.);
        self.zoom_by(by, Some(self.local(event.position)), false, cx);
        cx.stop_propagation();
    }

    fn pinched(&mut self, event: &PinchEvent, cx: &mut Context<Self>) {
        let by = f64::from(1. + event.delta).max(0.01).log2();
        self.zoom_by(by, Some(self.local(event.position)), false, cx);
        cx.stop_propagation();
    }

    fn set_expanded(&mut self, expanded: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.expanded == expanded {
            return;
        }
        self.expanded = expanded;
        self._escape = expanded.then(|| Self::collapse_on_escape(cx));
        if expanded {
            window.focus(&self.focus, cx);
        }
        (self.on_expand)(expanded, window, cx);
        cx.notify();
    }

    /// Takes Esc ahead of the window's own keys, unless a menu or a modal over the map claims it.
    fn collapse_on_escape(cx: &mut Context<Self>) -> Subscription {
        let map = cx.entity().downgrade();
        cx.intercept_keystrokes(move |event, window, cx| {
            let keystroke = &event.keystroke;
            if keystroke.key != "escape" || keystroke.modifiers.modified() {
                return;
            }
            if crate::ui::menu::is_open(cx) || crate::ui::modal::is_open(cx) {
                return;
            }
            cx.stop_propagation();
            let map = map.clone();
            window.defer(cx, move |window, cx| {
                let _ = map.update(cx, |map, cx| map.set_expanded(false, window, cx));
            });
        })
    }

    /// The map itself: the canvas, its pins as buttons for assistive technologies, and its
    /// controls over it.
    fn surface(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = colors(cx);
        let dark = c.dark;
        let narrow = narrow(window);
        let entity = cx.entity().downgrade();
        self.sync(window);
        if let Some(view) = self.view(narrow).filter(|_| self.live) {
            self.refresh_scene(view, dark, cx);
        }
        let view = self.view(narrow);
        let scene = self.scene.clone();
        let water = c.map_water;
        let overlay = view.map(|view| paint::overlay(style(dark), &view, &self.node.visible(), &self.shown_line));
        let bounds = self.bounds.clone();
        let id = self.id;
        let size = self.size;
        let live = self.live;
        let corners = if self.expanded { c.background } else { self.under };
        let map_canvas = canvas(
            move |frame, _, _| bounds.set(frame),
            move |frame, _, window, cx| {
                window.paint_quad(fill(frame, water));
                if let (Some(scene), Some(view)) = (&scene, &view)
                    && scene.has_tiles()
                {
                    window.paint_quad(fill(frame, scene.background));
                    scene.paint(view, frame, window, cx);
                }
                if let Some(items) = &overlay {
                    window.with_content_mask(Some(ContentMask { bounds: frame }), |window| {
                        paint_items(items, frame, window, cx)
                    });
                }
                let laid_out = (f64::from(f32::from(frame.size.width)), f64::from(f32::from(frame.size.height)));
                let changed = size != Some(laid_out);
                let map = entity.clone();
                pool::shown(id, map.clone(), cx);
                if changed || !live {
                    // A change made while the window paints is drawn only once the paint is over.
                    cx.defer(move |cx| {
                        let _ = map.update(cx, |map, cx| {
                            map.size = Some(laid_out);
                            map.live = true;
                            cx.notify();
                        });
                    });
                }
                let moving = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let _ = moving.update(cx, |map, cx| map.dragged(event, cx));
                });
                let lifting = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = lifting.update(cx, |map, cx| map.released(event.position, window, cx));
                });
            },
        )
        .size_full();
        let pin_buttons = self.view(narrow).map(|view| {
            self.node
                .visible()
                .into_iter()
                .map(|pin| {
                    let [x, y] = camera::project(view.camera, view.width, view.height, pin.lat, pin.lng);
                    div()
                        .id(SharedString::from(format!("map-pin-{}", pin.key)))
                        .absolute()
                        .left(px(x as f32 - 17.))
                        .top(px(y as f32 - 17.))
                        .size(px(34.))
                        .role(Role::Button)
                        .aria_label(pin.label.clone())
                        .aria_toggled(if pin.selected { Toggled::True } else { Toggled::False })
                })
                .collect::<Vec<_>>()
        });
        div()
            .id(("map", self.id))
            .relative()
            .size_full()
            .rounded(px(Radius::XL))
            .overflow_hidden()
            .bg(water)
            .cursor(if self.drag.as_ref().is_some_and(|drag| drag.moved >= CLICK_SLOP) {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::Arrow
            })
            .on_hover(cx.listener(|map, hovered: &bool, _, cx| {
                map.hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|map, event: &MouseDownEvent, _, cx| map.pressed(event, cx)))
            .on_scroll_wheel(cx.listener(|map, event: &ScrollWheelEvent, _, cx| map.scrolled(event, cx)))
            .on_pinch(cx.listener(|map, event: &PinchEvent, _, cx| map.pinched(event, cx)))
            .child(map_canvas)
            .children(pin_buttons.into_iter().flatten())
            .child(self.controls(window, cx))
            .child(
                canvas(|_, _, _| {}, move |bounds, _, window, _| paint_corners(bounds, Radius::XL, corners, window))
                    .absolute()
                    .inset_0(),
            )
    }

    fn controls(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let glass = |button: ActionButton| button.variant(Variant::Glass).surface(Surface::Background).round(true);
        let filter = (self.node.categories.len() >= 2).then(|| {
            let categories = self.node.categories.clone();
            let chosen = self.node.filter.clone();
            let on_filter = self.on_filter.clone();
            let title = chosen.clone().unwrap_or_else(|| "All places".to_owned());
            ActionMenu::new(("map-filter", self.id), title, move |_, _| {
                let mut menu = Menu::new();
                let all = on_filter.clone();
                menu = menu.choice(chosen.is_none(), "All places", None, None, move |window, cx| all(None, window, cx));
                for category in &categories {
                    let pick = on_filter.clone();
                    let named = category.clone();
                    menu = menu.choice(
                        chosen.as_ref() == Some(category),
                        category.clone(),
                        None,
                        None,
                        move |window, cx| pick(Some(named.clone()), window, cx),
                    );
                }
                menu
            })
            .button(|button| glass(button).size(ControlSize::Large).help("Filter places"))
        });
        let recenter = glass(ActionButton::icon(("map-recenter", self.id), "locate-fixed", "Show every place"))
            .size(ControlSize::Large)
            .on_click(cx.listener(|map, _, _, cx| map.recenter(cx)));
        let info = glass(ActionButton::icon(("map-info", self.id), "info", "About this map"))
            .size(ControlSize::Large)
            .selected(self.info_open)
            .on_click(cx.listener(|map, _, _, cx| {
                map.info_open = !map.info_open;
                cx.notify();
            }));
        let expand =
            glass(ActionButton::new(("map-expand", self.id), if self.expanded { "Collapse" } else { "Expand" }))
                .symbol(if self.expanded { "minimize-2" } else { "maximize-2" })
                .symbol_after()
                .size(ControlSize::Regular)
                .on_click(cx.listener(|map, _, window, cx| {
                    let expanded = !map.expanded;
                    map.set_expanded(expanded, window, cx);
                }));
        let shown = motion::eased(
            ("map-zoom-shown", self.id),
            if self.hovered { 1. } else { 0. },
            if Self::reduced() { Duration::ZERO } else { Duration::from_millis(150) },
            window,
            cx,
        );
        let zoom = |id: &'static str, symbol: &'static str, help: &'static str, by: f64| {
            glass(ActionButton::icon((id, self.id), symbol, help))
                .size(ControlSize::Regular)
                .on_click(cx.listener(move |map, _, _, cx| map.zoom_by(by, None, true, cx)))
        };
        let popover = |text: SharedString| {
            div()
                .absolute()
                .left(px(12.))
                .max_w(px(275.))
                .p(px(12.))
                .bg(c.popover)
                .border_1()
                .border_color(c.border_popover)
                .rounded(px(Radius::LG))
                .shadow(c.shadow(ShadowSize::Md, Opacity::Shadow))
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(c.muted_foreground)
                .child(text)
        };
        let lit = c.at(c.background, Opacity::Lit);
        div()
            .absolute()
            .inset_0()
            .child(
                pressable(div().absolute().top(px(12.)).left(px(12.)).flex().items_center().gap(px(8.)))
                    .children(filter)
                    .child(recenter)
                    .child(info),
            )
            .child(pressable(div().absolute().top(px(12.)).right(px(12.))).child(expand))
            .when(shown > 0., |controls| {
                controls.child(
                    pressable(div().absolute().bottom(px(12.)).right(px(12.)))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .opacity(shown)
                        .child(zoom("map-zoom-in", "plus", "Zoom in", 1.))
                        .child(zoom("map-zoom-out", "minus", "Zoom out", -1.)),
                )
            })
            .when(self.info_open, |controls| controls.child(popover(self.node.note.clone().into()).top(px(56.))))
            .when(tiles::store().failing(), |controls| controls.child(popover(ERROR.into()).bottom(px(28.))))
            .when(self.node.route == RouteKind::Overview, |controls| {
                controls.child(
                    div()
                        .absolute()
                        .bottom(px(22.))
                        .left(px(12.))
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(Radius::SM))
                        .bg(lit)
                        .text_size(px(10.))
                        .line_height(px(16.))
                        .text_color(c.muted_foreground)
                        .child("Route overview"),
                )
            })
            .child(
                pressable(div().absolute().bottom_0().left_0())
                    .pl(px(8.))
                    .pr(px(4.))
                    .py(px(1.))
                    .flex()
                    .bg(lit)
                    .text_size(px(9.))
                    .line_height(px(12.))
                    .text_color(c.muted_foreground)
                    .children(ATTRIBUTION.iter().enumerate().map(|(index, (text, link))| {
                        match link {
                            Some(link) => div()
                                .id(("map-attribution", index))
                                .cursor_pointer()
                                .hover(|text| text.text_color(c.foreground))
                                .on_click(move |_, _, cx| cx.open_url(link))
                                .child(*text)
                                .into_any_element(),
                            None => div().child(*text).into_any_element(),
                        }
                    })),
            )
    }
}

/// Controls over the map take their presses: none starts a drag of the map.
fn pressable(controls: Div) -> Div {
    controls.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// A window under 520 points wide, where the expanded map is closer to its edges and the pins'
/// labels are smaller.
fn narrow(window: &Window) -> bool {
    window.viewport_size().width < px(520.)
}

impl Render for MapView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_height = height(self.node.height, f32::from(window.viewport_size().width));
        let slot = div().w_full().h(px(row_height));
        if !self.expanded {
            return slot.child(self.surface(window, cx));
        }
        let c = colors(cx);
        let inset = if narrow(window) { 12. } else { 24. };
        let area = crate::window_frame::content_bounds(window);
        let surface = self.surface(window, cx);
        let cover = div()
            .id(("map-expanded", self.id))
            .track_focus(&self.focus)
            .occlude()
            .w(area.size.width)
            .h(area.size.height)
            .p(px(inset))
            .when(cfg!(target_os = "macos"), |cover| cover.pt(px(inset.max(crate::theme::TOP_BAR))))
            .bg(c.at(c.background, Opacity::Lit))
            .child(
                div()
                    .size_full()
                    .rounded(px(Radius::XL))
                    .shadow(c.shadow(ShadowSize::Xl, Opacity::ShadowStronger))
                    .child(surface),
            );
        slot.child(deferred(anchored().position(area.origin).child(cover)).with_priority(5))
    }
}
