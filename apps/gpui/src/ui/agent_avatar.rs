//! An agent's avatar, as the Mac app's `AgentAvatar.swift` draws it: its logo in a ring, with a
//! dot on its corner in the colour of how it is doing, cut out of the ring.

use std::f32::consts::FRAC_PI_4;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentState;
use motile_protocol::wire::Agent;

use crate::theme::{Colors, Layer, Surface, colors};
use crate::ui::{icons, logos};

pub const SIDE: f32 = 24.;
/// The dot is as large on every system as the composer strips' are, and cut out of what it lies on.
const DOT: f32 = 6.;
const CUT: f32 = 2.;
/// The room either side of the words of the pill that counts the rest.
const MORE_PADDING: f32 = 3.;
const MORE_TEXT: f32 = 10.;

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
    /// It lies over another, from which a ring of this colour sets it apart.
    ring: Option<Hsla>,
    /// The ring's colour while the pointer is over the row `group` names, which lights up.
    lit_ring: Option<(SharedString, Hsla)>,
}

impl AgentAvatar {
    pub fn new(agent: Option<Agent>) -> Self {
        Self { agent, state: None, side: SIDE, ring: None, lit_ring: None }
    }

    pub fn state(mut self, state: impl Into<Option<AgentState>>) -> Self {
        self.state = state.into();
        self
    }

    fn side(mut self, side: f32) -> Self {
        self.side = side;
        self
    }

    fn ringed(mut self, ground: Hsla, lit: Option<(SharedString, Hsla)>) -> Self {
        self.ring = Some(ground);
        self.lit_ring = lit;
        self
    }
}

impl RenderOnce for AgentAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let side = self.side;
        let circle = if self.state.is_some() { side - CUT } else { side };
        let inset = if self.ring.is_some() { 1.5 } else { 0. };
        let inner = circle - inset * 2.;
        let logo = match self.agent {
            Some(agent) => logos::agent_icon(agent, (inner * 0.56).round(), cx).into_any_element(),
            None => icons::symbol("bot", (inner * 0.48).round()).text_color(c.muted_foreground).into_any_element(),
        };
        let disc = div().absolute().top(px(inset)).left(px(inset)).size(px(inner)).rounded_full();
        let disc = match self.state {
            Some(_) => disc.child(cut_circle(side, inner, c.background_accent, c.border_accent)),
            None => disc.bg(c.background_accent).border_1().border_color(c.border_accent),
        };
        div()
            .relative()
            .size(px(side))
            .flex_none()
            .when_some(self.ring, |avatar, ring| {
                let ring = div().absolute().size(px(circle)).rounded_full().bg(ring);
                avatar.child(match self.lit_ring {
                    Some((group, lit)) => ring.group_hover(group, move |ring| ring.bg(lit)),
                    None => ring,
                })
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

/// A circle `side` wide at the top left of an avatar `box_side` wide, with the round gap around the
/// corner's dot cut out of it, filled and with an outline a point wide.
fn cut_circle(box_side: f32, side: f32, fill: Hsla, outline: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let radius = side / 2.;
            let gap = DOT / 2. + CUT;
            // Both middles lie on the diagonal, so the circles meet either side of it.
            let apart = (box_side - gap - radius) * std::f32::consts::SQRT_2;
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
    side: f32,
    rings: bool,
    /// The row that lights up under the pointer, whose light the rings take then.
    lit_in: Option<SharedString>,
}

impl AgentAvatars {
    pub fn new(agents: Vec<Option<Agent>>, more: usize) -> Self {
        Self { agents, more, side: SIDE, rings: true, lit_in: None }
    }

    pub fn side(mut self, side: f32) -> Self {
        self.side = side;
        self
    }

    /// Without the rings of the ground, for a ground that changes, as a button's.
    pub fn without_rings(mut self) -> Self {
        self.rings = false;
        self
    }

    /// How far each stands from the one before it: they overlap by a quarter.
    fn step(&self) -> f32 {
        self.side * 3. / 4.
    }

    /// Where the pill that counts the rest starts.
    fn more_left(&self) -> f32 {
        self.agents.len() as f32 * self.step()
    }

    fn more_text(&self) -> String {
        format!("+{}", self.more)
    }

    pub fn width(&self, window: &mut Window) -> f32 {
        if self.more == 0 {
            return self.agents.len().saturating_sub(1) as f32 * self.step() + self.side;
        }
        let words = crate::ui::text_width(&self.more_text(), MORE_TEXT, FontWeight::MEDIUM, window);
        self.more_left() + (words + MORE_PADDING * 2.).max(self.side)
    }

    pub fn lit_in(mut self, group: impl Into<SharedString>) -> Self {
        self.lit_in = Some(group.into());
        self
    }
}

impl RenderOnce for AgentAvatars {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let ringed = self.rings && self.agents.len() > 1;
        let (side, step) = (self.side, self.step());
        // The pill is in the flow, so that the row is as wide as it is; the avatars lie over it.
        let span = match self.more {
            0 => div().w(px(self.agents.len().saturating_sub(1) as f32 * step + side)),
            _ => div()
                .ml(px(self.more_left()))
                .min_w(px(side))
                .h(px(side))
                .px(px(MORE_PADDING))
                .rounded_full()
                .bg(c.border_accent)
                .flex()
                .items_center()
                .justify_center()
                .whitespace_nowrap()
                .text_size(px(MORE_TEXT))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_foreground)
                .child(self.more_text()),
        };
        let lit = self.lit_in.map(|group| (group, Surface::Background.layer(Layer::Row, c)));
        // The first lies over the second, so it is drawn last.
        let avatars = self.agents.into_iter().enumerate().rev().map(move |(index, agent)| {
            let avatar = AgentAvatar::new(agent).side(side);
            let avatar = if ringed { avatar.ringed(c.background, lit.clone()) } else { avatar };
            div().absolute().left(px(index as f32 * step)).child(avatar)
        });
        div().relative().flex_none().flex().h(px(side)).child(span).children(avatars)
    }
}
