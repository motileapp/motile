package app.motile.core

/** A condition of a rule, on what the client is doing: names joined by not, and, or. */
sealed interface ShortcutCondition {
    data class Name(val name: String) : ShortcutCondition
    data class Not(val of: ShortcutCondition) : ShortcutCondition
    data class And(val left: ShortcutCondition, val right: ShortcutCondition) : ShortcutCondition
    data class Or(val left: ShortcutCondition, val right: ShortcutCondition) : ShortcutCondition

    fun holds(context: Set<String>): Boolean = when (this) {
        is Name -> name == "true" || name in context
        is Not -> !of.holds(context)
        is And -> left.holds(context) && right.holds(context)
        is Or -> left.holds(context) || right.holds(context)
    }

    companion object {
        fun of(json: JSON?): ShortcutCondition? {
            json ?: return null
            val left = of(json.obj("left"))
            val right = of(json.obj("right"))
            return when (json.string("type")) {
                "name" -> Name(json.string("name"))
                "not" -> of(json.obj("of"))?.let(::Not)
                "and" -> if (left != null && right != null) And(left, right) else null
                "or" -> if (left != null && right != null) Or(left, right) else null
                else -> null
            }
        }
    }
}

/** A key and the modifiers held with it, as the core names them: `command` is the meta key here. */
data class ShortcutKeys(val key: String, val command: Boolean, val control: Boolean, val option: Boolean, val shift: Boolean)

/** A rule the core sent: the key that runs the command, where its condition holds. */
data class ShortcutRule(
    val command: String,
    val keys: ShortcutKeys,
    val whenCondition: ShortcutCondition?,
    /** The keys as they are drawn, one cap each. */
    val caps: List<String>,
) {
    constructor(json: JSON) : this(
        command = json.string("command"),
        keys = (json.obj("shortcut") ?: EmptyJSON).let {
            ShortcutKeys(it.string("key"), it.bool("command"), it.bool("control"), it.bool("option"), it.bool("shift"))
        },
        whenCondition = ShortcutCondition.of(json.obj("when")),
        caps = json.strings("caps"),
    )

    fun applies(context: Set<String>): Boolean = whenCondition?.holds(context) ?: true
}

/** One rule as the settings list it, or a command that has no key. */
data class ShortcutRow(
    val id: String,
    val command: String,
    val label: String,
    /** As the file writes it, as in `mod+shift+k`. Empty without a key. */
    val key: String,
    val caps: List<String>,
    val whenText: String,
    val custom: Boolean,
    val resettable: Boolean,
    val conflicts: List<String>,
) {
    constructor(json: JSON) : this(
        json.string("id"), json.string("command"), json.string("label"), json.string("key"), json.strings("caps"),
        json.string("when"), json.bool("custom"), json.bool("resettable"), json.strings("conflicts"),
    )

    /** The rule as the core takes it back, to replace or remove. */
    val rule: Map<String, Any?>
        get() = buildMap {
            put("command", command)
            put("key", key)
            if (whenText.isNotEmpty()) put("when", whenText)
        }
}

data class ShortcutGroup(val title: String, val rows: List<ShortcutRow>)

/** The keyboard shortcuts as the core last sent them. */
data class Shortcuts(
    val rules: List<ShortcutRule> = emptyList(),
    val groups: List<ShortcutGroup> = emptyList(),
    val commands: List<Pair<String, String>> = emptyList(),
    val conditions: List<String> = emptyList(),
    val issues: List<String> = emptyList(),
    val path: String = "",
) {
    constructor(json: JSON) : this(
        rules = json.objects("rules").map(::ShortcutRule),
        groups = json.objects("groups").map { group -> ShortcutGroup(group.string("title"), group.objects("rows").map(::ShortcutRow)) },
        commands = json.objects("commands").map { it.string("id") to it.string("label") },
        conditions = json.strings("conditions"),
        issues = json.strings("issues"),
        path = json.string("path"),
    )

    /** The command the keys run where `context` holds: the last rule that matches. `candidates` are the names the pressed key goes by. */
    fun command(candidates: Set<String>, command: Boolean, control: Boolean, option: Boolean, shift: Boolean, context: Set<String>): String? =
        rules.lastOrNull { rule ->
            rule.keys.command == command && rule.keys.control == control && rule.keys.option == option && rule.keys.shift == shift &&
                rule.keys.key in candidates && rule.applies(context)
        }?.command

    /** The rules of the command that run where `context` holds, each not taken by a later rule on its keys, in the order they were given. */
    fun effective(command: String, context: Set<String> = emptySet()): List<ShortcutRule> {
        val taken = mutableListOf<ShortcutKeys>()
        val found = mutableListOf<ShortcutRule>()
        for (rule in rules.asReversed()) {
            if (!rule.applies(context) || rule.keys in taken) continue
            taken.add(rule.keys)
            if (rule.command == command) found.add(0, rule)
        }
        return found
    }

    /** The keys of the command, as said in a tooltip: "⌘B". */
    fun label(command: String, context: Set<String> = emptySet()): String? = effective(command, context).firstOrNull()?.caps?.joinToString("")

    /** The text with the command's keys after it in parentheses, as a tooltip says them. */
    fun help(text: String, command: String, context: Set<String> = emptySet()): String = label(command, context)?.let { "$text ($it)" } ?: text
}

/** How a key being chosen reads, and what is wrong with it. */
data class ShortcutCheck(
    val caps: List<String> = emptyList(),
    val conflicts: List<String> = emptyList(),
    /** The condition's names the client doesn't know, which are never true. */
    val unknown: List<String> = emptyList(),
    val whenError: String? = null,
) {
    constructor(json: JSON) : this(json.strings("caps"), json.strings("conflicts"), json.strings("unknown"), json.optionalString("when_error"))
}
