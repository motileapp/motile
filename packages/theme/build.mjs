// Writes the design tokens of tokens.json out as tokens.css, theme.css, the clients' Tokens.swift,
// the GPUI app's tokens.rs, the Android app's Tokens.kt and colour resources, and the MapLibre
// style of OpenUI's map in each appearance. Run it with `pnpm tokens` after changing tokens.json;
// what it writes is never edited by hand.
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

extension Theme {
    /// The colour of a token by its name in tokens.json, as OpenUI's scenes and trees name them.
    static func named(_ name: String) -> PlatformColor? {
        switch name {
${names.map((name) => `        case "${name}": ${camel(name)}`).join("\n")}
        default: nil
        }
    }
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

    /// The colour of a token by its name in tokens.json, as OpenUI's scenes and trees name them.
    pub fn named(&self, name: &str) -> Option<Hsla> {
        Some(match name {
${names.map((name) => `            "${name}" => self.${snake(name)},`).join("\n")}
            _ => return None,
        })
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

    /** The colour of a token by its name in tokens.json, as OpenUI's scenes and trees name them. */
    fun named(name: String): Color? = when (name) {
${names.map((name) => `        "${name}" -> ${camel(name)}`).join("\n")}
        else -> null
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

/** OpenUI's map: OpenFreeMap's vector tiles (the OpenMapTiles schema) drawn in the tokens, with
 *  the route and the pins as layers whose sources the client fills. */
function mapStyle(mode) {
  const c = colors[mode]
  const name = ["coalesce", ["get", "name_en"], ["get", "name"]]
  const numbered = ["!", ["has", "icon"]]
  const selected = ["boolean", ["get", "selected"], false]
  const empty = { type: "geojson", data: { type: "FeatureCollection", features: [] } }
  const width = (stops) => ["interpolate", ["linear"], ["zoom"], ...stops]
  const round = { "line-cap": "round", "line-join": "round" }
  const overlap = { "text-allow-overlap": true, "text-ignore-placement": true }
  const tiles = (id, type, layer, rest) => ({ id, type, source: "openmaptiles", "source-layer": layer, ...rest })
  const style = {
    version: 8,
    name: `Motile ${mode}`,
    metadata: { "motile:generated": generated("OpenUI's map style.") },
    glyphs: "https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf",
    sources: {
      openmaptiles: {
        type: "vector",
        url: "https://tiles.openfreemap.org/planet",
        attribution:
          '<a href="https://openfreemap.org">OpenFreeMap</a> © <a href="https://www.openmaptiles.org">OpenMapTiles</a> · Data from <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
      },
      route: empty,
      pins: empty,
    },
    layers: [
      { id: "background", type: "background", paint: { "background-color": c["map-land"] } },
      tiles("landcover", "fill", "landcover", {
        filter: ["in", "class", "wood", "grass"],
        paint: { "fill-color": c["map-green"], "fill-opacity": 0.8 },
      }),
      tiles("parks", "fill", "park", { paint: { "fill-color": c["map-park"] } }),
      tiles("school", "fill", "landuse", {
        filter: ["in", "class", "school", "university", "hospital"],
        paint: { "fill-color": c["map-campus"] },
      }),
      tiles("water", "fill", "water", { paint: { "fill-color": c["map-water"] } }),
      tiles("waterways", "line", "waterway", { paint: { "line-color": c["map-waterway"], "line-width": 1 } }),
      tiles("buildings", "fill", "building", { minzoom: 14, paint: { "fill-color": c["map-building"] } }),
      tiles("streets", "line", "transportation", {
        filter: ["!in", "class", "rail", "transit", "ferry", "motorway", "trunk", "primary"],
        layout: round,
        paint: { "line-color": c["map-street"], "line-width": width([9, 0.3, 12, 1.5, 14, 2.8, 17, 8]) },
      }),
      tiles("main-roads", "line", "transportation", {
        filter: ["in", "class", "motorway", "trunk", "primary"],
        layout: round,
        paint: { "line-color": c["map-road"], "line-width": width([9, 0.7, 12, 3, 14, 5, 17, 12]) },
      }),
      tiles("ferries", "line", "transportation", {
        filter: ["==", "class", "ferry"],
        paint: { "line-color": c["map-ferry"], "line-width": 0.8 },
      }),
      tiles("neighborhood-labels", "symbol", "place", {
        minzoom: 10,
        maxzoom: 15,
        filter: ["in", "class", "suburb", "quarter", "neighbourhood"],
        layout: {
          "text-field": name,
          "text-font": ["Noto Sans Regular"],
          "text-size": 10,
          "text-letter-spacing": 0.08,
          "text-transform": "uppercase",
          "text-max-width": 8,
        },
        paint: { "text-color": c["muted-foreground"], "text-halo-color": c["map-land"], "text-halo-width": 1.2 },
      }),
      tiles("city-labels", "symbol", "place", {
        minzoom: 7,
        maxzoom: 13,
        filter: ["in", "class", "city", "town"],
        layout: { "text-field": name, "text-font": ["Noto Sans Regular"], "text-size": 14, "text-max-width": 8 },
        paint: { "text-color": c["muted-foreground"], "text-halo-color": c["map-land"], "text-halo-width": 1.5 },
      }),
      {
        id: "route",
        type: "line",
        source: "route",
        layout: round,
        paint: { "line-color": c.foreground, "line-width": 3.2 },
      },
      {
        id: "pin-shadow",
        type: "circle",
        source: "pins",
        layout: { "circle-sort-key": ["case", selected, 1, 0] },
        paint: {
          "circle-radius": ["case", numbered, 14, 18],
          "circle-color": c.shadow,
          "circle-opacity": opacity[mode]["shadow-stronger"],
          "circle-blur": 0.5,
          "circle-translate": [0, 2],
        },
      },
      {
        id: "pin",
        type: "circle",
        source: "pins",
        layout: { "circle-sort-key": ["case", selected, 1, 0] },
        paint: {
          "circle-radius": ["case", numbered, 11, 16],
          "circle-color": ["case", numbered, c.foreground, c.card],
          "circle-stroke-width": ["case", numbered, 2, 1],
          "circle-stroke-color": ["case", numbered, c.card, c["border-card"]],
        },
      },
      {
        id: "pin-selected",
        type: "circle",
        source: "pins",
        filter: selected,
        paint: {
          "circle-radius": ["case", numbered, 14, 18],
          "circle-opacity": 0,
          "circle-stroke-width": 3,
          "circle-stroke-color": c.ring,
        },
      },
      {
        id: "pin-mark",
        type: "symbol",
        source: "pins",
        layout: {
          "symbol-sort-key": ["case", selected, 1, 0],
          "icon-image": ["coalesce", ["get", "icon"], ""],
          "text-field": ["case", numbered, ["get", "number"], ""],
          "text-font": ["Noto Sans Bold"],
          "text-size": 12,
          ...overlap,
          "icon-allow-overlap": true,
          "icon-ignore-placement": true,
        },
        paint: { "text-color": c.card },
      },
      {
        id: "pin-label",
        type: "symbol",
        source: "pins",
        layout: {
          "symbol-sort-key": ["case", selected, 1, 0],
          "text-field": ["get", "label"],
          "text-font": ["Noto Sans Bold"],
          "text-size": 12,
          "text-anchor": "top",
          "text-offset": ["case", numbered, ["literal", [0, 1.4]], ["literal", [0, 1.7]]],
          "text-max-width": 13,
          ...overlap,
        },
        paint: { "text-color": c.foreground, "text-halo-color": c["map-land"], "text-halo-width": 1.5 },
      },
    ],
  }
  return `${JSON.stringify(style, null, 2)}\n`
}

writeFileSync(here("tokens.css"), tokensCSS())
writeFileSync(here("theme.css"), themeCSS())
writeFileSync(here("../apple/Sources/MotileKit/Shared/Theme/Tokens.swift"), swift())
writeFileSync(here("../../apps/gpui/src/tokens.rs"), rust())
writeFileSync(here("../../apps/android/app/src/main/java/app/motile/theme/Tokens.kt"), kotlin())
writeFileSync(here("../../apps/android/app/src/main/res/values/tokens.xml"), androidResources("light"))
writeFileSync(here("../../apps/android/app/src/main/res/values-night/tokens.xml"), androidResources("dark"))
writeFileSync(here("map-style-light.json"), mapStyle("light"))
writeFileSync(here("map-style-dark.json"), mapStyle("dark"))
