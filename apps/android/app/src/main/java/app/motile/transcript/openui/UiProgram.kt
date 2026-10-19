package app.motile.transcript.openui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.core.JSON
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiSlot
import app.motile.core.UiTableCell
import app.motile.core.bool
import app.motile.core.obj
import app.motile.core.string
import app.motile.core.strings
import app.motile.transcript.CodeContent
import app.motile.transcript.Typesetting
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * A program row: its source, and the tree core resolved it into, which `ui_patch` events change.
 * The patches are applied off the main thread, in order, and the nodes they left alone stay the
 * same objects, so that only what changed is composed and measured again.
 */
class UiContent(json: JSON) {
    val code = CodeContent("openui-lang", json.string("source"), IntArray(0))
    val complete = json.bool("complete")
    /** The thread's server is too old to run the program's tools or keep its state. */
    val limited = json.bool("limited")

    /** Only the patching thread reads and writes it once the row is made. */
    private var source: JsonElement? = json["tree"]?.takeIf { it !is JsonNull }

    var tree by mutableStateOf(json.obj("tree")?.let(UiNode::from))
        private set
    var errors by mutableStateOf(json.strings("errors"))
        private set

    fun patch(ops: List<JSON>) {
        Typesetting.run({
            var errors: List<String>? = null
            for (op in ops) {
                if (op.string("op") == "errors") errors = op.strings("errors") else source = UiPatch.apply(source, op)
            }
            Pair(source, errors)
        }) { (patched, errors) ->
            val new = (patched as? JsonObject)?.let(UiNode::from)
            tree = new?.let { UiReuse.reuse(tree, it) }
            if (errors != null) this.errors = errors
        }
    }
}

/** Applies core's `ui_patch` ops to a tree's JSON, rebuilding only the objects on the way to what changed. */
object UiPatch {
    fun apply(tree: JsonElement?, op: JSON): JsonElement? = when (op.string("op")) {
        "root" -> op["node"]?.takeIf { it !is JsonNull }
        "replace" -> tree?.let { node(it, op.string("id")) { _ -> op["node"] ?: JsonNull } }
        "props" -> tree?.let {
            val set = op.obj("set") ?: JsonObject(emptyMap())
            val unset = op.strings("unset").toSet()
            node(it, op.string("id")) { found -> JsonObject(found.filterKeys { key -> key !in unset } + set) }
        }
        "insert" -> tree?.let {
            val key = op.string("key")
            val index = op["index"].let { (it as? JsonPrimitive)?.content?.toIntOrNull() ?: 0 }
            node(it, op.string("parent")) { found ->
                val list = (found[key] as? JsonArray)?.toMutableList() ?: mutableListOf()
                list.add(index.coerceIn(0, list.size), op["node"] ?: JsonNull)
                JsonObject(found + (key to JsonArray(list)))
            }
        }
        "remove" -> tree?.let { remove(it, op.string("id")) }
        else -> tree
    }

    private fun idOf(element: JsonElement): String? {
        val obj = element as? JsonObject ?: return null
        if (obj["component"] == null) return null
        return (obj["id"] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }

    /** Whether the node `id` can lie under the node `within`: ids are paths, and a node's descendants' extend its own. */
    private fun under(id: String, within: String): Boolean = within.isEmpty() || id == within || id.startsWith("$within/")

    private fun node(element: JsonElement, id: String, change: (JsonObject) -> JsonElement): JsonElement = rebuild(element) { candidate ->
        val found = idOf(candidate)
        when {
            found == id -> change(candidate as JsonObject)
            found != null && !under(id, found) -> candidate
            else -> null
        }
    }

    private fun remove(element: JsonElement, id: String): JsonElement = when (element) {
        is JsonArray -> {
            val kept = element.filter { idOf(it) != id }.map { remove(it, id) }
            if (kept.size == element.size && kept.indices.all { kept[it] === element[it] }) element else JsonArray(kept)
        }
        is JsonObject -> {
            val found = idOf(element)
            if (found != null && !under(id, found)) {
                element
            } else {
                var changed = false
                val entries = element.mapValues { (_, value) -> remove(value, id).also { if (it !== value) changed = true } }
                if (changed) JsonObject(entries) else element
            }
        }
        else -> element
    }

    /** Walks the element: `visit` answers a replacement for what it settles, or null to look inside. Unchanged parts stay the same objects. */
    private fun rebuild(element: JsonElement, visit: (JsonElement) -> JsonElement?): JsonElement {
        visit(element)?.let { return it }
        return when (element) {
            is JsonArray -> {
                val items = element.map { rebuild(it, visit) }
                if (items.indices.all { items[it] === element[it] }) element else JsonArray(items)
            }
            is JsonObject -> {
                var changed = false
                val entries = element.mapValues { (_, value) -> rebuild(value, visit).also { if (it !== value) changed = true } }
                if (changed) JsonObject(entries) else element
            }
            else -> element
        }
    }
}

/** Keeps the nodes of the tree before that a patch left as they were. */
object UiReuse {
    fun reuse(old: UiNode?, new: UiNode): UiNode {
        if (old == null || old.id != new.id || old.kind::class != new.kind::class) return new
        val before = nodes(old.kind).associateBy { it.id }
        val merged = new.copy(kind = mapNodes(new.kind) { child -> reuse(before[child.id], child) })
        return if (merged == old) old else merged
    }

    /** Every node right under the node, in its children, slots, inlines and cells. */
    fun nodes(kind: UiKind): List<UiNode> = buildList { mapNodes(kind) { add(it); it } }

    fun mapNodes(kind: UiKind, map: (UiNode) -> UiNode): UiKind {
        fun slot(slot: UiSlot) = slot.copy(children = slot.children.map(map))
        return when (kind) {
            is UiKind.Col -> kind.copy(children = kind.children.map(map))
            is UiKind.Row -> kind.copy(children = kind.children.map(map))
            is UiKind.Box -> kind.copy(children = kind.children.map(map))
            is UiKind.Grid -> kind.copy(children = kind.children.map(map))
            is UiKind.Card -> kind.copy(children = kind.children.map(map))
            is UiKind.Carousel -> kind.copy(children = kind.children.map(map))
            is UiKind.Popover -> kind.copy(trigger = slot(kind.trigger), content = slot(kind.content))
            is UiKind.Text -> kind.copy(inlines = kind.inlines.map(map))
            is UiKind.List -> kind.copy(items = kind.items.map(map))
            is UiKind.ListItem -> kind.copy(children = kind.children.map(map))
            is UiKind.Pressable -> kind.copy(children = kind.children.map(map))
            is UiKind.Form -> kind.copy(children = kind.children.map(map))
            is UiKind.Flashcard -> kind.copy(front = slot(kind.front), back = slot(kind.back))
            is UiKind.Table -> kind.copy(
                rows = kind.rows.map { row -> row.map { cell -> if (cell is UiTableCell.Node) UiTableCell.Node(map(cell.node)) else cell } },
            )
            else -> kind
        }
    }
}
