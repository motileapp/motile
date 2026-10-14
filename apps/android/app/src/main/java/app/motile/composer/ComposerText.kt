@file:OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)

package app.motile.composer

import android.content.ClipData
import android.view.KeyCharacterMap
import android.view.KeyEvent as AndroidKeyEvent
import android.webkit.MimeTypeMap
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.content.MediaType
import androidx.compose.foundation.content.ReceiveContentListener
import androidx.compose.foundation.content.TransferableContent
import androidx.compose.foundation.content.consume
import androidx.compose.foundation.content.contentReceiver
import androidx.compose.foundation.content.hasMediaType
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.OutputTransformation
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.SkillMenuKey
import app.motile.core.SkillPick
import app.motile.core.readSkillMenu
import app.motile.core.readSkillSpans
import app.motile.core.skillKey
import app.motile.composer.SkillChip.drawBox
import app.motile.platform.Platform
import app.motile.platform.scaled
import app.motile.theme.LocalDark
import app.motile.theme.Theme
import app.motile.ui.ImageFiles
import app.motile.ui.Text
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.math.max
import kotlin.math.min

/** The composer's text: its font, and how tall it grows. */
object ComposerText {
    /** Seventeen, as iOS's composer, with lines a chip fits on. */
    val style = Theme.ui(15f).copy(lineHeight = scaled(19f).sp)
    val verticalInset = 8.dp
    /** One line: a phone has no room to spare, and the composer grows with what is typed. */
    val minimumHeight = scaled(19f).dp + verticalInset * 2
    val maximumHeight = 180.dp
}

/**
 * What is written in the composer as it is edited: the text, which holds each skill as its `$name`,
 * and those of its `$name`s that show as chips. A chip is one piece: the cursor steps over it and a
 * delete takes it whole.
 */
@Stable
class ComposerTextModel(private val store: AppStore) {
    val state = TextFieldState()
    var chips by mutableStateOf(emptyList<ChipRange>())
        private set
    var focused by mutableStateOf(false)

    /** The draft the text was loaded from, and what was last written to it or read from it. */
    private var key: String? = null
    private var synced: String? = null
    /** The last skill picked from the menu that the text has put in. */
    private var picked: Int? = store.skillPick?.id
    /** An edit put in more than a character at once, as a paste does. */
    private var pasted = false

    /** The chips that are still where the text says, in order. */
    fun chips(text: CharSequence): List<ChipRange> = chips.filter { it.holds(text) }.sortedBy { it.start }

    /** The text with each chip's characters as one character that is no word, as the core and the menu read it. */
    private fun masked(text: String): String {
        val masked = StringBuilder(text)
        for (chip in chips(text)) for (index in chip.start until chip.end) masked.setCharAt(index, '￼')
        return masked.toString()
    }

    /** Shows the store's draft, when it isn't what is shown. */
    fun load(draftKey: String, draft: String) {
        if (draftKey == key && draft == synced) return
        key = draftKey
        synced = draft
        if (state.text.toString() != draft) state.setTextAndPlaceCursorAtEnd(draft)
        chips = emptyList()
        chipify(atEnd = true)
    }

    /** Keeps the store's draft as the text is, and makes chips of what was just finished. */
    fun changed(text: String) {
        if (key == store.draftKey && text != synced) {
            synced = text
            store.draft = text
        }
        val wasPasted = pasted
        pasted = false
        val cursor = state.selection.start
        if (wasPasted) return chipify(atEnd = true)
        if (cursor > 0 && cursor <= text.length && text[cursor - 1].isWhitespace()) chipify(atEnd = false)
    }

    /** Tells the menu what is typed before the cursor, or that nothing is. */
    fun readMenu() {
        val selection = state.selection
        if (!focused || !selection.collapsed) return store.readSkillMenu("")
        val text = state.text.toString()
        store.readSkillMenu(masked(text).substring(0, min(selection.start, text.length)))
    }

    /** Types the text where the cursor is, as a keyboard does. */
    fun insert(typed: String) {
        val selection = state.selection
        replace(selection.min, selection.max, typed)
    }

    /** Puts the picked skill in place of the word typed before the cursor. */
    fun put(pick: SkillPick?) {
        if (pick == null || pick.id == picked) return
        picked = pick.id
        val text = state.text.toString()
        val word = skillWord(masked(text), state.selection.start) ?: return
        val written = "$" + pick.name
        replace(word.first, word.last + 1, "$written ")
        chips = chips + ChipRange(word.first, written, pick.name, pick.label)
    }

    private fun replace(start: Int, end: Int, with: String) {
        state.edit {
            replace(start, end, with)
            selection = TextRange(start + with.length)
        }
        chips = shifted(chips, listOf(TextRange(start, end) to with.length))
    }

    /** Makes the text's `$name`s of skills chips: those a space or a line follows, and with `atEnd` the one that ends the text too. */
    private fun chipify(atEnd: Boolean) {
        val asked = state.text.toString()
        if (!asked.contains('$')) return
        store.readSkillSpans(asked) { spans ->
            if (state.text.toString() != asked) return@readSkillSpans
            val made = chips(asked)
            val ended = spans.filter { span ->
                if (span.start < 0 || span.end > asked.length) return@filter false
                if (made.any { it.start < span.end && span.start < it.end }) return@filter false
                if (span.end == asked.length) atEnd else asked[span.end].isWhitespace()
            }
            if (ended.isEmpty()) return@readSkillSpans
            chips = made + ended.map { ChipRange(it.start, asked.substring(it.start, it.end), it.name, it.label) }
        }
    }

    /** Keeps the chips where their text moved to, and has a delete that cuts into one take it whole. */
    val keeper = InputTransformation {
        val changes = changes
        if (changes.changeCount == 1 && chips.isNotEmpty()) {
            val removed = changes.getOriginalRange(0)
            val added = changes.getRange(0)
            val cut = chips.firstOrNull { it.start < removed.max && removed.min < it.end }
            if (added.collapsed && !removed.collapsed && cut != null && (removed.min > cut.start || removed.max < cut.end)) {
                revertAllChanges()
                replace(min(removed.min, cut.start), max(removed.max, cut.end), "")
            }
        }
        val edits = (0 until this.changes.changeCount).map { index ->
            this.changes.getOriginalRange(index) to this.changes.getRange(index).length
        }
        if (edits.any { (removed, length) -> length - removed.length > 1 }) pasted = true
        if (chips.isNotEmpty()) chips = shifted(chips, edits)
    }

    /** Shows each chip's `$name` as its chip. */
    fun output(styles: Pair<SpanStyle, SpanStyle>) = OutputTransformation {
        val written = asCharSequence().toString()
        for (chip in chips(written).asReversed()) {
            val shown = SkillChip.shown(chip.label)
            replace(chip.start, chip.end, shown)
            addStyle(styles.first, chip.start, chip.start + shown.length)
            addStyle(styles.second, chip.start + 1, chip.start + 2)
        }
    }

    /** Where each chip is in the text as shown. */
    fun shownChips(): List<IntRange> {
        var shift = 0
        return chips(state.text).map { chip ->
            val length = SkillChip.shown(chip.label).length
            val start = chip.start + shift
            shift += length - chip.written.length
            start until start + length
        }
    }

    companion object {
        /** The chips after edits, each a range of the text before them and the length it was replaced with, in order. A chip an edit touches is gone. */
        fun shifted(chips: List<ChipRange>, edits: List<Pair<TextRange, Int>>): List<ChipRange> = chips.mapNotNull { chip ->
            var shift = 0
            for ((removed, length) in edits) {
                when {
                    removed.max <= chip.start -> shift += length - removed.length
                    removed.min >= chip.end -> continue
                    else -> return@mapNotNull null
                }
            }
            chip.copy(start = chip.start + shift)
        }
    }
}

/**
 * The composer's text view: grows with what is typed up to a height, past which it scrolls, says
 * `placeholder` while empty and takes pasted images. Enter is a line break on the screen's keyboard
 * and sends from a keyboard with keys; while the skills' menu is open, the arrows, Enter, Tab and
 * Esc are the menu's.
 */
@Composable
fun ComposerTextField(model: ComposerTextModel, placeholder: String, focusRequester: FocusRequester, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    val dark = LocalDark.current
    val style = ComposerText.style.copy(color = palette.foreground)
    val output = remember(model, palette) { model.output(SkillChip.styles(palette)) }
    val scroll = rememberScrollState()
    val layout = remember { arrayOfNulls<() -> TextLayoutResult?>(1) }

    LaunchedEffect(model) {
        snapshotFlow { store.draftKey to store.draft }.collect { (key, draft) -> model.load(key, draft) }
    }
    LaunchedEffect(model) {
        snapshotFlow { model.state.text.toString() }.collect { model.changed(it) }
    }
    LaunchedEffect(model) {
        snapshotFlow { Triple(model.state.text.toString(), model.state.selection, model.focused) }.collect { model.readMenu() }
    }
    LaunchedEffect(store.skillPick) { model.put(store.skillPick) }

    BasicTextField(
        state = model.state,
        modifier = modifier
            .heightIn(min = ComposerText.minimumHeight, max = ComposerText.maximumHeight)
            .semantics { contentDescription = "Message" }
            .focusRequester(focusRequester)
            .onFocusChanged { model.focused = it.isFocused }
            .onPreviewKeyEvent { store.composerKey(it) }
            .contentReceiver(PastedImages(store)),
        textStyle = style,
        inputTransformation = model.keeper,
        outputTransformation = output,
        lineLimits = TextFieldLineLimits.MultiLine(),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        onTextLayout = { result -> layout[0] = result },
        cursorBrush = SolidColor(palette.primary),
        scrollState = scroll,
        decorator = { field ->
            Box(Modifier.padding(horizontal = 2.dp, vertical = ComposerText.verticalInset)) {
                if (model.state.text.isEmpty()) Text(placeholder, style = style, color = palette.mutedStrongerForeground, maxLines = 1)
                Box(Modifier.clipToBounds().drawBehind { drawChips(model, layout[0]?.invoke(), scroll, palette, dark) }) { field() }
            }
        },
    )
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawChips(
    model: ComposerTextModel,
    layout: TextLayoutResult?,
    scroll: ScrollState,
    palette: app.motile.theme.Palette,
    dark: Boolean,
) {
    if (layout == null) return
    for (range in model.shownChips()) drawBox(layout, range.first, range.last + 1, scroll.value.toFloat(), palette, dark)
}

/** What the composer does with a key of a keyboard with keys, ahead of the text: Enter sends, and the skills' menu takes its keys. */
private fun AppStore.composerKey(event: KeyEvent): Boolean {
    if (event.isShiftPressed || event.isAltPressed || event.isCtrlPressed || event.isMetaPressed) return false
    val native = event.nativeKeyEvent
    val screenKey = native.deviceId == KeyCharacterMap.VIRTUAL_KEYBOARD || native.flags and AndroidKeyEvent.FLAG_SOFT_KEYBOARD != 0
    val menuOpen = skillMenu != null
    val action: () -> Unit = when (event.key) {
        Key.Enter, Key.NumPadEnter -> when {
            screenKey -> return false
            menuOpen -> ({ skillKey(SkillMenuKey.Pick) })
            else -> ({ send() })
        }
        Key.Tab -> if (menuOpen) ({ skillKey(SkillMenuKey.Pick) }) else return false
        Key.DirectionUp -> if (menuOpen) ({ skillKey(SkillMenuKey.Up) }) else return false
        Key.DirectionDown -> if (menuOpen) ({ skillKey(SkillMenuKey.Down) }) else return false
        Key.Escape -> if (menuOpen) ({ skillKey(SkillMenuKey.Dismiss) }) else return false
        else -> return false
    }
    if (event.type == KeyEventType.KeyDown) action()
    return true
}

/** A copied image is attached; anything else is pasted as text. */
private class PastedImages(private val store: AppStore) : ReceiveContentListener {
    override fun onReceive(transferableContent: TransferableContent): TransferableContent? {
        if (!transferableContent.hasMediaType(MediaType.Image)) return transferableContent
        val images = mutableListOf<ClipData.Item>()
        val rest = transferableContent.consume { item ->
            val image = item.uri != null && item.text == null
            if (image) images += item
            image
        }
        if (images.isEmpty()) return rest
        attaching.launch {
            val files = images.mapNotNull { item -> savePasted(item) }
            withContext(Dispatchers.Main) { store.attach(files) }
        }
        return rest
    }

    private fun savePasted(item: ClipData.Item): File? {
        val uri = item.uri ?: return null
        val resolver = Platform.context.contentResolver
        val type = resolver.getType(uri) ?: "image/png"
        val extension = MimeTypeMap.getSingleton().getExtensionFromMimeType(type) ?: "png"
        val bytes = runCatching { resolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull() ?: return null
        return ImageFiles.saveForAttaching(bytes, extension)
    }

    companion object {
        val attaching = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    }
}

/** Gives the demo driver the composer's keyboard and typing while the composer is shown. */
@Composable
fun ComposerDemoHooks(model: ComposerTextModel, focusRequester: FocusRequester) {
    DisposableEffect(model) {
        app.motile.DemoHooks.focusComposer = { runCatching { focusRequester.requestFocus() } }
        app.motile.DemoHooks.typeInComposer = { model.insert(it) }
        onDispose {
            app.motile.DemoHooks.focusComposer = null
            app.motile.DemoHooks.typeInComposer = null
        }
    }
}
