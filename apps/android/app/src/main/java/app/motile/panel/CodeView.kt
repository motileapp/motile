package app.motile.panel

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberScrollableState
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.round
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.iconSize
import app.motile.theme.IconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.OverPicture
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.wash
import app.motile.theme.Opacity
import app.motile.ui.ActionButton
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.rememberMenuState
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** The sizes of the code view. Lines are as small as on the Mac, so that as much of one fits. */
private object CodeSheet {
    val headingHeight = scaled(34f).dp
    val lineHeight = 18.dp
    val fileGap = 12.dp
    val mediaInset = 12.dp
    val mediaMaxHeight = 320.dp
    /** The height of an image or a video until its picture is there. */
    val mediaWaitingHeight = 96.dp
    val textInset = 8.dp
    const val FONT = 12.5f
    const val NUMBER_FONT = 11f
    /** A line is drawn no longer than this, which no screen shows past anyway. */
    const val MOST_COLUMNS = 3000
}

/** Where each file's items are in the list: its heading, and its first line when it shows any. */
private class CodeLayout(document: CodeDocument, collapsed: Set<String>) {
    private val headed = document.headed
    val headings = IntArray(document.files.size)
    private val lineStarts = IntArray(document.files.size) { -1 }
    private val rows = IntArray(document.files.size)

    init {
        var index = 0
        document.files.forEachIndexed { file, code ->
            if (document.headed && file > 0) index += 1
            headings[file] = index
            if (document.headed) index += 1
            if (document.headed && code.path in collapsed) return@forEachIndexed
            if (code.media != null) {
                index += 1
                return@forEachIndexed
            }
            lineStarts[file] = index
            rows[file] = code.lines.size
            index += code.lines.size
        }
    }

    /** The line at the item, when it is one. */
    fun line(item: Int): CodePlace? {
        for (file in lineStarts.indices) {
            val start = lineStarts[file]
            if (start >= 0 && item >= start && item < start + rows[file]) return CodePlace(file, item - start)
        }
        return null
    }

    fun isHeading(item: Int): Boolean = headed && headings.contains(item)
}

/** The lines selected, all of them, of one file. */
private data class CodeSelection(val file: Int, val from: Int, val to: Int) {
    fun contains(file: Int, line: Int) = file == this.file && line in from..to
}

/**
 * Shows the files of a diff one under the other, or one whole file, drawing only the lines on
 * screen. The numbers and the headings stay while the lines scroll sideways together, and the
 * heading of the file at the top stays pinned there. Tapping a heading closes or opens its file;
 * holding a line selects it, and dragging on selects the lines down to the finger, to copy.
 *
 * A pull request's diff passes `marks`, which can show a box to mark each file viewed (`onViewed`)
 * and a bar beside the lines with conversations. Where `marks.commentable`, a tap on a line's
 * number calls `onComment` with the line, whose `CodeFile.commentable` says how GitHub counts it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun CodeView(
    document: CodeDocument,
    modifier: Modifier = Modifier,
    collapsed: Set<String> = emptySet(),
    reveal: Pair<String, Int>? = null,
    marks: CodeMarks = CodeMarks(),
    onToggle: (String) -> Unit = {},
    onOpenFile: (String) -> Unit = {},
    onViewed: (String) -> Unit = {},
    onComment: (CodePlace) -> Unit = {},
    onMedia: (CodeFile) -> Unit = {},
    onOpenMedia: (CodeFile) -> Unit = {},
) {
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val codeStyle = remember(density) {
        TextStyle(fontFamily = FontFamily.Monospace, fontSize = with(density) { CodeSheet.FONT.dp.toSp() }, color = palette.foreground)
    }
    val numberStyle = remember(density) {
        TextStyle(fontSize = with(density) { CodeSheet.NUMBER_FONT.dp.toSp() }, fontFeatureSettings = "tnum")
    }
    val measurer = rememberTextMeasurer()
    val advance = remember(codeStyle) { measurer.measure("0000000000", codeStyle).size.width / 10f }
    val layout = remember(document, collapsed) { CodeLayout(document, collapsed) }
    val numberWidth = remember(document) { numberWidth(document) }
    val gutter = numberWidth * (if (document.headed) 2 else 1) + 4.dp
    val listState = remember(document.id) { LazyListState() }
    var x by remember(document.id) { mutableFloatStateOf(0f) }
    var selection by remember(document) { mutableStateOf<CodeSelection?>(null) }
    var wanted by remember { mutableStateOf<String?>(null) }
    var revealed by remember { mutableIntStateOf(0) }
    val menu = rememberMenuState()
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()

    LaunchedEffect(reveal) {
        val reveal = reveal ?: return@LaunchedEffect
        if (reveal.second == revealed) return@LaunchedEffect
        revealed = reveal.second
        wanted = reveal.first
    }
    // Brings the file's heading to the top, now or once the document that has it is shown.
    LaunchedEffect(wanted, layout) {
        val path = wanted ?: return@LaunchedEffect
        val file = document.files.indexOfFirst { it.path == path }
        if (file < 0) return@LaunchedEffect
        wanted = null
        x = 0f
        listState.scrollToItem(layout.headings[file])
    }

    BoxWithConstraints(modifier.fillMaxSize().clipToBounds()) {
        val columns = min(document.files.maxOfOrNull { it.columns } ?: 0, CodeSheet.MOST_COLUMNS)
        val contentWidth = with(density) { (gutter + CodeSheet.textInset + 24.dp).toPx() } + columns * advance
        val most by rememberUpdatedState(max(0f, contentWidth - constraints.maxWidth))
        val sideways = rememberScrollableState { delta ->
            val before = x
            x = (x - delta).coerceIn(0f, most)
            before - x
        }
        val shownX = { x.coerceIn(0f, most) }
        val gutterPx = with(density) { gutter.toPx() }

        fun placeAt(point: Offset): CodePlace? {
            val items = listState.layoutInfo.visibleItemsInfo.filter { point.y >= it.offset && point.y < it.offset + it.size }
            if (items.any { layout.isHeading(it.index) }) return null
            return items.firstNotNullOfOrNull { layout.line(it.index) }
        }

        LazyColumn(
            state = listState,
            contentPadding = PaddingValues(bottom = (if (document.headed) CodeSheet.fileGap else 0.dp) + panelEndRoom),
            modifier = Modifier
                .fillMaxSize()
                .scrollable(sideways, Orientation.Horizontal)
                .pointerInput(layout) {
                    var anchor: CodePlace? = null
                    var last = Offset.Zero
                    detectDragGesturesAfterLongPress(
                        onDragStart = { point ->
                            val place = placeAt(point) ?: return@detectDragGesturesAfterLongPress
                            anchor = place
                            last = point
                            selection = CodeSelection(place.file, place.line, place.line)
                            haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        },
                        onDrag = { change, _ ->
                            val start = anchor ?: return@detectDragGesturesAfterLongPress
                            last = change.position
                            val place = placeAt(change.position) ?: return@detectDragGesturesAfterLongPress
                            if (place.file != start.file) return@detectDragGesturesAfterLongPress
                            val next = CodeSelection(start.file, min(start.line, place.line), max(start.line, place.line))
                            if (next != selection) haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                            selection = next
                        },
                        onDragEnd = {
                            if (anchor != null) menu.open(last.round())
                            anchor = null
                        },
                        onDragCancel = { anchor = null },
                    )
                },
        ) {
            document.files.forEachIndexed { index, file ->
                val closed = document.headed && file.path in collapsed
                if (document.headed) {
                    if (index > 0) item(key = "gap:${file.path}", contentType = "gap") { FileGap() }
                    stickyHeader(key = "heading:${file.path}", contentType = "heading") {
                        CodeHeading(
                            file, closed, marks,
                            onToggle = {
                                // A file that is closed from under its pinned heading leaves the view at its heading.
                                val heading = layout.headings[index]
                                if (!closed && listState.firstVisibleItemIndex > heading) scope.launch { listState.scrollToItem(heading) }
                                onToggle(file.path)
                            },
                            onOpen = { onOpenFile(file.path) },
                            onViewed = { onViewed(file.path) },
                        )
                    }
                }
                if (closed) return@forEachIndexed
                val media = file.media
                if (media != null) {
                    item(key = "media:${file.path}", contentType = "media") {
                        CodeMediaBlock(file, media, onMedia = { onMedia(file) }, onOpen = { onOpenMedia(file) })
                    }
                    return@forEachIndexed
                }
                items(file.lines.size, key = { "line:${file.path}:$it" }, contentType = { "line" }) { line ->
                    CodeLine(
                        file = file,
                        line = line,
                        headed = document.headed,
                        numberWidth = numberWidth,
                        gutter = gutter,
                        codeStyle = codeStyle,
                        numberStyle = numberStyle,
                        advance = advance,
                        marked = marks.marked[file.path]?.contains(line) == true,
                        selected = selection?.contains(index, line) == true,
                        x = shownX,
                        onTap = { point ->
                            Platform.endEditing()
                            if (marks.commentable && point.x < gutterPx && file.commentable(line) != null) {
                                onComment(CodePlace(index, line))
                                return@CodeLine
                            }
                            selection = null
                        },
                    )
                }
            }
        }
        MenuPopup(menu) {
            item("Copy", Symbol.Copy) { selection?.let { selectedText(document, it) }?.let(Platform::copy) }
        }
    }
}

/** How wide the column of each line's numbers is, for the largest number of the document. */
private fun numberWidth(document: CodeDocument): Dp {
    var last = 1
    for (file in document.files) {
        last = max(last, file.old.maxOrNull() ?: 0)
        last = max(last, file.new.maxOrNull() ?: 0)
    }
    val digits = max(2, last.toString().length)
    return (digits * 6.8f + 12).dp
}

private fun selectedText(document: CodeDocument, selection: CodeSelection): String? {
    val file = document.files.getOrNull(selection.file) ?: return null
    val lines = (selection.from..selection.to)
        .filter { it in file.lines.indices && (file.kind(it) != CodeFile.Kind.Note || selection.from == selection.to) }
        .map { file.lines[it] }
    return lines.joinToString("\n").ifEmpty { null }
}

/** The room between two files, and the line above the heading of the second. */
@Composable
private fun FileGap() {
    Column(Modifier.fillMaxWidth()) {
        Spacer(Modifier.height(CodeSheet.fileGap))
        Box(Modifier.fillMaxWidth().height(1.dp).background(LocalPalette.current.border))
    }
}

/** The name of the file with what happened to it and how many lines changed, and the buttons that close its lines and open the file. */
@Composable
private fun CodeHeading(file: CodeFile, closed: Boolean, marks: CodeMarks, onToggle: () -> Unit, onOpen: () -> Unit, onViewed: () -> Unit) {
    val palette = LocalPalette.current
    CompositionLocalProvider(LocalSurface provides Surface.Secondary) {
        Column(Modifier.fillMaxWidth().background(palette.backgroundSecondary)) {
            HighlightButton(onClick = onToggle, modifier = Modifier.fillMaxWidth().height(CodeSheet.headingHeight - 1.dp), radius = 0.dp) {
                Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                    Icon(if (closed) Symbol.ChevronRight else Symbol.ChevronDown, IconSize.Xs, Modifier.padding(start = 9.dp), color = palette.mutedStrongerForeground)
                    Icon(FileSymbol.symbol(file.path), iconSize(text = 12.5f), Modifier.padding(start = 6.dp), color = palette.mutedForeground)
                    Text(
                        headingTitle(file, palette), Modifier.padding(start = 4.dp).weight(1f), size = 12.5f, maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                    )
                    Spacer(Modifier.width(10.dp))
                    if (marks.viewable) ViewedBox(file.path in marks.viewed, onViewed)
                    if (file.added + file.removed > 0) {
                        Text(LineCountText.text(file.added, file.removed), Modifier.padding(end = 4.dp), style = LineCountText.style, maxLines = 1)
                    }
                    ActionButton(
                        icon = Symbol.SquareArrowOutUpRight, help = "Open the file", size = ControlSize.Small,
                        modifier = Modifier.padding(end = 6.dp), onClick = onOpen,
                    )
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(palette.border))
        }
    }
}

private fun headingTitle(file: CodeFile, palette: Palette): AnnotatedString = buildAnnotatedString {
    val folder = file.path.substringBeforeLast('/', "")
    if (folder.isNotEmpty()) withStyle(SpanStyle(color = palette.mutedForeground)) { append("$folder/") }
    withStyle(SpanStyle(color = palette.foreground, fontWeight = FontWeight.Medium)) { append(file.path.substringAfterLast('/')) }
    val note = when (file.change) {
        "added" -> "new"
        "deleted" -> "deleted"
        "renamed" -> file.from?.let { "was $it" }
        else -> null
    }
    if (note != null) withStyle(SpanStyle(color = palette.mutedStrongerForeground, fontSize = Theme.ui(11.5f).fontSize)) { append("   $note") }
}

/** The box that marks a file of a pull request viewed. */
@Composable
private fun ViewedBox(viewed: Boolean, onViewed: () -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.xs)
    Row(
        Modifier
            .fillMaxHeight()
            .toggleable(viewed, role = Role.Checkbox) { onViewed() }
            .padding(start = 6.dp, end = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val box = Modifier.size(14.dp)
        if (viewed) {
            Box(box.background(palette.primary, shape), contentAlignment = Alignment.Center) {
                Icon(Symbol.Check, IconSize.Xs, color = palette.primaryForeground)
            }
        } else {
            Box(box.border(1.dp, palette.border, shape))
        }
        Text("Viewed", Modifier.padding(start = 6.dp), size = 11.5f, color = if (viewed) palette.foreground else palette.mutedForeground, maxLines = 1)
    }
}

/** The picture of an image or a video, as large as it is or smaller to fit, or the room it waits in. A video's picture is its first frame, under the sign that it plays. */
@Composable
private fun CodeMediaBlock(file: CodeFile, media: CodeMedia, onMedia: () -> Unit, onOpen: () -> Unit) {
    val palette = LocalPalette.current
    LaunchedEffect(media) { if (media.picture == null && !media.asked) onMedia() }
    BoxWithConstraints(Modifier.fillMaxWidth().padding(CodeSheet.mediaInset)) {
        val picture: ImageBitmap? = media.picture
        if (picture == null) {
            Box(
                Modifier.fillMaxWidth().height(CodeSheet.mediaWaitingHeight).background(palette.backgroundSecondary, RoundedCornerShape(Radius.md)),
                contentAlignment = Alignment.Center,
            ) {
                if (media.failed) {
                    Text(
                        if (media.video) "This video couldn't be shown." else "This image couldn't be shown.",
                        size = 12.5f, color = palette.mutedForeground,
                    )
                }
            }
            return@BoxWithConstraints
        }
        val density = LocalDensity.current
        val width = with(density) { picture.width.toDp() }
        val height = with(density) { picture.height.toDp() }
        val scale = max(0f, min(1f, min(maxWidth / width, CodeSheet.mediaMaxHeight / height)))
        Box(
            Modifier
                .size(width * scale, height * scale)
                .clip(RoundedCornerShape(Radius.sm))
                .clickable(onClick = onOpen),
            contentAlignment = Alignment.Center,
        ) {
            Image(picture, contentDescription = file.path.substringAfterLast('/'), contentScale = ContentScale.FillBounds, modifier = Modifier.fillMaxSize())
            if (media.video) Icon(Symbol.CirclePlay, IconSize.Hero, color = OverPicture.foreground)
        }
    }
}

/** One line: its colour for what happened to it, its numbers, and its text, which moves sideways with the others. */
@Composable
private fun CodeLine(
    file: CodeFile,
    line: Int,
    headed: Boolean,
    numberWidth: Dp,
    gutter: Dp,
    codeStyle: TextStyle,
    numberStyle: TextStyle,
    advance: Float,
    marked: Boolean,
    selected: Boolean,
    x: () -> Float,
    onTap: (Offset) -> Unit,
) {
    val palette = LocalPalette.current
    val kind = file.kind(line)
    val fill = when (kind) {
        CodeFile.Kind.Added -> palette.success.wash()
        CodeFile.Kind.Removed -> palette.destructive.wash()
        CodeFile.Kind.Note -> palette.backgroundSecondary
        CodeFile.Kind.Unchanged -> Color.Transparent
    }
    val currentTap by rememberUpdatedState(onTap)
    Box(
        Modifier
            .fillMaxWidth()
            .height(CodeSheet.lineHeight)
            .background(fill)
            .pointerInput(Unit) { detectTapGestures { currentTap(it) } },
    ) {
        val spans = file.spans.getOrNull(line)
        val note = kind == CodeFile.Kind.Note
        val text = remember(file, line, spans, palette) { lineText(file.lines[line], spans, palette) }
        // A note stays where it is while the lines move sideways.
        if (note) {
            BasicText(
                text, Modifier.align(Alignment.CenterStart).padding(start = gutter + CodeSheet.textInset),
                style = codeStyle.copy(color = palette.mutedForeground), softWrap = false, maxLines = 1, overflow = TextOverflow.Clip,
            )
            return@Box
        }
        val density = LocalDensity.current
        val inset = with(density) { CodeSheet.textInset.toPx() }
        Box(Modifier.padding(start = gutter).fillMaxSize().clipToBounds()) {
            if (selected) {
                // A line that is selected is lit a little past its end, as its line break.
                val width = with(density) { ((text.length + 1) * advance).toDp() }
                Box(
                    Modifier
                        .offset { IntOffset((inset - x()).roundToInt(), 0) }
                        .width(width)
                        .fillMaxHeight()
                        .background(palette.primary.at(Opacity.ColorTintStronger)),
                )
            }
            BasicText(
                text,
                Modifier
                    .align(Alignment.CenterStart)
                    .wrapContentWidth(Alignment.Start, unbounded = true)
                    .offset { IntOffset((inset - x()).roundToInt(), 0) },
                style = codeStyle.copy(color = palette.foreground),
                softWrap = false,
                maxLines = 1,
            )
        }
        if (headed) LineNumber(file.old[line], numberWidth, numberStyle)
        LineNumber(file.new[line], gutter - 4.dp - (if (headed) numberWidth else 0.dp), numberStyle, Modifier.padding(start = if (headed) numberWidth else 0.dp))
        if (marked) {
            Box(
                Modifier
                    .padding(start = 3.dp, top = 5.dp)
                    .size(3.dp, CodeSheet.lineHeight - 10.dp)
                    .background(palette.primary, CircleShape),
            )
        }
    }
}

@Composable
private fun LineNumber(number: Int, width: Dp, style: TextStyle, modifier: Modifier = Modifier) {
    Box(modifier.width(width).fillMaxHeight(), contentAlignment = Alignment.CenterEnd) {
        if (number > 0) BasicText("$number", Modifier.padding(end = 6.dp), style = style.copy(color = LocalPalette.current.mutedStrongerForeground), maxLines = 1)
    }
}

/** The line's text in its colours, with each tab as far as the next stop of four columns. */
private fun lineText(line: String, spans: IntArray?, palette: Palette): AnnotatedString {
    val cut = min(line.length, CodeSheet.MOST_COLUMNS)
    val tabbed = line.indexOf('\t').let { it in 0 until cut }
    // Where each unit of the line is in the text drawn.
    val at = if (tabbed) IntArray(cut + 1) else null
    val drawn = if (at == null) {
        line.substring(0, cut)
    } else {
        buildString {
            for (index in 0 until cut) {
                at[index] = length
                val unit = line[index]
                if (unit == '\t') repeat(4 - length % 4) { append(' ') } else append(unit)
            }
            at[cut] = length
        }
    }
    if (spans == null || spans.isEmpty()) return AnnotatedString(drawn)
    val syntax = Theme.syntax(palette)
    return buildAnnotatedString {
        append(drawn)
        var index = 0
        while (index + 2 < spans.size) {
            val start = spans[index]
            val end = start + spans[index + 1]
            val colour = spans[index + 2]
            index += 3
            if (end > line.length || colour <= 0 || colour >= syntax.size) continue
            val from = min(start, cut).let { at?.get(it) ?: it }
            val to = min(end, cut).let { at?.get(it) ?: it }
            if (to > from) addStyle(SpanStyle(color = syntax[colour]), from, to)
        }
    }
}
