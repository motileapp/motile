// Writes the design tokens of tokens.json out as tokens.css, theme.css and the clients'
// Tokens.swift. Run it with `pnpm tokens` after changing tokens.json; the three are generated
// and never edited by hand.
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

writeFileSync(here("tokens.css"), tokensCSS())
writeFileSync(here("theme.css"), themeCSS())
writeFileSync(here("../apple/Sources/MotileKit/Shared/Theme/Tokens.swift"), swift())
