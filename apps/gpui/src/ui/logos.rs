//! The marks of Motile, the agents, GitHub and Google, the icon of a project, and the labels of a
//! server and of a pull request, as the Mac app's `Icons.swift` draws them.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Agent, PullRequest};

use crate::models::{Project, Server};
use crate::theme::colors;
use crate::ui::{OnClick, icons, tooltip};

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
            .child(icons::symbol("folder", size * 0.78))
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
pub fn server_label(server: &Server, size: f32, cx: &App) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("server-label-{}", server.id)))
        .flex()
        .items_center()
        .gap(px(size * 0.3))
        .text_color(colors(cx).tertiary)
        .child(icons::symbol("server", size * 0.82))
        .child(div().text_size(px(size)).truncate().child(server.name.clone()))
        .tooltip(tooltip(format!("On {}", server.name)))
}

/// The symbol of what became of a pull request.
pub fn pull_request_symbol(pull_request: &PullRequest) -> &'static str {
    if pull_request.merged {
        return "git-merge";
    }
    if pull_request.closed {
        return "git-pull-request-closed";
    }
    if pull_request.draft { "git-pull-request-draft" } else { "git-pull-request" }
}

/// The colour of what became of a pull request.
pub fn pull_request_color(pull_request: &PullRequest, cx: &App) -> Hsla {
    let c = colors(cx);
    if pull_request.merged {
        return c.merged;
    }
    if pull_request.closed {
        return c.danger;
    }
    if pull_request.draft { c.secondary } else { c.success }
}

/// A thread's pull request: what became of it, and its number. In the colour of what became of
/// it, or as quiet as the row it is in. When it has a click, its number underlines under the
/// pointer.
#[derive(IntoElement)]
pub struct PullRequestLabel {
    id: ElementId,
    pull_request: PullRequest,
    colored: bool,
    on_click: Option<OnClick>,
}

impl PullRequestLabel {
    pub fn new(id: impl Into<ElementId>, pull_request: &PullRequest) -> Self {
        Self { id: id.into(), pull_request: pull_request.clone(), colored: true, on_click: None }
    }

    pub fn colored(mut self, colored: bool) -> Self {
        self.colored = colored;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for PullRequestLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = if self.colored { pull_request_color(&self.pull_request, cx) } else { colors(cx).tertiary };
        let group: SharedString = format!("pull-request-label-{:?}", self.id).into();
        let clickable = self.on_click.is_some();
        div()
            .id(self.id)
            .group(group.clone())
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(2.))
            .text_color(color)
            .child(icons::symbol(pull_request_symbol(&self.pull_request), 11.))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .when(clickable, |number| number.group_hover(group, |number| number.text_decoration_1()))
                    .child(self.pull_request.number.to_string()),
            )
            .tooltip(tooltip(self.pull_request.title.clone()))
            .when(clickable, |label| label.active(|label| label.opacity(0.7)))
            .when_some(self.on_click, |label, on_click| {
                label.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}
