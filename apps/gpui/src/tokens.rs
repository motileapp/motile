// The design tokens of the GPUI app. Generated from tokens.json by build.mjs: edit tokens.json and run `pnpm tokens`.
#![allow(dead_code)]

use gpui_kit::{Hsla, rgb};

/// Every colour of the design, in one appearance.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub background: Hsla,
    pub background_accent: Hsla,
    pub background_accent_stronger: Hsla,
    pub background_accent_strongest: Hsla,
    pub background_accent_larger: Hsla,
    pub background_accent_larger_stronger: Hsla,
    pub background_secondary: Hsla,
    pub background_secondary_accent: Hsla,
    pub background_secondary_accent_stronger: Hsla,
    pub card: Hsla,
    pub card_accent: Hsla,
    pub popover: Hsla,
    pub popover_accent: Hsla,
    pub foreground: Hsla,
    pub emphasized_foreground: Hsla,
    pub muted_foreground: Hsla,
    pub muted_stronger_foreground: Hsla,
    pub muted_strongest_foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub success: Hsla,
    pub success_foreground: Hsla,
    pub warning: Hsla,
    pub warning_foreground: Hsla,
    pub pending: Hsla,
    pub pending_foreground: Hsla,
    pub process: Hsla,
    pub process_foreground: Hsla,
    pub merged: Hsla,
    pub merged_foreground: Hsla,
    pub border: Hsla,
    pub border_card: Hsla,
    pub border_input: Hsla,
    pub border_popover: Hsla,
    pub input: Hsla,
    pub composer: Hsla,
    pub border_composer: Hsla,
    pub border_accent: Hsla,
    pub ring: Hsla,
    pub overlay: Hsla,
    pub shadow: Hsla,
    pub chart_1: Hsla,
    pub chart_2: Hsla,
    pub chart_3: Hsla,
    pub chart_4: Hsla,
    pub chart_5: Hsla,
    pub claude: Hsla,
    pub openai: Hsla,
    pub google_blue: Hsla,
    pub google_red: Hsla,
    pub google_yellow: Hsla,
    pub google_green: Hsla,
    pub linear: Hsla,
    pub window_close: Hsla,
    pub window_minimize: Hsla,
    pub window_zoom: Hsla,
    pub syntax_comment: Hsla,
    pub syntax_keyword: Hsla,
    pub syntax_string: Hsla,
    pub syntax_constant: Hsla,
    pub syntax_function: Hsla,
    pub syntax_type: Hsla,
    pub syntax_tag: Hsla,
}

impl Tokens {
    pub fn light() -> Self {
        Self {
            background: hex(0xf6f7f9),
            background_accent: hex(0xe4e6eb),
            background_accent_stronger: hex(0xd9dce3),
            background_accent_strongest: hex(0xced2da),
            background_accent_larger: hex(0xe8eaee),
            background_accent_larger_stronger: hex(0xdfe2e8),
            background_secondary: hex(0xeaecf0),
            background_secondary_accent: hex(0xdfe2e8),
            background_secondary_accent_stronger: hex(0xd4d8e0),
            card: hex(0xffffff),
            card_accent: hex(0xe6e8ec),
            popover: hex(0xffffff),
            popover_accent: hex(0xeaecf0),
            foreground: hex(0x22242b),
            emphasized_foreground: hex(0x000000),
            muted_foreground: hex(0x606470),
            muted_stronger_foreground: hex(0x8e919d),
            muted_strongest_foreground: hex(0xadb1bc),
            primary: hex(0x2a5bd7),
            primary_foreground: hex(0xffffff),
            destructive: hex(0xc62828),
            destructive_foreground: hex(0xffffff),
            success: hex(0x047857),
            success_foreground: hex(0xffffff),
            warning: hex(0xc74f0a),
            warning_foreground: hex(0xffffff),
            pending: hex(0x947100),
            pending_foreground: hex(0xffffff),
            process: hex(0x0284c7),
            process_foreground: hex(0xffffff),
            merged: hex(0x8250df),
            merged_foreground: hex(0xffffff),
            border: hex(0xe2e3e5),
            border_card: hex(0xe9ebf0),
            border_input: hex(0xeaecf0),
            border_popover: hex(0xe6e8eb),
            input: hex(0xe8eaee),
            composer: hex(0xffffff),
            border_composer: hex(0xeaecf0),
            border_accent: hex(0xd9dce5),
            ring: hex(0x22242b),
            overlay: hex(0x000000),
            shadow: hex(0x000000),
            chart_1: hex(0x2a5bd7),
            chart_2: hex(0x047857),
            chart_3: hex(0xc74f0a),
            chart_4: hex(0x8250df),
            chart_5: hex(0x0284c7),
            claude: hex(0xd97757),
            openai: hex(0x22242b),
            google_blue: hex(0x4285f4),
            google_red: hex(0xea4335),
            google_yellow: hex(0xfbbc05),
            google_green: hex(0x34a853),
            linear: hex(0x22242b),
            window_close: hex(0xff5f57),
            window_minimize: hex(0xfebc2e),
            window_zoom: hex(0x28c840),
            syntax_comment: hex(0x6e7781),
            syntax_keyword: hex(0xcf222e),
            syntax_string: hex(0x0a3069),
            syntax_constant: hex(0x0550ae),
            syntax_function: hex(0x8250df),
            syntax_type: hex(0x953800),
            syntax_tag: hex(0x116329),
        }
    }

    pub fn dark() -> Self {
        Self {
            background: hex(0x0a0b0f),
            background_accent: hex(0x191a1f),
            background_accent_stronger: hex(0x212329),
            background_accent_strongest: hex(0x2b2c32),
            background_accent_larger: hex(0x14151a),
            background_accent_larger_stronger: hex(0x191a1f),
            background_secondary: hex(0x111217),
            background_secondary_accent: hex(0x191a1f),
            background_secondary_accent_stronger: hex(0x212227),
            card: hex(0x15161b),
            card_accent: hex(0x1e1f26),
            popover: hex(0x191a1f),
            popover_accent: hex(0x212329),
            foreground: hex(0xd0d2d9),
            emphasized_foreground: hex(0xffffff),
            muted_foreground: hex(0x9a9eab),
            muted_stronger_foreground: hex(0x646875),
            muted_strongest_foreground: hex(0x4a4e5a),
            primary: hex(0x4f7cff),
            primary_foreground: hex(0xffffff),
            destructive: hex(0xdf6c6a),
            destructive_foreground: hex(0xffffff),
            success: hex(0x41b76f),
            success_foreground: hex(0xffffff),
            warning: hex(0xe98538),
            warning_foreground: hex(0xffffff),
            pending: hex(0xd3b142),
            pending_foreground: hex(0xffffff),
            process: hex(0x2aa9e5),
            process_foreground: hex(0xffffff),
            merged: hex(0xb480ff),
            merged_foreground: hex(0xffffff),
            border: hex(0x191a1e),
            border_card: hex(0x1e1f25),
            border_input: hex(0x1a1b21),
            border_popover: hex(0x222228),
            input: hex(0x14151a),
            composer: hex(0x111217),
            border_composer: hex(0x1a1b21),
            border_accent: hex(0x25262c),
            ring: hex(0xd0d2d9),
            overlay: hex(0x000000),
            shadow: hex(0x000000),
            chart_1: hex(0x4f7cff),
            chart_2: hex(0x41b76f),
            chart_3: hex(0xe98538),
            chart_4: hex(0xb480ff),
            chart_5: hex(0x2aa9e5),
            claude: hex(0xd97757),
            openai: hex(0xd0d2d9),
            google_blue: hex(0x4285f4),
            google_red: hex(0xea4335),
            google_yellow: hex(0xfbbc05),
            google_green: hex(0x34a853),
            linear: hex(0xd0d2d9),
            window_close: hex(0xff5f57),
            window_minimize: hex(0xfebc2e),
            window_zoom: hex(0x28c840),
            syntax_comment: hex(0x8b949e),
            syntax_keyword: hex(0xff7b72),
            syntax_string: hex(0xa5d6ff),
            syntax_constant: hex(0x79c0ff),
            syntax_function: hex(0xd2a8ff),
            syntax_type: hex(0xffa657),
            syntax_tag: hex(0x7ee787),
        }
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// How much of a colour shows: every opacity in the app is one of these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opacity {
    Overlay,
    OverlayPulled,
    Disabled,
    Lit,
    Shadow,
    ShadowStronger,
    ShadowStrongest,
    ColorTint,
    ColorTintStronger,
    ColorTintChart,
    InputGlassTint,
    ShimmerBandEdge,
    ShimmerBandMiddle,
    Glow,
    GlowFaint,
    GlowBright,
}

impl Opacity {
    pub fn light(self) -> f32 {
        match self {
            Opacity::Overlay => 0.5,
            Opacity::OverlayPulled => 0.4,
            Opacity::Disabled => 0.45,
            Opacity::Lit => 0.88,
            Opacity::Shadow => 0.06,
            Opacity::ShadowStronger => 0.14,
            Opacity::ShadowStrongest => 0.25,
            Opacity::ColorTint => 0.12,
            Opacity::ColorTintStronger => 0.2,
            Opacity::ColorTintChart => 0.25,
            Opacity::InputGlassTint => 0.8,
            Opacity::ShimmerBandEdge => 0.12,
            Opacity::ShimmerBandMiddle => 0.55,
            Opacity::Glow => 0.4,
            Opacity::GlowFaint => 0.3,
            Opacity::GlowBright => 0.5,
        }
    }

    pub fn dark(self) -> f32 {
        match self {
            Opacity::Overlay => 0.6,
            Opacity::OverlayPulled => 0.4,
            Opacity::Disabled => 0.45,
            Opacity::Lit => 0.88,
            Opacity::Shadow => 0.2,
            Opacity::ShadowStronger => 0.4,
            Opacity::ShadowStrongest => 0.6,
            Opacity::ColorTint => 0.1,
            Opacity::ColorTintStronger => 0.16,
            Opacity::ColorTintChart => 0.2,
            Opacity::InputGlassTint => 0.8,
            Opacity::ShimmerBandEdge => 0.12,
            Opacity::ShimmerBandMiddle => 0.55,
            Opacity::Glow => 0.4,
            Opacity::GlowFaint => 0.3,
            Opacity::GlowBright => 0.5,
        }
    }
}

/// How round corners are: every radius is one of these.
pub struct Radius;

impl Radius {
    pub const XS: f32 = 4.;
    pub const SM: f32 = 6.;
    pub const MD: f32 = 8.;
    pub const LG: f32 = 12.;
    pub const XL: f32 = 16.;
    pub const XXL: f32 = 24.;
}

/// A shadow's size: how far down it falls and its blur, as CSS gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShadowSize {
    Sm,
    Md,
    Lg,
    Xl,
}

impl ShadowSize {
    pub fn down(self) -> f32 {
        match self {
            ShadowSize::Sm => 2.5,
            ShadowSize::Md => 4.,
            ShadowSize::Lg => 8.,
            ShadowSize::Xl => 16.,
        }
    }

    pub fn blur(self) -> f32 {
        match self {
            ShadowSize::Sm => 8.,
            ShadowSize::Md => 12.,
            ShadowSize::Lg => 24.,
            ShadowSize::Xl => 40.,
        }
    }
}
