package app.motile.transcript.openui

import android.graphics.BitmapFactory
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.LayoutModifier
import androidx.compose.ui.layout.IntrinsicMeasureScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasurePolicy
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.UiFit
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiPhotoState
import app.motile.core.UiPhotos
import app.motile.core.UiRadius
import app.motile.core.UiTableCell
import app.motile.core.UiTextStyle
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.value
import app.motile.transcript.PictureCache
import app.motile.transcript.mediaFile
import app.motile.ui.Glyph
import app.motile.ui.Icon
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.max
import kotlin.math.roundToInt

/** A photo of the web, fetched once and kept with the transcript's pictures. */
private object WebPictures {
    suspend fun load(url: String, maxPixels: Int): ImageBitmap? = PictureCache.cached(url) ?: withContext(Dispatchers.IO) {
        runCatching {
            val connection = URL(url).openConnection() as HttpURLConnection
            connection.setRequestProperty("User-Agent", "Motile")
            connection.connectTimeout = 15_000
            connection.readTimeout = 30_000
            val bytes = connection.inputStream.use { it.readBytes() }
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            var sample = 1
            while (max(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPixels) sample *= 2
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
                ?.asImageBitmap()?.also { PictureCache.put(url, it) }
        }.getOrNull()
    }
}

/**
 * A photo the width of its column at its aspect ratio, on the surface's box: shimmering while it is
 * looked for, fading in once it is here, Lucide's image where there is none. An image's own `src`
 * is fetched first; when it fails, core looks one up instead.
 */
@Composable
fun UiImage(node: UiNode, name: String, src: String?, aspect: Float, radius: UiRadius, fit: UiFit, photos: UiPhotos) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val host = LocalUiHost.current
    val shape = uiShape(radius)
    val source = when {
        photos.state == UiPhotoState.Found -> photos.media.firstOrNull()?.let { "media:$it" }
        photos.state == UiPhotoState.Idle -> src
        else -> null
    }
    var picture by remember(source) { mutableStateOf(source?.let { PictureCache.cached(it.removePrefix("media:")) }) }
    var failed by remember(source) { mutableStateOf(false) }
    val shown = remember(source) { Animatable(if (picture != null) 1f else 0f) }
    val maxPixels = with(LocalDensity.current) { 720.dp.roundToPx() }
    LaunchedEffect(source) {
        if (source == null || picture != null) return@LaunchedEffect
        val loaded = if (source.startsWith("media:")) {
            val id = source.removePrefix("media:")
            store.mediaFile(id)?.let { PictureCache.decode(it, id, maxPixels) }
        } else {
            WebPictures.load(source, maxPixels)
        }
        if (loaded == null) {
            failed = true
            if (photos.state == UiPhotoState.Idle) host?.send(node.id, "fallback")
            return@LaunchedEffect
        }
        picture = loaded
        if (Platform.reducesMotion) shown.snapTo(1f) else shown.animateTo(1f, tween(300))
    }
    val waiting = picture == null && !failed && (photos.state == UiPhotoState.Waiting || source != null)
    Box(
        Modifier
            .uiNode(node)
            .fillMaxWidth()
            .aspectRatio(aspect.coerceIn(0.2f, 5f))
            .clip(shape)
            .background(surface.color(Surface.Layer.Box), shape)
            .then(if (waiting) Modifier.shimmer() else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        val image = picture
        when {
            image != null -> Image(
                image, name, Modifier.fillMaxSize().alpha(shown.value),
                contentScale = if (fit == UiFit.Contain) ContentScale.Fit else ContentScale.Crop,
            )
            !waiting -> Glyph(Symbol.Image.character.orEmpty(), 26.dp, color = palette.mutedStrongerForeground)
        }
    }
}

/** Motile's shimmer: a band that sweeps across every 1.4 seconds, still where motion is reduced. */
@Composable
private fun Modifier.shimmer(): Modifier {
    if (Platform.reducesMotion) return this
    val palette = LocalPalette.current
    val dark = LocalDark.current
    val phase by rememberInfiniteTransition(label = "shimmer").animateFloat(
        0f, 1f, infiniteRepeatable(tween(1400, easing = LinearEasing), RepeatMode.Restart), label = "phase",
    )
    val band = palette.foreground.copy(alpha = Opacity.ColorTint.value(dark))
    val edge = Opacity.ShimmerBandEdge.value(dark)
    val middle = Opacity.ShimmerBandMiddle.value(dark)
    return drawBehind {
        val width = size.width * 0.6f
        val start = -width + phase * (size.width + width)
        drawRect(
            Brush.horizontalGradient(
                0f to Color.Transparent,
                0.2f to band.copy(alpha = band.alpha * edge),
                0.5f to band.copy(alpha = band.alpha * middle),
                0.8f to band.copy(alpha = band.alpha * edge),
                1f to Color.Transparent,
                startX = start, endX = start + width,
            ),
        )
    }
}

/** The Lucide glyph core names, in a box of its size. */
@Composable
fun UiIcon(node: UiNode, kind: UiKind.Icon) {
    val glyph = kind.icon
    if (glyph == null) {
        Box(Modifier.uiNode(node).size(kind.size.dp))
        return
    }
    Glyph(glyph.glyph, kind.size.dp, Modifier.uiNode(node), color = kind.color.color())
}

/**
 * A table in a card: a header row, then the rows, with lines between them. Its columns share the
 * width as a browser's automatic table layout does, and the table scrolls sideways when even their
 * least doesn't fit.
 */
@Composable
fun UiTable(node: UiNode, kind: UiKind.Table) {
    val palette = LocalPalette.current
    val narrow = LocalUiNarrow.current
    val shape = RoundedCornerShape(Radius.lg)
    val columns = kind.columns.size
    if (columns == 0) return
    val horizontal = if (narrow) 10.dp else 12.dp
    val vertical = if (narrow) 8.dp else 10.dp
    val policy = remember(columns) { TablePolicy(columns) }
    Box(
        Modifier.uiNode(node).fillMaxWidth().clip(shape).background(palette.card, shape).border(1.dp, palette.borderCard, shape),
    ) {
        CompositionLocalProvider(LocalSurface provides Surface.Card) {
            Layout(
                content = {
                    kind.columns.forEach { column ->
                        Cell(column.numeric, palette.cardAccent, line = true, horizontal, 8.dp) {
                            Text(
                                column.title, style = UiStyle.text(UiTextStyle.Label, false), color = palette.foreground, maxLines = 1,
                                align = if (column.numeric) TextAlign.End else null, overflow = TextOverflow.Clip,
                            )
                        }
                    }
                    kind.rows.forEachIndexed { index, row ->
                        for (column in 0 until columns) {
                            val numeric = kind.columns[column].numeric
                            Cell(numeric, null, line = index < kind.rows.lastIndex, horizontal, vertical) {
                                when (val cell = row.getOrNull(column)) {
                                    is UiTableCell.Text -> {
                                        val style = UiStyle.text(UiTextStyle.Small, narrow).let { if (numeric) it.copy(fontFeatureSettings = "tnum", textAlign = TextAlign.End) else it }
                                        UiRichText(cell.text, style, palette.foreground, maxLines = if (numeric) 1 else Int.MAX_VALUE)
                                    }
                                    is UiTableCell.Node -> UiNodeView(cell.node)
                                    null -> Unit
                                }
                            }
                        }
                    }
                },
                modifier = Modifier.then(TableWidth(policy)).horizontalScroll(rememberScrollState()),
                measurePolicy = policy,
            )
        }
    }
}

/** A cell: its content in from its sides, centred across the row, a line under it where one runs. */
@Composable
private fun Cell(trailing: Boolean, fill: Color?, line: Boolean, horizontal: androidx.compose.ui.unit.Dp, vertical: androidx.compose.ui.unit.Dp, content: @Composable () -> Unit) {
    val border = LocalPalette.current.borderCard
    Box(
        Modifier
            .then(if (fill != null) Modifier.background(fill) else Modifier)
            .drawBehind { if (line) drawRect(border, Offset(0f, size.height - 1.dp.toPx()), Size(size.width, 1.dp.toPx())) }
            .padding(horizontal = horizontal, vertical = vertical),
        contentAlignment = if (trailing) Alignment.CenterEnd else Alignment.CenterStart,
    ) { content() }
}

/** Tells the table the width it has, which the sideways scroll inside it hides from it. */
private class TableWidth(val policy: TablePolicy) : LayoutModifier {
    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        policy.available = constraints.maxWidth
        val placeable = measurable.measure(constraints)
        return layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurable: IntrinsicMeasurable, width: Int): Int {
        policy.available = width
        return measurable.maxIntrinsicHeight(width)
    }

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurable: IntrinsicMeasurable, width: Int): Int {
        policy.available = width
        return measurable.minIntrinsicHeight(width)
    }

    override fun equals(other: Any?): Boolean = other is TableWidth && other.policy === policy

    override fun hashCode(): Int = System.identityHashCode(policy)
}

private class TablePolicy(val columns: Int) : MeasurePolicy {
    var available = 0

    /** Each column's least and most width: its widest unbreakable part, and its widest cell on one line. */
    private fun bounds(measurables: List<IntrinsicMeasurable>): Pair<FloatArray, FloatArray> {
        val least = FloatArray(columns)
        val most = FloatArray(columns)
        measurables.forEachIndexed { index, cell ->
            val column = index % columns
            least[column] = max(least[column], cell.minIntrinsicWidth(Constraints.Infinity).toFloat())
            most[column] = max(most[column], cell.maxIntrinsicWidth(Constraints.Infinity).toFloat())
        }
        return least to most
    }

    private fun widths(measurables: List<IntrinsicMeasurable>): IntArray {
        val (least, most) = bounds(measurables)
        val width = available.toFloat()
        val totalMost = most.sum()
        val totalLeast = least.sum()
        val shares = FloatArray(columns) { column ->
            when {
                totalMost <= width -> most[column] + (width - totalMost) * (if (totalMost > 0) most[column] / totalMost else 1f / columns)
                totalLeast <= width -> {
                    val spread = (0 until columns).sumOf { (most[it] - least[it]).toDouble() }.toFloat()
                    least[column] + if (spread > 0) (width - totalLeast) * (most[column] - least[column]) / spread else 0f
                }
                else -> least[column]
            }
        }
        val widths = IntArray(columns) { shares[it].roundToInt() }
        if (totalLeast <= width) widths[columns - 1] += available - widths.sum()
        return widths
    }

    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val widths = widths(measurables)
        val rows = measurables.indices.chunked(columns)
        val heights = rows.map { row -> row.maxOf { measurables[it].maxIntrinsicHeight(widths[it % columns]) } }
        val placeables = measurables.mapIndexed { index, cell -> cell.measure(Constraints.fixed(widths[index % columns], heights[index / columns])) }
        return layout(widths.sum(), heights.sum()) {
            var y = 0
            rows.forEachIndexed { row, cells ->
                var x = 0
                for (index in cells) {
                    placeables[index].place(x, y)
                    x += placeables[index].width
                }
                y += heights[row]
            }
        }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        bounds(measurables).second.sum().roundToInt()

    override fun IntrinsicMeasureScope.minIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        bounds(measurables).first.sum().roundToInt()

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        maxIntrinsicHeight(measurables, width)

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int {
        val widths = widths(measurables)
        return measurables.indices.chunked(columns).sumOf { row -> row.maxOf { measurables[it].maxIntrinsicHeight(widths[it % columns]) } }
    }
}

/** Until the web view comes: a card with the video's title, which opens it. */
@Composable
fun UiYouTube(node: UiNode, kind: UiKind.YouTube) {
    val url = kind.url ?: return
    StandIn(node, kind.title, "YouTube", Symbol.CirclePlay, Modifier.clickable(role = Role.Button) { Platform.open(url) })
}

/** Until the web view comes: a card with the view's title, or the spinner while the program streams. */
@Composable
fun UiHtmlCard(node: UiNode, kind: UiKind.HtmlView) {
    val palette = LocalPalette.current
    if (kind.streaming) {
        Card(node) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Spinner(13f, color = palette.mutedForeground)
                Text("Preparing the interactive view", style = UiStyle.text(UiTextStyle.Small, false), color = palette.mutedForeground)
            }
        }
        return
    }
    StandIn(node, kind.title, "Runs once it opens here", Symbol.Blocks)
}

/** Until the map comes: a card of the map's height that lists its places. */
@Composable
fun UiMapCard(node: UiNode, kind: UiKind.Map) {
    val palette = LocalPalette.current
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.width.toDp().value }
    val height = minOf(kind.height.coerceIn(200f, 720f), max(280f, 0.82f * window))
    val shape = RoundedCornerShape(Radius.xl)
    CompositionLocalProvider(LocalSurface provides Surface.Card) {
        Column(
            Modifier.uiNode(node).fillMaxWidth().height(height.dp).clip(shape).background(palette.card, shape)
                .border(1.dp, palette.borderCard, shape).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Map", style = UiStyle.text(UiTextStyle.CardTitle, false), color = palette.foreground)
            kind.pins.filter { it.visible }.forEach { pin ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(pin.icon ?: "${pin.number}", style = UiStyle.text(UiTextStyle.Small, false), color = palette.mutedForeground, maxLines = 1)
                    Text(pin.label, style = UiStyle.text(UiTextStyle.Small, false), color = palette.foreground, maxLines = 1)
                    pin.category?.let { Text(it, style = UiStyle.text(UiTextStyle.Caption, false), color = palette.mutedStrongerForeground, maxLines = 1) }
                }
            }
        }
    }
}

@Composable
private fun StandIn(node: UiNode, title: String, detail: String, symbol: Symbol, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    Card(node, modifier) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(symbol, 16f, color = palette.mutedForeground)
            Column(Modifier.weight(1f)) {
                Text(title, style = UiStyle.text(UiTextStyle.CardTitle, false), color = palette.foreground)
                Text(detail, style = UiStyle.text(UiTextStyle.Caption, false), color = palette.mutedForeground)
            }
        }
    }
}

@Composable
private fun Card(node: UiNode, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.lg)
    CompositionLocalProvider(LocalSurface provides Surface.Card) {
        Box(
            Modifier.uiNode(node).fillMaxWidth().clip(shape).background(palette.card, shape).border(1.dp, palette.borderCard, shape)
                .then(modifier).padding(16.dp),
        ) { content() }
    }
}
