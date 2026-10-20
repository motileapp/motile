//! The inputs: Button, IconButton, Pressable, Input, TextArea, Select, SegmentedControl, Slider,
//! Checkbox, RadioGroup, DatePicker, Timer and Form, from the kit. What the user does goes to core;
//! while a field is typed in or a slider dragged, the control keeps its own value and core hears
//! of it no more than every `THROTTLE`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::{
    Align, ButtonVariant, Choice, ControlSize as UiSize, RadioVariant, TimerButton, UiKind, UiNode,
};
use serde_json::Value;

use super::layout::column;

/// A text area's line, and the room above and below its lines, as LAYOUT.md has them.
const TEXT_AREA_LINE: f32 = 18.;
const TEXT_AREA_ROOM: f32 = 19.;
use super::style::{self, CAPTION, LABEL, Look};
use super::text::rich;
use super::{UiCx, framed, node};
use crate::theme::{ControlSize, Layer, Opacity, Radius, colors};
use crate::ui::{
    ActionButton, InputField, OptionCard, PickMark, PickRow, Segmented, Select, Slider, TextArea, Variant,
    text_area_state,
};

const THROTTLE: Duration = Duration::from_millis(100);
const FIELD_LABEL_GAP: f32 = 8.;
const FIELD_ERROR_GAP: f32 = 4.;
const RADIO_LABEL_GAP: f32 = 10.;
const RADIO_ROW_GAP: f32 = 2.;
const CHOICE_GAP: f32 = 8.;
const CARD_CHOICE: f32 = 200.;
const PICKED_MARK_INSET: f32 = 8.;
const TIMER_LABEL_GAP: f32 = 2.;

pub fn input(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = div().id(cx.id(&node.id, "node"));
    let id = node.id.as_str();
    match &node.kind {
        UiKind::Button { label, variant, tint, size, icon, block, disabled } => {
            let c = colors(app);
            let tint = tint.as_deref().and_then(|tint| c.named(tint));
            let (sender, node_id) = (cx.clone(), id.to_string());
            let button = ActionButton::new(cx.id(id, "button"), label.clone())
                .variant(variant_of(*variant))
                .size(size_of(*size))
                .surface(cx.surface)
                .tint(tint)
                .when_some(icon.as_ref(), |button, icon| button.symbol(icon.name.clone()))
                .fills(*block)
                .disabled(disabled.is_some())
                .on_click(move |_, _, cx| sender.send(&node_id, UiEvent::Press, cx));
            element.flex_none().child(button)
        }
        UiKind::IconButton { icon, label, tooltip, variant, disabled } => {
            let symbol = icon.as_ref().map_or("circle", |icon| icon.name.as_str()).to_string();
            let help = tooltip.clone().unwrap_or_else(|| label.clone());
            let (sender, node_id) = (cx.clone(), id.to_string());
            let button = ActionButton::icon(cx.id(id, "button"), symbol, help)
                .variant(variant_of(*variant))
                .size(ControlSize::Regular)
                .surface(cx.surface)
                .disabled(disabled.is_some())
                .on_click(move |_, _, cx| sender.send(&node_id, UiEvent::Press, cx));
            element.flex_none().child(button)
        }
        UiKind::Pressable { .. } => pressable(element, node, cx, window, app),
        UiKind::Input { value, number, label, placeholder, error, disabled } => {
            let field = field(id, value, *number, placeholder.as_deref(), disabled.is_some(), cx, window, app);
            let input = InputField::new(&field).surface(cx.surface).size(ControlSize::Regular);
            labelled(element, label.as_deref(), input, error.as_deref(), disabled.is_some(), cx, app)
        }
        UiKind::TextArea { value, label, placeholder, rows, error, disabled } => {
            let area = area(id, value, placeholder.as_deref(), *rows, cx, window, app);
            let lines = (*rows).clamp(2, 20) as f32;
            let control = div()
                .h(px(lines * TEXT_AREA_LINE + TEXT_AREA_ROOM))
                .child(TextArea::new(&area).fills(true).surface(cx.surface));
            labelled(element, label.as_deref(), control, error.as_deref(), disabled.is_some(), cx, app)
        }
        UiKind::Select { options, selected, label, placeholder, error, disabled, .. } => {
            let labels = options.iter().map(|option| SharedString::from(option.label.clone())).collect();
            let (sender, node_id, values) = (cx.clone(), id.to_string(), values(options));
            let select = Select::new(cx.id(id, "select"), labels, selected.map(|index| index as usize))
                .placeholder(placeholder.clone())
                .surface(cx.surface)
                .disabled(disabled.is_some())
                .on_select(move |index, _, cx| choose(&sender, &node_id, &values, index, cx));
            labelled(element, label.as_deref(), select, error.as_deref(), false, cx, app)
        }
        UiKind::SegmentedControl { options, selected, block, disabled, .. } => {
            let labels = options.iter().map(|option| SharedString::from(option.label.clone())).collect();
            let (sender, node_id, values) = (cx.clone(), id.to_string(), values(options));
            let control =
                Segmented::new(cx.id(id, "segmented"), labels, selected.map_or(usize::MAX, |index| index as usize))
                    .fills(*block)
                    .disabled(disabled.is_some())
                    .on_select(move |index, _, cx| choose(&sender, &node_id, &values, index, cx));
            match block {
                true => element.child(control),
                false => element.max_w_full().child(
                    div()
                        .id(cx.id(id, "scroll"))
                        .max_w_full()
                        .overflow_x_scroll()
                        .child(div().flex_none().child(control)),
                ),
            }
        }
        UiKind::Slider { value, min, max, step, label, readout, head, marks, disabled, .. } => {
            let throttle = Throttle::of(cx.id(id, "throttle"), window, app);
            let (sender, node_id, ends) = (cx.clone(), id.to_string(), throttle.clone());
            let (sender_end, node_end) = (cx.clone(), id.to_string());
            let marks = marks.iter().map(|mark| (min + mark.at * (max - min), SharedString::from(mark.label.clone())));
            let slider = Slider::new(cx.id(id, "slider"), *value, *min, *max)
                .step(Some(*step))
                .surface(cx.surface)
                .marks(marks.collect())
                .disabled(disabled.is_some())
                .when(*head, |slider| {
                    let slider = match label {
                        Some(label) => slider.label(label.clone()),
                        None => slider,
                    };
                    slider.readout(readout.clone())
                })
                .on_change(move |value, _, cx| {
                    throttle.offer(Value::from(value), cx, {
                        let (sender, node_id) = (sender.clone(), node_id.clone());
                        move |value, cx| sender.send(&node_id, UiEvent::Value { value }, cx)
                    });
                })
                .on_editing(move |editing, _, cx| {
                    if !editing {
                        ends.flush(cx, |value, cx| sender_end.send(&node_end, UiEvent::Value { value }, cx));
                    }
                });
            element.w_full().child(slider)
        }
        UiKind::Checkbox { checked, label, strike, error, disabled } => {
            let (sender, node_id, checked) = (cx.clone(), id.to_string(), *checked);
            let row = PickRow::new(cx.id(id, "check"), label.clone(), checked, true)
                .struck(checked && *strike)
                .disabled(disabled.is_some())
                .on_press(move |_, cx| sender.send(&node_id, UiEvent::Value { value: Value::Bool(!checked) }, cx));
            element.flex().flex_col().child(row).children(error_text(error.as_deref(), app))
        }
        UiKind::RadioGroup { options, selected, label, variant, error, disabled, .. } => {
            let choices = radio(id, options, *selected, *variant, disabled.is_some(), cx);
            let label = label.as_ref().map(|label| {
                rich(cx.id(id, "label"), &plain(label), Look::new(LABEL, colors(app).foreground), cx, app)
                    .mb(px(RADIO_LABEL_GAP))
            });
            element.flex().flex_col().children(label).child(choices).children(error_text(error.as_deref(), app))
        }
        UiKind::DatePicker { value, text, min, max, label, error, disabled } => {
            let day = |text: &Option<String>| {
                text.as_deref().and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
            };
            let (sender, node_id) = (cx.clone(), id.to_string());
            let picked = day(value);
            let field = crate::ui::DateField::new(cx.id(id, "date"), picked)
                .when(picked.is_some(), |field| field.text(text.clone()))
                .placeholder(text.clone())
                .when_some(label.clone(), |field, label| field.label(label))
                .range(day(min), day(max))
                .surface(cx.surface)
                .disabled(disabled.is_some())
                .on_pick(move |day, _, cx| {
                    let value = Value::String(day.format("%Y-%m-%d").to_string());
                    sender.send(&node_id, UiEvent::Value { value }, cx)
                });
            labelled(element, label.as_deref(), field, error.as_deref(), false, cx, app)
        }
        UiKind::Timer { label, time, done, toggle, reset, running, .. } => {
            timer(element, id, label.as_deref(), time, *done, *running, toggle, reset, cx, app)
        }
        UiKind::Form { children, spacing } => {
            column(element, children, Align::Stretch, spacing, false, cx, window, app)
        }
        _ => element,
    }
}

fn plain(text: &str) -> motile_core::render::openui::tree::Rich {
    motile_core::render::openui::tree::Rich { text: text.to_string(), ..Default::default() }
}

fn variant_of(variant: ButtonVariant) -> Variant {
    match variant {
        ButtonVariant::Primary => Variant::Primary,
        ButtonVariant::Danger => Variant::Danger,
        ButtonVariant::Success => Variant::Success,
        ButtonVariant::Outline => Variant::Outline,
        ButtonVariant::Secondary => Variant::Secondary,
        ButtonVariant::Ghost => Variant::Ghost,
    }
}

fn size_of(size: UiSize) -> ControlSize {
    match size {
        UiSize::Small => ControlSize::Small,
        UiSize::Regular => ControlSize::Regular,
        UiSize::Large => ControlSize::Large,
    }
}

fn values(options: &[Choice]) -> Rc<Vec<String>> {
    Rc::new(options.iter().map(|option| option.value.clone()).collect())
}

fn choose(cx: &UiCx, node: &str, values: &[String], index: usize, app: &mut App) {
    let Some(value) = values.get(index) else { return };
    cx.send(node, UiEvent::Value { value: Value::String(value.clone()) }, app);
}

fn error_text(error: Option<&str>, app: &App) -> Option<Div> {
    let error = error?;
    Some(CAPTION.text_box().mt(px(FIELD_ERROR_GAP)).text_color(colors(app).destructive).child(error.to_string()))
}

/// A field with its label over it and its error under it.
#[allow(clippy::too_many_arguments)]
fn labelled(
    element: Stateful<Div>,
    label: Option<&str>,
    control: impl IntoElement,
    error: Option<&str>,
    disabled: bool,
    cx: &UiCx,
    app: &App,
) -> Stateful<Div> {
    let c = colors(app);
    let label = label.map(|label| {
        rich(cx.id(label, "field-label"), &plain(label), Look::new(LABEL, c.foreground), cx, app)
            .mb(px(FIELD_LABEL_GAP))
    });
    element
        .w_full()
        .flex()
        .flex_col()
        .children(label)
        .child(
            div()
                .relative()
                .w_full()
                .when(disabled, |field| field.opacity(c.opacity(Opacity::Disabled)))
                .child(control)
                .when(disabled, |field| field.child(div().absolute().inset_0().occlude())),
        )
        .children(error_text(error, app))
}

/// What a control offered core last, and when: a value offered sooner than `THROTTLE` after the
/// last waits, and goes when the time is up or the editing ends.
struct Throttled {
    sent: Option<Instant>,
    waiting: Option<Value>,
    timer: Option<Task<()>>,
}

#[derive(Clone)]
struct Throttle(Entity<Throttled>);

impl Throttle {
    fn of(id: ElementId, window: &mut Window, app: &mut App) -> Self {
        Self(window.use_keyed_state(id, app, |_, _| Throttled { sent: None, waiting: None, timer: None }))
    }

    fn offer(&self, value: Value, app: &mut App, send: impl Fn(Value, &mut App) + 'static) {
        let due = self.0.read(app).sent.is_none_or(|sent| sent.elapsed() >= THROTTLE);
        if due {
            self.0.update(app, |throttled, _| {
                throttled.sent = Some(Instant::now());
                throttled.waiting = None;
            });
            send(value, app);
            return;
        }
        let waits = self.0.read(app).timer.is_some();
        self.0.update(app, |throttled, _| throttled.waiting = Some(value));
        if waits {
            return;
        }
        let entity = self.0.clone();
        let left = THROTTLE.saturating_sub(self.0.read(app).sent.map_or(Duration::ZERO, |sent| sent.elapsed()));
        let timer = app.spawn(async move |cx| {
            cx.background_executor().timer(left).await;
            cx.update(|cx| {
                let value = entity.update(cx, |throttled, _| {
                    throttled.timer = None;
                    throttled.sent = Some(Instant::now());
                    throttled.waiting.take()
                });
                if let Some(value) = value {
                    send(value, cx);
                }
            });
        });
        self.0.update(app, |throttled, _| throttled.timer = Some(timer));
    }

    fn flush(&self, app: &mut App, send: impl Fn(Value, &mut App)) {
        let value = self.0.update(app, |throttled, _| {
            throttled.timer = None;
            throttled.sent = Some(Instant::now());
            throttled.waiting.take()
        });
        if let Some(value) = value {
            send(value, app);
        }
    }
}

/// A line's state: what is typed, kept while it is typed in, and what core last said it is.
struct Field {
    state: Entity<InputState>,
    shown: String,
    throttle: Option<Throttle>,
    sink: Option<(UiCx, String, bool)>,
    _changes: Subscription,
}

#[allow(clippy::too_many_arguments)]
fn field(
    id: &str,
    value: &str,
    number: bool,
    placeholder: Option<&str>,
    disabled: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Entity<InputState> {
    let placeholder = placeholder.unwrap_or_default().to_string();
    let kept = window.use_keyed_state(cx.id(id, "field"), app, |window, field_cx| {
        let state = field_cx.new(|state_cx| InputState::new(window, state_cx).placeholder(placeholder.clone()));
        let changes = field_cx.subscribe(&state, |field: &mut Field, state, event: &InputEvent, field_cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let text = state.read(field_cx).value().to_string();
            if text == field.shown {
                return;
            }
            field.shown = text.clone();
            let (Some((ui, node, number)), Some(throttle)) = (field.sink.clone(), field.throttle.clone()) else {
                return;
            };
            let Some(value) = typed(&text, number) else { return };
            field_cx.defer(move |app| {
                throttle.offer(value, app, move |value, app| ui.send(&node, UiEvent::Value { value }, app));
            });
        });
        Field { state, shown: String::new(), throttle: None, sink: None, _changes: changes }
    });
    let throttle = Throttle::of(cx.id(id, "throttle"), window, app);
    let state = kept.read(app).state.clone();
    let focused = state.read(app).focus_handle(app).is_focused(window);
    if !focused && typed(&kept.read(app).shown, number) != typed(value, number) {
        let value = value.to_string();
        kept.update(app, |field, _| field.shown = value.clone());
        state.update(app, |state, state_cx| state.set_value(value, window, state_cx));
    }
    let sink = (!disabled).then(|| (cx.clone(), id.to_string(), number));
    kept.update(app, |field, _| {
        field.sink = sink;
        field.throttle = Some(throttle);
    });
    state
}

/// What a field's text says to core: the text, or for a number field a number once it reads as
/// one, commas dropped, and nothing while it is empty.
fn typed(text: &str, number: bool) -> Option<Value> {
    if !number {
        return Some(Value::String(text.to_string()));
    }
    let digits: String = text.chars().filter(|character| *character != ',').collect();
    if digits.trim().is_empty() {
        return Some(Value::Null);
    }
    digits.trim().parse::<f64>().ok().filter(|number| number.is_finite()).map(Value::from)
}

struct Area {
    state: Entity<TextareaState>,
    shown: String,
    throttle: Option<Throttle>,
    sink: Option<(UiCx, String)>,
    _changes: Subscription,
}

fn area(
    id: &str,
    value: &str,
    placeholder: Option<&str>,
    rows: u32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Entity<TextareaState> {
    let placeholder = placeholder.unwrap_or_default().to_string();
    let lines = rows.clamp(2, 20) as usize;
    let kept = window.use_keyed_state(cx.id(id, "area"), app, |window, area_cx| {
        let state = text_area_state(placeholder.clone(), lines, lines, window, area_cx);
        let changes = area_cx.subscribe(&state, |area: &mut Area, state, event: &InputEvent, area_cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let text = state.read(area_cx).value().to_string();
            if text == area.shown {
                return;
            }
            area.shown = text.clone();
            let (Some((ui, node)), Some(throttle)) = (area.sink.clone(), area.throttle.clone()) else { return };
            area_cx.defer(move |app| {
                throttle
                    .offer(Value::String(text), app, move |value, app| ui.send(&node, UiEvent::Value { value }, app));
            });
        });
        Area { state, shown: String::new(), throttle: None, sink: None, _changes: changes }
    });
    let throttle = Throttle::of(cx.id(id, "throttle"), window, app);
    let state = kept.read(app).state.clone();
    let focused = state.read(app).focus_handle(app).is_focused(window);
    if !focused && kept.read(app).shown != value {
        let value = value.to_string();
        kept.update(app, |area, _| area.shown = value.clone());
        state.update(app, |state, state_cx| state.set_value(value, window, state_cx));
    }
    let sink = Some((cx.clone(), id.to_string()));
    kept.update(app, |area, _| {
        area.sink = sink;
        area.throttle = Some(throttle);
    });
    state
}

#[allow(clippy::too_many_arguments)]
fn radio(id: &str, options: &[Choice], selected: Option<u32>, variant: RadioVariant, disabled: bool, cx: &UiCx) -> Div {
    let values = values(options);
    let press = |index: usize| {
        let (sender, node_id, values) = (cx.clone(), id.to_string(), values.clone());
        move |cx: &mut App| choose(&sender, &node_id, &values, index, cx)
    };
    let chosen = |index: usize| selected == Some(index as u32);
    match variant {
        RadioVariant::List => div().flex().flex_col().gap(px(RADIO_ROW_GAP)).children(options.iter().enumerate().map(
            |(index, option)| {
                let press = press(index);
                PickRow::new(cx.id(id, &format!("option-{index}")), option.label.clone(), chosen(index), false)
                    .when_some(option.description.clone(), |row, description| row.detail(description))
                    .disabled(disabled)
                    .on_press(move |_, cx| press(cx))
            },
        )),
        RadioVariant::Pills => div().flex().flex_row().flex_wrap().gap(px(CHOICE_GAP)).children(
            options.iter().enumerate().map(|(index, option)| {
                let press = press(index);
                ActionButton::new(cx.id(id, &format!("option-{index}")), option.label.clone())
                    .variant(if chosen(index) { Variant::Accent } else { Variant::Secondary })
                    .surface(cx.surface)
                    .disabled(disabled)
                    .on_click(move |_, _, cx| press(cx))
            }),
        ),
        RadioVariant::Cards => {
            let count = (((cx.width + CHOICE_GAP) / (CARD_CHOICE + CHOICE_GAP)).floor() as usize).max(1);
            let cards: Vec<AnyElement> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let press = press(index);
                    OptionCard::new(cx.id(id, &format!("option-{index}")), option.label.clone(), chosen(index))
                        .when_some(option.description.clone(), |card, description| card.detail(description))
                        .surface(cx.surface)
                        .disabled(disabled)
                        .on_press(move |_, cx| press(cx))
                        .into_any_element()
                })
                .collect();
            let mut lines = div().flex().flex_col().gap(px(CHOICE_GAP));
            let mut cards = cards.into_iter().peekable();
            while cards.peek().is_some() {
                let mut line = div().flex().flex_row().items_stretch().gap(px(CHOICE_GAP));
                for _ in 0..count {
                    let cell = div().flex_basis(px(0.)).flex_grow(1.).min_w_0().flex();
                    line = line.child(match cards.next() {
                        Some(card) => cell.child(card),
                        None => cell,
                    });
                }
                lines = lines.child(line);
            }
            lines
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn timer(
    element: Stateful<Div>,
    id: &str,
    label: Option<&str>,
    time: &str,
    done: bool,
    running: bool,
    toggle: &TimerButton,
    reset: &TimerButton,
    cx: &UiCx,
    app: &App,
) -> Stateful<Div> {
    let c = colors(app);
    let clock = style::type_of(motile_core::render::openui::tree::TextStyle::Timer, cx.narrow());
    let read = div()
        .flex()
        .flex_col()
        .gap(px(TIMER_LABEL_GAP))
        .children(label.map(|label| LABEL.text_box().text_color(c.foreground).child(label.to_string())))
        .child(clock.text_box().text_color(if done { c.success } else { c.foreground }).child(time.to_string()));
    let button = |button: &TimerButton, primary: bool, event: UiEvent| {
        let (sender, node_id) = (cx.clone(), id.to_string());
        ActionButton::new(cx.id(id, if primary { "toggle" } else { "reset" }), button.label.clone())
            .size(ControlSize::Small)
            .variant(if primary { Variant::Primary } else { Variant::Ghost })
            .surface(cx.surface)
            .when_some(button.icon.as_ref(), |action, icon| action.symbol(icon.name.clone()))
            .disabled(button.disabled.is_some())
            .on_click(move |_, _, cx| sender.send(&node_id, event.clone(), cx))
    };
    let toggle_event = if running { UiEvent::Pause } else { UiEvent::Start };
    element
        .w_full()
        .py(px(12.))
        .px(px(16.))
        .rounded(px(Radius::LG))
        .border_1()
        .border_color(cx.surface.border(c))
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .child(read)
        .child(div().flex().flex_row().gap(px(8.)).child(button(toggle, true, toggle_event)).child(button(
            reset,
            false,
            UiEvent::Reset,
        )))
}

fn pressable(element: Stateful<Div>, item: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let UiKind::Pressable { children, padding, background, inner_surface, radius, selected, ring, disabled } =
        &item.kind
    else {
        return element;
    };
    let c = colors(app);
    let radius = style::radius(radius);
    let inner = if *inner_surface { cx.surface.in_box() } else { cx.surface };
    let fill = background.as_ref().map(|background| style::color(background, cx, app));
    let rest = match (fill, selected) {
        (Some(fill), _) => fill,
        (None, Some(true)) => cx.surface.layer(Layer::RowSelected, c),
        (None, _) => transparent_black(),
    };
    let ground = (fill.is_some() || *selected == Some(true)).then_some(rest);
    let inner_cx = cx.on(inner).over(ground).within(cx.width - 2. * padding);
    let laid: Vec<AnyElement> = children
        .iter()
        .map(|child| {
            let laid = node(child, &inner_cx, window, app)
                .when(super::layout::keeps_width(&child.kind), |child| child.self_start());
            framed(&child.id, laid)
        })
        .collect();
    let (lit, pressed) = match fill {
        Some(_) => (c.wash(c.foreground, false), c.wash(c.foreground, true)),
        None => (cx.surface.layer(Layer::Row, c), cx.surface.layer(Layer::RowSelected, c)),
    };
    let wash = fill.is_some();
    let (sender, node_id) = (cx.clone(), item.id.clone());
    let (keys, key_id) = (cx.clone(), item.id.clone());
    let enabled = disabled.is_none();
    let focus = crate::ui::focus_state(&cx.id(&item.id, "press"), window, app);
    let ring_shown = enabled && crate::ui::focus_shown(&focus, window);
    let overlay = |color: Hsla| div().absolute().inset_0().rounded(px(radius)).bg(color);
    let element = element
        .relative()
        .p(px(*padding))
        .rounded(px(radius))
        .bg(rest)
        .flex()
        .flex_col()
        .when(!enabled, |element| element.opacity(c.opacity(Opacity::Disabled)))
        .when(enabled, |element| {
            element
                .cursor_pointer()
                .when(!wash, |element| element.hover(move |style| style.bg(lit)).active(move |style| style.bg(pressed)))
                .track_focus(&focus)
                .on_click(move |_, _, cx| sender.send(&node_id, UiEvent::Press, cx))
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if !matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        return;
                    }
                    cx.stop_propagation();
                    keys.send(&key_id, UiEvent::Press, cx)
                })
        })
        .when(enabled && wash, |element| {
            element.child(
                div()
                    .id(cx.id(&item.id, "wash"))
                    .absolute()
                    .inset_0()
                    .rounded(px(radius))
                    .hover(move |style| style.bg(lit))
                    .active(move |style| style.bg(pressed)),
            )
        })
        .children(laid);
    style::keep_corners(element, children, radius, *padding, None, cx, app)
        .when(*ring, |element| {
            element.child(overlay(transparent_black()).border_1().border_color(c.border_accent)).child(
                div().absolute().top(px(PICKED_MARK_INSET)).right(px(PICKED_MARK_INSET)).child(PickMark::new(
                    cx.id(&item.id, "mark"),
                    true,
                    false,
                )),
            )
        })
        .when(ring_shown, |element| element.child(crate::ui::focus_ring(radius, app)))
}
