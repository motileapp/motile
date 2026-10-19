//! Modals, as the Mac app's `ModalMac.swift` draws them: cards over the window, the last opened
//! on top, on the one scrim. Esc or a click on the scrim closes the top one; the ones under it
//! wait, their controls disabled. A view draws its modal while its state says it is open, and for
//! a moment after with `leaving`, which `presented` keeps, so that the card fades out. The modal
//! tells the view when to close. While any is open the window's own views lose the keys, and get
//! them back after.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{Opacity, Radius, ShadowSize, colors};
use crate::ui::control::{ActionButton, Variant};
use crate::ui::motion::{self, Tween};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// How far a modal stands from the window's edges.
const MARGIN: f32 = 32.;
/// The deferred priority of the bottom modal; each above it takes two more.
const PRIORITY: usize = 10;

/// The modals open, in the order they opened, the ones drawn this frame, what had the keys
/// before the first, and the scrim they share.
struct Stack {
    order: Vec<ElementId>,
    drawn: Vec<ElementId>,
    focused_before: Option<FocusHandle>,
    scrim: Tween,
    /// The deferred priority of the scrim: under the top modal, over the ones under it.
    scrim_priority: usize,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            drawn: Vec::new(),
            focused_before: None,
            scrim: Tween::new(0., 0., motion::MODAL),
            scrim_priority: PRIORITY,
        }
    }
}

impl Global for Stack {}

/// Whether a modal is open over the window.
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<Stack>().is_some_and(|stack| !stack.order.is_empty())
}

/// Forgets the modals that weren't drawn this frame, and draws again when one of them was on top.
/// The window's root runs it once everything else has been laid out.
pub fn end_frame(window: &mut Window, cx: &mut App) {
    let Some(stack) = cx.try_global::<Stack>() else { return };
    if stack.order.is_empty() && stack.drawn.is_empty() {
        return;
    }
    let stack = cx.global_mut::<Stack>();
    let drawn = std::mem::take(&mut stack.drawn);
    let before = stack.order.len();
    stack.order.retain(|id| drawn.contains(id));
    if stack.order.len() == before {
        return;
    }
    window.request_animation_frame();
    if !stack.order.is_empty() {
        return;
    }
    let Some(focused) = stack.focused_before.take() else { return };
    window.on_next_frame(move |window, cx| window.focus(&focused, cx));
}

/// Where the modal is in the stack, putting it on top when it has just opened.
fn place(id: &ElementId, window: &Window, cx: &mut App) -> (usize, bool) {
    let focused = window.focused(cx);
    let stack = cx.default_global::<Stack>();
    let opened = !stack.order.contains(id);
    if opened {
        if stack.order.is_empty() {
            stack.focused_before = focused;
        }
        stack.order.push(id.clone());
    }
    stack.drawn.push(id.clone());
    let index = stack.order.iter().position(|shown| shown == id).unwrap_or(0);
    let top = index + 1 == stack.order.len();
    if opened {
        crate::ui::overlay_opened(cx);
    }
    (index, top)
}

/// The scrim under the top modal, one for all of them, which fades in as the first opens and out
/// as the last closes. The window's root lays it out after everything else, once the modals
/// drawn this frame are known.
#[derive(IntoElement)]
pub struct Scrim;

impl RenderOnce for Scrim {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().children(scrim(window, cx))
    }
}

fn scrim(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let stack = cx.try_global::<Stack>()?;
    let top = stack.order.iter().rposition(|id| stack.drawn.contains(id));
    let stack = cx.global_mut::<Stack>();
    stack.scrim.retarget(if top.is_some() { 1. } else { 0. });
    if let Some(top) = top {
        stack.scrim_priority = PRIORITY + 2 * top - 1;
    }
    let (opacity, priority) = (stack.scrim.value(), stack.scrim_priority);
    if stack.scrim.moving() {
        window.request_animation_frame();
    }
    if opacity <= 0. {
        return None;
    }
    let size = crate::window_frame::content_size(window);
    let scrim = div().w(size.width).h(size.height).bg(colors(cx).scrim).opacity(opacity);
    Some(
        deferred(anchored().position(crate::window_frame::content_origin(window)).child(scrim))
            .with_priority(priority)
            .into_any_element(),
    )
}

/// The item a modal shows while it is open, and the last one while the modal fades out after
/// it closed, with whether it is leaving. `id` names it among the view's modals.
pub fn presented<T: Clone + 'static>(
    id: impl Into<ElementId>,
    item: Option<T>,
    window: &mut Window,
    cx: &mut App,
) -> Option<(T, bool)> {
    motion::lingering(id, item, motion::MODAL, window, cx)
}

/// A sheet a view keeps as a view of its own, while it is open and while it fades out after:
/// `leaves` tells the sheet which, for it to give its modal.
pub fn present_sheet<S: 'static>(
    id: impl Into<ElementId>,
    sheet: Option<Entity<S>>,
    leaves: impl FnOnce(&mut S, bool),
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<S>> {
    let (sheet, leaving) = presented(id, sheet, window, cx)?;
    sheet.update(cx, |sheet, _| leaves(sheet, leaving));
    Some(sheet)
}

/// A card over the window that waits until it is done with. `on_close` lets it go: Esc, a click
/// on the scrim, or what the content does.
#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    width: Option<f32>,
    content: AnyElement,
    on_close: Run,
    on_return: Option<Run>,
    leaving: bool,
}

pub fn modal(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> Modal {
    Modal {
        id: id.into(),
        width: None,
        content: content.into_any_element(),
        on_close: Rc::new(on_close),
        on_return: None,
        leaving: false,
    }
}

impl Modal {
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// What Return does, unless a field inside takes it.
    pub fn on_return(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_return = Some(Rc::new(run));
        self
    }

    /// It has closed and fades out, taking nothing more.
    pub fn leaving(mut self, leaving: bool) -> Self {
        self.leaving = leaving;
        self
    }
}

/// What a modal keeps from frame to frame: what has its keys, whether it has been laid out, and
/// where it was in the stack, which it keeps while it fades out.
struct Kept {
    focus: FocusHandle,
    laid_out: bool,
    index: usize,
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| Kept {
            focus: cx.focus_handle(),
            laid_out: false,
            index: 0,
        });
        let leaving = self.leaving;
        let (index, top) = if leaving {
            (state.read(cx).index, false)
        } else {
            let (index, top) = place(&self.id, window, cx);
            state.update(cx, |kept, _| kept.index = index);
            (index, top)
        };
        let focus = state.read(cx).focus.clone();
        // On its first frame the modal isn't in the tree yet to tell whether a field of it has the keys.
        if !leaving && !state.read(cx).laid_out {
            state.update(cx, |kept, _| kept.laid_out = true);
            window.request_animation_frame();
        } else if top && !focus.contains_focused(window, cx) {
            focus.focus(window, cx);
        }
        let card_id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "card".into());
        let shown = motion::eased_from(card_id, 0., if leaving { 0. } else { 1. }, motion::MODAL, window, cx);
        let size = crate::window_frame::content_size(window);
        let (close, scrim_close) = (self.on_close.clone(), self.on_close.clone());
        let on_return = self.on_return;
        let card = div()
            .id("modal-card")
            .relative()
            .top(px(8. * (1. - shown)))
            .opacity(shown)
            .when_some(self.width, |card, width| card.w(px(width)))
            .max_w(size.width - px(2. * MARGIN))
            .max_h(size.height - px(2. * MARGIN))
            .bg(c.background)
            .rounded(px(Radius::XL))
            .border_1()
            .border_color(c.border)
            .shadow(c.shadow(ShadowSize::Xl, Opacity::ShadowStrongest))
            .overflow_hidden()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(crate::ui::inert_scope(!top && !leaving, self.content))
            .when(leaving, |card| card.child(div().absolute().inset_0().occlude()));
        let layer = div().id(self.id).w(size.width).h(size.height).flex().items_center().justify_center().child(card);
        let layer = if leaving {
            layer
        } else {
            // A click beside the top card falls on the scrim, and closes it.
            layer
                .occlude()
                .track_focus(&focus)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    let run = match event.keystroke.key.as_str() {
                        "escape" => Some(close.clone()),
                        "enter" => on_return.clone(),
                        _ => None,
                    };
                    let Some(run) = run else { return };
                    cx.stop_propagation();
                    run(window, cx);
                })
                .when(top, |layer| {
                    layer.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        scrim_close(window, cx)
                    })
                })
        };
        deferred(anchored().position(crate::window_frame::content_origin(window)).child(layer))
            .with_priority(PRIORITY + 2 * index)
    }
}

/// One of the answers a confirmation offers besides the one that lets it go.
pub struct Answer {
    title: SharedString,
    variant: Variant,
    action: Run,
}

impl Answer {
    pub fn new(title: impl Into<SharedString>, action: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { title: title.into(), variant: Variant::Primary, action: Rc::new(action) }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn danger(self) -> Self {
        self.variant(Variant::Danger)
    }
}

/// What a confirmation puts to the user: a title, what it means, and the answers besides the one
/// that lets it go. The last answer is the one Return picks.
pub struct Confirmation {
    pub title: SharedString,
    pub message: SharedString,
    pub answers: Vec<Answer>,
    /// What the button that lets it go says.
    pub dismissal: SharedString,
}

impl Confirmation {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self { title: title.into(), message: SharedString::default(), answers: Vec::new(), dismissal: "Cancel".into() }
    }

    pub fn message(mut self, message: impl Into<SharedString>) -> Self {
        self.message = message.into();
        self
    }

    pub fn answer(mut self, answer: Answer) -> Self {
        self.answers.push(answer);
        self
    }

    pub fn dismissal(mut self, dismissal: impl Into<SharedString>) -> Self {
        self.dismissal = dismissal.into();
        self
    }
}

/// How wide a confirmation's text is at least. Its buttons widen it when they need more.
const TEXT_WIDTH: f32 = 360.;

/// A confirmation as a modal: its title and message, the button that lets it go, then its
/// answers. The button that lets it go is the primary one when there are no answers.
pub fn confirmation(
    id: impl Into<ElementId>,
    confirmation: Confirmation,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Modal {
    let c = colors(cx);
    let close: Run = Rc::new(on_close);
    let lone = confirmation.answers.is_empty();
    let dismiss = close.clone();
    let mut buttons = vec![
        ActionButton::new("dismissal", confirmation.dismissal)
            .variant(if lone { Variant::Primary } else { Variant::Secondary })
            .on_click(move |_, window, cx| dismiss(window, cx))
            .into_any_element(),
    ];
    let mut on_return = close.clone();
    for (index, answer) in confirmation.answers.into_iter().enumerate() {
        let close = close.clone();
        let action = answer.action;
        let run: Run = Rc::new(move |window: &mut Window, cx: &mut App| {
            action(window, cx);
            close(window, cx);
        });
        on_return = run.clone();
        buttons.push(
            ActionButton::new(("answer", index), answer.title)
                .variant(answer.variant)
                .on_click(move |_, window, cx| run(window, cx))
                .into_any_element(),
        );
    }
    // Its width is the buttons', or the text's when they need less: the text adds no width of
    // its own, so that it wraps rather than widens the card.
    let content = div()
        .p(px(20.))
        .min_w(px(TEXT_WIDTH + 40.))
        .flex()
        .flex_col()
        .child(
            div()
                .w_0()
                .min_w_full()
                .text_size(px(15.))
                .line_height(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(confirmation.title),
        )
        .when(!confirmation.message.is_empty(), |content| {
            content.child(
                div()
                    .w_0()
                    .min_w_full()
                    .pt(px(6.))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .text_color(c.muted_foreground)
                    .child(confirmation.message),
            )
        })
        .child(div().pt(px(22.)).flex().justify_end().gap(px(8.)).children(buttons));
    let on_close = close.clone();
    modal(id, content, move |window, cx| on_close(window, cx)).on_return(move |window, cx| on_return(window, cx))
}

/// The alert of an error: its first sentence as the title when it is short, and the rest below
/// it; otherwise "Something went wrong" over all of it.
pub fn error_alert(message: &str) -> (String, String) {
    let mut sentences = message.splitn(2, ". ");
    let first = sentences.next().unwrap_or_default();
    if first.is_empty() || first.chars().count() > 60 || first.contains('\n') {
        return ("Something went wrong".into(), message.into());
    }
    let title = first.strip_suffix('.').unwrap_or(first);
    (title.into(), sentences.next().unwrap_or_default().into())
}

#[cfg(test)]
mod tests {
    use super::error_alert;

    #[test]
    fn an_error_is_titled_by_its_first_short_sentence() {
        assert_eq!(
            error_alert("Your server isn't connected. Try again once it is."),
            ("Your server isn't connected".into(), "Try again once it is.".into())
        );
        assert_eq!(error_alert("The branch is gone."), ("The branch is gone".into(), String::new()));
        let long = "This sentence goes on for longer than sixty characters before it ends. Then more.";
        assert_eq!(error_alert(long), ("Something went wrong".into(), long.into()));
        assert_eq!(error_alert("A line\nand another"), ("Something went wrong".into(), "A line\nand another".into()));
    }
}
