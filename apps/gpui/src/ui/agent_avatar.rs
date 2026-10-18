//! An agent's avatar, as the Mac app's `AgentAvatar.swift` draws it: its logo in a ring, with a
//! dot on its corner in the colour of how it is doing, cut out of the ring as the menu button's
//! dot is.

use std::f32::consts::FRAC_PI_4;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentState;
use motile_protocol::wire::Agent;

use crate::theme::{Colors, colors};
use crate::ui::{icons, logos};

pub const SIDE: f32 = 24.;
/// How far each avatar of a row of them stands from the one before it: they overlap by a quarter.
const STEP: f32 = SIDE * 3. / 4.;
/// The dot is the menu button's: as large on every system, and cut out of what it lies on.
const DOT: f32 = 8.;
const CUT: f32 = 2.;

/// The colour of how an agent is doing.
pub fn state_color(state: AgentState, c: &Colors) -> Hsla {
    match state {
        AgentState::Working => c.process,
        AgentState::Waiting => c.warning,
        AgentState::Done => c.success,
        AgentState::Failed => c.destructive,
        AgentState::Stopped => c.muted_stronger_foreground,
    }
}

#[derive(IntoElement)]
pub struct AgentAvatar {
    agent: Option<Agent>,
    state: Option<AgentState>,
    /// It lies over another, from which a ring of this colour sets it apart.
    ring: Option<Hsla>,
}

impl AgentAvatar {
    pub fn new(agent: Option<Agent>) -> Self {
        Self { agent, state: None, ring: None }
    }

    pub fn state(mut self, state: impl Into<Option<AgentState>>) -> Self {
        self.state = state.into();
        self
    }

    fn ringed(mut self, ground: Hsla) -> Self {
        self.ring = Some(ground);
        self
    }
}

impl RenderOnce for AgentAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let circle = if self.state.is_some() { SIDE - CUT } else { SIDE };
        let inset = if self.ring.is_some() { 1.5 } else { 0. };
        let inner = circle - inset * 2.;
        let logo = match self.agent {
            Some(agent) => logos::agent_icon(agent, (inner * 0.56).round(), cx).into_any_element(),
            None => icons::symbol("bot", (inner * 0.48).round()).text_color(c.muted_foreground).into_any_element(),
        };
        let disc = div().absolute().top(px(inset)).left(px(inset)).size(px(inner)).rounded_full();
        let disc = match self.state {
            Some(_) => disc.child(cut_circle(inner, c.background_accent, c.border_accent)),
            None => disc.bg(c.background_accent).border_1().border_color(c.border_accent),
        };
        div()
            .relative()
            .size(px(SIDE))
            .flex_none()
            .when_some(self.ring, |avatar, ring| {
                avatar.child(div().absolute().size(px(circle)).rounded_full().bg(ring))
            })
            .child(disc.flex().items_center().justify_center().child(logo))
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

/// A circle `side` wide at the avatar's top left, with the round gap around the corner's dot cut
/// out of it, filled and with an outline a point wide.
fn cut_circle(side: f32, fill: Hsla, outline: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let radius = side / 2.;
            let gap = DOT / 2. + CUT;
            // Both middles lie on the diagonal, so the circles meet either side of it.
            let apart = (SIDE - gap - radius) * std::f32::consts::SQRT_2;
            let at =
                |r: f32, angle: f32| bounds.origin + point(px(radius + r * angle.cos()), px(radius + r * angle.sin()));
            let meets = |r: f32| {
                let along = (apart * apart - gap * gap + r * r) / (2. * apart);
                let off = (along / r).clamp(-1., 1.).acos();
                (at(r, FRAC_PI_4 - off), at(r, FRAC_PI_4 + off))
            };
            let (from, to) = meets(radius);
            let mut shape = PathBuilder::fill();
            shape.move_to(from);
            shape.arc_to(point(px(radius), px(radius)), px(0.), true, false, to);
            shape.arc_to(point(px(gap), px(gap)), px(0.), false, true, from);
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

/// The first few agents of a group side by side, each over the next and ringed by the ground,
/// and how many more there are. How they are doing is left to what is written beside them.
#[derive(IntoElement)]
pub struct AgentAvatars {
    agents: Vec<Option<Agent>>,
    more: usize,
}

impl AgentAvatars {
    pub fn new(agents: Vec<Option<Agent>>, more: usize) -> Self {
        Self { agents, more }
    }
}

impl RenderOnce for AgentAvatars {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let ringed = self.agents.len() > 1;
        let count = self.agents.len() + usize::from(self.more > 0);
        let width = count.saturating_sub(1) as f32 * STEP + SIDE;
        let more = (self.more > 0).then(|| {
            div()
                .absolute()
                .left(px(self.agents.len() as f32 * STEP))
                .size(px(SIDE))
                .rounded_full()
                .bg(c.background_accent)
                .border_1()
                .border_color(c.border_accent)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_foreground)
                .child(format!("+{}", self.more))
        });
        // The first lies over the second, so it is drawn last.
        let avatars = self.agents.into_iter().enumerate().rev().map(move |(index, agent)| {
            let avatar = AgentAvatar::new(agent);
            let avatar = if ringed { avatar.ringed(c.background) } else { avatar };
            div().absolute().left(px(index as f32 * STEP)).child(avatar)
        });
        div().relative().flex_none().w(px(width)).h(px(SIDE)).children(more).children(avatars)
    }
}
