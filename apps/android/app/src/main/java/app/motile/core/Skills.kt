package app.motile.core

import app.motile.platform.Symbol

/** The skills that fit the word being typed before the composer's cursor, as the core ranks them, and the word they would replace. */
data class SkillMenu(val typed: String, val rows: List<SkillRow>) {
    constructor(json: JSON) : this(json.string("typed"), json.objects("rows").map(::SkillRow))
}

data class SkillRow(
    val name: String,
    val label: String,
    val description: String,
    /** Where it comes from, and the symbol that says so. */
    val source: String,
    val symbol: Symbol,
) {
    val id: String get() = name

    constructor(json: JSON) : this(
        json.string("name"), json.string("label"), json.string("description"), json.string("source"),
        Symbol.lucide(json.string("symbol")) ?: Symbol.Box,
    )
}

/** A skill a text starts, where its `$name` is, in UTF-16 units as Kotlin's strings count. */
data class SkillSpan(val start: Int, val length: Int, val name: String, val label: String) {
    constructor(json: JSON) : this(json.int("start"), json.int("len"), json.string("name"), json.string("label"))

    val end: Int get() = start + length
}

/** The skill the composer's text puts in place of the word typed before its cursor. */
data class SkillPick(val id: Int, val name: String, val label: String)

enum class SkillMenuKey { Up, Down, Pick, Dismiss }

/** Asks the core which skills fit the word that ends `beforeCursor`, the composer's text up to its cursor. Only the last answer counts. */
fun AppStore.readSkillMenu(beforeCursor: String) {
    if (!beforeCursor.contains('$') && !beforeCursor.contains('/')) {
        skillMenuDismissed = null
        return closeSkillMenu()
    }
    val project = composerProject ?: return closeSkillMenu()
    val account = composerAccount ?: return closeSkillMenu()
    skillMenusAsked += 1
    val asked = skillMenusAsked
    val fields = mapOf(
        "server_id" to project.serverID, "project_id" to project.id, "account" to account.id,
        "before_cursor" to beforeCursor.takeLast(200),
    )
    core.send("skill_menu", fields) { result ->
        if (asked != skillMenusAsked) return@send
        val answer = result.getOrNull() ?: return@send
        val menu = SkillMenu(answer)
        if (menu.typed != skillMenuDismissed) skillMenuDismissed = null
        if (menu.rows.isEmpty() || skillMenuDismissed != null) {
            skillMenu = null
            return@send
        }
        if (menu.rows.map { it.name } != skillMenu?.rows?.map { it.name }) skillHighlight = 0
        skillMenu = menu
    }
}

fun AppStore.closeSkillMenu() {
    skillMenusAsked += 1
    skillMenu = null
}

/** Leaves the menu closed until something else is typed. */
fun AppStore.dismissSkillMenu() {
    skillMenuDismissed = skillMenu?.typed
    closeSkillMenu()
}

fun AppStore.moveSkillHighlight(step: Int) {
    val count = skillMenu?.rows?.size ?: return
    if (count == 0) return
    skillHighlight = (skillHighlight + step + count) % count
}

/** Puts the skill, or the lit one, in place of the word typed. */
fun AppStore.pickSkill(row: SkillRow? = null) {
    val menu = skillMenu ?: return
    val picked = row ?: menu.rows.getOrNull(skillHighlight) ?: return
    skillPick = SkillPick((skillPick?.id ?: 0) + 1, picked.name, picked.label)
    closeSkillMenu()
}

fun AppStore.skillKey(key: SkillMenuKey) = when (key) {
    SkillMenuKey.Up -> moveSkillHighlight(-1)
    SkillMenuKey.Down -> moveSkillHighlight(1)
    SkillMenuKey.Pick -> pickSkill()
    SkillMenuKey.Dismiss -> dismissSkillMenu()
}

/** The skills of the composer's agent that `text` names by their `$name`s. */
fun AppStore.readSkillSpans(text: String, then: (List<SkillSpan>) -> Unit) {
    if (!text.contains('$')) return
    val project = composerProject ?: return
    val account = composerAccount ?: return
    val fields = mapOf("server_id" to project.serverID, "project_id" to project.id, "account" to account.id, "text" to text)
    core.send("skill_chips", fields) { result ->
        val answer = result.getOrNull() ?: return@send
        then(answer.objects("chips").map(::SkillSpan))
    }
}
