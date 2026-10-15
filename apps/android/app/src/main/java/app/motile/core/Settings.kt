package app.motile.core

import app.motile.platform.Symbol

/** A page of the settings, listed in the settings' sidebar. */
enum class SettingsSection(val raw: String) {
    General("general"), Keyboard("keyboard"), Servers("servers"), Agents("agents"), Projects("projects"),
    TextGeneration("textGeneration"), PullRequests("pullRequests");

    val title: String
        get() = when (this) {
            General -> "General"
            Keyboard -> "Keybindings"
            Servers -> "Servers"
            Agents -> "Agents"
            Projects -> "Projects"
            TextGeneration -> "Text Generation"
            PullRequests -> "Pull Requests"
        }

    val symbol: Symbol
        get() = when (this) {
            General -> Symbol.SlidersHorizontal
            Keyboard -> Symbol.Keyboard
            Servers -> Symbol.Server
            Agents -> Symbol.Bot
            Projects -> Symbol.Folder
            TextGeneration -> Symbol.PencilLine
            PullRequests -> Symbol.GitPullRequest
        }

    companion object {
        /** The sections this client has: the keyboard's only where there is one to set, as on iOS. */
        val shown: List<SettingsSection> = entries.filter { it != Keyboard }
    }
}

/** What automatically pulling does, said under its switch. */
const val pullDefaultBranchCaption = "Keeps each project's folder current while it is on its default branch with no changes or commits of its own."

/** One group of settings, as the settings' search finds it. Its `id` is the group's on its page. */
data class SettingsEntry(
    val id: String,
    val title: String,
    val section: SettingsSection,
    /** Words the group is also found by, beyond its title and its section's. */
    val keywords: String,
) {
    companion object {
        val all = listOf(
            SettingsEntry("account", "Account", SettingsSection.General, "signed in email sign out"),
            SettingsEntry("updates", "Version", SettingsSection.General, "version check for updates release"),
            SettingsEntry("appearance", "Theme", SettingsSection.General, "appearance light dark system mode"),
            SettingsEntry("messages", "Sent while the agent works", SettingsSection.General, "messages queue steer interrupt"),
            SettingsEntry("storage", "Images and videos", SettingsSection.General, "storage cache clear media disk"),
            SettingsEntry("shortcuts", "Keybindings", SettingsSection.Keyboard, "keys keyboard shortcuts hotkeys commands record keybindings.json"),
            SettingsEntry("servers", "Servers", SettingsSection.Servers, "add remove machine agents connected"),
            SettingsEntry("continue-limits", "Continue after usage limits", SettingsSection.Servers, "rate limit reset resume quota wait"),
            SettingsEntry("continue-restarts", "Continue after restarts", SettingsSection.Servers, "restart update crash resume interrupted"),
            SettingsEntry("agent-accounts", "Accounts", SettingsSection.Agents, "claude codex sign in login folder subscription plan api key router"),
            SettingsEntry("projects", "Projects", SettingsSection.Projects, "add remove folder icon setup worktree script"),
            SettingsEntry("text-model", "Model", SettingsSection.TextGeneration, "titles branch names commit messages pull requests writer"),
            SettingsEntry("branch-names", "Branch names", SettingsSection.TextGeneration, "instructions prefix naming"),
            SettingsEntry("merged", "Mark the thread done after merge or close", SettingsSection.PullRequests, "merge close finished"),
            SettingsEntry("worktrees", "Remove the thread's worktree after merge", SettingsSection.PullRequests, "merge pushed branch clean up"),
            SettingsEntry("pull", "Automatically pull", SettingsSection.PullRequests, "default branch main merge sync fetch fast-forward"),
        )

        /** The groups every word of the query is found in, by title, section or keywords, and the shortcuts' commands it finds by name. */
        fun matching(query: String, shortcuts: Shortcuts = Shortcuts()): List<SettingsEntry> {
            val words = query.split(Regex("\\s+")).filter { it.isNotEmpty() }
            if (words.isEmpty()) return emptyList()
            val commands = if (SettingsSection.Keyboard in SettingsSection.shown) {
                shortcuts.commands.map { SettingsEntry("shortcut-${it.first}", it.second, SettingsSection.Keyboard, "shortcut keys") }
            } else emptyList()
            return (all + commands).filter { entry ->
                val text = "${entry.title} ${entry.section.title} ${entry.keywords}"
                entry.section in SettingsSection.shown && words.all { text.contains(it, ignoreCase = true) }
            }
        }
    }
}
