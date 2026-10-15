// Writes the design tokens of tokens.json out as tokens.css, theme.css, the clients' Tokens.swift,
// the GPUI app's tokens.rs and the Android app's Tokens.kt and colour resources. Run it with
// `pnpm tokens` after changing tokens.json; what it writes is never edited by hand.
import { readFileSync, writeFileSync } from "node:fs"

const here = (path) => new URL(path, import.meta.url)
const tokens = JSON.parse(readFileSync(here("tokens.json"), "utf8"))
const { radius, colors, opacity, shadow } = tokens
const names = Object.keys(colors.light)
const opacities = Object.keys(opacity.light)

const percent = (value) => `${Math.round(value * 100)}%`
const camel = (name) => name.replace(/-([a-z0-9])/g, (_, letter) => letter.toUpperCase())
const swiftName = (name) => (name === "2xl" ? "xxl" : camel(name))
const hex = (value) => `0x${value.slice(1)}`
const snake = (name) => name.replace(/-/g, "_")
const pascal = (name) => camel(`-${name === "2xl" ? "xxl" : name}`)
const float = (value) => (Number.isInteger(value) ? `${value}.` : `${value}`)

const generated = (comment) => `${comment} Generated from tokens.json by build.mjs: edit tokens.json and run \`pnpm tokens\`.`

function tokensCSS() {
  const theme = (mode) =>
    [
      ...names.map((name) => `  --${name}: ${colors[mode][name]};`),
      "",
      ...opacities.map((name) => `  --opacity-${name}: ${percent(opacity[mode][name])};`),
    ].join("\n")
  return `/* ${generated("The design tokens as plain CSS variables, for pages Tailwind doesn't build.")}
   Opacities are percentages, so that Tailwind's colour/opacity takes them. */

:root {
${theme("light")}
}

.dark {
${theme("dark")}
}

:root,
.dark {
  /* A shadow's colour where nothing else is said: shadow at opacity shadow. */
  --shadow-color: rgb(from var(--shadow) r g b / var(--opacity-shadow));
}
`
}

function themeCSS() {
  return `/* ${generated("The design tokens for Tailwind, in the marketing site and the web app.")}
   Dark is the default: the page starts with the "dark" class on <html> unless the theme cookie
   says light. Nothing but these colours, shadows and radii exists: Tailwind's own palette and
   scales are gone. An opacity is one of the tokens, as Tailwind's own modifier:
   bg-overlay/(--opacity-overlay), opacity-(--opacity-disabled),
   shadow-lg shadow-shadow/(--opacity-shadow-stronger). */

@import "./tokens.css";

@custom-variant dark (&:where(.dark, .dark *));

@theme inline {
  --font-heading: var(--font-sans);
  --font-sans: "DM Sans Variable", sans-serif;

  --color-*: initial;
${names.map((name) => `  --color-${name}: var(--${name});`).join("\n")}

  --radius-*: initial;
${Object.entries(radius)
  .map(([name, value]) => `  --radius-${name}: ${value}px;`)
  .join("\n")}

  --shadow-*: initial;
${Object.entries(shadow)
  .map(([name, value]) => `  --shadow-${name}: ${value} var(--tw-shadow-color, var(--shadow-color));`)
  .join("\n")}
}

@layer base {
  * {
    @apply border-border;
  }
  body {
    @apply bg-background text-foreground;
  }
  html {
    @apply font-sans;
    color-scheme: light;
    scrollbar-gutter: stable;
  }
  html.dark {
    color-scheme: dark;
  }
}
`
}

function swift() {
  const cases = (keys, value) =>
    keys.map((name) => `        case .${swiftName(name)}: ${value(name)}`).join("\n")
  const shadows = Object.entries(shadow).map(([name, value]) => [name, value.split(" ").map(parseFloat)])
  return `// ${generated("The design tokens of the clients.")}
import SwiftUI

extension Theme {
${names.map((name) => `    static let ${camel(name)} = tone(${hex(colors.light[name])}, ${hex(colors.dark[name])})`).join("\n")}
}

extension Color {
${names.map((name) => `    static let theme${camel(`-${name}`)} = Color(platform: Theme.${camel(name)})`).join("\n")}
}

/// How much of a colour shows: every opacity in the clients is one of these.
enum Opacity {
    case ${opacities.map(swiftName).join(", ")}

    var light: CGFloat {
        switch self {
${cases(opacities, (name) => opacity.light[name])}
        }
    }

    var dark: CGFloat {
        switch self {
${cases(opacities, (name) => opacity.dark[name])}
        }
    }
}

/// How round corners are: every radius is one of these.
enum Radius {
${Object.entries(radius)
  .map(([name, value]) => `    static let ${swiftName(name)}: CGFloat = ${value}`)
  .join("\n")}
}

/// A shadow's size: how far down it falls and its blur, as CSS gives them.
enum ShadowSize {
    case ${shadows.map(([name]) => name).join(", ")}

    var down: CGFloat {
        switch self {
${shadows.map(([name, [, down]]) => `        case .${name}: ${down}`).join("\n")}
        }
    }

    var blur: CGFloat {
        switch self {
${shadows.map(([name, [, , blur]]) => `        case .${name}: ${blur}`).join("\n")}
        }
    }
}
`
}

function rust() {
  const shadows = Object.entries(shadow).map(([name, value]) => [name, value.split(" ").map(parseFloat)])
  const palette = (mode) => names.map((name) => `            ${snake(name)}: hex(${hex(colors[mode][name])}),`).join("\n")
  const opacityArms = (mode) => opacities.map((name) => `            Opacity::${pascal(name)} => ${float(opacity[mode][name])},`).join("\n")
  return `// ${generated("The design tokens of the GPUI app.")}
#![allow(dead_code)]

use gpui_kit::{Hsla, rgb};

/// Every colour of the design, in one appearance.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
${names.map((name) => `    pub ${snake(name)}: Hsla,`).join("\n")}
}

impl Tokens {
    pub fn light() -> Self {
        Self {
${palette("light")}
        }
    }

    pub fn dark() -> Self {
        Self {
${palette("dark")}
        }
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// How much of a colour shows: every opacity in the app is one of these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opacity {
${opacities.map((name) => `    ${pascal(name)},`).join("\n")}
}

impl Opacity {
    pub fn light(self) -> f32 {
        match self {
${opacityArms("light")}
        }
    }

    pub fn dark(self) -> f32 {
        match self {
${opacityArms("dark")}
        }
    }
}

/// How round corners are: every radius is one of these.
pub struct Radius;

impl Radius {
${Object.entries(radius)
  .map(([name, value]) => `    pub const ${(name === "2xl" ? "xxl" : name).toUpperCase()}: f32 = ${float(value)};`)
  .join("\n")}
}

/// A shadow's size: how far down it falls and its blur, as CSS gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShadowSize {
${shadows.map(([name]) => `    ${pascal(name)},`).join("\n")}
}

impl ShadowSize {
    pub fn down(self) -> f32 {
        match self {
${shadows.map(([name, [, down]]) => `            ShadowSize::${pascal(name)} => ${float(down)},`).join("\n")}
        }
    }

    pub fn blur(self) -> f32 {
        match self {
${shadows.map(([name, [, , blur]]) => `            ShadowSize::${pascal(name)} => ${float(blur)},`).join("\n")}
        }
    }
}
`
}

function kotlin() {
  const shadows = Object.entries(shadow).map(([name, value]) => [name, value.split(" ").map(parseFloat)])
  const color = (value) => `Color(0xFF${value.slice(1).toUpperCase()})`
  const palette = (mode) => names.map((name) => `            ${camel(name)} = ${color(colors[mode][name])},`).join("\n")
  const kotlinName = (name) => (name === "2xl" ? "xxl" : camel(name))
  return `// ${generated("The design tokens of the Android app.")}
package app.motile.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** Every colour of the design, in one appearance. */
class Palette(
${names.map((name) => `    val ${camel(name)}: Color,`).join("\n")}
) {
    companion object {
        val light = Palette(
${palette("light")}
        )

        val dark = Palette(
${palette("dark")}
        )
    }
}

/** How much of a colour shows: every opacity in the app is one of these. */
enum class Opacity(val light: Float, val dark: Float) {
${opacities.map((name) => `    ${pascal(name)}(${opacity.light[name]}f, ${opacity.dark[name]}f),`).join("\n")}
}

/** How round corners are: every radius is one of these. */
object Radius {
${Object.entries(radius)
  .map(([name, value]) => `    val ${kotlinName(name)} = ${value}.dp`)
  .join("\n")}
}

/** A shadow's size: how far down it falls and its blur, as CSS gives them. */
enum class ShadowSize(val down: Float, val blur: Float) {
${shadows.map(([name, [, down, blur]]) => `    ${pascal(name)}(${down}f, ${blur}f),`).join("\n")}
}
`
}

/** The colours Android draws before the app does: the window behind it and the launcher icon. */
function androidResources(mode) {
  return `<?xml version="1.0" encoding="utf-8"?>
<!-- ${generated("The design tokens Android itself draws with.")} -->
<resources>
    <color name="background">${colors[mode].background}</color>
    <color name="icon_background">${colors.dark.background}</color>
    <color name="icon_mark">${colors.dark["emphasized-foreground"]}</color>
</resources>
`
}

writeFileSync(here("tokens.css"), tokensCSS())
writeFileSync(here("theme.css"), themeCSS())
writeFileSync(here("../apple/Sources/MotileKit/Shared/Theme/Tokens.swift"), swift())
writeFileSync(here("../../apps/gpui/src/tokens.rs"), rust())
writeFileSync(here("../../apps/android/app/src/main/java/app/motile/theme/Tokens.kt"), kotlin())
writeFileSync(here("../../apps/android/app/src/main/res/values/tokens.xml"), androidResources("light"))
writeFileSync(here("../../apps/android/app/src/main/res/values-night/tokens.xml"), androidResources("dark"))
