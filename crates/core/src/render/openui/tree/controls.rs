//! The Inputs group (`controls.tsx`): buttons, fields and choices with the values they show.

use std::sync::LazyLock;

use motile_openui::format::{self, DateStyle, DateTime, NumberFormat};
use motile_openui::{ActionStep, Element, Value, js};
use regress::Regex;

use super::color;
use super::resolve::{Cx, Pass, block_radius, flag, items, number, radius, steps, str_of, string};
use super::*;

static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("a valid pattern"));
static CURRENCY_FIRST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[$€£¥₹]").expect("a valid pattern"));

/// `toLocaleString("en-US", { maximumFractionDigits: 2 })`.
pub(super) fn number_text(value: f64) -> String {
    NumberFormat::decimal(0, 2).format(value)
}

/// A field's value as text: `String(value ?? "")`.
fn shown(value: Option<&Value>) -> String {
    value.filter(|value| !value.is_null()).map(js::to_string).unwrap_or_default()
}

fn label(element: &Element) -> Option<String> {
    str_of(element, "label").filter(|label| !label.is_empty()).map(str::to_owned)
}

fn choices(value: Option<&Value>) -> Vec<Choice> {
    items(value)
        .into_iter()
        .map(|(_, option)| match option {
            Value::Object(fields) => {
                let field = |key: &str| fields.get(key).filter(|value| !value.is_null()).map(js::to_string);
                Choice {
                    value: field("value").unwrap_or_default(),
                    label: field("label").unwrap_or_default(),
                    description: field("description").filter(|description| !description.is_empty()),
                }
            }
            option => {
                let text = js::to_string(option);
                Choice { value: text.clone(), label: text, description: None }
            }
        })
        .collect()
}

fn selected(choices: &[Choice], value: &str) -> Option<u32> {
    choices.iter().position(|choice| choice.value == value).map(|index| index as u32)
}

/// `YYYY-MM-DD`, or none.
fn date(value: Option<&str>) -> Option<String> {
    value.filter(|value| DATE.find(value).is_some()).map(str::to_owned)
}

/// A countdown's time: `m:ss`, `h:mm:ss` from an hour.
fn clock(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

/// Whether a press runs a tool.
fn runs_tools(element: &Element) -> bool {
    element
        .prop("action")
        .and_then(Value::as_plan)
        .is_some_and(|plan| plan.steps.iter().any(|step| matches!(step, ActionStep::Run { .. })))
}

impl Pass<'_, '_> {
    /// Why a control takes no presses, if it doesn't.
    fn disabled(&self, element: &Element, program: bool) -> Option<Disabled> {
        if program {
            Some(Disabled::Program)
        } else if self.ctx.streaming {
            Some(Disabled::Streaming)
        } else if runs_tools(element) && !self.ctx.tools {
            Some(Disabled::NoTools)
        } else {
            None
        }
    }

    pub(super) fn button(&mut self, element: &Element) -> UiKind {
        let tone = match string(element, "color").as_str() {
            "danger" => Some("destructive"),
            "success" => Some("success"),
            _ => None,
        };
        let variant = match string(element, "variant").as_str() {
            "outline" => ButtonVariant::Outline,
            "soft" => ButtonVariant::Secondary,
            "ghost" => ButtonVariant::Ghost,
            _ => match tone {
                Some("destructive") => ButtonVariant::Danger,
                Some(_) => ButtonVariant::Success,
                None => ButtonVariant::Primary,
            },
        };
        let solid = matches!(variant, ButtonVariant::Primary | ButtonVariant::Danger | ButtonVariant::Success);
        let size = match string(element, "size").as_str() {
            "sm" => ControlSize::Small,
            "lg" => ControlSize::Large,
            _ => ControlSize::Regular,
        };
        UiKind::Button {
            label: string(element, "label"),
            variant,
            tint: tone.filter(|_| !solid).map(str::to_owned),
            size,
            icon: str_of(element, "icon").filter(|icon| !icon.is_empty()).and_then(glyph),
            block: flag(element, "block"),
            disabled: self.disabled(element, flag(element, "disabled")),
        }
    }

    pub(super) fn icon_button(&mut self, element: &Element) -> UiKind {
        let tooltip = label(element);
        let variant = match string(element, "variant").as_str() {
            "ghost" => ButtonVariant::Ghost,
            "solid" => ButtonVariant::Primary,
            _ => ButtonVariant::Outline,
        };
        UiKind::IconButton {
            icon: glyph(&string(element, "icon")),
            label: tooltip.clone().unwrap_or_else(|| string(element, "icon")),
            tooltip,
            variant,
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn pressable(&mut self, element: &Element, cx: Cx) -> UiKind {
        let background_value = str_of(element, "background");
        let background = background_value.and_then(|value| color::background(value, cx.ink));
        let inner_surface = background.as_ref().is_some_and(color::is_theme);
        let ink = background_value.and_then(color::ink_on).or(cx.ink);
        let items = items(element.prop("children"));
        let row_inside = items.iter().any(|(_, value)| {
            value.as_element().is_some_and(|child| {
                child.component == "Row" || (child.component == "Box" && string(child, "direction") == "row")
            })
        });
        let inner = Cx { ink, cell: false, typography: false, media: false, part: super::resolve::Part::Plain, ..cx };
        let children = self.items("children", &items, |_| inner);
        let selected = element.prop("selected").map(js::truthy);
        UiKind::Pressable {
            children,
            padding: steps(element.prop("padding")).unwrap_or(0.0),
            background,
            inner_surface,
            radius: element.prop("radius").and_then(radius).unwrap_or(block_radius(cx, Radius::LG)),
            selected,
            ring: selected == Some(true) && !row_inside,
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn input(&mut self, element: &Element) -> UiKind {
        let value = self.value(element, "value");
        UiKind::Input {
            value: shown(value.as_deref()),
            number: string(element, "type") == "number",
            label: label(element),
            placeholder: str_of(element, "placeholder").map(str::to_owned),
            error: self.error(),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn text_area(&mut self, element: &Element) -> UiKind {
        let value = self.value(element, "value");
        UiKind::TextArea {
            value: shown(value.as_deref()),
            label: label(element),
            placeholder: str_of(element, "placeholder").map(str::to_owned),
            rows: js::round(number(element, "rows").unwrap_or(3.0)).clamp(2.0, 20.0) as u32,
            error: self.error(),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn select(&mut self, element: &Element) -> UiKind {
        let value = shown(self.value(element, "value").as_deref());
        let options = choices(element.prop("options"));
        UiKind::Select {
            selected: selected(&options, &value),
            value,
            options,
            label: label(element),
            placeholder: str_of(element, "placeholder").map_or_else(|| "Choose…".to_owned(), str::to_owned),
            error: self.error(),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn segmented(&mut self, element: &Element) -> UiKind {
        let value = shown(self.value(element, "value").as_deref());
        let options = choices(element.prop("options"));
        UiKind::SegmentedControl {
            selected: selected(&options, &value),
            value,
            options,
            block: flag(element, "block"),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn slider(&mut self, element: &Element) -> UiKind {
        let min = number(element, "min").unwrap_or(0.0);
        let max = number(element, "max").filter(|max| *max > min).unwrap_or(min + 100.0);
        let step = number(element, "step").filter(|step| *step > 0.0).unwrap_or(1.0);
        let raw = self.value(element, "value").map_or(f64::NAN, |value| js::to_number(&value));
        let value = if raw.is_finite() { raw.clamp(min, max) } else { min };
        let unit = string(element, "unit");
        let readout = if CURRENCY_FIRST.find(&unit).is_some() {
            format!("{unit}{}", number_text(value))
        } else {
            format!("{}{unit}", number_text(value))
        };
        let marks = items(element.prop("marks"))
            .into_iter()
            .filter_map(|(_, mark)| {
                let at_value = mark.get("value").and_then(Value::as_number).filter(|value| value.is_finite())?;
                if !(min..=max).contains(&at_value) {
                    return None;
                }
                let at = (at_value - min) / (max - min);
                let edge = if at <= 0.02 {
                    MarkEdge::Start
                } else if at >= 0.98 {
                    MarkEdge::End
                } else {
                    MarkEdge::Middle
                };
                Some(SliderMark { at, label: mark.get("label").map(js::to_string).unwrap_or_default(), edge })
            })
            .collect();
        UiKind::Slider {
            at: (value - min) / (max - min),
            value,
            min,
            max,
            step,
            label: label(element),
            head: flag(element, "label") || flag(element, "unit"),
            readout,
            marks,
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn checkbox(&mut self, element: &Element) -> UiKind {
        let checked = self.value(element, "checked").is_some_and(|value| matches!(*value, Value::Bool(true)));
        UiKind::Checkbox {
            checked,
            label: string(element, "label"),
            strike: checked && flag(element, "lineThrough"),
            error: self.error(),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn radio_group(&mut self, element: &Element, cx: Cx) -> UiKind {
        let value = shown(self.value(element, "value").as_deref());
        let options = choices(element.prop("options"));
        let variant = match string(element, "variant").as_str() {
            "pills" => RadioVariant::Pills,
            "cards" => RadioVariant::Cards,
            _ => RadioVariant::List,
        };
        UiKind::RadioGroup {
            selected: selected(&options, &value),
            value,
            options,
            label: label(element),
            variant,
            error: self.error(),
            disabled: self.disabled(element, false),
            radius: block_radius(cx, Radius::LG),
        }
    }

    pub(super) fn date_picker(&mut self, element: &Element) -> UiKind {
        let value = self.value(element, "value");
        let value = date(value.as_deref().and_then(Value::as_str));
        let text = value
            .as_deref()
            .and_then(|value| format::parse_iso(value, |_| 0.0))
            .map_or_else(|| "Date".to_owned(), |ms| format::format_date(&DateTime::from_ms(ms), DateStyle::Date));
        UiKind::DatePicker {
            value,
            text,
            min: date(str_of(element, "min")),
            max: date(str_of(element, "max")),
            label: label(element),
            error: self.error(),
            disabled: self.disabled(element, false),
        }
    }

    pub(super) fn timer(&mut self, element: &Element, cx: Cx) -> UiKind {
        let raw = self.value(element, "seconds").map_or(f64::NAN, |value| js::to_number(&value));
        let left = if raw.is_finite() { raw.floor().max(0.0) } else { 0.0 };
        let widgets = self.widgets();
        let streaming = self.ctx.streaming.then_some(Disabled::Streaming);
        let toggle_label = if widgets.running {
            "Pause"
        } else if widgets.started.is_none() || left == 0.0 {
            "Start"
        } else {
            "Resume"
        };
        let icon = if widgets.running { "pause" } else { "play" };
        UiKind::Timer {
            label: label(element),
            time: clock(left as u64),
            seconds: left,
            running: widgets.running,
            done: left == 0.0 && widgets.started.is_some(),
            toggle: TimerButton {
                label: toggle_label.to_owned(),
                icon: glyph(icon),
                disabled: streaming.or((left == 0.0).then_some(Disabled::Program)),
            },
            reset: TimerButton {
                label: "Reset".to_owned(),
                icon: glyph("rotate-ccw"),
                disabled: streaming.or(widgets.started.is_none().then_some(Disabled::Program)),
            },
            radius: block_radius(cx, Radius::LG),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_in_minutes_and_hours() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(95), "1:35");
        assert_eq!(clock(3725), "1:02:05");
    }

    #[test]
    fn formats_numbers_as_en_us_with_two_decimals_at_most() {
        assert_eq!(number_text(1234.567), "1,234.57");
        assert_eq!(number_text(-0.5), "-0.5");
        assert_eq!(number_text(3.0), "3");
    }
}
