package app.motile.core

import app.motile.panel.PanelTab
import app.motile.platform.Platform

// What the keyboard shortcuts' commands do, on a keyboard with keys, and what their conditions ask about the client.

/** The names that hold now for the rules' conditions, with what has the keys: the composer, or another field to type in. */
fun AppStore.shortcutContext(composerFocus: Boolean, editableFocus: Boolean): Set<String> = buildSet {
    if (composerFocus) add("composerFocus")
    if (editableFocus) add("editableFocus")
    if (selectedThread != null) {
        add("threadOpen")
        if (activity.running) add("turnRunning")
    }
    if (rightPanel.isOpen) add("rightPanelOpen")
    if (settings != null) add("settingsOpen")
    if (showsUsage) add("usagePageOpen")
}

/** The threads the jump keys and the next and previous thread go through: the sidebar's, as it lists them, or what its search found. */
val AppStore.jumpThreads: List<ThreadInfo>
    get() = if (isSearchingThreads) searchResults.mapNotNull { it.threadInfo } else activeThreads

/** The keys that open the thread at `index` among `jumpThreads`, drawn on its row while they are held. */
fun AppStore.jumpHint(index: Int): String? {
    if (!showsJumpHints || index >= 9) return null
    return shortcuts.label("thread.jump.${index + 1}")
}

/** Runs the command. Whether there was anything for it to do; without, the keys go on to whatever else takes them. */
fun AppStore.perform(command: String): Boolean {
    Regex("""thread\.jump\.([1-9])""").matchEntire(command)?.let { return showThread(it.groupValues[1].toInt() - 1) }
    usageCommands[command]?.let { action ->
        if (!showsUsage) return false
        action()
        return true
    }
    when (command) {
        "commandPalette.toggle" -> return togglePanel(PanelPage.Commands)
        "threadPicker.toggle" -> return togglePanel(PanelPage.Threads)
        "settings.open" -> {
            if (!account.signedIn) return false
            openSettings()
        }
        "usage.open" -> {
            if (!account.signedIn) return false
            openUsage()
        }
        "appearance.cycle" -> {
            val all = Appearance.entries
            Appearance.choose(all[(all.indexOf(Appearance.chosen) + 1) % all.size])
        }
        "chat.new" -> {
            if (!account.signedIn || servers.isEmpty()) return false
            newThread()
        }
        "chat.newLocal" -> startNewThread(composerProject?.takeIf(::startsThreads) ?: return false)
        "chat.newWithoutProject" -> {
            startNewThread(noProjectToOpen ?: return false)
        }
        "navigation.back", "navigation.forward" -> return go(back = command == "navigation.back")
        "thread.previous" -> return showThreadOffset(-1)
        "thread.next" -> return showThreadOffset(1)
        "thread.done" -> {
            val thread = selectedThread ?: return false
            if (onRemovedServer(thread)) return false
            toggleDone()
        }
        "thread.undo" -> {
            if (undo == null) return false
            performUndo()
        }
        "thread.stop" -> {
            if (!activity.busy || openThreadRemovedServer != null) return false
            stop()
        }
        "thread.steerQueuedMessage" -> {
            if (openThreadRemovedServer != null) return false
            sendNow(activity.queued.firstOrNull()?.id ?: return false)
        }
        "thread.editQueuedMessage" -> {
            if (openThreadRemovedServer != null) return false
            takeBack(activity.queued.lastOrNull()?.id ?: return false)
        }
        "pullRequest.copyLink" -> Platform.copy(shownPullRequest?.first ?: return false)
        "pullRequest.copyNumber" -> Platform.copy("#${shownPullRequest?.second ?: return false}")
        "composer.sendAlternate" -> {
            if (selectedThread == null || !activity.running || !canSend) return false
            send(alternate = true)
        }
        "composer.project" -> {
            if (selectedDraft == null) return false
            openPanel(PanelPage.DraftProject)
        }
        "composer.branch" -> {
            val project = composerProject ?: return false
            if (!canSwitchBranches(project)) return false
            showBranches(project)
        }
        "rightPanel.toggle" -> rightPanel.isOpen = !rightPanel.isOpen
        "rightPanel.toggleMaximized" -> {
            if (!rightPanel.canMaximize) return false
            rightPanel.toggleMaximized()
        }
        "rightPanel.new" -> {
            if (panelUnavailable != null) return false
            rightPanel.openBlank()
        }
        "rightPanel.nextTab", "rightPanel.previousTab" -> {
            if (!rightPanel.isOpen || rightPanel.tabs.tabs.size <= 1) return false
            rightPanel.activate(offset = if (command == "rightPanel.nextTab") 1 else -1)
        }
        "rightPanel.diff" -> {
            if (panelUnavailable != null || panelTarget?.repository != true) return false
            rightPanel.showDiff()
        }
        "rightPanel.files", "rightPanel.agents" -> {
            if (panelUnavailable != null) return false
            rightPanel.open(if (command == "rightPanel.files") PanelTab.Files else PanelTab.Agents)
        }
        "rightPanel.pullRequest", "rightPanel.pullRequests" -> {
            val all = command == "rightPanel.pullRequests"
            if (panelUnavailable != null || pullRequestsUnavailable != null || (all && !pullRequestsExtended)) return false
            rightPanel.open(if (all) PanelTab.PullRequests else PanelTab.PullRequest)
        }
        "rightPanel.linear" -> {
            if (panelUnavailable != null || !isOn(Integration.Linear) || linearUnavailable != null) return false
            rightPanel.open(PanelTab.Linear)
        }
        "rightPanel.ricrac" -> {
            if (panelUnavailable != null || !isOn(Integration.Ricrac) || ricracUnavailable != null) return false
            rightPanel.open(PanelTab.Ricrac)
        }
        "rightPanel.close" -> return rightPanel.closeActive()
        "sidebar.toggle" -> {
            if (sidebarToggle == null) return false
            sidebarToggle?.invoke()
        }
        else -> return false
    }
    return true
}

/** What toggles the sidebar on this screen: the drawer's, where the sidebar lies over the thread. The layout sets it. */
var sidebarToggle: (() -> Unit)? = null

private val AppStore.usageCommands: Map<String, () -> Unit>
    get() = mapOf(
        "usage.cost" to { usage.tab = UsageModel.Tab.Cost },
        "usage.tokens" to { usage.tab = UsageModel.Tab.Tokens },
        "usage.limits" to { usage.tab = UsageModel.Tab.Limits },
        "usage.period.day" to { usage.period = UsageModel.Period.Day },
        "usage.period.week" to { usage.period = UsageModel.Period.Week },
        "usage.period.month" to { usage.period = UsageModel.Period.Month },
        "usage.period.quarter" to { usage.period = UsageModel.Period.Quarter },
    )

/** Opens the command panel on the page, or closes it when it is already there. */
private fun AppStore.togglePanel(page: PanelPage): Boolean {
    if (panel == page) {
        closePanel()
        return true
    }
    if (!account.signedIn || (servers.isEmpty() && removedServers.isEmpty())) return false
    openPanel(page)
    return true
}

private fun AppStore.showThread(index: Int): Boolean {
    val thread = jumpThreads.getOrNull(index) ?: return false
    show(Selection.Thread(thread.id))
    return true
}

/** Opens the thread `offset` places from the open one in the sidebar. From a draft, the next is the first and the previous the last. */
private fun AppStore.showThreadOffset(offset: Int): Boolean {
    val threads = jumpThreads
    if (threads.isEmpty()) return false
    val open = selectedThread
    val index = open?.let { thread -> threads.indexOfFirst { it.id == thread.id } } ?: -1
    if (index < 0) return showThread(if (offset > 0) 0 else threads.size - 1)
    return showThread((index + offset).coerceIn(0, threads.size - 1))
}

/** The url and the number of the pull request the right panel shows, while its tab is the one in front. */
private val AppStore.shownPullRequest: Pair<String, Int>?
    get() {
        if (!rightPanel.isOpen) return null
        val active = rightPanel.tabs.active
        if (active != PanelTab.PullRequest && active !is PanelTab.PullRequestNumber) return null
        return shownPullRequestLink
    }

/** The pull request tab sets what it shows, for copying its link and number. */
var shownPullRequestLink: Pair<String, Int>? = null

// Changing the keyboard shortcuts, from the settings.

/** Gives the command the key, in place of the rule of `row` when it has one. */
fun AppStore.setShortcut(command: String, key: String, whenText: String?, replacing: ShortcutRow? = null) {
    val fields = buildMap<String, Any?> {
        put("command", command)
        put("key", key)
        if (!whenText.isNullOrBlank()) put("when", whenText)
        if (replacing != null && replacing.key.isNotEmpty()) put("replace", replacing.rule)
    }
    changeShortcuts("set_keybinding", fields)
}

fun AppStore.removeShortcut(row: ShortcutRow) = changeShortcuts("remove_keybinding", mapOf("rule" to row.rule))

fun AppStore.resetShortcut(command: String) = changeShortcuts("reset_keybinding", mapOf("command" to command))

/** Reads the key and the condition, as the rule of `row` if it has one, and says what they clash with. */
fun AppStore.checkShortcut(key: String, whenText: String?, row: ShortcutRow?, done: (ShortcutCheck) -> Unit) {
    val fields = buildMap<String, Any?> {
        put("key", key)
        whenText?.let { put("when", it) }
        if (row != null && row.key.isNotEmpty()) put("row", row.rule)
    }
    core.send("check_keybinding", fields) { result -> result.getOrNull()?.let { done(ShortcutCheck(it)) } }
}

private fun AppStore.changeShortcuts(type: String, fields: Map<String, Any?>) {
    core.send(type, fields) { result -> result.coreError?.let { errorMessage = it.message } }
}
