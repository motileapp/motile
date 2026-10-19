//! A chart's legend, which the client lays out (LAYOUT.md's Chart): under the plot, wrapping, or a
//! pie's column of slices and their shares beside its disc or under it.

use std::sync::Arc;

use gpui_kit::*;
use motile_core::render::openui::canvas::{Color, Legend, Scene};

use super::paint::Palette;
use crate::theme::UI_FONT;
use crate::transcript::rows::font;
use crate::ui::tabular_numbers;

const DOT: f32 = 8.;
const UNDER_PLOT: f32 = 10.;
const ITEM_GAP: f32 = 16.;
const LINE_GAP: f32 = 6.;
const DOT_GAP: f32 = 7.;
const PIE_GAP: f32 = 24.;
const ROW_GAP: f32 = 9.;
const PIE_DOT_GAP: f32 = 8.;
const SHARE_GAP: f32 = 8.;

/// The legend's text styles: a chart's labels, a pie's labels, and a pie's shares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LegendText {
    Caption,
    Small,
    Share,
}

impl LegendText {
    pub fn size(self) -> f32 {
        match self {
            LegendText::Caption => 12.,
            LegendText::Small | LegendText::Share => 13.,
        }
    }

    pub fn line_height(self) -> f32 {
        match self {
            LegendText::Caption => 18.,
            LegendText::Small | LegendText::Share => 20.,
        }
    }

    pub fn font(self) -> Font {
        match self {
            LegendText::Caption | LegendText::Small => font(UI_FONT, FontWeight::NORMAL, false),
            LegendText::Share => Font { features: tabular_numbers(), ..font(UI_FONT, FontWeight::MEDIUM, false) },
        }
    }
}

/// How wide text is set and where it breaks, on any thread.
pub trait Measure {
    fn width(&self, text: &str, style: LegendText) -> f32;
    /// The lines the text takes at `width`, at least one.
    fn lines(&self, text: &str, style: LegendText, width: f32) -> Vec<String>;
}

impl Measure for Arc<TextSystem> {
    fn width(&self, text: &str, style: LegendText) -> f32 {
        let id = self.resolve_font(&style.font());
        text.chars().filter_map(|c| self.advance(id, px(style.size()), c).ok()).map(|size| f32::from(size.width)).sum()
    }

    fn lines(&self, text: &str, style: LegendText, width: f32) -> Vec<String> {
        let mut wrapper = self.line_wrapper(style.font(), px(style.size()));
        let fragments = [LineFragment::text(text)];
        let breaks: Vec<usize> =
            wrapper.wrap_line(&fragments, px(width), IndentAdjustment::NoIndent).map(|boundary| boundary.ix).collect();
        let mut lines = Vec::with_capacity(breaks.len() + 1);
        let mut from = 0;
        for at in breaks.into_iter().chain([text.len()]) {
            lines.push(text[from..at].trim_end().to_string());
            from = at;
        }
        lines
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub color: Color,
}

/// A line of the legend, its top at `top`; `x` is where it starts, or where it ends when `end`.
#[derive(Clone, PartialEq, Debug)]
pub struct Label {
    pub text: String,
    pub x: f32,
    pub top: f32,
    pub end: bool,
    pub style: LegendText,
}

/// The view's size for a column width, where the scene lies in it, and its legend's dots and
/// lines, all in points from its top left corner.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Arrangement {
    pub width: f32,
    pub height: f32,
    pub scene_x: f32,
    pub scene_y: f32,
    pub dots: Vec<Dot>,
    pub labels: Vec<Label>,
}

/// The view's height for a column `width` wide, its legend set in the app's typeface.
pub fn height(scene: &Scene, width: f32, text: &Arc<TextSystem>) -> f32 {
    arrange(scene, width, text).height
}

pub fn arrange(scene: &Scene, width: f32, measure: &impl Measure) -> Arrangement {
    let Some(legend) = scene.legend.as_ref().filter(|legend| !legend.entries.is_empty()) else {
        return Arrangement { width: scene.width, height: scene.height, ..Arrangement::default() };
    };
    if legend.entries.iter().any(|entry| entry.share.is_some()) {
        return pie(scene, legend, width, measure);
    }
    chart(scene, legend, width, measure)
}

fn chart(scene: &Scene, legend: &Legend, width: f32, measure: &impl Measure) -> Arrangement {
    let style = LegendText::Caption;
    let line_height = style.line_height();
    let top = scene.height + UNDER_PLOT;
    let mut arrangement = Arrangement { width, ..Arrangement::default() };
    let (mut x, mut line) = (0., 0.);
    for entry in &legend.entries {
        let item = DOT + DOT_GAP + measure.width(&entry.label, style);
        if x > 0. && x + item > width {
            line += 1.;
            x = 0.;
        }
        let line_top = top + line * (line_height + LINE_GAP);
        arrangement.dots.push(Dot { x, y: line_top + (line_height - DOT) / 2., color: entry.color.clone() });
        let label = Label { text: entry.label.clone(), x: x + DOT + DOT_GAP, top: line_top, end: false, style };
        arrangement.labels.push(label);
        x += item + ITEM_GAP;
    }
    arrangement.height = top + (line + 1.) * line_height + line * LINE_GAP;
    arrangement
}

fn pie(scene: &Scene, legend: &Legend, width: f32, measure: &impl Measure) -> Arrangement {
    let column = if legend.beside { width - scene.width - PIE_GAP } else { width }.max(1.);
    let line_height = LegendText::Small.line_height();
    let rows: Vec<(Vec<String>, Option<&String>)> = legend
        .entries
        .iter()
        .map(|entry| {
            let share = entry.share.as_ref().map_or(0., |share| measure.width(share, LegendText::Share) + SHARE_GAP);
            let room = (column - DOT - PIE_DOT_GAP - share).max(1.);
            (measure.lines(&entry.label, LegendText::Small, room), entry.share.as_ref())
        })
        .collect();
    let row_height = |lines: &[String]| lines.len().max(1) as f32 * line_height;
    let column_height =
        rows.iter().map(|(lines, _)| row_height(lines)).sum::<f32>() + ROW_GAP * rows.len().saturating_sub(1) as f32;
    let (column_x, column_y, scene_y, height) = if legend.beside {
        let height = scene.height.max(column_height);
        (scene.width + PIE_GAP, (height - column_height) / 2., (height - scene.height) / 2., height)
    } else {
        let column_y = scene.height + PIE_GAP;
        (0., column_y, 0., column_y + column_height)
    };
    let mut arrangement = Arrangement { width, height, scene_y, ..Arrangement::default() };
    let mut top = column_y;
    for ((lines, share), entry) in rows.iter().zip(&legend.entries) {
        let tall = row_height(lines);
        arrangement.dots.push(Dot { x: column_x, y: top + (tall - DOT) / 2., color: entry.color.clone() });
        for (index, line) in lines.iter().enumerate() {
            arrangement.labels.push(Label {
                text: line.clone(),
                x: column_x + DOT + PIE_DOT_GAP,
                top: top + index as f32 * line_height,
                end: false,
                style: LegendText::Small,
            });
        }
        if let Some(share) = share {
            arrangement.labels.push(Label {
                text: (*share).clone(),
                x: column_x + column,
                top: top + (tall - line_height) / 2.,
                end: true,
                style: LegendText::Share,
            });
        }
        top += tall + ROW_GAP;
    }
    arrangement
}

/// Paints the legend with the view's top left corner at `origin`.
pub fn paint_legend(
    arrangement: &Arrangement,
    origin: Point<Pixels>,
    palette: &Palette,
    window: &mut Window,
    cx: &mut App,
) {
    for dot in &arrangement.dots {
        let bounds = Bounds::new(origin + point(px(dot.x), px(dot.y)), size(px(DOT), px(DOT)));
        window.paint_quad(fill(bounds, palette.resolve(&dot.color, 1.)).corner_radii(px(DOT / 2.)));
    }
    for label in &arrangement.labels {
        let color = match label.style {
            LegendText::Caption => palette.colors.muted_foreground,
            LegendText::Small | LegendText::Share => palette.colors.foreground,
        };
        let run = TextRun {
            len: label.text.len(),
            font: label.style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(label.text.clone().into(), px(label.style.size()), &[run], None);
        let left = if label.end { px(label.x) - shaped.width } else { px(label.x) };
        let at = origin + point(left, px(label.top));
        let _ = shaped.paint(at, px(label.style.line_height()), TextAlign::Left, None, window, cx);
    }
}
