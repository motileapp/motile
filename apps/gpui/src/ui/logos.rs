//! The marks of Motile, the agents, GitHub and Google, and the icon of a project.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::Agent;

use crate::models::{Project, Server};
use crate::theme::colors;
use crate::ui::icons;

/// An agent's logo: Claude's in its colour, OpenAI's in the colour of the text.
pub fn agent_icon(agent: Agent, size: f32, cx: &App) -> Svg {
    match agent {
        Agent::Claude => svg().path("icons/claude.svg").size(px(size)).flex_shrink_0().text_color(rgb(0xD97757)),
        Agent::Codex => svg().path("icons/openai.svg").size(px(size)).flex_shrink_0().text_color(colors(cx).text),
    }
}

pub fn github_mark(size: f32, cx: &App) -> Svg {
    svg().path("icons/github-mark.svg").size(px(size)).flex_shrink_0().text_color(colors(cx).text)
}

/// Google's "G", drawn as four arcs in its colours.
pub fn google_mark(size: f32) -> Div {
    let part = |name: &str, color: u32| {
        svg()
            .path(SharedString::from(format!("icons/google-{name}.svg")))
            .absolute()
            .size(px(size))
            .text_color(rgb(color))
    };
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .child(part("blue", 0x4285F5))
        .child(part("green", 0x33A854))
        .child(part("yellow", 0xFABD05))
        .child(part("red", 0xEB4236))
}

/// The app's icon: its mark on a dark tile.
pub fn app_logo(size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.225))
        .bg(linear_gradient(180., linear_color_stop(rgb(0x3a3a3c), 0.), linear_color_stop(rgb(0x111112), 1.)))
        .flex()
        .items_center()
        .justify_center()
        .child(svg().path("icons/motile-mark.svg").size(px(size * 0.6)).text_color(white()))
}

/// A project's icon, or a folder when it has none.
pub fn project_icon(project: Option<&Project>, size: f32, cx: &App) -> AnyElement {
    let path = project.and_then(|project| project.icon_path.clone());
    let Some(path) = path else {
        return div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(colors(cx).secondary)
            .child(icons::symbol("folder", size * 0.82))
            .into_any_element();
    };
    img(std::path::PathBuf::from(path))
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.22))
        .object_fit(ObjectFit::Contain)
        .into_any_element()
}

/// The server a thread or project is on, for when there is more than one.
pub fn server_label(server: &Server, size: f32, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(size * 0.3))
        .text_color(colors(cx).tertiary)
        .child(icons::symbol("server.rack", size * 0.82))
        .child(div().text_size(px(size)).truncate().child(server.name.clone()))
}
