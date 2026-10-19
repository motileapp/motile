package app.motile.transcript.canvas

import app.motile.core.JSON
import app.motile.core.bool
import app.motile.core.int
import app.motile.core.numbers
import app.motile.core.obj
import app.motile.core.objects
import app.motile.core.optionalDouble
import app.motile.core.optionalString
import app.motile.core.parseJSON
import app.motile.core.string
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

// A canvas component's scene, as crates/core/src/render/openui/canvas/scene.rs defines it: shapes and
// labels in points from its top left corner, painted in order, and the regions a finger lights.

class Scene(
    val width: Float,
    val height: Float,
    /** A formula's baseline from the top, which an inline one shares with the text around it. */
    val baseline: Float?,
    val label: String?,
    val items: List<SceneItem>,
    /** The last one that holds the point wins. */
    val hits: List<SceneHit>,
    val legend: SceneLegend?,
    val motion: SceneMotion?,
) {
    constructor(json: JSON) : this(
        width = json.float("width"),
        height = json.float("height"),
        baseline = json.optionalDouble("baseline")?.toFloat(),
        label = json.optionalString("label"),
        items = json.objects("items").mapNotNull(SceneItem::of),
        hits = json.objects("hits").mapNotNull(SceneHit::of),
        legend = json.obj("legend")?.let(::SceneLegend),
        motion = json.obj("motion")?.let(SceneMotion::of),
    )

    /** The hit whose region holds the point, the last of them when several do. */
    fun hit(x: Float, y: Float): Int? = hits.indexOfLast { it.region.holds(x, y) }.takeIf { it >= 0 }

    companion object {
        fun parse(text: String): Scene? = parseJSON(text)?.let(::Scene)
    }
}

sealed interface SceneItem {
    class Path(val verbs: String, val points: FloatArray, val paint: ScenePaint) : SceneItem

    /** Its corners are ellipses `rx` wide and `ry` tall, already no more than half its sides. */
    class Rect(val x: Float, val y: Float, val width: Float, val height: Float, val rx: Float, val ry: Float, val paint: ScenePaint) : SceneItem

    class Circle(val cx: Float, val cy: Float, val r: Float, val paint: ScenePaint) : SceneItem

    /** A line of text whose baseline starts, centres or ends at (`x`, `y`), by `anchor`. */
    class Label(val text: String, val x: Float, val y: Float, val anchor: Anchor, val font: SceneFont, val color: SceneColor, val opacity: Float) : SceneItem

    enum class Anchor { Start, Middle, End }

    companion object {
        fun of(json: JSON): SceneItem? = when (json.string("kind")) {
            "path" -> Path(json.string("verbs"), json.floats("points"), ScenePaint(json))
            "rect" -> Rect(json.float("x"), json.float("y"), json.float("width"), json.float("height"), json.float("rx"), json.float("ry"), ScenePaint(json))
            "circle" -> Circle(json.float("cx"), json.float("cy"), json.float("r"), ScenePaint(json))
            "text" -> Label(
                json.string("text"), json.float("x"), json.float("y"),
                when (json.string("anchor")) {
                    "middle" -> Anchor.Middle
                    "end" -> Anchor.End
                    else -> Anchor.Start
                },
                SceneFont(json), SceneColor.of(json["color"]), json.float("opacity", 1f),
            )
            else -> null
        }
    }
}

/** How a shape is painted: its fill (a colour or a gradient), then its stroke, both at `opacity`. */
class ScenePaint(val fill: SceneColor?, val gradient: SceneGradient?, val fillOpacity: Float, val stroke: SceneStroke?, val opacity: Float) {
    constructor(json: JSON) : this(
        fill = json["fill"]?.let(SceneColor::of),
        gradient = json.obj("gradient")?.let(::SceneGradient),
        fillOpacity = json.float("fill_opacity", 1f),
        stroke = json.obj("stroke")?.let(::SceneStroke),
        opacity = json.float("opacity", 1f),
    )
}

/** Miters are limited at 4, as in SVG. `dash` is dash and gap lengths, starting with a dash. */
class SceneStroke(val color: SceneColor, val width: Float, val cap: Cap, val join: Join, val dash: FloatArray) {
    constructor(json: JSON) : this(
        color = SceneColor.of(json["color"]),
        width = json.float("width"),
        cap = when (json.string("cap")) {
            "round" -> Cap.Round
            "square" -> Cap.Square
            else -> Cap.Butt
        },
        join = when (json.string("join")) {
            "round" -> Join.Round
            "bevel" -> Join.Bevel
            else -> Join.Miter
        },
        dash = json.floats("dash"),
    )

    enum class Cap { Butt, Round, Square }
    enum class Join { Miter, Round, Bevel }
}

/** A linear gradient from (`x1`, `y1`) to (`x2`, `y2`), padded beyond its ends. */
class SceneGradient(val x1: Float, val y1: Float, val x2: Float, val y2: Float, val stops: List<Stop>) {
    constructor(json: JSON) : this(
        json.float("x1"), json.float("y1"), json.float("x2"), json.float("y2"),
        json.objects("stops").map { Stop(it.float("at"), SceneColor.of(it["color"]), it.float("opacity", 1f)) },
    )

    class Stop(val at: Float, val color: SceneColor, val opacity: Float)
}

/** The text styles of crates/openui/LAYOUT.md, or the UI typeface at a size. */
sealed interface SceneFont {
    /** 11 regular, tabular digits. */
    data object Tick : SceneFont

    /** 12 regular. */
    data object Caption : SceneFont

    data class Ui(val size: Float, val weight: Weight) : SceneFont

    enum class Weight { Regular, Medium, Semibold }

    companion object {
        operator fun invoke(json: JSON): SceneFont = when (json.string("role")) {
            "tick" -> Tick
            "caption" -> Caption
            else -> Ui(
                json.float("size", 14f),
                when (json.string("weight")) {
                    "medium" -> Weight.Medium
                    "semibold" -> Weight.Semibold
                    else -> Weight.Regular
                },
            )
        }
    }
}

/**
 * A colour: the text's colour where the scene lies, a token of packages/theme/tokens.json by name,
 * a token's wash, a layer of the surface the scene lies on, or sRGB bytes.
 */
sealed interface SceneColor {
    data object Current : SceneColor
    data class Token(val name: String) : SceneColor
    data class Wash(val name: String) : SceneColor
    data class Surface(val layer: Layer) : SceneColor
    data class Rgba(val r: Int, val g: Int, val b: Int, val a: Int) : SceneColor

    /** The parts of the surface a scene lies on, as `Surface` names them. */
    enum class Layer { Base, Border, Row, Control, Box }

    companion object {
        fun of(json: JsonElement?): SceneColor = when (json) {
            is JsonPrimitive -> if (json.content == "current") Current else Token(json.content)
            is JsonArray -> {
                val bytes = json.map { (it as? JsonPrimitive)?.intOrNull ?: 0 }
                Rgba(bytes.getOrElse(0) { 0 }, bytes.getOrElse(1) { 0 }, bytes.getOrElse(2) { 0 }, bytes.getOrElse(3) { 255 })
            }
            is JsonObject -> json.optionalString("wash")?.let(::Wash)
                ?: Surface(Layer.entries.firstOrNull { it.name.lowercase() == json.optionalString("surface") } ?: Layer.Base)
            else -> Current
        }
    }
}

/** A part of the scene a finger lights: while it does, `lit` is painted before `items[at]` (after them all when `at` is their count) and `tip` shows. */
class SceneHit(val region: Region, val at: Int, val lit: List<SceneItem>, val tip: Tip) {
    sealed interface Region {
        fun holds(x: Float, y: Float): Boolean

        class Rect(val x: Float, val y: Float, val width: Float, val height: Float) : Region {
            override fun holds(x: Float, y: Float) = x >= this.x && x <= this.x + width && y >= this.y && y <= this.y + height
        }

        class Circle(val cx: Float, val cy: Float, val r: Float) : Region {
            override fun holds(x: Float, y: Float) = Math.hypot((x - cx).toDouble(), (y - cy).toDouble()) <= r
        }

        /** A ring's part between two radii, from the angle `start` to `end`, in radians clockwise from the right, `end` at most a turn past `start`. */
        class Sector(val cx: Float, val cy: Float, val inner: Float, val outer: Float, val start: Float, val end: Float) : Region {
            override fun holds(x: Float, y: Float): Boolean {
                val (dx, dy) = (x - cx).toDouble() to (y - cy).toDouble()
                val distance = Math.hypot(dx, dy)
                if (distance < inner || distance > outer) return false
                val turn = 2 * Math.PI
                val angle = Math.atan2(dy, dx)
                return angle - turn * Math.floor((angle - start) / turn) <= end
            }
        }
    }

    /** A tooltip, centred on `x` with its top at `y`: its title, then a row for each value. */
    class Tip(val x: Float, val y: Float, val title: String, val rows: List<Row>) {
        class Row(val color: SceneColor?, val name: String, val value: String)
    }

    companion object {
        fun of(json: JSON): SceneHit? {
            val region = when (json.string("shape")) {
                "rect" -> Region.Rect(json.float("x"), json.float("y"), json.float("width"), json.float("height"))
                "circle" -> Region.Circle(json.float("cx"), json.float("cy"), json.float("r"))
                "sector" -> Region.Sector(json.float("cx"), json.float("cy"), json.float("inner"), json.float("outer"), json.float("start"), json.float("end"))
                else -> return null
            }
            val tip = json.obj("tip") ?: return null
            val rows = tip.objects("rows").map { Tip.Row(it["color"]?.let(SceneColor::of), it.string("name"), it.string("value")) }
            return SceneHit(region, json.int("at"), json.objects("lit").mapNotNull(SceneItem::of), Tip(tip.float("x"), tip.float("y"), tip.string("title"), rows))
        }
    }
}

/** A chart's key, which the client lays out: under the plot, or `beside` a pie's disc. */
class SceneLegend(val beside: Boolean, val entries: List<Entry>) {
    constructor(json: JSON) : this(
        json.bool("beside"),
        json.objects("entries").map { Entry(SceneColor.of(it["color"]), it.string("label"), it.optionalString("share")) },
    )

    /** `share` is a pie slice's share of the whole, "42.5%". */
    class Entry(val color: SceneColor, val label: String, val share: String?)

    /** A pie's legend: a column of entries with their shares. */
    val isPie: Boolean get() = entries.any { it.share != null }
}

/** What a client animates rather than paints: Celebration's confetti and Flashcard's flip. */
sealed interface SceneMotion {
    companion object {
        fun of(json: JSON): SceneMotion? = when (json.string("kind")) {
            "confetti" -> Confetti(json)
            "flip" -> Flip(json)
            else -> null
        }
    }
}

/**
 * Pieces that fall over the whole window. Each starts with its top at `top`, its left at its `x`
 * share of the window's width, after its `delay`, and over its `duration` moves `drift` across and
 * `fall` window heights down, turns by `turn` degrees and fades to `endOpacity`, on the cubic Bézier
 * `curve`; all in seconds.
 */
class Confetti(
    val duration: Float,
    val curve: FloatArray,
    val width: Float,
    val height: Float,
    val radius: Float,
    val top: Float,
    val fall: Float,
    val endOpacity: Float,
    val pieces: List<Piece>,
) : SceneMotion {
    constructor(json: JSON) : this(
        json.float("duration"), json.floats("curve"), json.float("width"), json.float("height"), json.float("radius"),
        json.float("top"), json.float("fall"), json.float("end_opacity"),
        json.objects("pieces").map {
            Piece(it.float("x"), SceneColor.of(it["color"]), it.float("delay"), it.float("duration"), it.float("drift"), it.float("turn"))
        },
    )

    class Piece(val x: Float, val color: SceneColor, val delay: Float, val duration: Float, val drift: Float, val turn: Float)
}

/** A card turning over as a horizontal squash about its centre: `frames` are [moment, width's scale] pairs to interpolate between; the back shows from `swapAt` on. */
class Flip(val duration: Float, val curve: FloatArray, val swapAt: Float, val frames: List<FloatArray>) : SceneMotion {
    constructor(json: JSON) : this(
        json.float("duration"), json.floats("curve"), json.float("swap_at"),
        (json["frames"] as? JsonArray)?.mapNotNull { frame -> (frame as? JsonArray)?.let { FloatArray(it.size) { at -> (it[at] as? JsonPrimitive)?.content?.toFloatOrNull() ?: 0f } } } ?: emptyList(),
    )
}

private fun JSON.float(key: String, default: Float = 0f): Float = optionalDouble(key)?.toFloat() ?: default

private fun JSON.floats(key: String): FloatArray = numbers(key).let { list -> FloatArray(list.size) { list[it].toFloat() } }
