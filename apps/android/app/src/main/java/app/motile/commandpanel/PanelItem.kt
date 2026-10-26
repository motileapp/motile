package app.motile.commandpanel

import app.motile.platform.Symbol
import app.motile.core.Project as ProjectInfo

internal class PanelSection(val title: String, val items: List<PanelItem>)

/** What a row shows before its text. */
internal sealed interface PanelIcon {
    data class Glyph(val symbol: Symbol) : PanelIcon
    data class Project(val project: ProjectInfo?) : PanelIcon
}

internal class PanelItem(
    val id: String,
    val title: String,
    val detail: String,
    val icon: PanelIcon,
    /** It leads to another page of the panel. */
    val keepsOpen: Boolean = false,
    val action: () -> Unit,
) {
    /** Shown instead of `detail`, each part after its symbol. */
    var detailParts: List<Pair<Symbol, String>> = emptyList()
    /** The detail follows the title on its line. */
    var oneLine = false
    /** The keys of the command it runs, as the shortcuts have them. */
    var keys: String? = null
    /** Its place among what can be selected, for moving through with the arrow keys. */
    var index = -1
    /** Said at the row's end, as a warning with `warns`. */
    var note: String? = null
    var warns = false
    /** It is shown and can't be selected. */
    var off = false
    /** What it started is still going on. */
    var busy = false
    /** Stands in for a row that is on its way, with as many lines of text. */
    var placeholderLines = 0

    val selectable: Boolean get() = !off && placeholderLines == 0

    companion object {
        fun placeholder(position: Int, lines: Int) = PanelItem("placeholder-$position", "", "", PanelIcon.Glyph(Symbol.Folder)) {}.apply {
            placeholderLines = lines
            index = position
        }
    }
}
