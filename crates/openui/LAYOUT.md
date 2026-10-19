# How the clients draw OpenUI

The geometry and behaviour of open-intelligent-ui's 55 components as the Mac app (AppKit), the iOS
app (UIKit), the GPUI app and the Android app (Compose) draw them, inline in the transcript.
Written from open-intelligent-ui's `components.css`, `theme.css`, `style.ts`, `map.css` and
components at 0321343, in Motile's tokens. Where its look and Motile's components disagree,
Motile's wins; each such place says so ("Motile's").

## Who does what

| Core resolves (the client never computes it) | The client does |
| --- | --- |
| Every prop: colours as token names or an RGBA with its readable ink, lengths in points, numbers, currencies and dates formatted, text as runs and links, URLs checked, Lucide names as codepoints of `lucide.ttf` | Lays out with the rules below, in its own text system |
| The context rules: flow spacing, card typography, media rows, grid columns, table column alignment, list marker kinds | Draws what core resolved |
| Behaviour: timers, validation, `Query` refresh, Celebration's trigger, the map's filter, selection and camera targets, carousel paging targets, popover state where the tree depends on it, what is disabled while the program streams | Sends `Command::Ui` events: press, value, scroll index, select, flip, expand |
| Canvas scenes, as pure functions of (node, width): Chart, Svg, Math, Celebration's keyframes, Flashcard's flip | Paints the scene: Core Graphics, GPUI `canvas()` + `PathBuilder`, Compose `Canvas` |

A client keeps its own value only while a slider is dragged or a field typed in, and sends throttled
updates (PLAN decision 6).

## Units

- Lengths are points (dp on Android). The model's `gap` and `padding` count 4-point steps: `gap: 3`
  is 12. Core multiplies; a resolved node carries points.
- Text sizes, line heights and `ControlSize` heights are as on the Mac and enlarged on iOS and
  Android by `Platform.scale` (1.14; heights rounded, as `scaled()`), as all Motile text and
  controls are. Every other length here is the same on every client. Frame dumps compare the Mac
  with GPUI, and iOS with Android.
- `Length` (`width`, `height`): core gives points, or a fraction of the parent's content box. `em` and
  `rem` count 14 points; `vw` is the transcript column's width; `vh` the window's height; `fr` and
  `auto` are as if omitted. A percentage height of a parent with no fixed height is ignored.
- Lines are 1 point wide, at the pixel grid's nearest half point so they stay crisp.

## Narrow column

open-intelligent-ui's `max-width: 640px` rules apply while the transcript column (the width the row
is measured at) is under 640 points. They are constants of the clients' OpenUI style, picked by
that width; the tree doesn't change.

| What | Wide | Narrow |
| --- | --- | --- |
| Card padding (outline and soft, not nested) | 20 | 16 |
| Table cell padding (vertical, horizontal) | 10, 12; header 8, 12 | 8, 10; header 8, 10 |
| `title-2xl` | 26 / 30 | 24 / 28 |
| `title-3xl` | 32 / 36 | 28 / 32 |
| List text lane (`wide`) | 76 lane, 16 gap; timeline 24 gap | 64 lane, 20 gap; timeline 20 gap |
| Map expanded inset (window under 520 wide) | 24 | 12 |
| Map pin label | 12, 160 max | 10, 120 max |

## Colours

Token names are `packages/theme/tokens.json`'s (Swift `Theme.camelCase`, GPUI `snake_case`, Kotlin
`camelCase`). A wash is the colour at `color-tint` (`.wash()`, `Colors::wash`, `Color.wash()`).
"The surface" is the `Surface` the node lies on: `background` in the transcript, `card` inside a
Card, a box's inner surface inside a soft card or a filled Box.

| OpenUI | As text, icon, stroke | As a background |
| --- | --- | --- |
| `ink` | `foreground` | `foreground` |
| `muted` | `muted-foreground` | `muted-foreground` |
| `faint` | `muted-stronger-foreground` | `muted-stronger-foreground` |
| `canvas` | the surface's colour | the surface's colour |
| `surface` | the surface's `control` layer | the surface's `box` layer |
| `surface-strong` | (not a text colour) | the surface's `control` layer |
| `border` | (not a text colour) | the surface's border |
| `info` | `foreground` | `foreground`'s wash |
| `success` | `success` | `success`'s wash |
| `warning` | `warning` | `warning`'s wash |
| `danger` | `destructive` | `destructive`'s wash |
| `purple` | `merged` | `merged`'s wash |
| `pink` | `pink` | `pink`'s wash |
| `chart1`…`chart6` | `chart-1`…`chart-6` | `chart-1`…`chart-6` |
| A CSS colour | core's RGBA | core's RGBA; what lies on it takes core's ink for `ink`, ink at 72% for `muted`, at 55% for `faint` |

open-intelligent-ui's own greys (ink at 4–10%) are Motile's surface layers: hover is the surface's
`row`, pressed and selected its `rowSelected`, a track or a soft fill its `control`, `border` the
surface's border, `border-strong` `border-accent`. Its soft tints are washes (Motile's), so they
stack on any surface. Disabled is `.opacity(.disabled)` (Motile's 0.45, not 0.4). Shadows are
`ShadowSize.md` at `shadow` for popovers and tooltips, `sm` at `shadow-stronger` for pins and glass.
Status colours on a solid button are Motile's: text `destructive-foreground` / `success-foreground`.

Radii (`radius` prop and the components' own): `sm` `Radius.sm` (6), `md` `Radius.md` (8), `lg`
`Radius.lg` (12), `xl` `Radius.xl` (16), `full` half the shorter side, a number in points.
open-intelligent-ui's own 10, 14 and 20 become 8, 12 and 16 (Motile's).

## Type

The UI typeface of each client (Apple's system font, DM Sans in GPUI, Android's system font), never
Inter. No tracking is added. Weights: open-intelligent-ui's 440 is regular, 560 medium, 640
semibold. Every line height below is fixed (minimum = maximum), so the four text systems agree.
Titles and anything marked "digits" use tabular figures (`uiDigits`, `tnum`).

| Style | Size / line | Weight | Apple | GPUI | Android |
| --- | --- | --- | --- | --- | --- |
| `body` (Text md, the default) | 14 / 24 | regular | `Theme.proseFont`, `proseLineHeight` | `PROSE_SIZE`, `PROSE_LINE_HEIGHT` | `Theme.proseFont`, `proseLineHeight` |
| `body-card` (Text md in a card or cell) | 14 / 22 | regular | `proseFont` | `PROSE_SIZE` | `proseFont` |
| `large` (Text lg) | 16 / 26 | regular | `ui(16)` | `UI_FONT` 16 | `Theme.ui(16f)` |
| `small` (Text sm, cells, descriptions) | 13 / 20 | regular | `ui(13)` | 13 | `ui(13f)` |
| `caption` (Text xs, Caption) | 12 / 18 | regular | `Theme.smallFont` | 12 | `Theme.smallFont` |
| `label` (Label, field labels, legends) | 13 / 18 | medium | `ui(13, .medium)` | 13 medium | `ui(13f, Medium)` |
| `title-sm` | 14 / 20 | semibold, digits | `Theme.heading(4)` | `heading_size(4)` | `Theme.heading(4)` |
| `title-md` (Title's default) | 15 / 22 | semibold, digits | `heading(3)` | `heading_size(3)` | `heading(3)` |
| `title-lg` | 17 / 24 | semibold, digits | `heading(2)` | `heading_size(2)` | `heading(2)` |
| `title-xl` | 20 / 26 | semibold, digits | `heading(1)` | `heading_size(1)` | `heading(1)` |
| `title-2xl` | 26 / 30 (narrow 24 / 28) | semibold, digits | `uiDigits(26, .semibold)` | 26 | `uiDigits(26f, SemiBold)` |
| `title-3xl` | 32 / 36 (narrow 28 / 32) | semibold, digits | `uiDigits(32, .semibold)` | 32 | `uiDigits(32f, SemiBold)` |
| `card-title` | 14 / 20 | medium, digits | `uiDigits(14, .medium)` | 14 medium | `uiDigits(14f, Medium)` |
| `card-subtitle` | 13 / 18, `muted-foreground` | regular | `ui(13)` | 13 | `ui(13f)` |
| `card-heading` | 16 / 22 | semibold, digits | `uiDigits(16, .semibold)` | 16 | `uiDigits(16f, SemiBold)` |
| `readout` (slider value, chart values) | 12 / 18 | medium, digits | `uiDigits(12, .medium)` | 12 | `uiDigits(12f, Medium)` |
| `tick` (axis ticks, slider marks, hints) | 11 / 14 | regular, digits | `uiDigits(11)` | 11 | `uiDigits(11f)` |
| `timer` | 28 / 32 | semibold, digits | `uiDigits(28, .semibold)` | 28 | `uiDigits(28f, SemiBold)` |
| `code` | 12.5 / 18 | mono | `Theme.codeFont`, `codeLineHeight` | `CODE_SIZE`, `CODE_LINE_HEIGHT` | `Theme.codeFont` |
| `control` | `ControlSize.font` | medium | `ControlSize` | `ControlSize` | `ControlSize` |

open-intelligent-ui sets its page at 16 and its cards at 14; Motile's prose is 14, so its sizes are
taken a step down (Motile's): 16 → 14, 18 → 16, 15 → 14, 14 → 13, 12 → 12, title 15/17/21/25/32/40
→ 14/15/17/20/26/32, the titles on Motile's own heading scale.

Inline formatting inside any text (core's runs): `**bold**` is semibold in `foreground`, `*italic*`
italic, `~~strike~~` struck through, `` `code` `` `Theme.inlineCodeFont` (12.5 mono) in
`foreground` on `background-accent`, a link `primary`. Text wraps at words, and inside a word too
long for the line (`overflow-wrap: anywhere`). A `\n` in Text is a line break; runs of spaces are
already one (core).

## Spacing

One scale, in four tiers that always nest: tight 4 < inner 8 < group 12 (and 16 for media rows) <
section 24. Control gap 8, field gap 24, action gap 40.

### Flow

A column without a `gap` (Col, a column Box, Card) is a flow: core sets the space above each child
but the first by the first rule that matches.

| Rule (this child, after the one before it) | Space |
| --- | --- |
| A Text, a Caption, or a Row that holds a Badge as a direct child, after a Title | 8 |
| A Title after a Caption | 4 |
| A Text after a Text | 12 |
| Anything, in a flow inside another flow, or in a Card | 12 |
| Anything after a Title | 12 |
| Anything else (the outermost flow) | 24 |

A Divider adds 4 under itself to whatever follows (its own margin), and 4 above itself when it is
first or in a column with a gap. A Form is a column with gap 24; its last child, when it is a Button
or a Row holding a Button, is 40 below the one before.

### The row in the transcript

A `Ui` row is spaced from the rows around it as a prose table is: 12 from the paragraph before and
after (`Typesetter.spaceAbove`'s `table` case and its counterparts).

### A message's values

A user row with `values` (what a program sent with the message, PLAN decision 12) ends its bubble
with a chip: an `.outline` `.small` `ActionButton` on the bubble's surface with Lucide `braces` and
core's `label`, at the bubble's start, 8 under the text or the attachments, 14 above the bubble's
bottom; the bubble is at least as wide as the chip. A press opens Motile's popover (Popover's panel
below: its chrome, padding, placement and closing; at most min(320, window − 24) wide) listing core's
`entries` 8 apart: each its `name` in `caption` `muted-foreground` and 2 under it its `value` in
`small` `foreground`, wrapping.

### Errors

- A field's error (Input, TextArea, Select, DatePicker, Checkbox, RadioGroup): core's text in
  `caption`, `destructive`, 4 under the control, as wide as the field, wrapping.
- The row's errors (`errors`, once the program is complete): one line each in the transcript's own
  error row (the `destructive` wash, `Radius.lg`, Lucide `circle-alert` at 13 and the text at 13 in
  `destructive`, its paddings), 8 under the program, or alone when there is no tree. A program that
  can't run as written is an error of the reply, so it reads like the transcript's other errors.

## Layout

The containers, in terms all three layout systems follow. A child never makes its parent wider than
the column (`min-width: 0` everywhere): what is too wide shrinks, wraps or scrolls.

### Row (and Box with `direction: row`)

- Children side by side, `gap` (default 8 for Row; 0 for a Box), centred vertically by default.
  `align`: start / center / end / stretch (stretch: each child as tall as the row). `justify`:
  start / center / end / between, for the space left when nothing grows.
- Each child's base width, and whether it grows or shrinks:

  | Child | Base width | Grows | Shrinks |
  | --- | --- | --- | --- |
  | Box with `flex: n` | 0 | weight n | no (base 0) |
  | Spacer | 0 | weight 1 | no |
  | Box with a width in points | that width | no | no |
  | Button, IconButton, Badge | its own width | no | no |
  | Card | the row's width | no | yes |
  | Box with a width in % | that share of the row | no | yes |
  | Anything else | its widest single-line width | no | yes |

- Layout: free = row width − base widths − gaps. Free ≥ 0: growing children share it by weight;
  with none, `justify` places the children. Free < 0: shrinking children lose it in proportion to
  their base widths, down to 0; text then wraps at its new width. The row is as tall as its tallest
  child.
- `wrap: true`: children go on the next line when the next one's base width doesn't fit; each line
  is laid out on its own as above; lines are `gap` apart.
- So `Box(flex 2)` beside `Box(flex 1)` takes two thirds of what is left; two Cards side by side
  share the row equally.

### Col (and Box, a column by default)

- Children stacked, `gap` points apart, or the flow's spacing without one.
- `align`: stretch (default: each child as wide as the column), or start / center / end (each child
  at its own width, at most the column's, at that side); center and end also align the lines of
  text inside.
- Grow and `justify` only matter inside a Box with a fixed height: `flex` weights then share the
  height left over.

### Box

`padding` in steps; `background` per the colour table (a raw colour makes what lies on it take core's
inks); `border` 1 in the surface's border; `radius` rounds it and clips its content (without a
radius nothing is clipped); `width`, `height` per Length. Its content lies on the box's inner surface
when it has a token background.

### Grid

- `columns` c (default 2, 1…12), gap g (default 12), width W.
- Without `minChildWidth`: c columns of (W − (c − 1)·g) / c.
- With `minChildWidth` m: least = min(W, max(m, (W − (c − 1)·g) / c)); count = max(1,
  ⌊(W + g) / (least + g)⌋); each column (W − (count − 1)·g) / count. So at most c columns, each at
  least m wide, one column when even one doesn't fit. Core gives the count for a width; clients
  compute it with this formula at their width (it is the same function, `motile_ui_layout`).
- Cells fill rows left to right; a row is as tall as its tallest cell and every cell in it is
  stretched to that height.

### Card

| | |
| --- | --- |
| Look | Motile's card: `card` fill, 1 `border-card`, `Radius.lg`, content on `Surface.card` |
| Padding | 20 (narrow 16), or the `padding` prop |
| Nested (a Card in a Card) | `Radius.md`, padding 16 |
| `soft` | No border; fill the surface's `box` layer; content on that box's surface; padding 20 (narrow 16) |
| Width | The parent's |

### Context rules (core applies them)

- Card title: a Title sm or md that is a Card's first child, or that is inside a Row that is a Card's
  first child, is `card-title`; a Text or Caption right after it (its sibling) is `card-subtitle`,
  2 below it in a Card with a gap (on top of the gap), 8 in a flow (the flow's rule wins).
- Card typography: the direct children of a Card, of a grid cell that is a flow, and of a carousel
  item that is a flow: Title lg, xl, 2xl, 3xl → `card-heading`; Text with no size or md →
  `body-card`.
- Media row: a Row whose direct child is an Image or SearchImage, or a Box holding only one: the row
  aligns to the top, gap 16; that child is clamp(120, a third of the row, 200) wide, never grows or
  shrinks, and its image is 4:3.

## Components

### Layout

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| Col | Above. The root of a program is a Col; at the root its width is the column's. | none |
| Row | Above. | none |
| Box | Above. | none |
| Grid | Above. | none |
| Card | Above. | group (`<section>`) |
| Carousel | See below. | group, its items in order |
| Popover | See below. | trigger: button, expanded state; panel: dialog |
| Divider | 1 line in the surface's border across the width; 4 above and below as in Flow. | hidden |
| Spacer | Grows in a Row (weight 1, base 0); nothing in a column. | hidden |

**Carousel**

- A horizontal track that scrolls and snaps each item's leading edge to the track's leading edge.
  Items are clamp(80, `itemWidth` (240), 640) wide and at most 85% of the column; `gap` (default 12).
  The first item starts at the column's edge; the track has no inset. Items are as tall as the
  tallest; a Card or Pressable item fills that height.
- The track draws 4 outside itself above and below (its focus rings and pins aren't cut), without
  taking that room in the layout. Its scroll indicator is hidden.
- Buttons (Mac and GPUI only, where a pointer is): Motile's glass button (`GlassButton`, the map's
  controls' look; Motile's), 34 round, Lucide `chevron-left` / `chevron-right` 18, 8 from the
  carousel's sides, centred vertically. Each shows unless the track is within 4 of that end. A press
  pages by 85% of the visible width, smoothly, to core's snap target. Touch scrolls natively, with
  snapping to item edges.
- Sends a `scroll index` event when the leading item changes.

**Popover**

- Trigger: its slot laid inline, gap 4 between its parts, no chrome; hover (pointer): its text in
  `muted-foreground`. A press opens and closes.
- Panel: Motile's popover: `popover` fill, 1 `border-popover`, `Radius.lg`, `ShadowSize.md`; padding
  12 vertical, 14 horizontal; a column with gap 8; as wide as its content, at most min(280, window −
  24); text `body` at 14 / 21 in `foreground`. Its content lies on `Surface.popover`.
- Placed 8 below the trigger, its leading edge at the trigger's, kept 12 inside the window; above
  the trigger instead when fewer than 220 are left below and more than 240 above.
- Closes on a press outside it, Esc (Mac, GPUI, a hardware keyboard), and when the transcript
  scrolls. iOS: a popover that never adapts to a sheet. Android: a `Popup`. GPUI: the modal stack's
  popover.

### Text

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| Title | Its style by size and context; `color` per the colour table (default `foreground`). | heading, level 1 for 3xl, 2 for 2xl/xl/lg, 3 for md, 4 for sm |
| Text | `size` → `caption` / `small` / `body` (md) / `large`; `weight` regular / medium / semibold; `italic`; `lineThrough`; `tabularNums` (digits); colour default `foreground`; `maxLines` cuts it with "…" at the end of the last line. A list `text` lays its strings and inline components on one run of lines (below). | text |
| Caption | `caption` in `muted-foreground` (or `color`). | text |
| Label | `label` in `foreground`. | text |
| Badge | Motile's `Chip` (Motile's: 20 tall, 11.5 medium, padding 7, its corners), not open-intelligent-ui's 26 pill. `neutral` and `info`: the chip without a tone. `success`, `warning`, `danger`, `purple`, `pink`: the chip with that tone (`success`, `warning`, `destructive`, `merged`, `pink`): text in the colour on its wash. `icon`: the chip's symbol before the text. Never wraps or shrinks; in a column it keeps its width at the start; in a Row it is centred vertically. | text |
| Link | Text in `primary` (Motile's link colour, no underline), then Lucide `arrow-up-right` at 0.85 of the text size, 2 after it. Pointer: the pointing hand. Opens the URL in the browser. Without a valid URL: plain text. Without text: the URL's host (core). | link |
| List, ListItem | See below. | list; items |
| Markdown | Core parses it into the paragraphs prose rows have (`markdown.rs`); the client sets them with its prose typesetter (`Typesetter.prose` and its GPUI and Android counterparts): Motile's prose, not open-intelligent-ui's 16 / 1.65 rhythm. | as prose |
| Quote | Motile's prose quote: a bar 2 wide in `border` at 1 from the left, the text 14 in, `body` in `muted-foreground`. `cite` 4 under it, `small` in `muted-stronger-foreground`. (open-intelligent-ui's filled callout is Motile's quote.) | blockquote |
| Code | Motile's code box, as prose draws one: the surface's `box` fill, 1 in the surface's border, `Radius.lg`; a header as tall as `CodeHeader` with the language (core's, "code" when none) in `Theme.smallMono` `muted-foreground` 14 from the left and `CopyButton` at the right, which shows its check for a moment; the code in `code` style, 14 from the sides, 12 under it, coloured by core's spans with `Theme.syntax`, wrapping at any character. | code; the button "Copy code" |
| Math | Core's glyph outlines and rules (RaTeX) at 1.12 × the text size around it; paint them in the text colour. Block: on its own line, centred, 4 above and below, scrolls horizontally when wider than the column. `inline`: an inline box in a Text run (below). | image, labelled with the TeX |

**Inline runs** (Text with a list `text`): each component is an inline box that never breaks, 1 from
the text on each side, its bottom 0.15 × the text size under the baseline; a Badge in one is raised
0.1 × the text size more. Apple: a text attachment whose bounds core's size gives; GPUI: GPUI Kit's
inline tokens; Android: `InlineTextContent`. The line grows to fit a taller box.

**List**

| | Lane, gap to the text | Marker |
| --- | --- | --- |
| `bullet` (default) | 14, 8 (24, 12 in a timeline) | A 5 dot in `muted-stronger-foreground`, centred in the lane and on the first line |
| `number` | 24, 12 | A 24 circle in `foreground` with the number in the surface's colour, 13 digits, at the top |
| `none` | no lane | none |
| A ListItem's `marker` of 1–2 letters or digits | 24, 12 (in any list but a wide one) | A pill 20 tall, at least 20 wide, padding 5, `foreground`, text 12 medium digits in the surface's colour |
| Any `marker` longer than 2 in the list (wide) | 76, 16 (narrow 64, 20) | That text, 13 digits in `muted-foreground`, leading, on one line |

- Items 10 apart. An item is its marker lane and its body side by side, both at the top; the first
  line's height centres a dot or a number on it.
- Body: the title in the context's text style (medium when the item has a description or
  children); the description 2 under it, `small` in `muted-foreground`; the children 8 under that,
  stacked with no gap. An item with a description or children has 4 more under it.
- Timeline: items 0 apart, each 18 above the next (the last none). A line 1 wide in `border-accent`
  joins the markers: its centre at 12 in the lane, from 1.7 × the text size under the item's top to
  0.15 × the text size above its bottom. A bullet becomes a 9 circle in the surface's colour with a 2
  ring in `foreground`. Wide: the lane's text gap is 24 (narrow 20) and a 9 ring circle sits at 4
  (narrow 6) after the lane, 0.55 × the text size from the top; the line's centre is 7.5 after the
  lane (narrow 9.5). `none` without text markers: no line.
- Strings in `items` are items with only a title.

### Inputs

Controls are Motile's kit (Motile's look, not open-intelligent-ui's pills): `ActionButton`,
`ActionMenu`, `Segmented`, `InputField`, `TextArea`, `PickMark`, `GlassButton`, `CopyButton`, in
`apps/gpui/src/ui` and `apps/android/.../ui` the same. What is missing (`Slider`, a `success`
variant, a date field) is added to all three kits. While a program streams, core disables its
actions.

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| Button | `ActionButton`. `variant`: solid → `.primary` (with `color`: danger → `.danger`, success → `.success`, the new variant: `success` fill, `success-foreground` text); outline → `.outline`; soft → `.secondary`; ghost → `.ghost`; with a `color` these take it as their `tint` (`destructive`, `success`). `size`: sm → `.small`, md → `.regular`, lg → `.large`. `icon` → its symbol. `block` → fills the width. Otherwise at its own width: at the start in a column, centred vertically in a Row; never wraps or shrinks. `disabled` → `.opacity(.disabled)`, no press. | button, its label |
| IconButton | `ActionButton` with only a symbol, `.regular` (square), the symbol at the size's own. outline → `.outline`, ghost → `.ghost`, solid → `.primary`. `label` is its tooltip (pointer) and its name. | button, `label` or the icon's name |
| Pressable | See below. | button, pressed state when `selected` is given |
| Input | `InputField` (`.filled`, `.regular`), the column's width. `type: number`: the decimal keypad; what is typed stays while it doesn't parse yet (the client's draft), a number when it does, commas dropped, nothing when empty. `placeholder` in the field's placeholder colour. | text field, `label` or `placeholder` |
| TextArea | Motile's `TextArea`, `rows` lines (2–20, default 3) tall, scrolling past that. Return is a line break. | text field |
| Select | `ActionMenu`, `.regular`, filling the width, the chosen label (or the placeholder, "Choose…" by default, in the placeholder colour) at the start and its chevron at the end. Opens the options with a check by the chosen one: a menu on the Mac and GPUI, a sheet on iOS (`Choice`), a dropdown on Android. | pop-up button |
| SegmentedControl | `Segmented`, `.regular`, at its own width at the start, scrolling horizontally when wider than the column; `block` → `fills`. | radio group of radios |
| Slider | See below. | adjustable, `label`, the readout as its value |
| Checkbox | A row at least `ControlSize.regular` tall: `PickMark` (box) then the label (`body`), 8 apart, centred. The whole row toggles. Checked with `lineThrough`: the label struck through in `muted-foreground`. | checkbox |
| RadioGroup | See below. | radio group, `label` |
| DatePicker | A field like `InputField` (`.regular`, the column's width) showing core's formatted date or "Date" in the placeholder colour, Lucide `calendar` at the end. A press opens the system's picker, held to `min`…`max`: Mac a graphical `NSDatePicker` in a popover, iOS an inline `UIDatePicker` in a popover, Android `DatePickerDialog`, GPUI a month grid drawn in its menu. | button, `label`, the date |
| Timer | See below. | timer, `label` or "Timer" |
| Form | A column with gap 24; the last Button (or Row with one) 40 below; a field with a `label` is the label (`label` style) 8 above the control. | form |

A `label` on Input, TextArea, Select or DatePicker: `label` style in `foreground`, 8 above the
control, the pair a column.

**Pressable**

- A column, `padding` steps, `background` per the colour table, `radius` (default `Radius.lg`).
- Without a background (Motile's row light, not open-intelligent-ui's ink overlay and scale): hover
  (pointer) fills it behind the content with the surface's `row`, pressed and selected with its
  `rowSelected`. With a background: hover and press lay `foreground`'s wash over it.
- Selected, unless its only child is a Row: a 1 border in `border-accent` inside its edge and a
  picked `PickMark` (ring) 8 from its top and trailing edges, over the content. A Row in it (a
  checklist line) shows its own state and gets neither.
- A press on a control inside it goes to that control, not to the Pressable. Enter and Space press
  it when focused. Its accessibility label is its text (core gives it).

**Slider** (new in the kits)

- Head, when there is a `label` or a `unit`: the label (`small`, `foreground`, wrapping) and core's
  readout (`readout`, on one line) at either end, on one baseline, at least 12 apart.
- Under it a control `ControlSize.large` tall: a track `ControlSize.small` tall, fully round, in the
  surface's `control`, centred vertically; an 18 thumb in `foreground` inside it, 3 from its ends.
  The thumb's centre is at 12 + (W − 24) × t, t = (value − min) / (max − min). No fill before the
  thumb.
- Marks: a row 18 tall, 2 into the control's bottom; each label `tick` in `muted-foreground`, centred
  under its t, the first leading at 0 when t ≤ 0.02, the last trailing at W when t ≥ 0.98.
- Dragging or a press on the track moves the thumb, snapped to core's `step`; the client keeps its
  value while dragging and sends throttled updates. Arrow keys step it (Mac, GPUI); accessibility
  increments and decrements by a step.

**RadioGroup**

- `label` (`label` style) 10 above the options.
- `list` (default): rows 2 apart, each at least `ControlSize.regular` tall: `PickMark` (ring) then the
  text, 8 apart, centred; a description 2 under the label in `small` `muted-foreground`.
- `pills`: wrapping, 8 apart: `ActionButton` `.regular`, `.secondary`, the chosen one `.accent`
  (Motile's choice that is on, not an ink pill).
- `cards`: a grid of columns at least min(W, 200) wide: count = max(1, ⌊(W + 8) / 208⌋), 8 apart.
  Each a box padding 14 vertical, 16 horizontal, `Radius.lg`, 1 in the surface's border; hover the
  surface's `row`; chosen: 1 `primary` border and `primary`'s wash. The label medium, the
  description 2 under it in `small` `muted-foreground`, both leading, at the top.

**Timer**

- A box: 1 in the surface's border, `Radius.lg`, padding 12 vertical, 16 horizontal; its read and
  its buttons at either end, wrapping under each other when they don't fit, 12 apart, centred.
- Read: the `label` (`label` style), 2 above the time (`timer` style; `success` once done).
- Buttons 8 apart, `.small`: `.primary` with Lucide `play` / `pause` ("Start", "Pause", "Resume");
  `.ghost` with `rotate-ccw` ("Reset"). Their labels and whether they are disabled are core's; core
  counts the seconds.

### Media and data

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| SearchImage, Image | See below. | image, `alt` or `query` |
| Icon | The Lucide glyph core names, 14 / 18 / 24 / 32 for sm / md / lg / xl, in `color` (default the text colour); its box as wide and tall as the size; never shrinks. An unknown name draws nothing. | hidden |
| Table | See below. | table, header row |
| Chart | See below. | image, core's label ("bar chart of … by …") |
| Map, MapMarker | See below. MapMarker draws nothing itself. | region "Map"; pins as buttons |
| YouTube | 16:9, the column's width, `Radius.lg`, clipped, `overlay` (black) behind; the web view of PLAN decision 19. | the title, or "YouTube video" |

**Image** (SearchImage and Image)

- The column's width (or the media row's), height width / `aspectRatio` (0.2–5, default 1.5), radius
  (default `Radius.lg`), clipped, the surface's `box` behind.
- Waiting: Motile's shimmer over the box (a band from `shimmer-band-edge` to `shimmer-band-middle`
  and back, sweeping across every 1.4 s, linear). Loaded: the photo fades in over 0.3 s, ease;
  `cover` fills and crops, `contain` fits whole on the box. Nothing found or failed: Lucide `image`
  26 in `muted-stronger-foreground`, centred, no shimmer. The photos and the fallback order are core's
  and the server's (PLAN decision 17).

**Table** (Motile's: a card, Motile's prose table's text sizes)

- A card (`card`, 1 `border-card`, `Radius.lg`), clipped, the column's width; the cells inside on
  `Surface.card`.
- Header row: `card-accent` fill; `label` style in `foreground`; one line each; padding 8 vertical,
  12 horizontal (narrow 8, 10).
- Body cells: `small` in `foreground`; padding 10 vertical, 12 horizontal (narrow 8, 10); centred
  vertically; a 1 line in `border-card` under every row but the last, and under the header. A cell
  that is a component is laid out at the cell's width.
- Numeric columns (core's flag): header and cells trailing, digits, one line.
- Column widths (the automatic table layout): each column's least width is its widest unbreakable
  part (a word, or a one-line cell) plus padding; its most, its widest cell on one line plus
  padding. With W the table's width: Σmost ≤ W → each gets most + (W − Σmost) × most / Σmost. Σleast
  ≤ W → least + (W − Σleast) × (most − least) / Σ(most − least). Otherwise each gets its least and
  the table scrolls horizontally inside its frame.
- No row hover (Motile's tables have none).

**Chart** (a canvas scene; the chrome around it)

- The scene is core's for (node, width): the plot `height` (140–480, default 240) tall and the
  column wide; gridlines (zero solid, the rest dashed 3/4), tick labels, category labels, bars
  (the two corners away from the baseline 4, the others square; in a stack only the topmost
  segment with a height), lines (2.2 wide, round), dots (3, 4.5 hovered, ringed 1.5 in the surface's colour),
  area gradients, arcs (pie and donut, 2 apart in the surface's colour), scatter points (5, 6.5
  hovered, 78%), and the hover bands with their tooltip rows. Paint it: colours are tokens
  (`border` for grid, `muted-stronger-foreground` ticks in `tick`, `muted-foreground` categories in
  `caption`, `chart-1…6` series, the surface's `row` for a hovered bar band, `border-accent` dashed
  3/3 for the hover guide). Text is drawn by the client at core's anchor (end-, middle-aligned).
  No label leaves the scene: core estimates a label at 0.7 em a character, makes the y gutter
  that wide plus 12, and sets an x label that would cross an edge against that edge instead of
  centring it.
- Hover (pointer): the band under the pointer is lit and its tooltip shows; leaving hides it. Touch:
  a tap shows the tapped band's, a horizontal drag moves it, a vertical drag scrolls the transcript,
  a tap elsewhere hides it.
- Tooltip: Motile's popover look (`popover`, 1 `border-popover`, `Radius.lg`, `ShadowSize.md`),
  padding 10 vertical, 12 horizontal, at least 110 wide, rows 3 apart; centred on core's x (kept 70
  from the plot's ends), its top at core's y. The heading `caption` semibold `foreground`; each row
  an 8 dot in its series' colour, 6, the series name in `caption` `muted-foreground`, and the value
  in `readout` `foreground` at the end, at least 8 after the name.
- Legend (two series or more, not pie): 10 under the plot, wrapping, items 16 apart and lines 6
  apart; an 8 dot, 7, the name in `caption` `muted-foreground`.
- Pie and donut: the disc (core's size) and a legend column beside it, 24 apart; the legend at least
  160 wide and growing, else under the disc; rows 9 apart, each an 8 dot, 8, the label (`small`,
  wrapping, growing) and the share (`small` medium digits).

**Map** (MapLibre, PLAN decision 18)

- Height min(clamp(200, `height` (400), 720), max(280, 0.82 × the window's width)); the column's
  width; `Radius.xl` (Motile's 16 for its 20), clipped; `map-water` until tiles draw. Live while on
  screen, a snapshot otherwise.
- Style: `packages/theme/map-style-light.json` and `-dark.json`, by the appearance; switching
  appearance swaps the style and keeps the camera. The client fills the `route` source (a
  LineString of core's points) and the `pins` source (Points with `label`, `number`, `icon` (an
  image id, for an emoji pin) and `selected`), and adds each emoji pin's image: the emoji drawn at 18
  into a 34 square, named by its `icon` id, at the screen's scale.
- Controls (Motile's `GlassButton` look over the map): at the top left, 12 in, 8 apart: the filter
  (only with two categories or more: a glass capsule 34 tall, `control` text "All places" or the
  category, Lucide `chevron-down` 14, 12 and 30 padding; its choices are `Choice`s), recenter (34
  round, Lucide `locate-fixed` 18, "Show every place") and info (34 round, Lucide `info` 18, "About
  this map", expanded state). At the top right, 12 in: Expand / Collapse (a capsule 28 tall, padding
  4 and 10, Lucide `maximize-2` / `minimize-2` 14 after the text). Zoom: pinch on touch; on Mac and
  GPUI + and − buttons, 28 each, glass, at the bottom right, showing while the pointer is over the
  map.
- Note (info open): 56 from the top, 12 from the left, at most 275 wide, padding 12, Motile's popover
  look, `caption` in `muted-foreground`: core's text. Error: the same box 28 from the bottom: "Map
  tiles are temporarily unavailable. The places are still shown." Route overview: 22 from the
  bottom, 12 from the left, padding 2 and 7, `Radius.sm`, the surface's colour at `lit`, 10 / 16
  `muted-foreground`: "Route overview".
- Attribution: the style's, at the bottom left, 9 / 12 in `muted-foreground` on the surface's colour
  at `lit`, links opening in the browser.
- Gestures: a drag pans; the wheel and the trackpad scroll the transcript, never zoom; no rotation
  or pitch. On touch, one finger scrolls the transcript and two pan and pinch the map. A pin press
  selects it (`select` event); while expanded it also collapses.
- Camera (core's targets): the first marker at zoom 10.5 while the program streams; once it ends,
  fit every visible marker with padding 70 top, 72 sides, 50 bottom, zoom at most 13, flying 920 ms;
  never again once the user moved it. Recenter fits with padding 55, 40 right, 35 bottom, 34 left,
  zoom at most 14, animated. After framing, the route draws from its start over 760 ms, ease in-out
  (quadratic), through core's partial line.
- Expanded: Mac and GPUI: over the window, 24 in (12 under 520 wide), the surface's colour around it
  at `lit`, `ShadowSize.xl` at `shadow-stronger`; Esc collapses. iOS and Android: a full-screen
  cover, Collapse at its top right.
- Pins have no entry animation (they are style layers; open-intelligent-ui's 104 ms pop is dropped).

### Play

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| Flashcard | See below. | button, pressed when flipped; the face that shows |
| Celebration | Draws nothing in the row. When core fires it: 70 pieces, 8 × 12, corners 2, in `chart-1…6`, over the whole window, under nothing and above everything, taking no presses, for 2.2 s, each moving on core's keyframes (curve cubic-bezier(0.2, 0.6, 0.4, 1)). | hidden |

**Flashcard**

- The column's width, at least 168 tall (or as tall as its taller face). Each face: padding 20 top,
  18 sides, 30 bottom; a column, centred both ways, gap 8, text centred; `Radius.lg`; 1 in the
  surface's border (hover (pointer): `border-accent`). Front: the surface's colour. Back: the
  surface's `box`.
- A string face is a word: front `title-lg`, back `body` medium. The hint (core's, "Tap to flip" by
  default) `tick` in `muted-stronger-foreground`, centred, 10 above the bottom, front only.
- A press flips it (`flip` event): core's flip, a horizontal squash: the front narrows to 0 about its
  centre, the back widens from 0; 450 ms in all, cubic-bezier(0.3, 0.7, 0.3, 1). GPUI has no 3D, so
  every client draws this one flip.

### Drawing

| Component | Geometry and behaviour | Accessibility |
| --- | --- | --- |
| Svg | Width: `width`, else the column's, at most max(280, 1.5 × the viewBox's width), centred. Height: `height`, else width × the viewBox's height / width. The viewBox fits inside, centred, aspect kept. Not clipped. Paint core's flattened paths in order. | image |
| G, Circle, Ellipse, Rect, Line, Path, Polyline, Polygon | Core's paths with fill, stroke, width and opacity resolved; strokes with round caps and joins. Colour tokens as text colours; the default is `foreground` (the text colour). | none |
| SvgText | Drawn by the client in its UI font at core's position (the baseline), size (× the viewBox's scale) and anchor (start, middle, end), weight regular / medium / semibold, in its fill (default `foreground`). | text |
| HtmlView | See below. | the title, or "Interactive view" |

**HtmlView**

- The column's width; height `height` (80–2400), else what the page reports (80–2400), 360 until it
  does. 1 in the surface's border, `Radius.lg`, clipped, the surface's colour behind.
- While the program streams: Motile's `Spinner` and "Preparing the interactive view" (`small`,
  `muted-foreground`), centred. Then the sandboxed web view of PLAN decision 19, in the appearance's
  colour scheme.

## States

| State | Where | Look |
| --- | --- | --- |
| Hover | Mac, GPUI (pointer only) | The kit's own for controls; the surface's `row` for rows, choices and pressables |
| Pressed | All | As hover, or the kit's pressed look; `rowSelected` for pressables |
| Selected | Segmented, choices, pressables, pins | Per component above |
| Disabled | Buttons, inputs, actions while streaming | `.opacity(.disabled)`, no presses, no hover |
| Focus | Mac, GPUI; iOS and Android with a keyboard | The platform's focus ring; GPUI a 2 ring in `ring`, 2 outside the control, its radius plus 2 |

## Motion

| What | Duration | Curve | Reduced motion |
| --- | --- | --- | --- |
| Control colours (buttons, choices, links, pressables) | The kit's | The kit's | instant |
| Segmented selection | The kit's | The kit's | instant |
| Image fade-in | 300 ms | ease | instant |
| Image shimmer | 1.4 s a sweep, repeating | linear | still |
| Carousel paging | the platform's smooth scroll | | jumps |
| Flashcard flip | 450 ms | cubic-bezier(0.3, 0.7, 0.3, 1) | the face swaps at once |
| Celebration | 1.2–2.1 s a piece, 0–360 ms delays, 2.2 s in all | cubic-bezier(0.2, 0.6, 0.4, 1) | nothing drawn |
| Map camera fit | 920 ms | MapLibre's fly | jumps |
| Map route draw | 760 ms | ease in-out quadratic | drawn at once |
| Glass controls' hover | 120 ms | ease-out | instant |

Reduced motion is the system's setting: `NSWorkspace.accessibilityDisplayShouldReduceMotion`,
`UIAccessibility.isReduceMotionEnabled`, Android's animator duration scale of 0, and GPUI's reading
of the desktop's setting where it has one.
