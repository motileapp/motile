package app.motile.core

import android.webkit.MimeTypeMap
import app.motile.platform.Symbol
import java.io.File
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.UUID
import kotlin.math.abs
import kotlin.math.max

// What the core sends, read from its JSON. The shapes are defined in crates/core/src/api.rs and
// crates/protocol/src/wire.rs.

data class Account(
    val signedIn: Boolean = false,
    val email: String = "",
    val name: String? = null,
    val picture: String? = null,
    val deviceKey: String = "",
    val authURL: String = "",
    val error: String? = null,
) {
    constructor(json: JSON) : this(
        signedIn = json.bool("signed_in"),
        email = json.obj("user")?.string("email") ?: "",
        name = json.obj("user")?.optionalString("name"),
        picture = json.obj("user")?.optionalString("picture"),
        deviceKey = json.string("device_key"),
        authURL = json.string("auth_url"),
        error = json.optionalString("error"),
    )
}

enum class Agent(val raw: String) {
    Claude("claude"), Codex("codex");

    val title: String
        get() = when (this) {
            Claude -> "Claude Code"
            Codex -> "Codex"
        }

    companion object {
        fun of(raw: String?): Agent? = entries.firstOrNull { it.raw == raw }
    }
}

data class ModelInfo(
    val id: String,
    val name: String,
    /** The name without its maker, for the model picker. */
    val shortName: String,
    val agent: Agent,
    /** The account whose agent lists it. Empty from a server that lists models for an agent as a whole. */
    val account: String,
    val efforts: List<String>,
    /** The effort the agent runs it at when the thread doesn't pick one. Missing when the server couldn't learn it. */
    val defaultEffort: String?,
) {
    constructor(json: JSON, shortNames: JSON?) : this(
        id = json.string("id"),
        name = json.string("name"),
        shortName = shortNames?.optionalString(json.string("id")) ?: json.string("name"),
        agent = Agent.of(json.string("agent")) ?: Agent.Claude,
        account = json.string("account"),
        efforts = json.strings("efforts"),
        defaultEffort = json.optionalString("default_effort"),
    )

    /** What the effort menu offers: the efforts, with the agent's own default as "" ahead of them only when the server couldn't say which one that is. */
    val effortChoices: List<String>
        get() = if (efforts.isEmpty() || defaultEffort != null) efforts else listOf("") + efforts

    fun runs(under: AgentAccount): Boolean = agent == under.agent && (account.isEmpty() || account == under.id)
}

/** One of an agent's accounts on a server: the folder its CLI keeps the sign-in in. */
data class AgentAccount(
    val id: String,
    val agent: Agent,
    val name: String,
    /** `CLAUDE_CONFIG_DIR` or `CODEX_HOME`. Empty for the agent's usual folder, which one account of each agent can use. */
    val folder: String,
    /** A Codex account that keeps only its sign-in in `folder` and shares the rest, its sessions too, with `~/.codex`. */
    val sharesSessions: Boolean,
    val variables: List<Variable>,
    /** Who is signed in, as the agent's CLI last said. */
    val email: String?,
    val plan: String?,
) {
    data class Variable(
        val name: String,
        /** Empty for a sensitive one: its value stays on the server, which keeps it when it is saved without one. */
        val value: String,
        val sensitive: Boolean,
    )

    constructor(json: JSON) : this(
        id = json.string("id"),
        agent = Agent.of(json.string("agent")) ?: Agent.Claude,
        name = json.string("name"),
        folder = json.string("folder"),
        sharesSessions = json.bool("shares_sessions"),
        variables = json.objects("variables").map { Variable(it.string("name"), it.string("value"), it.bool("sensitive")) },
        email = json.optionalString("email"),
        plan = json.optionalString("plan"),
    )

    val json: Map<String, Any?>
        get() = mapOf(
            "id" to id, "agent" to agent.raw, "name" to name, "folder" to folder, "shares_sessions" to sharesSessions,
            "variables" to variables.map { mapOf("name" to it.name, "value" to it.value, "sensitive" to it.sensitive) },
        )

    /** The variable the account's folder is given to its agent as. */
    val folderVariable: String get() = if (agent == Agent.Claude) "CLAUDE_CONFIG_DIR" else "CODEX_HOME"

    /** The folder the agent's CLI uses when the account gives it none. */
    val usualFolder: String get() = if (agent == Agent.Claude) "~/.claude" else "~/.codex"

    /** The folder the name suggests, as `~/.claude-personal`, with a number when another account keeps that one. */
    fun suggestedFolder(besides: List<AgentAccount>): String {
        if (name.isBlank()) return ""
        val words = name.lowercase().split(Regex("[^a-z0-9]+")).filter { it.isNotEmpty() }
        val base = "~/.${agent.raw}-${if (words.isEmpty()) "account" else words.joinToString("-")}"
        val taken = besides.map { it.folder }.toSet()
        return generateSequence(1) { it + 1 }.map { if (it == 1) base else "$base-$it" }.first { it !in taken }
    }

    /** What signs the account in, run on its server. */
    val signInCommand: String
        get() {
            val prefix = if (folder.isEmpty()) "" else "$folderVariable=$folder "
            return prefix + if (agent == Agent.Claude) "claude auth login" else "codex login"
        }

    companion object {
        /** A new account of `agent`, until the server keeps it. */
        fun new(agent: Agent) = AgentAccount("", agent, "", "", false, emptyList(), null, null)
    }
}

data class Server(
    val id: String,
    val name: String,
    /** The name cut short, for the places a long name crowds. */
    val shortName: String,
    val platform: String,
    /** The server was removed from the account. Its threads are the copies the account's other servers keep, to read and to go on with on another server. */
    val removed: Boolean,
    val state: State,
    val error: String?,
    /** "relay" or "direct". */
    val path: String?,
    val rttMs: Int?,
    val home: String,
    /** The version of the server's program. */
    val version: String,
    val protocolVersion: Int,
    val models: List<ModelInfo>,
    /** The model picked to write titles, commit messages and pull requests there. */
    val textModel: String?,
    val doneOnMerge: Boolean,
    val removeMergedWorktrees: Boolean,
    val pullDefaultBranch: Boolean,
    val continueAfterLimits: Boolean,
    val continueAfterRestarts: Boolean,
    /** How the writer there is told to name branches, and what that is until it is changed. */
    val branchInstructions: String,
    val defaultBranchInstructions: String,
    /** The agents installed on the server, with their versions. */
    val agents: Map<Agent, String>,
    /** The agents' accounts, the default ones first. */
    val agentAccounts: List<AgentAccount>,
    /** The account a new thread is better off starting on, by account id and model id. */
    val roomierAccounts: Map<String, Map<String, String>>,
    /** Whether the server has ever told us about itself. */
    val known: Boolean,
    /** The update the server is putting in place, as it last said. */
    val update: ServerUpdate?,
    /** How its copies of the account's other servers' threads stand. */
    val sync: ServerSync? = null,
) {
    enum class State(val raw: String) {
        Connecting("connecting"), Connected("connected"), Disconnected("disconnected"), Refused("refused");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Connecting
        }
    }

    /** Whether threads there can move between accounts and agents. */
    val switchesAccounts: Boolean get() = protocolVersion >= 19

    /** Whether it signs its agents' accounts in for the client. */
    val signsInAccounts: Boolean get() = protocolVersion >= 31

    /** Whether the server keeps where the user moves its threads in the sidebar. */
    val movesThreads: Boolean get() = protocolVersion >= 21

    /** Whether the server asks its agents for their models when asked to. */
    val refreshesModels: Boolean get() = protocolVersion >= 23

    /** Whether the server can keep its projects' folders pulled on their default branch. */
    val pullsDefaultBranch: Boolean get() = protocolVersion >= 25

    fun accounts(of: Agent): List<AgentAccount> = agentAccounts.filter { it.agent == of }

    /** Whether the account is its agent's default: the first of its agent's accounts. */
    fun isDefault(account: AgentAccount): Boolean = accounts(account.agent).firstOrNull()?.id == account.id

    /** Each model once, however many accounts run it, for picking one by its id. */
    val distinctModels: List<ModelInfo>
        get() {
            val seen = HashSet<String>()
            return models.filter { seen.add(it.id) }
        }

    /** The account with that id, or the agent's default one. */
    fun account(id: String?, of: Agent): AgentAccount? {
        val accounts = accounts(of)
        return accounts.firstOrNull { it.id == id } ?: accounts.firstOrNull()
    }

    /** The account and model a new thread starts on, when another account of its agent runs the model and has used clearly less of its limits. */
    fun roomier(than: String?, model: String?): Pair<String, String>? {
        val found = Server.model(model, account = than, among = models) ?: return null
        val account = account(than, of = found.agent) ?: return null
        val roomier = roomierAccounts[account.id]?.get(found.id) ?: return null
        return roomier to found.id
    }

    /** The model's name, with the account's when its agent has more than one. */
    fun label(of: ModelInfo, account: String?): String {
        if (accounts(of.agent).size <= 1) return of.shortName
        val found = account(account, of.agent) ?: return of.shortName
        return "${of.shortName} · ${found.name}"
    }

    companion object {
        operator fun invoke(json: JSON): Server {
            val info = json.obj("info")
            val version = info?.string("version") ?: ""
            val installed = LinkedHashMap<Agent, String>()
            for (agent in info?.objects("agents") ?: emptyList()) {
                val kind = Agent.of(agent.string("agent")) ?: continue
                val agentVersion = agent.optionalString("version") ?: continue
                installed[kind] = agentVersion
            }
            val accounts = (info?.objects("agent_accounts") ?: emptyList()).map(::AgentAccount)
            val settings = info?.obj("pull_request_settings")
            val continues = info?.obj("continue_settings")
            val naming = info?.obj("branch_instructions")
            return Server(
                id = json.string("id"),
                name = json.string("name"),
                shortName = json.string("short_name"),
                platform = json.string("platform"),
                removed = json.bool("removed"),
                state = State.of(json.string("state")),
                error = json.optionalString("error"),
                path = json.optionalString("path"),
                rttMs = json.optionalInt("rtt_ms"),
                home = info?.string("home") ?: "",
                version = version,
                protocolVersion = info?.int("protocol") ?: 0,
                models = (info?.objects("models") ?: emptyList()).map { ModelInfo(it, json.obj("short_model_names")) },
                textModel = info?.optionalString("text_model"),
                doneOnMerge = settings?.bool("done_on_merge") ?: false,
                removeMergedWorktrees = settings?.bool("remove_merged_worktrees") ?: false,
                pullDefaultBranch = settings?.bool("pull_default_branch") ?: false,
                continueAfterLimits = continues?.bool("after_limits") ?: false,
                continueAfterRestarts = continues?.bool("after_restarts") ?: false,
                branchInstructions = naming?.string("text") ?: "",
                defaultBranchInstructions = naming?.string("default") ?: "",
                agents = installed,
                agentAccounts = accounts.ifEmpty { Agent.entries.map(::defaultAccount) },
                roomierAccounts = json.obj("roomier_accounts")?.let { byAccount -> byAccount.keys.associateWith(byAccount::stringMap) } ?: emptyMap(),
                known = info != null,
                update = info?.obj("update")?.let { ServerUpdate(it, version) },
                sync = json.obj("sync")?.let(::ServerSync),
            )
        }

        /** The account a server from before accounts has for each agent. */
        private fun defaultAccount(agent: Agent) = AgentAccount.new(agent).copy(id = agent.raw, name = "Default")

        /** The model with that id as the account lists it, or else the first one offered. */
        fun model(id: String?, account: String?, among: List<ModelInfo>): ModelInfo? {
            val named = among.filter { it.id == id }
            val listedByAccount = { model: ModelInfo -> model.account.isEmpty() || model.account == account }
            return named.firstOrNull(listedByAccount) ?: named.firstOrNull() ?: among.firstOrNull(listedByAccount) ?: among.firstOrNull()
        }
    }
}

/** A server that is installing a new version of itself. */
/** What a server keeps of the account's other servers' threads, as the core says it. */
data class ServerSync(
    val state: State,
    /** "Synced", "Syncing · 48%", "Couldn't sync from vibe". */
    val text: String,
    /** Why the last round couldn't copy to it. */
    val error: String?,
    /** When a round last left its copies whole. */
    val syncedAt: Double?,
) {
    constructor(json: JSON) : this(
        State.of(json.string("state")),
        json.string("text"),
        json.optionalString("error"),
        json.optionalDouble("synced_at"),
    )

    enum class State(val raw: String) {
        Synced("synced"), Copying("copying"), Waiting("waiting"), Failed("failed"), Off("off");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Off
        }
    }
}

data class ServerUpdate(
    /** The version it had when the update began. */
    val from: String,
    /** How much of the download has arrived, when the server knows how much there is. */
    val fraction: Double? = null,
    /** The new version is installed, and the server starts it once its agents have finished. */
    val waiting: Boolean = false,
    /** The new version is installed and the server is starting it. */
    val restarting: Boolean = false,
) {
    constructor(json: JSON, from: String) : this(
        from = from,
        fraction = json.optionalDouble("percent")?.let { it / 100 },
        waiting = json.string("state") == "waiting",
        restarting = json.string("state") == "restarting",
    )
}

/** When an updated server restarts while its agents work. */
enum class RestartWhen(val raw: String) {
    /** Once no agent works. */
    Idle("idle"),
    /** At once: the agents are stopped and their threads continue after the restart. */
    Now("now"),
}

/** A branch of a project's repository. */
data class Branch(
    val name: String,
    val current: Boolean,
    /** The one the remote starts new work from. */
    val isDefault: Boolean,
    /** Only on the remote so far. */
    val remote: Boolean,
) {
    val id: String get() = name

    constructor(json: JSON) : this(json.string("name"), json.bool("current"), json.bool("default"), json.bool("remote"))
}

data class GitStatus(
    /** `null` when no branch is checked out. */
    val branch: String?,
    /** The checked-out branch is the one the remote starts new work from. */
    val isDefault: Boolean,
    /** The branch new work starts from. `null` from a server that doesn't say yet. */
    val defaultBranch: String?,
    /** The checked-out branch has a commit a worktree can start from. */
    val hasCommit: Boolean,
    val remote: Boolean,
    val added: Int,
    val removed: Int,
    val pullRequest: PullRequest?,
) {
    constructor(json: JSON) : this(
        branch = json.optionalString("branch"),
        isDefault = json.bool("default"),
        defaultBranch = json.optionalString("default_branch"),
        hasCommit = !json.bool("no_commit"),
        remote = json.bool("remote"),
        added = json.int("added"),
        removed = json.int("removed"),
        pullRequest = json.obj("pull_request")?.let(::PullRequest),
    )
}

data class PullRequest(val number: Int, val title: String, val url: String, val state: State) {
    enum class State {
        Open, Draft, Merged,
        /** Closed without being merged. */
        Closed;

        val symbol: Symbol
            get() = when (this) {
                Open -> Symbol.GitPullRequest
                Draft -> Symbol.GitPullRequestDraft
                Merged -> Symbol.GitMerge
                Closed -> Symbol.GitPullRequestClosed
            }
    }

    constructor(json: JSON) : this(
        number = json.int("number"),
        title = json.string("title"),
        url = json.string("url"),
        state = when {
            json.bool("merged") -> State.Merged
            json.bool("closed") -> State.Closed
            json.bool("draft") -> State.Draft
            else -> State.Open
        },
    )
}

/** The git button of a project, as the core worked it out: what it does when clicked, the menu behind it and what is said under the menu. */
data class GitControl(val quick: GitQuick, val menu: List<GitMenuItem>, val warning: String?) {
    constructor(json: JSON) : this(
        quick = GitQuick(json.obj("quick") ?: EmptyJSON),
        menu = json.objects("menu").map(::GitMenuItem),
        warning = json.optionalString("warning"),
    )
}

/** The symbol of a git action, as the server spells it: "commit_push". */
object GitSymbol {
    fun symbol(action: String?): Symbol = when (action) {
        "pull" -> Symbol.CloudDownload
        "push", "commit_push", "commit_push_pr" -> Symbol.CloudUpload
        "create_pr" -> Symbol.GitPullRequestCreate
        else -> Symbol.GitCommitHorizontal
    }
}

/** What the button does: an action, or opening the pull request at `url`. With neither it is off, and `hint` says why. */
data class GitQuick(
    val label: String,
    /** Said before the label: "Merged". */
    val state: String?,
    val action: String?,
    val url: String?,
    val hint: String?,
    val confirm: GitConfirm?,
    /** The pull request the button opens, for its symbol and its title. */
    val pullRequest: PullRequest?,
) {
    constructor(json: JSON) : this(
        label = json.string("label"),
        state = json.optionalString("state"),
        action = json.optionalString("action"),
        url = json.optionalString("url"),
        hint = json.optionalString("hint"),
        confirm = json.obj("confirm")?.let(::GitConfirm),
        pullRequest = json.obj("pull_request")?.let(::PullRequest),
    )

    val title: String get() = state?.let { "$it $label" } ?: label
}

data class GitMenuItem(
    val label: String,
    val action: String,
    /** Why it can't run now. */
    val reason: String?,
    val confirm: GitConfirm?,
) {
    val id: String get() = action

    constructor(json: JSON) : this(
        json.string("label"), json.string("action"), json.optionalString("reason"), json.obj("confirm")?.let(::GitConfirm),
    )
}

/** Asked before an action that pushes from the default branch. */
data class GitConfirm(val title: String, val description: String, val proceed: String, val branchOff: String) {
    constructor(json: JSON) : this(json.string("title"), json.string("description"), json.string("proceed"), json.string("branch_off"))
}

enum class GitStage(val raw: String) {
    Message("message"),
    Commit("commit"),
    Push("push"),
    PullRequestText("pull_request_text"),
    PullRequest("pull_request"),
    Pull("pull"),
    Merge("merge"),
    AutoMerge("auto_merge"),
    UpdateBranch("update_branch"),
    Close("close"),
    Reopen("reopen"),
    Revert("revert"),
    /** A stage of a newer server. */
    Unknown("unknown");

    val label: String
        get() = when (this) {
            Message -> "Writing Commit"
            Commit -> "Committing"
            Push -> "Pushing"
            PullRequestText -> "Writing PR"
            PullRequest -> "Creating PR"
            Pull -> "Pulling"
            Merge -> "Merging PR"
            AutoMerge -> "Setting Auto-merge"
            UpdateBranch -> "Updating Branch"
            Close -> "Closing PR"
            Reopen -> "Reopening PR"
            Revert -> "Reverting PR"
            Unknown -> "Working"
        }

    companion object {
        fun of(raw: String): GitStage = entries.firstOrNull { it.raw == raw } ?: Unknown
    }
}

/** What a git action or a project add did, or why it couldn't, shown at the top of the thread until it is dismissed. A git notice belongs to its checkout; a project's shows anywhere. */
data class Notice(
    val checkoutID: String? = null,
    val title: String,
    val description: String? = null,
    val failed: Boolean = false,
    /** Still under way, like a clone. */
    val pending: Boolean = false,
    /** The pull request to open. */
    val url: String? = null,
    /** The action that follows, like a push after a commit. */
    val next: String? = null,
    /** The project to open a thread in. */
    val projectID: String? = null,
    /** Tells apart two notices that read alike, so that one going away by itself leaves the other. */
    val stamp: Long = System.nanoTime(),
) {
    val nextLabel: String?
        get() = when (next) {
            "push" -> "Push"
            "create_pr" -> "Create PR"
            else -> null
        }
}

/** An action that waits for the user to say where it should happen. */
data class PendingGit(
    val project: Project,
    val action: String,
    val confirm: GitConfirm,
    val message: String? = null,
    val paths: List<String> = emptyList(),
) {
    val id: String get() = "${project.id}:$action"
}

/** A file with changes that aren't committed. */
data class ChangedFile(val path: String, val change: String, val added: Int, val removed: Int) {
    val id: String get() = path

    constructor(json: JSON) : this(json.string("path"), json.string("change"), json.int("added"), json.int("removed"))
}

/** A git worktree of a project, made for the thread whose `cwd` it is. */
data class Worktree(val path: String, val branch: String?, val git: GitStatus?, val gitControl: GitControl?)

data class Project(
    val id: String,
    val serverID: String,
    val path: String,
    val name: String,
    val branch: String?,
    /** What its server last read from git there. */
    val git: GitStatus?,
    /** The git button for its repository. */
    val gitControl: GitControl?,
    /** The worktree of the thread the project is seen from, when it works in one. */
    val worktree: Worktree?,
    val worktrees: List<Worktree>,
    /** The shell script that runs in every new worktree. */
    val setup: String?,
    /** The icon as a file on this device, once the core has fetched it. */
    val iconPath: String?,
    /** Its server's "No project": each of its threads works in a folder of its own. */
    val noProject: Boolean,
    /** The same on every server that has the project, which a picker lists once. */
    val key: String,
    val createdAt: Double,
) {
    /** Names the folder git works in for it: its worktree, or the project's folder. */
    val checkoutID: String get() = "$id:${worktree?.path ?: path}"

    /** The pull request that was opened for the thread, or the one of its worktree's branch. The project's own folder is shared by its threads, so its branch says nothing of one. */
    fun pullRequest(of: ThreadInfo): PullRequest? = of.pullRequest ?: worktrees.firstOrNull { it.path == of.cwd }?.git?.pullRequest

    /** The project as the thread works in it: with the branch and the git state of its worktree, when it has one. */
    fun seen(from: ThreadInfo): Project {
        val worktree = worktrees.firstOrNull { it.path == from.cwd } ?: return this
        return copy(worktree = worktree, branch = worktree.branch, git = worktree.git, gitControl = worktree.gitControl)
    }

    companion object {
        const val NO_PROJECT_KEY = "no-project"

        operator fun invoke(json: JSON, serverID: String): Project {
            val controls = json.obj("worktree_controls")
            return Project(
                id = json.string("id"),
                serverID = serverID,
                path = json.string("path"),
                name = json.string("name"),
                branch = json.optionalString("branch"),
                git = json.obj("git")?.let(::GitStatus),
                gitControl = json.obj("git_control")?.let(::GitControl),
                worktree = null,
                worktrees = json.objects("worktrees").map { worktree ->
                    val path = worktree.string("path")
                    Worktree(path, worktree.optionalString("branch"), worktree.obj("git")?.let(::GitStatus), controls?.obj(path)?.let(::GitControl))
                },
                setup = json.optionalString("setup"),
                iconPath = json.optionalString("icon_path"),
                noProject = json.bool("no_project"),
                key = json.string("key"),
                createdAt = json.double("created_at"),
            )
        }

        /** How a new thread's headline reads with the project picked: "Let’s build in motile", or "Let’s build without a project". */
        fun headline(project: Project?): Pair<String, String> = when {
            project == null -> "Let’s build in" to "a project"
            project.noProject -> "Let’s build" to "without a project"
            else -> "Let’s build in" to project.name
        }
    }
}

enum class Access(val raw: String) {
    Supervised("supervised"),
    AcceptEdits("accept_edits"),
    Auto("auto"),
    Full("full");

    val label: String
        get() = when (this) {
            Supervised -> "Supervised"
            AcceptEdits -> "Auto-accept edits"
            Auto -> "Auto"
            Full -> "Full access"
        }

    val detail: String
        get() = when (this) {
            Supervised -> "Ask before commands and file changes."
            AcceptEdits -> "Auto-approve edits, ask before other actions."
            Auto -> "The agent approves routine actions itself."
            Full -> "Allow commands and edits without prompts."
        }

    val symbol: Symbol
        get() = when (this) {
            Supervised -> Symbol.Shield
            AcceptEdits -> Symbol.PencilLine
            Auto -> Symbol.Sparkles
            Full -> Symbol.LockOpen
        }

    companion object {
        fun of(raw: String?): Access? = entries.firstOrNull { it.raw == raw }
    }
}

data class ThreadInfo(
    val id: String,
    val serverID: String,
    val title: String,
    val projectID: String,
    val cwd: String,
    val agent: Agent,
    /** The id of the agent's account the thread works with. */
    val agentAccount: String,
    val model: String?,
    val effort: String?,
    val access: Access,
    val createdAt: Double,
    val updatedAt: Double,
    val doneAt: Double?,
    /** Where the sidebar lists it among the active threads, the highest first: when it was created or last came back from done, until the user moves it. */
    val position: Double,
    val running: Boolean,
    /** The turn is over, but the agent still watches something it left running. */
    val monitoring: Boolean,
    val needsApproval: Boolean,
    /** What the tool call that waits for the user asks of them. */
    val asking: Ask?,
    /** How many agents the thread's agent has started that still work. */
    val agents: Int,
    val turnEndedAt: Double?,
    /** The pull request that was opened for it or linked to it. */
    val pullRequest: PullRequest?,
    /** Its agent is told what happens on its pull request. */
    val watching: Boolean,
    /** What a commit, a push or the like that was started from it is at. */
    val gitStage: GitStage?,
    /** Why the agent stopped before it finished, until it works again. */
    val interruption: Interruption?,
    val unread: Boolean,
    /** The thread whose agent started this one. The sidebar lists only threads without one. */
    val parentID: String? = null,
    /** How the agents its agent started are doing, theirs too. */
    val children: ThreadChildren? = null,
) {
    val isDone: Boolean get() = doneAt != null

    /** The agent's process is still there, working or monitoring. */
    val busy: Boolean get() = running || monitoring

    /** It works, or an agent it started does. */
    val working: Boolean get() = running || (children?.running ?: 0) > 0

    /** Its agent ended its turn, and the agents it started work on. */
    val waitsOnAgents: Boolean get() = !running && (children?.running ?: 0) > 0

    /** Its agent's process is still there or the agents it started work, so it can't be done yet. */
    val unfinished: Boolean get() = busy || waitsOnAgents

    /** What it, or else an agent it started, waits for the user to answer. */
    val waitsFor: Ask? get() = asking ?: children?.waiting?.firstOrNull()?.ask

    val monitoringSince: Double get() = turnEndedAt ?: updatedAt

    /** What its list is in order of, the highest first: when it was marked done, or else its position. Active threads keep their place when something happens in them. */
    val listedAt: Double get() = if (isDone) doneAt ?: 0.0 else position

    companion object {
        operator fun invoke(json: JSON): ThreadInfo {
            val agent = Agent.of(json.string("agent")) ?: Agent.Claude
            val createdAt = json.double("created_at")
            // A server from before positions lists its threads by when they were created.
            val position = json.double("position")
            val needsApproval = json.bool("needs_approval")
            return ThreadInfo(
                id = json.string("id"),
                serverID = json.string("server_id"),
                title = json.string("title"),
                projectID = json.string("project_id"),
                cwd = json.string("cwd"),
                agent = agent,
                agentAccount = json.optionalString("agent_account")?.takeIf { it.isNotEmpty() } ?: agent.raw,
                model = json.optionalString("model"),
                effort = json.optionalString("effort"),
                access = Access.of(json.string("access")) ?: Access.Full,
                createdAt = createdAt,
                updatedAt = json.double("updated_at"),
                doneAt = json.optionalDouble("done_at"),
                position = if (position > 0) position else createdAt,
                running = json.bool("running"),
                monitoring = json.bool("monitoring"),
                needsApproval = needsApproval,
                // A server from before `asking` only says that something waits.
                asking = if (needsApproval) Ask.of(json.string("asking")) ?: Ask.Approval else null,
                agents = json.int("agents"),
                turnEndedAt = json.optionalDouble("turn_ended_at"),
                pullRequest = json.obj("pull_request")?.let(::PullRequest),
                watching = json.bool("watching"),
                gitStage = json.optionalString("git_stage")?.let(GitStage::of),
                interruption = json.obj("interruption")?.let(Interruption::of),
                unread = json.bool("unread"),
                parentID = json.obj("parent")?.optionalString("thread_id"),
                children = json.obj("children")?.let(::ThreadChildren),
            )
        }

        /** Whether `one` is listed before `other`: by `listedAt`, then by id, the highest first. */
        fun listed(one: ThreadInfo, other: ThreadInfo): Boolean =
            one.listedAt > other.listedAt || (one.listedAt == other.listedAt && one.id > other.id)

        /** The order of `listed`, for sorting. */
        val order = Comparator<ThreadInfo> { one, other ->
            when {
                listed(one, other) -> -1
                listed(other, one) -> 1
                else -> 0
            }
        }
    }
}

/** Where the thread is, or would go, in a list in the order of `ThreadInfo.listed`. */
fun List<ThreadInfo>.place(of: ThreadInfo): Int {
    var low = 0
    var high = size
    while (low < high) {
        val middle = (low + high) / 2
        if (ThreadInfo.listed(this[middle], of)) low = middle + 1 else high = middle
    }
    return low
}

fun List<ThreadInfo>.indexOfListed(thread: ThreadInfo): Int? {
    val place = place(thread)
    return if (place < size && this[place].id == thread.id) place else null
}

/** What a tool call that waits for the user asks of them. */
enum class Ask(val raw: String) {
    Approval("approval"), Question("question"), Plan("plan");

    val word: String
        get() = when (this) {
            Approval -> "Approval"
            Question -> "Question"
            Plan -> "Plan"
        }

    /** What a thread is said to do while it waits. */
    val phrase: String
        get() = when (this) {
            Approval -> "needs approval"
            Question -> "has a question"
            Plan -> "has a plan"
        }

    val symbol: Symbol
        get() = when (this) {
            Approval -> Symbol.Shield
            Question -> Symbol.MessageCircleQuestionMark
            Plan -> Symbol.ClipboardList
        }

    /** What the transcript says while the agent waits for it. */
    val waiting: String
        get() = when (this) {
            Approval -> "Waiting for your approval"
            Question -> "Waiting for your answer"
            Plan -> "Waiting for you to review the plan"
        }

    companion object {
        fun of(raw: String): Ask? = entries.firstOrNull { it.raw == raw }
    }
}

/** Why a thread's agent stopped before it finished. */
sealed interface Interruption {
    /** The agent reached its usage limit. `resetsAt` is when it resets, when the agent said; with `continues` the thread goes on by itself then. */
    data class Limit(val resetsAt: Double?, val continues: Boolean) : Interruption

    /** The server restarted while the agent worked. */
    data object Restart : Interruption

    /** What a thread's status says of it. */
    val word: String
        get() = when (this) {
            is Limit -> "limited"
            Restart -> "interrupted"
        }

    val title: String
        get() = when (this) {
            is Limit -> "Usage limit reached"
            Restart -> "Interrupted"
        }

    val symbol: Symbol
        get() = when (this) {
            is Limit -> Symbol.Clock
            Restart -> Symbol.RefreshCw
        }

    /** When the thread goes on, or what it waits for to be continued. */
    fun detail(now: Double = Time.now()): String = when (this) {
        Restart -> "Your server restarted before the agent finished"
        is Limit -> when {
            resetsAt == null -> "Reset time unknown"
            resetsAt > now -> if (continues) "Continues at ${Time.stamp(resetsAt)}" else "Resets at ${Time.stamp(resetsAt)}"
            continues -> "Continues in a moment"
            else -> "The limit has reset"
        }
    }

    companion object {
        fun of(json: JSON): Interruption? = when (json.string("kind")) {
            "limit" -> Limit(json.optionalDouble("resets_at"), json.bool("continues"))
            "restart" -> Restart
            else -> null
        }
    }
}

data class Activity(
    val running: Boolean = false,
    val monitoring: Boolean = false,
    val thinking: Boolean = false,
    /** The agent is making its conversation shorter to go on with it. */
    val compacting: Boolean = false,
    /** How many agents it has started that still work. */
    val agents: Int = 0,
    val startedAt: Double? = null,
    /** The tool calls the running turn waits with until they are allowed or refused. */
    val approvals: List<Approval> = emptyList(),
    /** The messages that wait for the agent to take them. */
    val queued: List<QueuedMessage> = emptyList(),
) {
    val busy: Boolean get() = running || monitoring

    companion object {
        /** What the thread list says the agent does, until the thread's server says more. */
        fun of(thread: ThreadInfo) = Activity(running = thread.running, monitoring = thread.monitoring)

        fun of(json: JSON, waiting: List<JSON>) = Activity(
            running = json.bool("running"),
            monitoring = json.bool("monitoring"),
            thinking = json.bool("thinking"),
            compacting = json.bool("compacting"),
            agents = json.int("agents"),
            startedAt = json.optionalDouble("started_at"),
            approvals = waiting.map(::Approval),
            // What the agent is told of the agents it started isn't the user's to send or take back.
            queued = json.objects("queued").filter { it.obj("notice") == null }.map(::QueuedMessage),
        )

        /** What is shown between sending a first message and the server saying the turn runs. */
        val starting: Activity get() = Activity(running = true, startedAt = Time.now())
    }
}

/** The agents a thread's agent started, and theirs, as the thread shows them. */
data class ThreadChildren(
    val running: Int,
    /** The agents of the first few that work, whose avatars stand side by side. */
    val working: List<Agent>,
    /** Those that wait for the user, the one that has waited longest first. */
    val waiting: List<Waiting>,
    /** "Run greet.py needs your approval", "2 agents need you". */
    val waitingText: String?,
    /** "1 agent", "3 agents", which the agents strip says "working" after. */
    val runningCount: String?,
    /** "1 agent working", "3 agents working", while they work. */
    val runningText: String?,
    /** "Stop 1 agent?", "Stop 3 agents?", while they work. */
    val stopQuestion: String?,
) {
    data class Waiting(val threadID: String, val title: String, val ask: Ask)

    constructor(json: JSON) : this(
        running = json.int("running"),
        working = json.strings("working").mapNotNull { Agent.of(it) },
        waiting = json.objects("waiting").map {
            Waiting(it.string("thread_id"), it.string("title"), Ask.of(it.string("ask")) ?: Ask.Approval)
        },
        waitingText = json.optionalString("waiting_text"),
        runningCount = json.optionalString("running_count"),
        runningText = json.optionalString("running_text"),
        stopQuestion = json.optionalString("stop_question"),
    )
}

/** A stop that asks first, since it ends the agents the thread started. */
data class StopQuestion(val threadID: String, val question: String)

/** A program asks to run `command` in its thread's folder. */
data class UiConfirm(val threadID: String, val rowID: String, val callID: Long, val command: String)

/** How an agent is doing, which the dot on its avatar shows. */
enum class AgentState(val raw: String) {
    /** `Limited`: it reached its account's usage limit, and waits for the limit to reset. */
    Working("working"), Waiting("waiting"), Limited("limited"), Done("done"), Failed("failed"), Stopped("stopped");

    /** Its word is in its colour: one that is done or works says so quietly. */
    val flagged: Boolean get() = this == Waiting || this == Limited || this == Failed || this == Stopped

    companion object {
        fun of(raw: String?) = entries.firstOrNull { it.raw == raw } ?: Done
    }
}

/** An agent the thread's agent started. What it did is a transcript of its own, or a thread of its own when it was started through your server. */
data class AgentInfo(
    /** The tool call that started it. */
    val id: String,
    /** The agent that started it, when the thread's own didn't. */
    val parent: String?,
    val title: String,
    val kind: String?,
    val status: ToolStatus,
    val state: AgentState,
    /** Whether it is listed under the previous agents: it and every agent of its family ended. */
    val previous: Boolean,
    /** "Working", "Needs approval", "Failed". */
    val label: String,
    /** What it is doing now, or what it reported once it has ended. */
    val detail: String,
    val startedAt: Double,
    val durationMs: Int?,
    val tokens: Int?,
    val toolUses: Int?,
    /** The thread it works in, when it was started through your server. */
    val threadID: String? = null,
    /** The agent it runs as, whose logo it shows, when that is known. */
    val agent: Agent? = null,
    /** The model's short name, "Opus 5.5". */
    val model: String? = null,
    /** The account's name, when its agent has more than one. */
    val account: String? = null,
) {
    constructor(json: JSON) : this(
        id = json.string("id"),
        parent = json.optionalString("parent"),
        title = json.string("title"),
        kind = json.optionalString("kind"),
        status = ToolStatus.of(json.string("status")),
        state = AgentState.of(json.string("state")),
        previous = json.bool("previous"),
        label = json.string("label"),
        detail = json.string("detail"),
        startedAt = json.double("started_at"),
        durationMs = json.optionalInt("duration_ms"),
        tokens = json.optionalInt("tokens"),
        toolUses = json.optionalInt("tool_uses"),
        threadID = json.optionalString("thread_id"),
        agent = Agent.of(json.optionalString("agent")),
        model = json.optionalString("model"),
        account = json.optionalString("account"),
    )

    val working: Boolean get() = status == ToolStatus.Running

    /** "3 tools · 21k tokens", as far as either is known. */
    val usage: String?
        get() {
            val parts = listOfNotNull(toolUses?.let { "$it ${if (it == 1) "tool" else "tools"}" }, tokens?.let(::tokens))
            return if (parts.isEmpty()) null else parts.joinToString(" · ")
        }

    companion object {
        /** "820 tokens", "21.4K tokens", "4.57M tokens". */
        fun tokens(count: Int): String = "${count.toLong().abbreviated} tokens"
    }
}

/** How a tool call went. */
enum class ToolStatus(val raw: String) {
    Running("running"), Succeeded("succeeded"), Failed("failed");

    companion object {
        fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Succeeded
    }
}

/** A message sent while the agent was working. The transcript shows it as its last rows until the agent takes it. */
data class QueuedMessage(
    val id: String,
    val text: String,
    /** The files attached to it, as paths on the server. */
    val attachments: List<String>,
    /** The images and videos among them, by their path. */
    val media: Map<String, AttachedFile>,
) {
    constructor(json: JSON) : this(
        id = json.string("id"),
        text = json.string("text"),
        attachments = json.strings("attachments"),
        media = json.objects("media").reversed().associate { media ->
            media.string("src") to AttachedFile("", media.string("id"), media.bool("video"), media.optionalString("poster"))
        },
    )
}

/** A file attached to a message, as the transcript shows it. An image or a video names the file the core has it under, and a video the image that stands for it until it plays. */
data class AttachedFile(val name: String, val media: String?, val video: Boolean, val poster: String?) {
    constructor(json: JSON) : this(json.string("name"), json.optionalString("media"), json.bool("video"), json.optionalString("poster"))

    val json: Map<String, Any?> get() = mapOf("name" to name, "media" to media, "video" to video, "poster" to poster)

    /** The image a tile of it shows. */
    val picture: String? get() = if (video) poster else media
}

/** A file in the composer: on its way to the server, or there and ready to be sent. */
data class Attachment(
    val id: String,
    /** Where it is on this device. A file that came back from a queued message is only on its server. */
    val file: File?,
    val name: String,
    val bytes: Long?,
    val video: Boolean,
    /** Shown as a tile with its picture, not by its name. */
    val pictured: Boolean,
    val serverID: String,
    val state: State,
    /** Where it is on the server, once it is there. */
    val path: String? = null,
    val media: String? = null,
    val poster: String? = null,
) {
    sealed interface State {
        data class Uploading(val fraction: Double) : State
        data object Ready : State
        data class Failed(val message: String) : State
    }

    val attached: AttachedFile get() = AttachedFile(name, media, media != null && video, poster)

    val viewed: ViewedMedia?
        get() {
            if (file != null) return ViewedMedia(name, video, ViewedMedia.Source.LocalFile(file))
            return media?.let { ViewedMedia(name, video, ViewedMedia.Source.Media(it)) }
        }

    companion object {
        fun of(file: File, serverID: String): Attachment {
            val type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: ""
            val video = type.startsWith("video/")
            return Attachment(
                id = UUID.randomUUID().toString(),
                file = file,
                name = file.name,
                bytes = file.length().takeIf { file.exists() },
                video = video,
                pictured = video || type.startsWith("image/"),
                serverID = serverID,
                state = State.Uploading(0.0),
            )
        }

        fun of(path: String, shown: AttachedFile?, serverID: String) = Attachment(
            id = UUID.randomUUID().toString(),
            file = null,
            name = path.substringAfterLast('/'),
            bytes = null,
            video = shown?.video == true,
            pictured = shown != null,
            serverID = serverID,
            state = State.Ready,
            path = path,
            media = shown?.media,
            poster = shown?.poster,
        )
    }
}

/** An image or a video the viewer shows: a file on this device, or one the core has or fetches. */
data class ViewedMedia(val name: String, val video: Boolean, val source: Source) {
    sealed interface Source {
        data class LocalFile(val file: File) : Source
        data class Media(val id: String) : Source
    }
}

/** What the viewer has open: the images and videos of one message or of the composer, and which of them is shown. */
data class Viewing(val items: List<ViewedMedia>, val index: Int) {
    val item: ViewedMedia get() = items[index]
}

/** A tool call the turn waits with until the user has answered it. */
data class Approval(
    val id: String,
    /** What is asked for: the tool, or what to do with a plan. */
    val title: String,
    /** What the tool acts on: the whole command, the file. */
    val target: String,
    /** The target's colours, as the core's `[start, length, palette index]` spans. */
    val targetSpans: IntArray,
    val symbol: Symbol,
    val ask: Ask,
    /** What the buttons that allow and refuse it say. */
    val allowLabel: String,
    val refuseLabel: String,
    /** The questions the agent asks with it; allowing it takes an answer to each. */
    val questions: List<Question>,
) {
    constructor(json: JSON) : this(
        id = json.string("id"),
        title = json.string("title"),
        target = json.string("target"),
        targetSpans = json.ints("spans"),
        symbol = toolSymbol(json.string("icon")),
        ask = Ask.of(json.string("ask")) ?: Ask.Approval,
        allowLabel = json.string("allow"),
        refuseLabel = json.string("refuse"),
        questions = json.objects("questions").map(::Question),
    )

    override fun equals(other: Any?): Boolean =
        other is Approval && id == other.id && title == other.title && target == other.target && targetSpans.contentEquals(other.targetSpans) &&
            ask == other.ask && allowLabel == other.allowLabel && refuseLabel == other.refuseLabel && questions == other.questions

    override fun hashCode(): Int = id.hashCode()
}

/** The symbol of a tool, by the icon the core names. */
fun toolSymbol(icon: String): Symbol = when (icon) {
    "terminal" -> Symbol.Terminal
    "file" -> Symbol.FileText
    "edit" -> Symbol.Pencil
    "search" -> Symbol.Search
    "web" -> Symbol.Globe
    "agent" -> Symbol.Bot
    "watch" -> Symbol.Eye
    "question" -> Symbol.MessageCircleQuestionMark
    "todo" -> Symbol.ListChecks
    else -> Symbol.Wrench
}

data class Question(
    /** A word or two that says what it is about. */
    val header: String,
    val text: String,
    val options: List<Choice>,
    /** More than one option can be chosen. */
    val multiple: Boolean,
) {
    data class Choice(val label: String, val detail: String) {
        val id: String get() = label
    }

    val id: String get() = text

    constructor(json: JSON) : this(
        header = json.string("header"),
        text = json.string("text"),
        options = json.objects("options").map { Choice(it.string("label"), it.string("detail")) },
        multiple = json.bool("multiple"),
    )
}

data class EnrollToken(
    val command: String,
    val expiresAt: Double,
    /** The command's colours, as the core's `[start, length, palette index]` spans. */
    val spans: IntArray,
) {
    constructor(json: JSON) : this(json.string("command"), json.double("expires_at"), json.ints("spans"))

    fun isExpired(at: Double = Time.now()): Boolean = expiresAt <= at

    /** How long the command still works, as "14:59". */
    fun timeLeft(at: Double = Time.now()): String {
        val seconds = max(0, kotlin.math.ceil(expiresAt - at).toInt())
        return "%d:%02d".format(seconds / 60, seconds % 60)
    }

    override fun equals(other: Any?): Boolean = other is EnrollToken && command == other.command && expiresAt == other.expiresAt

    override fun hashCode(): Int = command.hashCode()
}

enum class GitHubState(val raw: String) {
    Ready("ready"), SignedOut("signed_out"), Missing("missing");

    companion object {
        fun of(raw: String?): GitHubState? = entries.firstOrNull { it.raw == raw }
    }
}

data class Repo(
    /** `owner/name`. */
    val name: String,
    val description: String?,
    val isPrivate: Boolean,
) {
    private val lowercasedName = name.lowercase()
    private val lowercasedRepoName = lowercasedName.substringAfter('/', lowercasedName)
    private val lowercasedDescription = description?.lowercase() ?: ""

    constructor(json: JSON) : this(json.string("name"), json.optionalString("description"), json.bool("private"))

    /** How well it answers a lowercased search, the best at 0: the name after the owner starting with it, then the whole name, then the name holding it, then the description. */
    fun rank(search: String): Int? = when {
        lowercasedRepoName.startsWith(search) -> 0
        lowercasedName.startsWith(search) -> 1
        lowercasedName.contains(search) -> 2
        lowercasedDescription.contains(search) -> 3
        else -> null
    }
}

/** The folders of a server under the path typed in the panel. */
data class FolderListing(
    val path: String,
    val typed: String,
    val parent: String?,
    val folders: List<Folder>,
) {
    data class Folder(
        val name: String,
        val path: String,
        /** What to type to look inside it. */
        val typed: String,
    )

    constructor(json: JSON) : this(
        path = json.string("path"),
        typed = json.string("typed"),
        parent = json.optionalString("parent"),
        folders = json.objects("folders").map { Folder(it.string("name"), it.string("path"), it.string("typed")) },
    )
}

/** An image in a project's folder that can be its icon. */
data class ProjectImage(
    val name: String,
    /** The folder it is in, inside the project's. Empty at its top. */
    val folder: String,
    val path: String,
) {
    constructor(json: JSON) : this(json.string("name"), json.string("folder"), json.string("path"))
}

/** "820", "1.5K", "45.7M", "123B", "1.2T": at most three significant digits. */
val Long.abbreviated: String
    get() {
        val units = listOf("", "K", "M", "B", "T")
        var value = abs(toDouble())
        var unit = 0
        while (value >= 999.5 && unit < units.size - 1) {
            value /= 1000
            unit += 1
        }
        val decimals = if (unit == 0 || value >= 100) 0 else if (value >= 10) 1 else 2
        var digits = "%.${decimals}f".format(java.util.Locale.ROOT, value)
        if (decimals > 0) digits = digits.trimEnd('0').trimEnd('.')
        return (if (this < 0) "-" else "") + digits + units[unit]
    }

val Int.abbreviated: String get() = toLong().abbreviated

object Time {
    fun now(): Double = System.currentTimeMillis() / 1000.0

    /** "now", "5m", "3h", "2d": how long ago, as short as a sidebar needs. */
    fun ago(timestamp: Double, now: Double = now()): String {
        val seconds = max(0.0, now - timestamp)
        return when {
            seconds < 60 -> "now"
            seconds < 3600 -> "${(seconds / 60).toInt()}m"
            seconds < 86400 -> "${(seconds / 3600).toInt()}h"
            else -> "${(seconds / 86400).toInt()}d"
        }
    }

    /** The time of day, with the date before it when that isn't today. */
    fun stamp(timestamp: Double): String {
        val date = Instant.ofEpochMilli((timestamp * 1000).toLong()).atZone(ZoneId.systemDefault())
        val time = date.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
        if (date.toLocalDate() == LocalDate.now()) return time
        return "${date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))}, $time"
    }

    /** "850ms", "12s", "3m 05s", "1h 02m". */
    fun duration(milliseconds: Int): String {
        if (milliseconds < 1000) return "${milliseconds}ms"
        val seconds = milliseconds / 1000
        if (seconds < 60) return "${seconds}s"
        return minutes(seconds)
    }

    /** "3m 05s", "1h 02m": the second unit always takes two digits, so a timer doesn't jump. */
    private fun minutes(seconds: Int): String =
        if (seconds < 3600) "${seconds / 60}m ${(seconds % 60).toString().padStart(2, '0')}s"
        else "${seconds / 3600}h ${(seconds % 3600 / 60).toString().padStart(2, '0')}m"

    /** A running timer: "5s", "12m 03s", "1h 03m". */
    fun elapsed(since: Double, now: Double = now()): String {
        val seconds = max(0, (now - since).toInt())
        if (seconds < 60) return "${seconds}s"
        return minutes(seconds)
    }
}

/** What the agents spent on the account's servers in a stretch of time, as the core added it up. */
data class UsageReport(
    val costUSD: Double,
    val tokens: Long,
    val unpricedTokens: Long,
    val cacheSavingsUSD: Double,
    /** The part that went into writing titles, branch names, commit messages and pull requests. */
    val writingCostUSD: Double,
    val writingTokens: Long,
    /** When each bucket starts, in seconds since 1970. */
    val starts: List<Double>,
    val agents: List<Series>,
    val kinds: List<Line>,
    val models: List<Line>,
    val projects: List<Line>,
    val servers: List<Line>,
    /** The agents' accounts, only when an agent has more than one on a server. */
    val accounts: List<Line>,
) {
    data class Series(val agent: Agent, val costUSD: Double, val tokens: Long, val costPoints: List<Double>, val tokenPoints: List<Double>) {
        val id: Agent get() = agent
    }

    /** A model, a project, a server, an account or a kind of token, and its part of the whole. */
    data class Line(
        val name: String,
        val agent: Agent?,
        /** The project's server, when more than one server spent. */
        val server: String?,
        val tokens: Long,
        val costUSD: Double?,
        val share: Double,
    ) {
        val id: String get() = "${agent?.raw ?: ""}/${server ?: ""}/$name"
    }

    companion object {
        operator fun invoke(json: JSON): UsageReport {
            val cost = json.double("cost_usd")
            val tokens = json.long("tokens")
            fun line(line: JSON, share: Double?) = Line(
                name = line.string("name"),
                agent = Agent.of(line.optionalString("agent")),
                server = line.optionalString("server"),
                tokens = line.long("tokens"),
                costUSD = line.optionalDouble("cost_usd"),
                share = share ?: line.double("share"),
            )
            return UsageReport(
                costUSD = cost,
                tokens = tokens,
                unpricedTokens = json.long("unpriced_tokens"),
                cacheSavingsUSD = json.double("cache_savings_usd"),
                writingCostUSD = json.double("writing_cost_usd"),
                writingTokens = json.long("writing_tokens"),
                starts = json.numbers("starts"),
                agents = json.objects("agents").map { series ->
                    Series(
                        Agent.of(series.string("agent")) ?: Agent.Claude, series.double("cost_usd"), series.long("tokens"),
                        series.numbers("cost_points"), series.numbers("token_points"),
                    )
                },
                kinds = json.objects("kinds").map { kind ->
                    val share = if (cost > 0) kind.double("cost_usd") / cost else kind.long("tokens").toDouble() / max(tokens, 1L)
                    line(kind, share)
                },
                models = json.objects("models").map { line(it, null) },
                projects = json.objects("projects").map { line(it, null) },
                servers = json.objects("servers").map { line(it, null) },
                accounts = json.objects("accounts").map { line(it, null) },
            )
        }
    }
}

/** How much of their plans the agents' logins have used: a section for each login, a row for each of its windows, and what kept a server from saying. */
data class LimitsReport(
    val sections: List<Section>,
    val notes: List<String>,
    /** Some of it is old or missing, from what was last read. */
    val stale: Boolean,
) {
    data class Section(
        val agent: Agent,
        /** The account's name on the server. */
        val name: String?,
        /** Who is signed in. */
        val account: String?,
        val plan: String?,
        val servers: List<String>,
        val windows: List<Window>,
        /** Why there are no windows. */
        val note: String?,
    ) {
        val id: String get() = "${agent.raw}/${name ?: ""}/${account ?: servers.joinToString(",")}/${note ?: ""}"
    }

    data class Window(
        val label: String,
        /** Between 0 and 100. */
        val usedPercent: Double,
        val used: String,
        val resetsIn: String?,
        val pace: Pace?,
        val tone: Tone,
        val resetCredits: Int,
    ) {
        enum class Pace(val raw: String) { Ahead("ahead"), On("on"), Under("under") }
        enum class Tone(val raw: String) { Success("success"), Pending("pending"), Warning("warning") }

        val id: String get() = label
    }

    companion object {
        operator fun invoke(json: JSON) = LimitsReport(
            sections = json.objects("sections").map { section ->
                Section(
                    agent = Agent.of(section.string("agent")) ?: Agent.Claude,
                    name = section.optionalString("name"),
                    account = section.optionalString("account"),
                    plan = section.optionalString("plan"),
                    servers = section.strings("servers"),
                    windows = section.objects("windows").map { window ->
                        Window(
                            label = window.string("label"),
                            usedPercent = window.double("used_percent"),
                            used = window.string("used"),
                            resetsIn = window.optionalString("resets_in"),
                            pace = Window.Pace.entries.firstOrNull { it.raw == window.optionalString("pace") },
                            tone = Window.Tone.entries.firstOrNull { it.raw == window.string("tone") } ?: Window.Tone.Success,
                            resetCredits = window.int("reset_credits"),
                        )
                    },
                    note = section.optionalString("note"),
                )
            },
            notes = json.strings("notes"),
            stale = json.bool("stale"),
        )
    }
}

/** What the servers found in the threads' messages for the sidebar's search, by thread. */
data class MessageSearch(
    val query: String = "",
    /** Some server may still answer. */
    val searching: Boolean = false,
    val matches: Map<String, MessageMatch> = emptyMap(),
) {
    companion object {
        fun of(json: JSON) = MessageSearch(
            query = json.string("query"),
            searching = json.bool("searching"),
            matches = json.objects("matches").reversed().associate { it.string("thread_id") to MessageMatch(it) },
        )
    }
}

/** The message a search found a thread by, in parts that set off what was searched for. */
data class MessageMatch(
    /** The user wrote it, or else the agent. */
    val byUser: Boolean,
    val parts: List<Part>,
) {
    data class Part(val text: String, val matched: Boolean)

    constructor(json: JSON) : this(
        byUser = json.string("speaker") == "user",
        parts = json.objects("parts").map { Part(it.string("text"), it.bool("matched")) },
    )
}
