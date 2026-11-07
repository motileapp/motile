package app.motile.core

import app.motile.platform.Platform
import app.motile.platform.Symbol

/** A page of the settings, listed in the settings' sidebar. */
enum class SettingsSection(val raw: String) {
    // In the order they are reached for: what the client is, then what it works with, and the keys last.
    General("general"), Accounts("accounts"), Servers("servers"), AgentGateway("agentGateway"), McpGateway("mcpGateway"), Projects("projects"),
    PullRequests("pullRequests"), TextGeneration("textGeneration"), Integrations("integrations"), Keyboard("keyboard");

    val title: String
        get() = when (this) {
            General -> "General"
            Keyboard -> "Keybindings"
            Servers -> "Servers"
            Accounts -> "Providers"
            AgentGateway -> "Agent Gateway"
            McpGateway -> "MCP Gateway"
            Projects -> "Projects"
            TextGeneration -> "Text Generation"
            PullRequests -> "Pull Requests"
            Integrations -> "Integrations"
        }

    val symbol: Symbol
        get() = when (this) {
            General -> Symbol.SlidersHorizontal
            Keyboard -> Symbol.Keyboard
            Servers -> Symbol.Server
            Accounts -> Symbol.Bot
            AgentGateway -> Symbol.Waypoints
            McpGateway -> Symbol.Plug
            Projects -> Symbol.Folder
            TextGeneration -> Symbol.PencilLine
            PullRequests -> Symbol.GitPullRequest
            Integrations -> Symbol.Blocks
        }

    companion object {
        /** The sections this client has: the keyboard's only where there is one to set, as on iOS. */
        val shown: List<SettingsSection> = entries.filter { it != Keyboard }
    }
}

/** What managing new threads does, said under its switch. */
const val managesNewThreadsDescription = "Picks the account for each new thread."
const val workingShelfDescription = "Show working threads on their own shelf."

/** What the log is, said beside the way to it. */
val logDescription get() = "What Motile did on this ${Platform.device}, such as connecting to your servers. It helps look into a problem."

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
            SettingsEntry("updates", "Updates", SettingsSection.General, "version check for updates release download install"),
            SettingsEntry("appearance", "Theme", SettingsSection.General, "appearance light dark system mode"),
            SettingsEntry("messages", "Sent while the agent works", SettingsSection.General, "messages queue steer interrupt"),
            SettingsEntry("working-shelf", "Working shelf", SettingsSection.General, "sidebar working threads shelf section running monitoring"),
            SettingsEntry("manage-new-threads", "Manage new threads automatically", SettingsSection.General, "provider limits usage accounts switch quota rate"),
            SettingsEntry("storage", "Images and videos", SettingsSection.General, "storage cache clear media disk"),
            SettingsEntry("log", "Log", SettingsSection.General, "troubleshooting debug problem connecting logs file"),
            SettingsEntry("agent-accounts", "Provider Accounts", SettingsSection.Accounts, "agents claude codex sign in account folder subscription plan api key router"),
            SettingsEntry("servers", "Servers", SettingsSection.Servers, "add remove unlink unlinked delete copies machine agents connected"),
            SettingsEntry("backup", "Backup", SettingsSection.Servers, "s3 bucket r2 b2 minio storage sync media restore"),
            SettingsEntry("continue-limits", "Continue after usage limits", SettingsSection.Servers, "rate limit reset resume quota wait"),
            SettingsEntry("continue-restarts", "Continue after restarts", SettingsSection.Servers, "restart update crash resume interrupted"),
            SettingsEntry("gateway", "Gateway", SettingsSection.AgentGateway, "add remove replace enroll docker unbind image volume code connected"),
            SettingsEntry("gateway-accounts", "Accounts", SettingsSection.AgentGateway, "claude codex sign in priority accounts pool limits next"),
            SettingsEntry("gateway-log", "Log", SettingsSection.AgentGateway, "requests errors capture captured headers body"),
            SettingsEntry("mcp-servers", "MCP Servers", SettingsSection.McpGateway, "mcp tools connectors linear github notion sentry"),
            SettingsEntry("mcp-activity", "Activity", SettingsSection.McpGateway, "calls tools log errors"),
            SettingsEntry("projects", "Projects", SettingsSection.Projects, "add remove folder icon setup worktree script"),
            SettingsEntry("merged", "Archive the thread after merge or close", SettingsSection.PullRequests, "merge close finished archive"),
            SettingsEntry("worktrees", "Remove the thread's worktree after merge", SettingsSection.PullRequests, "merge pushed branch clean up"),
            SettingsEntry("pull", "Automatically pull", SettingsSection.PullRequests, "default branch main merge sync fetch fast-forward"),
            SettingsEntry("text-model", "Model", SettingsSection.TextGeneration, "titles branch names commit messages pull requests writer"),
            SettingsEntry("branch-names", "Branch names", SettingsSection.TextGeneration, "instructions prefix naming"),
            SettingsEntry("integration-linear", "Linear", SettingsSection.Integrations, "issues tracker tab connect"),
            SettingsEntry("integration-ricrac", "Ricrac", SettingsSection.Integrations, "issues tracker tab connect"),
            SettingsEntry("shortcuts", "Keybindings", SettingsSection.Keyboard, "keys keyboard shortcuts hotkeys commands record keybindings.json"),
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
