//! An agent's avatar, as the Mac app's `AgentAvatar.swift` draws it: its logo in a ring, with a
//! dot on its corner in the colour of how it is doing, cut out of the ring.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentState;
use motile_protocol::wire::Agent;

use crate::theme::{Colors, colors};
use crate::ui::{icons, logos};

pub const SIDE: f32 = 24.;
/// The dot is as large on every system as the composer strips' are, and cut out of what it lies on.
const DOT: f32 = 6.;
const CUT: f32 = 2.;
/// The square of the agent's logo, or the bot when it has none, as a share of its circle.
const MARK: f32 = 0.6;
/// The ellipsis that stands for the agents there is no room for, as a share of its circle.
const ELLIPSIS: f32 = 0.72;

/// The colour of how an agent is doing.
pub fn state_color(state: AgentState, c: &Colors) -> Hsla {
    match state {
        AgentState::Working => c.process,
        AgentState::Waiting | AgentState::Limited => c.warning,
        AgentState::Done => c.success,
        AgentState::Failed => c.destructive,
        AgentState::Stopped => c.muted_stronger_foreground,
    }
}

#[derive(IntoElement)]
pub struct AgentAvatar {
    agent: Option<Agent>,
    state: Option<AgentState>,
    side: f32,
    /// The avatar this far before it lies over it, and is cut out of it with a ring around it.
    under: Option<f32>,
    /// It stands for the agents there is no room for.
    more: bool,
}

impl AgentAvatar {
    pub fn new(agent: Option<Agent>) -> Self {
        Self { agent, state: None, side: SIDE, under: None, more: false }
    }

    pub fn state(mut self, state: impl Into<Option<AgentState>>) -> Self {
        self.state = state.into();
        self
    }

    fn side(mut self, side: f32) -> Self {
        self.side = side;
        self
    }

    fn under(mut self, step: Option<f32>) -> Self {
        self.under = step;
        self
    }

    fn more() -> Self {
        Self { more: true, ..Self::new(None) }
    }
}

impl RenderOnce for AgentAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let side = self.side;
        let circle = if self.state.is_some() { side - CUT } else { side };
        let mark = match (self.more, self.agent) {
            (true, _) => icons::symbol_side("ellipsis", (circle * ELLIPSIS).round())
                .text_color(c.muted_foreground)
                .into_any_element(),
            (false, Some(agent)) => logos::agent_icon_side(agent, (circle * MARK).round(), cx).into_any_element(),
            (false, None) => {
                icons::symbol_side("bot", (circle * MARK).round()).text_color(c.muted_foreground).into_any_element()
            }
        };
        let disc = div().absolute().size(px(circle)).rounded_full();
        let fill = (c.background_accent, c.border_accent);
        let disc = match (self.under, self.state) {
            (Some(step), _) => {
                let radius = circle / 2.;
                disc.child(cut_circle(circle, point(radius - step, radius), radius + ring(side), fill))
            }
            (None, Some(_)) => {
                let dot = side - CUT - DOT / 2.;
                disc.child(cut_circle(circle, point(dot, dot), DOT / 2. + CUT, fill))
            }
            (None, None) => disc.bg(c.background_accent).border_1().border_color(c.border_accent),
        };
        div()
            .relative()
            .size(px(side))
            .flex_none()
            .child(disc.flex().items_center().justify_center().child(mark))
            .when_some(self.state, |avatar, state| {
                avatar.child(
                    div()
                        .absolute()
                        .right(px(CUT))
                        .bottom(px(CUT))
                        .size(px(DOT))
                        .rounded_full()
                        .bg(state_color(state, c)),
                )
            })
    }
}

/// How far each avatar side by side stands from the one before it: they overlap by a quarter.
fn step(side: f32) -> f32 {
    side * 3. / 4.
}

/// The ring around an avatar that is cut out of the one under it.
fn ring(side: f32) -> f32 {
    (side / 12.).floor().max(1.)
}

/// A circle `side` wide with the circle of `cut_radius` around `cut` (from the circle's top left)
/// cut out of it, filled and with an outline a point wide.
fn cut_circle(side: f32, cut: Point<f32>, cut_radius: f32, (fill, outline): (Hsla, Hsla)) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let radius = side / 2.;
            let (dx, dy) = (cut.x - radius, cut.y - radius);
            let apart = dx.hypot(dy);
            let toward = dy.atan2(dx);
            let at =
                |r: f32, angle: f32| bounds.origin + point(px(radius + r * angle.cos()), px(radius + r * angle.sin()));
            // Where a circle of `r` around the middle meets the cut, either side of the line to it.
            let meets = |r: f32| {
                let along = (apart * apart - cut_radius * cut_radius + r * r) / (2. * apart);
                let off = (along / r).clamp(-1., 1.).acos();
                (at(r, toward - off), at(r, toward + off))
            };
            let (from, to) = meets(radius);
            let mut shape = PathBuilder::fill();
            shape.move_to(from);
            shape.arc_to(point(px(radius), px(radius)), px(0.), true, false, to);
            shape.arc_to(point(px(cut_radius), px(cut_radius)), px(0.), false, true, from);
            shape.close();
            if let Ok(path) = shape.build() {
                window.paint_path(path, fill);
            }
            let line = radius - 0.5;
            let (from, to) = meets(line);
            let mut edge = PathBuilder::stroke(px(1.));
            edge.move_to(from);
            edge.arc_to(point(px(line), px(line)), px(0.), true, false, to);
            if let Ok(path) = edge.build() {
                window.paint_path(path, outline);
            }
        },
    )
    .absolute()
    .size_full()
}

/// The first few agents of a group side by side, each over the next, which has it cut out with a
/// ring around it, and an ellipsis for the agents there is no room for. How they are doing is
/// left to what is written beside them.
#[derive(IntoElement)]
pub struct AgentAvatars {
    agents: Vec<Option<Agent>>,
    more: usize,
    side: f32,
}

impl AgentAvatars {
    pub fn new(agents: Vec<Option<Agent>>, more: usize) -> Self {
        Self { agents, more, side: SIDE }
    }

    pub fn width(&self) -> f32 {
        let count = self.agents.len() + usize::from(self.more > 0);
        count.saturating_sub(1) as f32 * step(self.side) + self.side
    }
}

impl RenderOnce for AgentAvatars {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (side, step) = (self.side, step(self.side));
        let width = self.width();
        let more = (self.more > 0).then(AgentAvatar::more);
        let avatars = self.agents.into_iter().map(AgentAvatar::new).chain(more);
        // The first lies over the second, so it is drawn last.
        let avatars: Vec<_> = avatars
            .enumerate()
            .map(|(index, avatar)| {
                let avatar = avatar.side(side).under((index > 0).then_some(step));
                div().absolute().left(px(index as f32 * step)).child(avatar)
            })
            .collect();
        div().relative().flex_none().w(px(width)).h(px(side)).children(avatars.into_iter().rev())
    }
}
