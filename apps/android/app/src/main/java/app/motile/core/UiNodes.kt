// Written by scripts/ui-types.sh from crates/core/src/render/openui/tree.rs: do not edit.

package app.motile.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

data class UiNode(
    /** The path from the program's root, its props and indexes joined by `/`: `children/2`. */
    val id: String,
    val a11y: UiA11y?,
    val kind: UiKind,
) {
    companion object {
        fun from(json: JSON): UiNode? {
            return UiNode(
                id = json["id"].uiOptString() ?: "",
                a11y = json["a11y"].uiObject()?.let { UiA11y.from(it) },
                kind = UiKind.from(json) ?: return null,
            )
        }
    }
}

data class UiA11y(
    val role: UiRole,
    val label: String?,
    val value: String?,
    /** A heading's level. */
    val level: Int?,
    /** Pressed, expanded or checked, by the role. */
    val on: Boolean?,
) {
    companion object {
        fun from(json: JSON): UiA11y {
            return UiA11y(
                role = UiRole.of(json["role"].uiOptString()) ?: UiRole.Group,
                label = json["label"].uiOptString(),
                value = json["value"].uiOptString(),
                level = json["level"].uiOptInt(),
                on = json["on"].uiOptBool(),
            )
        }
    }
}

enum class UiRole(val raw: String) {
    Group("group"),
    Button("button"),
    Dialog("dialog"),
    Heading("heading"),
    Text("text"),
    Link("link"),
    List("list"),
    ListItem("list_item"),
    Blockquote("blockquote"),
    Code("code"),
    Image("image"),
    TextField("text_field"),
    PopUpButton("pop_up_button"),
    RadioGroup("radio_group"),
    Adjustable("adjustable"),
    Checkbox("checkbox"),
    Timer("timer"),
    Form("form"),
    Table("table"),
    Region("region"),
    Hidden("hidden");

    companion object {
        fun of(raw: String?): UiRole? = entries.firstOrNull { it.raw == raw }
    }
}

sealed interface UiKind {
    /** Its children stacked, each `spacing` below the one before. */
    data class Col(
        val children: kotlin.collections.List<UiNode>,
        val align: UiAlign,
        val spacing: kotlin.collections.List<Float>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Col {
                return Col(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    align = UiAlign.of(json["align"].uiOptString()) ?: UiAlign.Start,
                    spacing = json["spacing"].uiList().map { it.uiOptDouble()?.toFloat() ?: 0f },
                )
            }
        }
    }

    data class Row(
        val children: kotlin.collections.List<UiNode>,
        val gap: Float,
        val align: UiAlign,
        val justify: UiJustify,
        val wrap: Boolean,
        /** How wide each child starts and whether it grows or shrinks. */
        val sizing: kotlin.collections.List<UiSizing>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Row {
                return Row(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    gap = json["gap"].uiOptDouble()?.toFloat() ?: 0f,
                    align = UiAlign.of(json["align"].uiOptString()) ?: UiAlign.Start,
                    justify = UiJustify.of(json["justify"].uiOptString()) ?: UiJustify.Start,
                    wrap = json["wrap"].uiOptBool() ?: false,
                    sizing = json["sizing"].uiList().map { UiSizing.from(it.uiObject() ?: EmptyJSON) },
                )
            }
        }
    }

    data class Box(
        val children: kotlin.collections.List<UiNode>,
        val direction: UiDirection,
        /** Between children in a row; a column has `spacing`. */
        val gap: Float,
        val spacing: kotlin.collections.List<Float>,
        val sizing: kotlin.collections.List<UiSizing>,
        val padding: Float,
        val background: UiColor?,
        /** The content lies on the box's inner surface. */
        val innerSurface: Boolean,
        /** Rounds and clips; nothing is clipped without one. */
        val radius: UiRadius?,
        val border: Boolean,
        val width: UiLength?,
        val height: UiLength?,
        val flex: Float?,
        val align: UiAlign,
        val justify: UiJustify,
        val wrap: Boolean,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Box {
                return Box(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    direction = UiDirection.of(json["direction"].uiOptString()) ?: UiDirection.Row,
                    gap = json["gap"].uiOptDouble()?.toFloat() ?: 0f,
                    spacing = json["spacing"].uiList().map { it.uiOptDouble()?.toFloat() ?: 0f },
                    sizing = json["sizing"].uiList().map { UiSizing.from(it.uiObject() ?: EmptyJSON) },
                    padding = json["padding"].uiOptDouble()?.toFloat() ?: 0f,
                    background = UiColor.from(json["background"]),
                    innerSurface = json["inner_surface"].uiOptBool() ?: false,
                    radius = json["radius"].uiObject()?.let { UiRadius.from(it) },
                    border = json["border"].uiOptBool() ?: false,
                    width = json["width"].uiObject()?.let { UiLength.from(it) },
                    height = json["height"].uiObject()?.let { UiLength.from(it) },
                    flex = json["flex"].uiOptDouble()?.toFloat(),
                    align = UiAlign.of(json["align"].uiOptString()) ?: UiAlign.Start,
                    justify = UiJustify.of(json["justify"].uiOptString()) ?: UiJustify.Start,
                    wrap = json["wrap"].uiOptBool() ?: false,
                )
            }
        }
    }

    data class Grid(
        val children: kotlin.collections.List<UiNode>,
        /** The most columns; `grid_columns` gives how many fit a width. */
        val columns: Int,
        val minChildWidth: Float?,
        val gap: Float,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Grid {
                return Grid(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    columns = json["columns"].uiOptInt() ?: 0,
                    minChildWidth = json["min_child_width"].uiOptDouble()?.toFloat(),
                    gap = json["gap"].uiOptDouble()?.toFloat() ?: 0f,
                )
            }
        }
    }

    data class Card(
        val children: kotlin.collections.List<UiNode>,
        val soft: Boolean,
        /** A card in a card. */
        val nested: Boolean,
        /** The program's padding, else the client's for the column's width. */
        val padding: Float?,
        val spacing: kotlin.collections.List<Float>,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Card {
                return Card(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    soft = json["soft"].uiOptBool() ?: false,
                    nested = json["nested"].uiOptBool() ?: false,
                    padding = json["padding"].uiOptDouble()?.toFloat(),
                    spacing = json["spacing"].uiList().map { it.uiOptDouble()?.toFloat() ?: 0f },
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Carousel(
        val children: kotlin.collections.List<UiNode>,
        val itemWidth: Float,
        val gap: Float,
        /** The items as tall as the tallest: a Card or a Pressable. */
        val fill: kotlin.collections.List<Boolean>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Carousel {
                return Carousel(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    itemWidth = json["item_width"].uiOptDouble()?.toFloat() ?: 0f,
                    gap = json["gap"].uiOptDouble()?.toFloat() ?: 0f,
                    fill = json["fill"].uiList().map { it.uiOptBool() ?: false },
                )
            }
        }
    }

    data class Popover(
        val trigger: UiSlot,
        val content: UiSlot,
        val open: Boolean,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Popover {
                return Popover(
                    trigger = UiSlot.from(json["trigger"].uiObject() ?: EmptyJSON),
                    content = UiSlot.from(json["content"].uiObject() ?: EmptyJSON),
                    open = json["open"].uiOptBool() ?: false,
                )
            }
        }
    }

    data object Divider : UiKind

    data object Spacer : UiKind

    data class Title(
        val text: UiRich,
        val style: UiTextStyle,
        val color: UiColor,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Title? {
                return Title(
                    text = UiRich.from(json["text"].uiObject() ?: EmptyJSON),
                    style = UiTextStyle.of(json["style"].uiOptString()) ?: UiTextStyle.Body,
                    color = UiColor.from(json["color"]) ?: return null,
                )
            }
        }
    }

    data class Text(
        /** A U+FFFC stands for each of `inlines`, in order. */
        val text: UiRich,
        val inlines: kotlin.collections.List<UiNode>,
        val style: UiTextStyle,
        val color: UiColor,
        val weight: UiWeight,
        val italic: Boolean,
        val strike: Boolean,
        val digits: Boolean,
        val maxLines: Int?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Text? {
                return Text(
                    text = UiRich.from(json["text"].uiObject() ?: EmptyJSON),
                    inlines = json["inlines"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    style = UiTextStyle.of(json["style"].uiOptString()) ?: UiTextStyle.Body,
                    color = UiColor.from(json["color"]) ?: return null,
                    weight = UiWeight.of(json["weight"].uiOptString()) ?: UiWeight.Regular,
                    italic = json["italic"].uiOptBool() ?: false,
                    strike = json["strike"].uiOptBool() ?: false,
                    digits = json["digits"].uiOptBool() ?: false,
                    maxLines = json["max_lines"].uiOptInt(),
                )
            }
        }
    }

    data class Caption(
        val text: UiRich,
        val style: UiTextStyle,
        val color: UiColor,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Caption? {
                return Caption(
                    text = UiRich.from(json["text"].uiObject() ?: EmptyJSON),
                    style = UiTextStyle.of(json["style"].uiOptString()) ?: UiTextStyle.Body,
                    color = UiColor.from(json["color"]) ?: return null,
                )
            }
        }
    }

    data class Label(
        val text: String,
        val color: UiColor,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Label? {
                return Label(
                    text = json["text"].uiOptString() ?: "",
                    color = UiColor.from(json["color"]) ?: return null,
                )
            }
        }
    }

    data class Badge(
        val text: String,
        /** The chip's tone, a token; none for neutral and info. */
        val tone: String?,
        val icon: UiGlyph?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Badge {
                return Badge(
                    text = json["text"].uiOptString() ?: "",
                    tone = json["tone"].uiOptString(),
                    icon = json["icon"].uiObject()?.let { UiGlyph.from(it) },
                )
            }
        }
    }

    data class Link(
        val text: String,
        /** None when the program's URL isn't http(s): the text is plain. */
        val url: String?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Link {
                return Link(
                    text = json["text"].uiOptString() ?: "",
                    url = json["url"].uiOptString(),
                )
            }
        }
    }

    data class List(
        /** ListItems. */
        val items: kotlin.collections.List<UiNode>,
        val marker: UiListMarker,
        val timeline: Boolean,
        /** A marker longer than two characters: the markers have a lane of text. */
        val wide: Boolean,
    ) : UiKind {
        companion object {
            fun from(json: JSON): List {
                return List(
                    items = json["items"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    marker = UiListMarker.of(json["marker"].uiOptString()) ?: UiListMarker.Bullet,
                    timeline = json["timeline"].uiOptBool() ?: false,
                    wide = json["wide"].uiOptBool() ?: false,
                )
            }
        }
    }

    data class ListItem(
        val title: UiRich,
        val style: UiTextStyle,
        /** Medium, as it has a description or children. */
        val medium: Boolean,
        val description: UiRich?,
        val marker: UiItemMarker,
        val children: kotlin.collections.List<UiNode>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): ListItem? {
                return ListItem(
                    title = UiRich.from(json["title"].uiObject() ?: EmptyJSON),
                    style = UiTextStyle.of(json["style"].uiOptString()) ?: UiTextStyle.Body,
                    medium = json["medium"].uiOptBool() ?: false,
                    description = json["description"].uiObject()?.let { UiRich.from(it) },
                    marker = UiItemMarker.from(json["marker"].uiObject() ?: EmptyJSON) ?: return null,
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                )
            }
        }
    }

    data class Markdown(
        val blocks: kotlin.collections.List<UiMarkdownBlock>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Markdown {
                return Markdown(
                    blocks = json["blocks"].uiList().mapNotNull { UiMarkdownBlock.from(it.uiObject() ?: EmptyJSON) },
                )
            }
        }
    }

    data class Quote(
        val text: UiRich,
        val cite: UiRich?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Quote {
                return Quote(
                    text = UiRich.from(json["text"].uiObject() ?: EmptyJSON),
                    cite = json["cite"].uiObject()?.let { UiRich.from(it) },
                )
            }
        }
    }

    data class Code(
        /** "code" when the program names none. */
        val language: String,
        val code: String,
        /** `None` until the code has been highlighted, like a code row's. */
        val spans: kotlin.collections.List<Int>?,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Code {
                return Code(
                    language = json["language"].uiOptString() ?: "",
                    code = json["code"].uiOptString() ?: "",
                    spans = json["spans"]?.takeIf { it !is JsonNull }?.let { it.uiList().map { it.uiOptInt() ?: 0 } },
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Math(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Math {
                return Math(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Button(
        val label: String,
        val variant: UiButtonVariant,
        /** The colour an outline, soft or ghost button takes. */
        val tint: String?,
        val size: UiControlSize,
        val icon: UiGlyph?,
        val block: Boolean,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Button {
                return Button(
                    label = json["label"].uiOptString() ?: "",
                    variant = UiButtonVariant.of(json["variant"].uiOptString()) ?: UiButtonVariant.Primary,
                    tint = json["tint"].uiOptString(),
                    size = UiControlSize.of(json["size"].uiOptString()) ?: UiControlSize.Small,
                    icon = json["icon"].uiObject()?.let { UiGlyph.from(it) },
                    block = json["block"].uiOptBool() ?: false,
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class IconButton(
        val icon: UiGlyph?,
        val label: String,
        val tooltip: String?,
        val variant: UiButtonVariant,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): IconButton {
                return IconButton(
                    icon = json["icon"].uiObject()?.let { UiGlyph.from(it) },
                    label = json["label"].uiOptString() ?: "",
                    tooltip = json["tooltip"].uiOptString(),
                    variant = UiButtonVariant.of(json["variant"].uiOptString()) ?: UiButtonVariant.Primary,
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Pressable(
        val children: kotlin.collections.List<UiNode>,
        val padding: Float,
        val background: UiColor?,
        val innerSurface: Boolean,
        val radius: UiRadius,
        val selected: Boolean?,
        /** Selected, and its only child isn't a Row: the ring and the picked mark. */
        val ring: Boolean,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Pressable {
                return Pressable(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    padding = json["padding"].uiOptDouble()?.toFloat() ?: 0f,
                    background = UiColor.from(json["background"]),
                    innerSurface = json["inner_surface"].uiOptBool() ?: false,
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                    selected = json["selected"].uiOptBool(),
                    ring = json["ring"].uiOptBool() ?: false,
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Input(
        val value: String,
        val number: Boolean,
        val label: String?,
        val placeholder: String?,
        val error: String?,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Input {
                return Input(
                    value = json["value"].uiOptString() ?: "",
                    number = json["number"].uiOptBool() ?: false,
                    label = json["label"].uiOptString(),
                    placeholder = json["placeholder"].uiOptString(),
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class TextArea(
        val value: String,
        val label: String?,
        val placeholder: String?,
        val rows: Int,
        val error: String?,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): TextArea {
                return TextArea(
                    value = json["value"].uiOptString() ?: "",
                    label = json["label"].uiOptString(),
                    placeholder = json["placeholder"].uiOptString(),
                    rows = json["rows"].uiOptInt() ?: 0,
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Select(
        val value: String,
        val options: kotlin.collections.List<UiChoice>,
        val selected: Int?,
        val label: String?,
        /** Shown when no option is chosen. */
        val placeholder: String,
        val error: String?,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Select {
                return Select(
                    value = json["value"].uiOptString() ?: "",
                    options = json["options"].uiList().map { UiChoice.from(it.uiObject() ?: EmptyJSON) },
                    selected = json["selected"].uiOptInt(),
                    label = json["label"].uiOptString(),
                    placeholder = json["placeholder"].uiOptString() ?: "",
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class SegmentedControl(
        val value: String,
        val options: kotlin.collections.List<UiChoice>,
        val selected: Int?,
        val block: Boolean,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): SegmentedControl {
                return SegmentedControl(
                    value = json["value"].uiOptString() ?: "",
                    options = json["options"].uiList().map { UiChoice.from(it.uiObject() ?: EmptyJSON) },
                    selected = json["selected"].uiOptInt(),
                    block = json["block"].uiOptBool() ?: false,
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Slider(
        val value: Double,
        val min: Double,
        val max: Double,
        val step: Double,
        /** Where the thumb is, from 0 to 1. */
        val at: Double,
        val label: String?,
        val readout: String,
        /** The label and the readout show above the track. */
        val head: Boolean,
        val marks: kotlin.collections.List<UiSliderMark>,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Slider {
                return Slider(
                    value = json["value"].uiOptDouble() ?: 0.0,
                    min = json["min"].uiOptDouble() ?: 0.0,
                    max = json["max"].uiOptDouble() ?: 0.0,
                    step = json["step"].uiOptDouble() ?: 0.0,
                    at = json["at"].uiOptDouble() ?: 0.0,
                    label = json["label"].uiOptString(),
                    readout = json["readout"].uiOptString() ?: "",
                    head = json["head"].uiOptBool() ?: false,
                    marks = json["marks"].uiList().map { UiSliderMark.from(it.uiObject() ?: EmptyJSON) },
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Checkbox(
        val checked: Boolean,
        val label: String,
        val strike: Boolean,
        val error: String?,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Checkbox {
                return Checkbox(
                    checked = json["checked"].uiOptBool() ?: false,
                    label = json["label"].uiOptString() ?: "",
                    strike = json["strike"].uiOptBool() ?: false,
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class RadioGroup(
        val value: String,
        val options: kotlin.collections.List<UiChoice>,
        val selected: Int?,
        val label: String?,
        val variant: UiRadioVariant,
        val error: String?,
        val disabled: UiDisabled?,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): RadioGroup {
                return RadioGroup(
                    value = json["value"].uiOptString() ?: "",
                    options = json["options"].uiList().map { UiChoice.from(it.uiObject() ?: EmptyJSON) },
                    selected = json["selected"].uiOptInt(),
                    label = json["label"].uiOptString(),
                    variant = UiRadioVariant.of(json["variant"].uiOptString()) ?: UiRadioVariant.List,
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class DatePicker(
        /** `YYYY-MM-DD`. */
        val value: String?,
        /** The date as the field shows it, or "Date". */
        val text: String,
        val min: String?,
        val max: String?,
        val label: String?,
        val error: String?,
        val disabled: UiDisabled?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): DatePicker {
                return DatePicker(
                    value = json["value"].uiOptString(),
                    text = json["text"].uiOptString() ?: "",
                    min = json["min"].uiOptString(),
                    max = json["max"].uiOptString(),
                    label = json["label"].uiOptString(),
                    error = json["error"].uiOptString(),
                    disabled = UiDisabled.of(json["disabled"].uiOptString()),
                )
            }
        }
    }

    data class Timer(
        val label: String?,
        val time: String,
        val seconds: Double,
        val running: Boolean,
        val done: Boolean,
        val toggle: UiTimerButton,
        val reset: UiTimerButton,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Timer {
                return Timer(
                    label = json["label"].uiOptString(),
                    time = json["time"].uiOptString() ?: "",
                    seconds = json["seconds"].uiOptDouble() ?: 0.0,
                    running = json["running"].uiOptBool() ?: false,
                    done = json["done"].uiOptBool() ?: false,
                    toggle = UiTimerButton.from(json["toggle"].uiObject() ?: EmptyJSON),
                    reset = UiTimerButton.from(json["reset"].uiObject() ?: EmptyJSON),
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Form(
        val children: kotlin.collections.List<UiNode>,
        val spacing: kotlin.collections.List<Float>,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Form {
                return Form(
                    children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
                    spacing = json["spacing"].uiList().map { it.uiOptDouble()?.toFloat() ?: 0f },
                )
            }
        }
    }

    data class SearchImage(
        val query: String,
        val aspectRatio: Float,
        val radius: UiRadius,
        val fit: UiFit,
        val photos: UiPhotos,
    ) : UiKind {
        companion object {
            fun from(json: JSON): SearchImage {
                return SearchImage(
                    query = json["query"].uiOptString() ?: "",
                    aspectRatio = json["aspect_ratio"].uiOptDouble()?.toFloat() ?: 0f,
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                    fit = UiFit.of(json["fit"].uiOptString()) ?: UiFit.Cover,
                    photos = UiPhotos.from(json["photos"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Image(
        val src: String?,
        val alt: String,
        val aspectRatio: Float,
        val radius: UiRadius,
        val fit: UiFit,
        val photos: UiPhotos,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Image {
                return Image(
                    src = json["src"].uiOptString(),
                    alt = json["alt"].uiOptString() ?: "",
                    aspectRatio = json["aspect_ratio"].uiOptDouble()?.toFloat() ?: 0f,
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                    fit = UiFit.of(json["fit"].uiOptString()) ?: UiFit.Cover,
                    photos = UiPhotos.from(json["photos"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Icon(
        /** None for a name Lucide doesn't have: nothing is drawn. */
        val icon: UiGlyph?,
        val size: Float,
        val color: UiColor,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Icon? {
                return Icon(
                    icon = json["icon"].uiObject()?.let { UiGlyph.from(it) },
                    size = json["size"].uiOptDouble()?.toFloat() ?: 0f,
                    color = UiColor.from(json["color"]) ?: return null,
                )
            }
        }
    }

    data class Table(
        val columns: kotlin.collections.List<UiTableColumn>,
        val rows: kotlin.collections.List<kotlin.collections.List<UiTableCell>>,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Table {
                return Table(
                    columns = json["columns"].uiList().map { UiTableColumn.from(it.uiObject() ?: EmptyJSON) },
                    rows = json["rows"].uiList().map { it.uiList().mapNotNull { UiTableCell.from(it.uiObject() ?: EmptyJSON) } },
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Chart(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Chart {
                return Chart(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Map(
        val pins: kotlin.collections.List<UiMapPin>,
        val route: UiRouteKind,
        /** `[lat, lng]` along the streets, or straight between the pins shown. */
        val line: kotlin.collections.List<kotlin.collections.List<Double>>,
        val selected: String?,
        val height: Float,
        val categories: kotlin.collections.List<String>,
        /** The category shown, none for every place. */
        val filter: String?,
        val note: String,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Map {
                return Map(
                    pins = json["pins"].uiList().map { UiMapPin.from(it.uiObject() ?: EmptyJSON) },
                    route = UiRouteKind.of(json["route"].uiOptString()) ?: UiRouteKind.Off,
                    line = json["line"].uiList().map { it.uiList().map { it.uiOptDouble() ?: 0.0 } },
                    selected = json["selected"].uiOptString(),
                    height = json["height"].uiOptDouble()?.toFloat() ?: 0f,
                    categories = json["categories"].uiList().map { it.uiOptString() ?: "" },
                    filter = json["filter"].uiOptString(),
                    note = json["note"].uiOptString() ?: "",
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class MapMarker(
        val label: String,
        val lat: Double,
        val lng: Double,
        val category: String?,
        val icon: String?,
    ) : UiKind {
        companion object {
            fun from(json: JSON): MapMarker {
                return MapMarker(
                    label = json["label"].uiOptString() ?: "",
                    lat = json["lat"].uiOptDouble() ?: 0.0,
                    lng = json["lng"].uiOptDouble() ?: 0.0,
                    category = json["category"].uiOptString(),
                    icon = json["icon"].uiOptString(),
                )
            }
        }
    }

    data class YouTube(
        /** The embed's address; none for a video it can't name, which shows nothing. */
        val url: String?,
        val title: String,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): YouTube {
                return YouTube(
                    url = json["url"].uiOptString(),
                    title = json["title"].uiOptString() ?: "",
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Flashcard(
        val front: UiSlot,
        val back: UiSlot,
        val hint: String,
        val flipped: Boolean,
        /** The flip. */
        val canvas: UiCanvas,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Flashcard {
                return Flashcard(
                    front = UiSlot.from(json["front"].uiObject() ?: EmptyJSON),
                    back = UiSlot.from(json["back"].uiObject() ?: EmptyJSON),
                    hint = json["hint"].uiOptString() ?: "",
                    flipped = json["flipped"].uiOptBool() ?: false,
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Celebration(
        /** Counts the bursts: a new one fires the confetti. */
        val burst: Int,
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Celebration {
                return Celebration(
                    burst = json["burst"].uiOptInt() ?: 0,
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Svg(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Svg {
                return Svg(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class G(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): G {
                return G(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Circle(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Circle {
                return Circle(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Ellipse(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Ellipse {
                return Ellipse(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Rect(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Rect {
                return Rect(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Line(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Line {
                return Line(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Path(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Path {
                return Path(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Polyline(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Polyline {
                return Polyline(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Polygon(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): Polygon {
                return Polygon(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class SvgText(
        val canvas: UiCanvas,
    ) : UiKind {
        companion object {
            fun from(json: JSON): SvgText {
                return SvgText(
                    canvas = UiCanvas.from(json["canvas"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class HtmlView(
        val html: String,
        val height: Float?,
        val title: String,
        /** The spinner shows in its place. */
        val streaming: Boolean,
        val radius: UiRadius,
    ) : UiKind {
        companion object {
            fun from(json: JSON): HtmlView {
                return HtmlView(
                    html = json["html"].uiOptString() ?: "",
                    height = json["height"].uiOptDouble()?.toFloat(),
                    title = json["title"].uiOptString() ?: "",
                    streaming = json["streaming"].uiOptBool() ?: false,
                    radius = UiRadius.from(json["radius"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    companion object {
        fun from(json: JSON): UiKind? = when (json.string("component")) {
            "Col" -> Col.from(json)
            "Row" -> Row.from(json)
            "Box" -> Box.from(json)
            "Grid" -> Grid.from(json)
            "Card" -> Card.from(json)
            "Carousel" -> Carousel.from(json)
            "Popover" -> Popover.from(json)
            "Divider" -> Divider
            "Spacer" -> Spacer
            "Title" -> Title.from(json)
            "Text" -> Text.from(json)
            "Caption" -> Caption.from(json)
            "Label" -> Label.from(json)
            "Badge" -> Badge.from(json)
            "Link" -> Link.from(json)
            "List" -> List.from(json)
            "ListItem" -> ListItem.from(json)
            "Markdown" -> Markdown.from(json)
            "Quote" -> Quote.from(json)
            "Code" -> Code.from(json)
            "Math" -> Math.from(json)
            "Button" -> Button.from(json)
            "IconButton" -> IconButton.from(json)
            "Pressable" -> Pressable.from(json)
            "Input" -> Input.from(json)
            "TextArea" -> TextArea.from(json)
            "Select" -> Select.from(json)
            "SegmentedControl" -> SegmentedControl.from(json)
            "Slider" -> Slider.from(json)
            "Checkbox" -> Checkbox.from(json)
            "RadioGroup" -> RadioGroup.from(json)
            "DatePicker" -> DatePicker.from(json)
            "Timer" -> Timer.from(json)
            "Form" -> Form.from(json)
            "SearchImage" -> SearchImage.from(json)
            "Image" -> Image.from(json)
            "Icon" -> Icon.from(json)
            "Table" -> Table.from(json)
            "Chart" -> Chart.from(json)
            "Map" -> Map.from(json)
            "MapMarker" -> MapMarker.from(json)
            "YouTube" -> YouTube.from(json)
            "Flashcard" -> Flashcard.from(json)
            "Celebration" -> Celebration.from(json)
            "Svg" -> Svg.from(json)
            "G" -> G.from(json)
            "Circle" -> Circle.from(json)
            "Ellipse" -> Ellipse.from(json)
            "Rect" -> Rect.from(json)
            "Line" -> Line.from(json)
            "Path" -> Path.from(json)
            "Polyline" -> Polyline.from(json)
            "Polygon" -> Polygon.from(json)
            "SvgText" -> SvgText.from(json)
            "HtmlView" -> HtmlView.from(json)
            else -> null
        }
    }
}

enum class UiAlign(val raw: String) {
    Start("start"),
    Center("center"),
    End("end"),
    Stretch("stretch");

    companion object {
        fun of(raw: String?): UiAlign? = entries.firstOrNull { it.raw == raw }
    }
}

enum class UiJustify(val raw: String) {
    Start("start"),
    Center("center"),
    End("end"),
    Between("between");

    companion object {
        fun of(raw: String?): UiJustify? = entries.firstOrNull { it.raw == raw }
    }
}

/** A row's child: its width before the row's space is shared. */
data class UiSizing(
    val basis: UiBasis,
    val length: UiLength?,
    val grow: Float,
    val shrink: Boolean,
) {
    companion object {
        fun from(json: JSON): UiSizing {
            return UiSizing(
                basis = UiBasis.of(json["basis"].uiOptString()) ?: UiBasis.Content,
                length = json["length"].uiObject()?.let { UiLength.from(it) },
                grow = json["grow"].uiOptDouble()?.toFloat() ?: 0f,
                shrink = json["shrink"].uiOptBool() ?: false,
            )
        }
    }
}

enum class UiBasis(val raw: String) {
    Content("content"),
    Zero("zero"),
    Length("length"),
    Row("row"),
    Media("media");

    companion object {
        fun of(raw: String?): UiBasis? = entries.firstOrNull { it.raw == raw }
    }
}

data class UiLength(
    val unit: UiLengthUnit,
    val value: Float,
) {
    companion object {
        fun from(json: JSON): UiLength {
            return UiLength(
                unit = UiLengthUnit.of(json["unit"].uiOptString()) ?: UiLengthUnit.Points,
                value = json["value"].uiOptDouble()?.toFloat() ?: 0f,
            )
        }
    }
}

enum class UiLengthUnit(val raw: String) {
    Points("points"),
    Fraction("fraction"),
    Column("column"),
    Window("window");

    companion object {
        fun of(raw: String?): UiLengthUnit? = entries.firstOrNull { it.raw == raw }
    }
}

enum class UiDirection(val raw: String) {
    Row("row"),
    Col("col");

    companion object {
        fun of(raw: String?): UiDirection? = entries.firstOrNull { it.raw == raw }
    }
}

data class UiRadius(
    val kind: UiRadiusKind,
    /** 0 for `full`, half the shorter side. */
    val points: Float,
) {
    companion object {
        fun from(json: JSON): UiRadius {
            return UiRadius(
                kind = UiRadiusKind.of(json["kind"].uiOptString()) ?: UiRadiusKind.Xs,
                points = json["points"].uiOptDouble()?.toFloat() ?: 0f,
            )
        }
    }
}

enum class UiRadiusKind(val raw: String) {
    Xs("xs"),
    Sm("sm"),
    Md("md"),
    Lg("lg"),
    Xl("xl"),
    Full("full"),
    Points("points");

    companion object {
        fun of(raw: String?): UiRadiusKind? = entries.firstOrNull { it.raw == raw }
    }
}

/** Text or components, where a slot takes either. */
data class UiSlot(
    val text: UiRich?,
    val style: UiTextStyle,
    val children: kotlin.collections.List<UiNode>,
) {
    companion object {
        fun from(json: JSON): UiSlot {
            return UiSlot(
                text = json["text"].uiObject()?.let { UiRich.from(it) },
                style = UiTextStyle.of(json["style"].uiOptString()) ?: UiTextStyle.Body,
                children = json["children"].uiList().mapNotNull { UiNode.from(it.uiObject() ?: EmptyJSON) },
            )
        }
    }
}

/** Text with its inline formatting, as a prose row's: `[start, length, style bits]` runs. */
data class UiRich(
    val text: String,
    val runs: kotlin.collections.List<kotlin.collections.List<Int>>,
    val links: kotlin.collections.List<UiLink>,
) {
    companion object {
        fun from(json: JSON): UiRich {
            return UiRich(
                text = json["text"].uiOptString() ?: "",
                runs = json["runs"].uiList().map { it.uiList().map { it.uiOptInt() ?: 0 } },
                links = json["links"].uiList().map { UiLink.from(it.uiObject() ?: EmptyJSON) },
            )
        }
    }
}

data class UiLink(
    val start: Int,
    val len: Int,
    val url: String,
) {
    companion object {
        fun from(json: JSON): UiLink {
            return UiLink(
                start = json["start"].uiOptInt() ?: 0,
                len = json["len"].uiOptInt() ?: 0,
                url = json["url"].uiOptString() ?: "",
            )
        }
    }
}

enum class UiTextStyle(val raw: String) {
    Body("body"),
    BodyCard("body_card"),
    Large("large"),
    Small("small"),
    Caption("caption"),
    Label("label"),
    TitleSm("title_sm"),
    TitleMd("title_md"),
    TitleLg("title_lg"),
    TitleXl("title_xl"),
    Title2xl("title2xl"),
    Title3xl("title3xl"),
    CardTitle("card_title"),
    CardSubtitle("card_subtitle"),
    CardHeading("card_heading"),
    Readout("readout"),
    Tick("tick"),
    Timer("timer"),
    Code("code"),
    Word("word"),
    WordBack("word_back");

    companion object {
        fun of(raw: String?): UiTextStyle? = entries.firstOrNull { it.raw == raw }
    }
}

enum class UiWeight(val raw: String) {
    Regular("regular"),
    Medium("medium"),
    Semibold("semibold");

    companion object {
        fun of(raw: String?): UiWeight? = entries.firstOrNull { it.raw == raw }
    }
}

/** A Lucide icon: its name, and its character in `lucide.ttf`. */
data class UiGlyph(
    val name: String,
    val glyph: String,
) {
    companion object {
        fun from(json: JSON): UiGlyph {
            return UiGlyph(
                name = json["name"].uiOptString() ?: "",
                glyph = json["glyph"].uiOptString() ?: "",
            )
        }
    }
}

enum class UiListMarker(val raw: String) {
    Bullet("bullet"),
    Number("number"),
    Plain("plain");

    companion object {
        fun of(raw: String?): UiListMarker? = entries.firstOrNull { it.raw == raw }
    }
}

sealed interface UiItemMarker {
    data object Blank : UiItemMarker

    data object Dot : UiItemMarker

    data class Number(
        val number: Int,
    ) : UiItemMarker {
        companion object {
            fun from(json: JSON): Number {
                return Number(
                    number = json["number"].uiOptInt() ?: 0,
                )
            }
        }
    }

    /** One or two letters or digits, in a pill. */
    data class Chip(
        val text: String,
    ) : UiItemMarker {
        companion object {
            fun from(json: JSON): Chip {
                return Chip(
                    text = json["text"].uiOptString() ?: "",
                )
            }
        }
    }

    /** Any other marker, as text. */
    data class Label(
        val text: String,
    ) : UiItemMarker {
        companion object {
            fun from(json: JSON): Label {
                return Label(
                    text = json["text"].uiOptString() ?: "",
                )
            }
        }
    }

    companion object {
        fun from(json: JSON): UiItemMarker? = when (json.string("kind")) {
            "blank" -> Blank
            "dot" -> Dot
            "number" -> Number.from(json)
            "chip" -> Chip.from(json)
            "label" -> Label.from(json)
            else -> null
        }
    }
}

sealed interface UiMarkdownBlock {
    data class Prose(
        val prose: JSON,
    ) : UiMarkdownBlock {
        companion object {
            fun from(json: JSON): Prose {
                return Prose(
                    prose = json["prose"].uiObject() ?: EmptyJSON,
                )
            }
        }
    }

    data class Code(
        val language: String,
        val code: String,
        val spans: kotlin.collections.List<Int>?,
    ) : UiMarkdownBlock {
        companion object {
            fun from(json: JSON): Code {
                return Code(
                    language = json["language"].uiOptString() ?: "",
                    code = json["code"].uiOptString() ?: "",
                    spans = json["spans"]?.takeIf { it !is JsonNull }?.let { it.uiList().map { it.uiOptInt() ?: 0 } },
                )
            }
        }
    }

    companion object {
        fun from(json: JSON): UiMarkdownBlock? = when (json.string("kind")) {
            "prose" -> Prose.from(json)
            "code" -> Code.from(json)
            else -> null
        }
    }
}

/**
 * What `motile_ui_layout` lays out, which the client gives its width and text size: the
 * component and its props by name, in order, an Svg's shapes as `{kind, props}` too.
 */
data class UiCanvas(
    val kind: String,
    val props: JsonElement,
) {
    companion object {
        fun from(json: JSON): UiCanvas {
            return UiCanvas(
                kind = json["kind"].uiOptString() ?: "",
                props = json["props"] ?: JsonNull,
            )
        }
    }
}

enum class UiButtonVariant(val raw: String) {
    Primary("primary"),
    Danger("danger"),
    Success("success"),
    Outline("outline"),
    Secondary("secondary"),
    Ghost("ghost");

    companion object {
        fun of(raw: String?): UiButtonVariant? = entries.firstOrNull { it.raw == raw }
    }
}

enum class UiControlSize(val raw: String) {
    Small("small"),
    Regular("regular"),
    Large("large");

    companion object {
        fun of(raw: String?): UiControlSize? = entries.firstOrNull { it.raw == raw }
    }
}

/** Why a control takes no presses. */
enum class UiDisabled(val raw: String) {
    Program("program"),
    Streaming("streaming"),
    NoTools("no_tools");

    companion object {
        fun of(raw: String?): UiDisabled? = entries.firstOrNull { it.raw == raw }
    }
}

data class UiChoice(
    val value: String,
    val label: String,
    val description: String?,
) {
    companion object {
        fun from(json: JSON): UiChoice {
            return UiChoice(
                value = json["value"].uiOptString() ?: "",
                label = json["label"].uiOptString() ?: "",
                description = json["description"].uiOptString(),
            )
        }
    }
}

data class UiSliderMark(
    /** From 0 to 1. */
    val at: Double,
    val label: String,
    val edge: UiMarkEdge,
) {
    companion object {
        fun from(json: JSON): UiSliderMark {
            return UiSliderMark(
                at = json["at"].uiOptDouble() ?: 0.0,
                label = json["label"].uiOptString() ?: "",
                edge = UiMarkEdge.of(json["edge"].uiOptString()) ?: UiMarkEdge.Start,
            )
        }
    }
}

enum class UiMarkEdge(val raw: String) {
    Start("start"),
    Middle("middle"),
    End("end");

    companion object {
        fun of(raw: String?): UiMarkEdge? = entries.firstOrNull { it.raw == raw }
    }
}

enum class UiRadioVariant(val raw: String) {
    List("list"),
    Pills("pills"),
    Cards("cards");

    companion object {
        fun of(raw: String?): UiRadioVariant? = entries.firstOrNull { it.raw == raw }
    }
}

data class UiTimerButton(
    val label: String,
    val icon: UiGlyph?,
    val disabled: UiDisabled?,
) {
    companion object {
        fun from(json: JSON): UiTimerButton {
            return UiTimerButton(
                label = json["label"].uiOptString() ?: "",
                icon = json["icon"].uiObject()?.let { UiGlyph.from(it) },
                disabled = UiDisabled.of(json["disabled"].uiOptString()),
            )
        }
    }
}

enum class UiFit(val raw: String) {
    Cover("cover"),
    Contain("contain");

    companion object {
        fun of(raw: String?): UiFit? = entries.firstOrNull { it.raw == raw }
    }
}

/** The photos found for an image's query. */
data class UiPhotos(
    val state: UiPhotoState,
    /** Media refs, best first. */
    val media: kotlin.collections.List<String>,
) {
    companion object {
        fun from(json: JSON): UiPhotos {
            return UiPhotos(
                state = UiPhotoState.of(json["state"].uiOptString()) ?: UiPhotoState.Idle,
                media = json["media"].uiList().map { it.uiOptString() ?: "" },
            )
        }
    }
}

enum class UiPhotoState(val raw: String) {
    Idle("idle"),
    Waiting("waiting"),
    Found("found"),
    Missing("missing");

    companion object {
        fun of(raw: String?): UiPhotoState? = entries.firstOrNull { it.raw == raw }
    }
}

data class UiTableColumn(
    val title: String,
    /** Numbers: trailing, digits, one line. */
    val numeric: Boolean,
) {
    companion object {
        fun from(json: JSON): UiTableColumn {
            return UiTableColumn(
                title = json["title"].uiOptString() ?: "",
                numeric = json["numeric"].uiOptBool() ?: false,
            )
        }
    }
}

sealed interface UiTableCell {
    data class Text(
        val text: UiRich,
    ) : UiTableCell {
        companion object {
            fun from(json: JSON): Text {
                return Text(
                    text = UiRich.from(json["text"].uiObject() ?: EmptyJSON),
                )
            }
        }
    }

    data class Node(
        val node: UiNode,
    ) : UiTableCell {
        companion object {
            fun from(json: JSON): Node? {
                return Node(
                    node = UiNode.from(json["node"].uiObject() ?: EmptyJSON) ?: return null,
                )
            }
        }
    }

    companion object {
        fun from(json: JSON): UiTableCell? = when (json.string("kind")) {
            "text" -> Text.from(json)
            "node" -> Node.from(json)
            else -> null
        }
    }
}

data class UiMapPin(
    /** The label, numbered when another pin has it too: "Café (2)". */
    val key: String,
    val label: String,
    val lat: Double,
    val lng: Double,
    val category: String?,
    /** One emoji, else the pin shows `number`. */
    val icon: String?,
    val number: Int,
    val selected: Boolean,
    /** In the category shown. */
    val visible: Boolean,
) {
    companion object {
        fun from(json: JSON): UiMapPin {
            return UiMapPin(
                key = json["key"].uiOptString() ?: "",
                label = json["label"].uiOptString() ?: "",
                lat = json["lat"].uiOptDouble() ?: 0.0,
                lng = json["lng"].uiOptDouble() ?: 0.0,
                category = json["category"].uiOptString(),
                icon = json["icon"].uiOptString(),
                number = json["number"].uiOptInt() ?: 0,
                selected = json["selected"].uiOptBool() ?: false,
                visible = json["visible"].uiOptBool() ?: false,
            )
        }
    }
}

enum class UiRouteKind(val raw: String) {
    Off("off"),
    Loading("loading"),
    Street("street"),
    Overview("overview");

    companion object {
        fun of(raw: String?): UiRouteKind? = entries.firstOrNull { it.raw == raw }
    }
}

/** A colour: the text's own ("current"), a token by name, a token's wash, a layer of the surface it lies on, or `[r, g, b, a]`. */
sealed interface UiColor {
    data object Current : UiColor

    data class Token(val name: String) : UiColor

    data class Wash(val name: String) : UiColor

    data class Surface(val layer: String) : UiColor

    data class Rgba(val channels: List<Int>) : UiColor

    companion object {
        fun from(json: JsonElement?): UiColor? = when (json) {
            is JsonObject -> json["wash"].uiOptString()?.let(::Wash) ?: json["surface"].uiOptString()?.let(::Surface)
            is JsonArray -> Rgba(json.map { it.uiOptInt() ?: 0 })
            else -> json.uiOptString()?.let { if (it == "current") Current else Token(it) }
        }
    }
}

private fun JsonElement?.uiPrimitive(): JsonPrimitive? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }

private fun JsonElement?.uiOptString(): String? = uiPrimitive()?.takeIf { it.isString }?.content

private fun JsonElement?.uiOptBool(): Boolean? = uiPrimitive()?.booleanOrNull

private fun JsonElement?.uiOptDouble(): Double? = uiPrimitive()?.doubleOrNull

private fun JsonElement?.uiOptInt(): Int? = uiPrimitive()?.let { it.longOrNull?.toInt() ?: it.doubleOrNull?.toInt() }

private fun JsonElement?.uiList(): List<JsonElement> = (this as? JsonArray) ?: emptyList()

private fun JsonElement?.uiObject(): JSON? = this as? JsonObject
