//! The view a canvas component embeds: its scene with its legend, the hit under the pointer lit
//! and its tooltip over it.

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::canvas::Tip;

use super::hit::hit;
use super::legend::{Arrangement, arrange, paint_legend};
use super::paint::{Palette, Prepared, paint_scene};
use crate::theme::{Radius, Surface, UI_FONT, colors};
use crate::ui::{popover_card, tabular_numbers};

const TIP_PADDING_X: f32 = 12.;
const TIP_PADDING_Y: f32 = 10.;
const TIP_MIN_WIDTH: f32 = 110.;
const TIP_ROW_GAP: f32 = 3.;
const TIP_DOT: f32 = 8.;
const TIP_DOT_GAP: f32 = 6.;
const TIP_VALUE_GAP: f32 = 8.;
const TIP_TEXT: f32 = 12.;
const TIP_LINE: f32 = 18.;
/// Half the room the tooltip is centred in, wider than any.
const TIP_ROOM: f32 = 600.;

/// A scene as a component shows it, `width` wide (the column's, for a chart): with its legend,
/// lighting the hit under the pointer and showing its tooltip. It lies on `surface` (the
/// transcript's background unless set) beside text in `current` (`foreground` unless set), and
/// is as large as `arrange` says.
#[derive(IntoElement)]
pub struct SceneView {
    id: ElementId,
    prepared: Arc<Prepared>,
    width: f32,
    surface: Surface,
    current: Option<Hsla>,
}

impl SceneView {
    pub fn new(id: impl Into<ElementId>, prepared: Arc<Prepared>, width: f32) -> Self {
        Self { id: id.into(), prepared, width, surface: Surface::Background, current: None }
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn current(mut self, current: Hsla) -> Self {
        self.current = Some(current);
        self
    }
}

/// The hit the pointer lights, and where the scene was painted last, to find the hit under it.
#[derive(Default)]
struct Pointer {
    lit: Option<usize>,
    origin: Point<Pixels>,
}

/// The legend laid out for one scene at one width, kept until either changes.
#[derive(Default)]
struct Arranged {
    key: Option<(usize, u32)>,
    arrangement: Arc<Arrangement>,
}

impl RenderOnce for SceneView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let child = |name: &'static str| ElementId::NamedChild(Arc::new(self.id.clone()), name.into());
        let arrangement = arranged(child("legend"), &self.prepared, self.width, window, cx);
        let pointer = window.use_keyed_state(child("pointer"), cx, |_, _| Pointer::default());
        let lit = pointer.read(cx).lit.filter(|index| *index < self.prepared.scene.hits.len());
        let c = colors(cx);
        let palette = Palette::new(c, self.surface).current(self.current.unwrap_or(c.foreground));
        let scene_at = point(px(arrangement.scene_x), px(arrangement.scene_y));
        let tip = lit.map(|index| self.prepared.scene.hits[index].tip.clone());
        let painted = {
            let (prepared, arrangement, pointer) = (self.prepared.clone(), arrangement.clone(), pointer.clone());
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let origin = bounds.origin + scene_at;
                    pointer.update(cx, |pointer, _| pointer.origin = origin);
                    paint_legend(&arrangement, bounds.origin, &palette, window, cx);
                    paint_scene(&prepared, origin, &palette, lit, window, cx);
                },
            )
            .absolute()
            .size_full()
        };
        let moved = {
            let (prepared, pointer) = (self.prepared.clone(), pointer.clone());
            move |event: &MouseMoveEvent, _: &mut Window, cx: &mut App| {
                let at = event.position - pointer.read(cx).origin;
                let under = hit(&prepared.scene, f32::from(at.x), f32::from(at.y));
                light(&pointer, under, cx);
            }
        };
        let left = move |hovered: &bool, _: &mut Window, cx: &mut App| {
            if !*hovered {
                light(&pointer, None, cx);
            }
        };
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .w(px(arrangement.width))
            .h(px(arrangement.height))
            .when_some(self.prepared.scene.label.clone(), |view, label| view.role(Role::Image).aria_label(label))
            .child(painted)
            .on_mouse_move(moved)
            .on_hover(left)
            .when_some(tip, |view, tip| view.child(deferred(tooltip(&tip, scene_at, &palette, cx)).with_priority(1)))
    }
}

fn light(pointer: &Entity<Pointer>, lit: Option<usize>, cx: &mut App) {
    pointer.update(cx, |pointer, cx| {
        if pointer.lit != lit {
            pointer.lit = lit;
            cx.notify();
        }
    });
}

fn arranged(
    id: ElementId,
    prepared: &Arc<Prepared>,
    width: f32,
    window: &mut Window,
    cx: &mut App,
) -> Arc<Arrangement> {
    let key = (Arc::as_ptr(prepared) as usize, width.to_bits());
    let state = window.use_keyed_state(id, cx, |_, _| Arranged::default());
    if state.read(cx).key != Some(key) {
        let arrangement = Arc::new(arrange(&prepared.scene, width, cx.text_system()));
        state.update(cx, |arranged, _| *arranged = Arranged { key: Some(key), arrangement });
    }
    state.read(cx).arrangement.clone()
}

/// The tooltip, centred on the tip's x with its top at its y, in the scene's points from where the
/// scene lies in the view. It takes no input, so the pointer still moves over the scene under it.
fn tooltip(tip: &Tip, scene_at: Point<Pixels>, palette: &Palette, cx: &App) -> Div {
    let c = palette.colors;
    let title = div().font_weight(FontWeight::SEMIBOLD).text_color(c.foreground).child(tip.title.clone());
    let rows = tip.rows.iter().map(|row| {
        div()
            .flex()
            .items_center()
            .when_some(row.color.as_ref(), |line, color| {
                let dot = div().flex_none().size(px(TIP_DOT)).rounded_full().bg(palette.resolve(color, 1.));
                line.child(dot.mr(px(TIP_DOT_GAP)))
            })
            .child(div().flex_1().text_color(c.muted_foreground).child(row.name.clone()))
            .child(
                div()
                    .pl(px(TIP_VALUE_GAP))
                    .font_weight(FontWeight::MEDIUM)
                    .font_features(tabular_numbers())
                    .text_color(c.foreground)
                    .child(row.value.clone()),
            )
    });
    let card = popover_card(div(), Radius::LG, cx)
        .flex()
        .flex_col()
        .gap(px(TIP_ROW_GAP))
        .min_w(px(TIP_MIN_WIDTH))
        .px(px(TIP_PADDING_X))
        .py(px(TIP_PADDING_Y))
        .font_family(UI_FONT)
        .text_size(px(TIP_TEXT))
        .line_height(px(TIP_LINE))
        .whitespace_nowrap()
        .child(title)
        .children(rows);
    div()
        .absolute()
        .left(scene_at.x + px(tip.x - TIP_ROOM))
        .top(scene_at.y + px(tip.y))
        .w(px(TIP_ROOM * 2.))
        .flex()
        .justify_center()
        .items_start()
        .child(card)
}
