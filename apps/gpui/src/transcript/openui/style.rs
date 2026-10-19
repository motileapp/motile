//! LAYOUT.md's type, colours and radii in the GPUI app's terms.

use std::ops::Range;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::tree::{Color, Radius, RadiusKind, Rich, TextStyle, Weight};

use super::UiCx;
use crate::theme::{self, Radius as Radii, colors};
use crate::transcript::canvas::Palette;
use crate::transcript::prose;
use crate::transcript::rows::{self, Plain};

/// A text style's size, line height and weight, and whether its digits are of one width.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Type {
    pub size: f32,
    pub line: f32,
    pub weight: FontWeight,
    pub digits: bool,
    pub mono: bool,
}

impl Type {
    const fn new(size: f32, line: f32, weight: FontWeight, digits: bool) -> Self {
        Self { size, line, weight, digits, mono: false }
    }

    pub fn weight(self, weight: Weight) -> Self {
        let weight = match weight {
            Weight::Regular => FontWeight::NORMAL,
            Weight::Medium => FontWeight::MEDIUM,
            Weight::Semibold => FontWeight::SEMIBOLD,
        };
        Self { weight, ..self }
    }

    pub fn font(self, italic: bool) -> Font {
        let family = if self.mono { theme::MONO_FONT } else { theme::UI_FONT };
        let font = rows::font(family, self.weight, italic);
        match self.digits {
            true => Font { features: crate::ui::tabular_numbers(), ..font },
            false => font,
        }
    }

    /// A box of text in this style.
    pub fn text_box(self) -> Div {
        div()
            .text_size(px(self.size))
            .line_height(px(self.line))
            .font_weight(self.weight)
            .when(self.mono, |text| text.font_family(theme::MONO_FONT))
            .when(self.digits, |text| text.font_features(crate::ui::tabular_numbers()))
    }

    /// How wide `text` is on one line.
    pub fn width(self, text: &str, window: &Window) -> f32 {
        if text.is_empty() {
            return 0.;
        }
        let text = SharedString::from(text.to_string());
        let run = TextRun {
            len: text.len(),
            font: self.font(false),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        f32::from(window.text_system().shape_line(text, px(self.size), &[run], None).width)
    }
}

const REGULAR: FontWeight = FontWeight::NORMAL;
const MEDIUM: FontWeight = FontWeight::MEDIUM;
const SEMIBOLD: FontWeight = FontWeight::SEMIBOLD;

pub const BODY: Type = Type::new(theme::PROSE_SIZE, theme::PROSE_LINE_HEIGHT, REGULAR, false);
pub const SMALL: Type = Type::new(13., 20., REGULAR, false);
pub const CAPTION: Type = Type::new(12., 18., REGULAR, false);
pub const LABEL: Type = Type::new(13., 18., MEDIUM, false);
pub const READOUT: Type = Type::new(12., 18., MEDIUM, true);
pub const TICK: Type = Type::new(11., 14., REGULAR, true);

/// The style's type; the largest titles are smaller in a narrow column.
pub fn type_of(style: TextStyle, narrow: bool) -> Type {
    match style {
        TextStyle::Body => BODY,
        TextStyle::BodyCard => Type::new(theme::PROSE_SIZE, 22., REGULAR, false),
        TextStyle::Large => Type::new(16., 26., REGULAR, false),
        TextStyle::Small => SMALL,
        TextStyle::Caption => CAPTION,
        TextStyle::Label => LABEL,
        TextStyle::TitleSm => Type::new(theme::heading_size(4), 20., SEMIBOLD, true),
        TextStyle::TitleMd => Type::new(theme::heading_size(3), 22., SEMIBOLD, true),
        TextStyle::TitleLg | TextStyle::Word => Type::new(theme::heading_size(2), 24., SEMIBOLD, true),
        TextStyle::TitleXl => Type::new(theme::heading_size(1), 26., SEMIBOLD, true),
        TextStyle::Title2xl if narrow => Type::new(24., 28., SEMIBOLD, true),
        TextStyle::Title2xl => Type::new(26., 30., SEMIBOLD, true),
        TextStyle::Title3xl if narrow => Type::new(28., 32., SEMIBOLD, true),
        TextStyle::Title3xl => Type::new(32., 36., SEMIBOLD, true),
        TextStyle::CardTitle => Type::new(14., 20., MEDIUM, true),
        TextStyle::CardSubtitle => Type::new(13., 18., REGULAR, false),
        TextStyle::CardHeading => Type::new(16., 22., SEMIBOLD, true),
        TextStyle::Readout => READOUT,
        TextStyle::Tick => TICK,
        TextStyle::Timer => Type::new(28., 32., SEMIBOLD, true),
        TextStyle::Code => Type { mono: true, ..Type::new(theme::CODE_SIZE, theme::CODE_LINE_HEIGHT, REGULAR, false) },
        TextStyle::WordBack => Type::new(theme::PROSE_SIZE, theme::PROSE_LINE_HEIGHT, MEDIUM, false),
    }
}

/// A colour of the tree on the surface the node lies on.
pub fn color(color: &Color, cx: &UiCx, app: &App) -> Hsla {
    Palette::new(colors(app), cx.surface).resolve(color, 1.)
}

pub fn radius(radius: &Radius) -> f32 {
    match radius.kind {
        RadiusKind::Sm => Radii::SM,
        RadiusKind::Md => Radii::MD,
        RadiusKind::Lg => Radii::LG,
        RadiusKind::Xl => Radii::XL,
        RadiusKind::Full => 9999.,
        RadiusKind::Points => radius.points,
    }
}

/// How text of the tree is set: its type, colour and the touches a Text adds.
#[derive(Clone, Copy)]
pub struct Look {
    pub ty: Type,
    pub color: Hsla,
    pub italic: bool,
    pub strike: bool,
}

impl Look {
    pub fn new(ty: Type, color: Hsla) -> Self {
        Self { ty, color, italic: false, strike: false }
    }
}

/// Text with its inline formatting, as GPUI sets it: its string, the runs of its fonts and
/// colours, its links and its inline code, by bytes.
pub struct Set {
    pub text: SharedString,
    pub runs: Vec<TextRun>,
    pub links: Vec<(Range<usize>, String)>,
    pub code: Vec<Range<usize>>,
}

pub fn set(rich: &Rich, look: Look, app: &App) -> Set {
    let (text, styled, links) = prose::rich(&rich.text, &rich.runs, &rich.links);
    set_text(text, &styled, links, look, app)
}

pub fn set_text(
    text: String,
    styled: &[(Range<usize>, u32)],
    links: Vec<(Range<usize>, String)>,
    look: Look,
    app: &App,
) -> Set {
    let Look { ty, color, italic, strike } = look;
    let family = if ty.mono { theme::MONO_FONT } else { theme::UI_FONT };
    let plain = Plain { family, size: ty.size, color, weight: ty.weight, heading: false };
    let mut runs = rows::text_runs(&text, styled, &links, plain, app);
    if runs.is_empty() {
        runs.push(TextRun {
            len: text.len(),
            font: ty.font(italic),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    for run in &mut runs {
        if italic {
            run.font.style = FontStyle::Italic;
        }
        if ty.digits && !run.font.family.starts_with("Motile Inline") {
            run.font.features = crate::ui::tabular_numbers();
        }
        if strike {
            run.strikethrough = Some(StrikethroughStyle { thickness: px(1.), color: Some(run.color) });
        }
    }
    let code = styled.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    Set { text: text.into(), runs, links, code }
}
