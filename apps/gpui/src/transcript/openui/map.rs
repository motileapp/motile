//! Map: the map of `transcript::map` in the program's row. Each is kept by its window, row and
//! node, so that its camera stays when it scrolls away and back; the measuring pass only sizes its
//! box, and the map is made the first time the box is painted.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::UiNode;

use super::UiCx;
use crate::theme::colors;
use crate::transcript::map::height;
use crate::transcript::map::view::{MapNode, MapView, OnExpand, OnFilter, OnSelect};

/// How many maps are kept, the least recently drawn going first.
const KEPT: usize = 24;

#[derive(Default)]
struct Maps {
    views: HashMap<String, (Entity<MapView>, u64)>,
    drawn: u64,
}

impl Global for Maps {}

/// The map's box: the map's height for this window, and the map in it once it was painted.
pub fn map(element: Stateful<Div>, node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let Some(map) = MapNode::from_kind(&node.kind) else { return element };
    let radius = map.radius;
    let element = element.w_full().h(px(height(map.height, f32::from(window.viewport_size().width))));
    let key = format!("{:?}/{}/{}", window.window_handle().window_id(), cx.row, node.id);
    let streaming = cx.streaming;
    let c = colors(app);
    let under = cx.ground.unwrap_or_else(|| cx.surface.color(c));
    if let Some(view) = kept(&key, app) {
        view.update(app, |view, cx| view.set(map, streaming, under, cx));
        return element.child(view);
    }
    let (sink, id) = (cx.sink.clone(), node.id.clone());
    let make = canvas(
        |_, _, _| {},
        move |_, _, window, app| {
            if kept(&key, app).is_some() {
                return;
            }
            let send = move |event: UiEvent, app: &mut App| sink(&id, event, app);
            let select = send.clone();
            let filter = send.clone();
            let on_select: OnSelect = Rc::new(move |label, _, app| select(UiEvent::Select { value: Some(label) }, app));
            let on_filter: OnFilter = Rc::new(move |category, _, app| filter(UiEvent::Filter { category }, app));
            let on_expand: OnExpand = Rc::new(move |open, _, app| send(UiEvent::Expand { open }, app));
            let view = app.new(|cx| MapView::new(map, streaming, under, on_select, on_filter, on_expand, cx));
            keep(key, view, app);
            window.on_next_frame(|window, _| window.refresh());
        },
    )
    .size_full();
    element.rounded(px(radius)).overflow_hidden().bg(c.map_water).child(make)
}

/// The map kept for the key, which was drawn now.
fn kept(key: &str, app: &mut App) -> Option<Entity<MapView>> {
    let maps = app.default_global::<Maps>();
    maps.drawn += 1;
    let drawn = maps.drawn;
    let (view, last) = maps.views.get_mut(key)?;
    *last = drawn;
    Some(view.clone())
}

fn keep(key: String, view: Entity<MapView>, app: &mut App) {
    let maps = app.default_global::<Maps>();
    maps.drawn += 1;
    maps.views.insert(key, (view, maps.drawn));
    while maps.views.len() > KEPT {
        let Some(oldest) = maps.views.iter().min_by_key(|(_, (_, last))| *last).map(|(key, _)| key.clone()) else {
            break;
        };
        maps.views.remove(&oldest);
    }
}
