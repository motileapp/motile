package app.motile.core

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.BuildConfig
import app.motile.panel.PanelTarget
import app.motile.panel.RightPanel
import app.motile.platform.Platform
import app.motile.transcript.RowModel
import java.io.File
import java.util.TimeZone
import java.util.UUID
import kotlin.concurrent.thread

sealed interface Selection {
    val id: String

    data class Draft(override val id: String) : Selection
    data class Thread(override val id: String) : Selection
}

/** A thread that hasn't been sent yet: where it will start and with what. Its text is in `drafts`. */
data class ThreadDraft(
    val id: String = UUID.randomUUID().toString(),
    val projectID: String? = null,
    val model: String? = null,
    /** The id of the account of the model's agent the thread starts with. */
    val agentAccount: String? = null,
    val effort: String? = null,
    val access: Access = Access.Full,
    /** The thread starts in a new worktree, on a branch that starts from `base`. */
    val worktree: Boolean? = null,
    val base: String? = null,
) {
    val json: Map<String, Any?>
        get() = mapOf(
            "id" to id, "projectID" to projectID, "model" to model, "agentAccount" to agentAccount, "effort" to effort,
            "access" to access.raw, "worktree" to worktree, "base" to base,
        )

    companion object {
        fun of(json: JSON) = ThreadDraft(
            id = json.string("id"),
            projectID = json.optionalString("projectID"),
            model = json.optionalString("model"),
            agentAccount = json.optionalString("agentAccount"),
            effort = json.optionalString("effort"),
            access = Access.of(json.optionalString("access")) ?: Access.Full,
            worktree = json.optionalBool("worktree"),
            base = json.optionalString("base"),
        )
    }
}

/** A draft as the sidebar lists it. */
data class ListedDraft(
    val draft: ThreadDraft,
    val preview: String,
    /** Its first message is on its way to the server. */
    val sending: Boolean = false,
) {
    val id: String get() = draft.id
}

/** The first message of a draft, on its way to the server. */
data class SendingDraft(
    val text: String,
    val attachments: List<AttachedFile>,
    val activity: Activity,
    /** The client has gone elsewhere since, so the sidebar lists the draft. */
    val left: Boolean = false,
) {
    val preview: String get() = AppStore.preview(text, attachments.size) ?: ""
}

/** Where the command panel opens. */
sealed interface PanelPage {
    /** Everything that can be done from here. */
    data object Commands : PanelPage
    /** The projects, to start a thread in one. */
    data object Projects : PanelPage
    /** The projects, to choose the one the open draft works in. */
    data object DraftProject : PanelPage
    /** The threads, to open one. */
    data object Threads : PanelPage
    /** The servers, to add a project on one. */
    data object Servers : PanelPage
    /** The ways to add a project on the server. */
    data class Sources(val serverID: String) : PanelPage
    data class NewProject(val serverID: String) : PanelPage
    /** The repositories of the server's GitHub login, to clone one. */
    data class GitHub(val serverID: String) : PanelPage
    /** What GitHub needs on the server before its repositories can be listed. */
    data class GitHubSetup(val serverID: String) : PanelPage
    /** The server's folders, to add one. */
    data class Folder(val serverID: String) : PanelPage
    /** The folders and images of the project's server, to make one its icon. */
    data class Icon(val projectID: String) : PanelPage
}

/** A project that a server is making or cloning. */
data class AddingProject(val serverID: String, val name: String)

/** What the images and videos fetched from the servers take on this device, and what they may take. */
data class MediaStorage(val used: Long, val limit: Long)

data class UndoNotice(val threadIDs: List<String>, val text: String)

/**
 * Everything the views show. It mirrors the core: commands go to it, and its events change the
 * state here. All of it is used on the main thread.
 */
class AppStore(private val context: Context) {
    // Account
    var account by mutableStateOf(Account())
        private set
    /** Whether the core has sent what it remembers from last time. Until then nothing is drawn. */
    var ready by mutableStateOf(false)
        private set
    var signingIn by mutableStateOf(false)
        private set
    var signInError by mutableStateOf<String?>(null)
    var enrollToken by mutableStateOf<EnrollToken?>(null)
        private set
    var enrollTokenPending by mutableStateOf(false)
        private set
    var showsAddServer by mutableStateOf(false)
    /** The section of the settings open over the client, while they are. */
    var settings by mutableStateOf<SettingsSection?>(null)
    /** What the agents spent and what is left of their plans is open over the client. */
    var showsUsage by mutableStateOf(false)
    /** What the usage shows, kept while it is closed so that it opens with what was last read. */
    var usage by mutableStateOf(UsageModel())
        private set
    /** The group of settings a search picked, until its page has scrolled to it. */
    var settingsTarget by mutableStateOf<String?>(null)
    /** What is typed in the settings' search. */
    var settingsQuery by mutableStateOf("")
    /** The thread's settings are open over the client, where they aren't around the composer. */
    var showsThreadSettings by mutableStateOf(false)
    /** Files are being dragged over the thread. */
    var dropTargeted by mutableStateOf(false)
    /** Files are being dragged over the composer's text, which takes drops itself. */
    var composerDropTargeted by mutableStateOf(false)
    /** The branch picker is open. Its branches are `null` until your server has listed them. */
    var showsBranches by mutableStateOf(false)
    var listedBranches by mutableStateOf<Result<List<Branch>>?>(null)
        private set
    private var branchesProjectID: String? = null
    /** Each project's branches as last listed, which the picker opens on while it lists them again. */
    private val knownBranches = HashMap<String, List<Branch>>()
    /** The project whose changes the commit sheet is open on, with the files it was opened with. */
    var committingProject by mutableStateOf<Project?>(null)
    var gitFiles by mutableStateOf(emptyList<ChangedFile>())
        private set
    /** An action that pushes from the default branch, until the user says where it should happen. */
    var pendingGit by mutableStateOf<PendingGit?>(null)
    /** The stage a git action is at, by the checkout it runs in. */
    var gitStages by mutableStateOf(mapOf<String, GitStage>())
        private set
    /** The projects whose folders their servers are making git repositories. */
    var initializingGit by mutableStateOf(setOf<String>())
        private set
    /** The `draftWorktreeKey`s whose base is being pulled. */
    var pullingBases by mutableStateOf(setOf<String>())
        private set
    var notice by mutableStateOf<Notice?>(null)
        private set
    /** What a new worktree starts at, like `origin/main`, by `draftWorktreeKey`. */
    var worktreeStarts by mutableStateOf(mapOf<String, String>())
        private set
    var panel by mutableStateOf<PanelPage?>(null)
        private set
    /** What a server refused while the panel was adding a project. */
    var panelNotice by mutableStateOf<String?>(null)
    /** Whether GitHub can be used on each server, as last heard. */
    var github by mutableStateOf(mapOf<String, GitHubState>())
        private set
    /** The GitHub repositories each server last listed, and why one couldn't. */
    var repos by mutableStateOf(mapOf<String, List<Repo>>())
        private set
    var repoErrors by mutableStateOf(mapOf<String, String>())
        private set
    /** The servers that are listing their repositories now. */
    var listingRepos by mutableStateOf(setOf<String>())
        private set
    private val reposListed = HashMap<String, Double>()
    var addingProject by mutableStateOf<AddingProject?>(null)
        private set
    /** Counts up when a project has been made or cloned. */
    var projectsAdded by mutableIntStateOf(0)
        private set
    /** The project just made or cloned, and what to do with it once the server has told about it. */
    private var awaitedProject: Pair<String, (Project) -> Unit>? = null
    /** The folder just added as a project, which the next thread starts in once the server has told about it. */
    private var awaitedFolder: Pair<String, String>? = null
    /** Counts up when the composer should take the keyboard back. */
    var composerFocus by mutableIntStateOf(0)
        private set
    /** The skills that fit the word being typed in the composer, and the one lit. */
    var skillMenu by mutableStateOf<SkillMenu?>(null)
    var skillHighlight by mutableIntStateOf(0)
    /** The skill the composer's text puts in place of the word typed, once. */
    var skillPick by mutableStateOf<SkillPick?>(null)
    var skillMenusAsked = 0
    /** The word whose menu was put away, which stays away until another is typed. */
    var skillMenuDismissed: String? = null
    /** Counts up when a thread or a draft is brought to the front, which puts away a sidebar that lies over it. */
    var threadsShown by mutableIntStateOf(0)
        private set

    // What the servers hold
    var servers by mutableStateOf(emptyList<Server>())
        private set
    private val serverUpdates = mutableStateMapOf<String, ServerUpdate>()
    /** The folders the user added. "No project" of each server is in `noProjects`. */
    var projects by mutableStateOf(emptyList<Project>())
        private set
    /** The project of each server that threads start in without one. */
    var noProjects by mutableStateOf(emptyList<Project>())
        private set
    val projectsByID: Map<String, Project>
        get() = (projects + noProjects).reversed().associateBy { it.id }
    var threads by mutableStateOf(mapOf<String, ThreadInfo>())
        private set
    /** The threads as the sidebar lists them, kept in order as they change so that it never sorts them. */
    var activeThreads by mutableStateOf(emptyList<ThreadInfo>())
        private set
    var doneThreads by mutableStateOf(emptyList<ThreadInfo>())
        private set

    // The open thread
    /** Always a draft or a thread; `loadPreferences` opens the first draft. */
    private var selectionState by mutableStateOf<Selection>(Selection.Draft(""))
    var selection: Selection
        get() = selectionState
        private set(value) {
            selectionState = value
            Preferences.set("selection", value.id)
        }
    var activity by mutableStateOf(Activity())
        private set
    var transcriptIsEmpty by mutableStateOf(true)
        private set
    /** The agents the open thread's agent has started, in the order it started them. */
    var agents by mutableStateOf(emptyList<AgentInfo>())
        private set
    /** The drafts whose first message is on its way to the server, by draft. */
    var sendingDrafts by mutableStateOf(mapOf<String, SendingDraft>())
        private set
    var errorMessage by mutableStateOf<String?>(null)

    /** What deleting a thread takes with it. */
    fun deletionNote(thread: ThreadInfo?): String {
        val inWorktree = thread?.let { project(it.projectID)?.seen(it)?.worktree != null } ?: false
        return if (inWorktree) "Its worktree and the uncommitted changes there are deleted too. Its branch stays."
        else "The files the agent changed stay as they are."
    }

    fun alert(error: CoreError) {
        if (error.offline) return
        errorMessage = error.message
    }

    fun alert(result: Result<*>) {
        result.coreError?.let(::alert)
    }

    /** The error's first sentence as the alert's title, and the rest below it. */
    val errorAlert: Pair<String, String>
        get() {
            val message = errorMessage ?: ""
            val sentences = message.split(". ", limit = 2)
            val first = sentences.firstOrNull()
            if (first == null || first.length > 60 || first.contains('\n')) return "Something went wrong" to message
            val title = first.removeSuffix(".")
            return title to (if (sentences.size > 1) sentences[1] else "")
        }

    var threadDrafts by mutableStateOf(emptyList<ThreadDraft>())
        private set
    /** What the open draft said when it was opened, if it said anything. Its row in the sidebar shows this, so the sidebar doesn't change while the draft is being written. */
    private var openedDraftPreviewState by mutableStateOf<String?>(null)
    private var openedDraftPreview: String?
        get() = openedDraftPreviewState
        set(value) {
            openedDraftPreviewState = value
            Preferences.set("openedDraftPreview", value)
        }
    var undo by mutableStateOf<UndoNotice?>(null)
        private set
    /** The thread the sidebar brings into sight, opening Done if it is there: the one its pull request's end last marked done, or the one opened from the search. It stays until another is opened. */
    var revealedThreadID by mutableStateOf<String?>(null)
    /** Unknown until Settings asks for it. */
    var mediaStorage by mutableStateOf<MediaStorage?>(null)
        private set
    private var drafts by mutableStateOf(mapOf<String, String>())
    private var attachmentsByKey by mutableStateOf(mapOf<String, List<Attachment>>())
    /** The images and videos the viewer has open over the window. */
    var viewing by mutableStateOf<Viewing?>(null)
        private set
    /** How much of each image, video or file an agent sent has arrived, by its media id. */
    val mediaProgress = mutableStateMapOf<String, Double>()

    /** Counts up when the folder the open thread works in may have changed: a turn ended there, or git did something. */
    var workspaceVersion by mutableIntStateOf(0)
        private set
    /** The keyboard shortcuts, as the core read them from `keybindings.json` and the defaults. */
    var shortcuts by mutableStateOf(Shortcuts())
        private set
    /** The keys that open the sidebar's first threads are held, so their rows show them. */
    var showsJumpHints by mutableStateOf(false)
    /** What the sidebar's threads are searched for. Their messages are searched on the servers. */
    private var threadSearchState by mutableStateOf("")
    var threadSearch: String
        get() = threadSearchState
        set(value) {
            if (value == threadSearchState) return
            threadSearchState = value
            core.send("search_threads", json("query" to value))
        }
    var messageSearch by mutableStateOf(MessageSearch())
        private set

    val updater = AppUpdater()
    val rightPanel = RightPanel()
    val linear = Linear()
    val ricrac = Ricrac()
    var integrations by mutableStateOf(Integration.kept())
        private set
    val core = CoreBridge()
    val transcript = TranscriptModel()
    /** What the agent did that the right panel shows. */
    val agentTranscript = TranscriptModel()
    private var signInSession: SignInSession? = null
    private val lifecycle = Lifecycle()
    private val main = Handler(Looper.getMainLooper())
    private var undoTimer: Runnable? = null
    private var undoHeld = false
    private var openThreadID: String? = null
    /** The thread that was open when the client was last closed, until it has been opened again. */
    private var lastSelection: String? = null
    /** The draft whose server wasn't known yet when it was given its server's last choices. */
    private var draftAwaitingServer: String? = null
    private var addServerCount = 0

    init {
        loadPreferences()
        rightPanel.store = this
        linear.store = this
        ricrac.store = this
        updater.onFound = { asked -> showServerUpdates(asked) }
        AppUpdater.shared = updater
    }

    // Starting

    /** Starts the core. `environment` is what the dev scripts launch the client with, as the Apple clients read their environment. */
    fun start(environment: Map<String, String>) {
        core.decode = ::decode
        val dataDir = environment["MOTILE_DATA_DIR"] ?: dataFolder().path
        val authURL = environment["MOTILE_AUTH_URL"] ?: BuildConfig.AUTH_URL
        val config = buildMap<String, Any?> {
            put("data_dir", dataDir)
            put("auth_url", authURL)
            put("device_name", Platform.deviceName)
            put("platform", Platform.name)
            put("local_only", environment["MOTILE_LOCAL"] == "1")
            environment["MOTILE_SERVER_ADDR"]?.let { put("direct_addr", it) }
            // A phone has less room for the images and videos it keeps than a Mac.
            put("media_limit", 500_000_000)
        }
        if (!core.start(context, jsonOf(config) as JSON)) {
            errorMessage = "Motile couldn't start. Its data folder may not be writable."
        }
        environment["MOTILE_RELEASES_URL"]?.let { updater.releases = it }
        if (environment["MOTILE_DEMO"] != "1") updater.start()
        lifecycle.start(context, this)
    }

    /** The client is in front again. */
    fun becameActive() {
        markOpenThreadSeen()
        core.send("reload_keybindings")
        readGit(fetch = true)
        readWorktreeStart(fetch = true)
    }

    /** Where the core keeps the device's key, the copy of the threads and the fetched files. The key is this device's alone, so a backup never takes it. */
    private fun dataFolder(): File = File(context.noBackupFilesDir, "Motile").apply { mkdirs() }

    // Events

    /** Runs off the main thread: reads the event and returns what to do with it on the main thread. */
    private fun decode(event: JSON): (() -> Unit)? {
        when (event.string("type")) {
            "keybindings" -> {
                val shortcuts = Shortcuts(event.obj("keybindings") ?: EmptyJSON)
                return { if (this.shortcuts != shortcuts) this.shortcuts = shortcuts }
            }
            "thread_search" -> {
                val search = MessageSearch.of(event)
                return { if (messageSearch != search) messageSearch = search }
            }
            "account" -> {
                val account = Account(event.obj("account") ?: EmptyJSON)
                return { apply(account) }
            }
            "restored" -> return {
                ready = true
                ensureDraftProject()
            }
            "servers" -> {
                val servers = event.objects("servers").map { Server(it) }
                return { apply(servers) }
            }
            "threads" -> {
                val serverID = event.string("server_id")
                val threads = event.objects("threads").map { ThreadInfo(it) }
                return { applyThreads(threads, serverID) }
            }
            "thread_upsert" -> {
                val thread = ThreadInfo(event.obj("thread") ?: EmptyJSON)
                return { upsert(thread) }
            }
            "thread_deleted" -> {
                val threadID = event.string("thread_id")
                return { removeThread(threadID) }
            }
            "projects" -> {
                val serverID = event.string("server_id")
                val projects = event.objects("projects").map { Project(it, serverID) }
                return { applyProjects(projects, serverID) }
            }
            "server_update" -> {
                val serverID = event.string("server_id")
                val received = event.double("received")
                val total = event.optionalDouble("total")
                val waiting = event.bool("waiting")
                return {
                    val update = serverUpdates[serverID]
                    if (update != null) {
                        serverUpdates[serverID] = if (waiting) update.copy(waiting = true)
                        else update.copy(fraction = total?.takeIf { it > 0 }?.let { received / it })
                    }
                }
            }
            "git_progress" -> {
                val projectID = event.string("project_id")
                val threadID = event.optionalString("thread_id")
                val stage = GitStage.of(event.string("stage"))
                return {
                    project(projectID)?.let { project ->
                        val checkoutID = (threadID?.let { threads[it] }?.let { project.seen(it) } ?: project).checkoutID
                        if (gitStages[checkoutID] != null) gitStages = gitStages + (checkoutID to stage)
                    }
                }
            }
            "upload_progress" -> {
                val key = event.string("key")
                val sent = event.double("sent")
                val size = event.double("size")
                if (size <= 0) return null
                return {
                    changeAttachment(key) { attachment ->
                        if (attachment.state is Attachment.State.Uploading) attachment.copy(state = Attachment.State.Uploading(sent / size)) else attachment
                    }
                }
            }
            "media_progress" -> {
                val id = event.string("id")
                val received = event.double("received")
                val size = event.double("size")
                if (size <= 0) return null
                return { mediaProgress[id] = received / size }
            }
            "rows" -> {
                val threadID = event.string("thread_id")
                val reset = event.bool("reset")
                val start = event.int("start")
                val remove = event.int("remove")
                val rows = event.objects("rows").mapNotNull(RowModel::parse)
                val earlier = event.bool("earlier")
                return {
                    if (transcript.threadID == threadID) {
                        transcript.apply(reset, start, remove, rows, earlier)
                        // Assigned only when it changes: rows arrive many times a second.
                        val isEmpty = transcript.isEmpty
                        if (transcriptIsEmpty != isEmpty) transcriptIsEmpty = isEmpty
                        val turns = transcript.turns
                        if (rightPanel.turns != turns) rightPanel.turns = turns
                    }
                }
            }
            "agents" -> {
                val threadID = event.string("thread_id")
                val agents = event.objects("agents").map(::AgentInfo)
                return {
                    if (transcript.threadID == threadID) {
                        if (this.agents != agents) this.agents = agents
                        rightPanel.agentsChanged()
                    }
                }
            }
            "agent_rows" -> {
                val threadID = event.string("thread_id")
                val agentID = event.string("agent_id")
                val reset = event.bool("reset")
                val start = event.int("start")
                val remove = event.int("remove")
                val rows = event.objects("rows").mapNotNull(RowModel::parse)
                return {
                    if (transcript.threadID == threadID && rightPanel.shownAgent == agentID) {
                        agentTranscript.apply(reset, start, remove, rows, earlier = false)
                    }
                }
            }
            "code_spans" -> {
                val request = event.long("id")
                val lines = event.intLists("lines")
                val file = event.int("file")
                return { rightPanel.colour(request, file, lines) }
            }
            "spans" -> {
                val threadID = event.string("thread_id")
                val rowID = event.string("row_id")
                val spans = event.ints("spans")
                return {
                    if (transcript.threadID == threadID) {
                        transcript.apply(spans, rowID)
                        agentTranscript.apply(spans, rowID)
                    }
                }
            }
            "live" -> {
                val threadID = event.string("thread_id")
                val live = event.bool("live")
                return {
                    if (transcript.threadID == threadID) {
                        transcript.changeLive(live)
                        if (agentTranscript.threadID == threadID) agentTranscript.changeLive(live)
                    }
                }
            }
            "activity" -> {
                val threadID = event.string("thread_id")
                val activity = Activity.of(event.obj("activity") ?: EmptyJSON, event.objects("waiting"))
                return {
                    if (transcript.threadID == threadID) {
                        if (this.activity != activity) this.activity = activity
                        transcript.changeActivity(activity)
                    }
                }
            }
            "thread_error" -> {
                val message = event.string("message")
                return { errorMessage = message }
            }
            else -> return null
        }
    }

    private fun apply(account: Account) {
        val wasSignedIn = this.account.signedIn
        this.account = account
        if (wasSignedIn && !account.signedIn) {
            openEmptyDraft()
            enrollToken = null
            usage = UsageModel()
        }
    }

    private fun apply(servers: List<Server>) {
        val composerWasConnected = composerServer?.state == Server.State.Connected
        val wereConnected = this.servers.filter { it.state == Server.State.Connected }.map { it.id }.toSet()
        this.servers = servers
        for (server in servers) {
            if (server.state != Server.State.Connected || server.id in wereConnected) continue
            linear.read(server.id)
            ricrac.read(server.id)
        }
        if (!composerWasConnected && composerServer?.state == Server.State.Connected) {
            readGit(fetch = true)
            readWorktreeStart(fetch = true)
        }
        val known = servers.map { it.id }.toSet()
        if (projects.any { it.serverID !in known }) projects = projects.filter { it.serverID in known }
        if (noProjects.any { it.serverID !in known }) noProjects = noProjects.filter { it.serverID in known }
        if (threads.values.any { it.serverID !in known }) replaceThreads(threads.filterValues { it.serverID in known })
        // A server that is back with another version has finished updating.
        for (server in servers) {
            if (server.state != Server.State.Connected) continue
            val update = serverUpdates[server.id] ?: continue
            if (update.restarting && server.version != update.from) serverUpdates.remove(server.id)
        }
        showServerUpdates()
        ensureDraftProject()
        // The server has arrived; the install command has done its job.
        if (showsAddServer && servers.size > addServerCount) showsAddServer = false
    }

    private fun applyThreads(new: List<ThreadInfo>, serverID: String) {
        val kept = threads.filterValues { it.serverID != serverID }.toMutableMap()
        for (thread in new) kept[thread.id] = thread
        replaceThreads(kept)
        (selection as? Selection.Thread)?.let { if (threads[it.id] == null) openEmptyDraft() }
        markOpenThreadSeen()
        restoreSelection()
    }

    private fun upsert(thread: ThreadInfo) {
        val before = threads[thread.id]
        if (before == thread) return
        setThread(thread, thread.id)
        markOpenThreadSeen()
        before ?: return
        if (!before.isDone && thread.isDone && before.pullRequest != thread.pullRequest) revealedThreadID = thread.id
        if (selection != Selection.Thread(thread.id)) return
        if (before.turnEndedAt != thread.turnEndedAt || (before.running && !thread.running)) workspaceVersion += 1
    }

    /** A reply in the open thread has been seen once the client is in front. */
    private fun markOpenThreadSeen() {
        val thread = selectedThread ?: return
        if (!Platform.isActive || !thread.unread) return
        core.send("mark_seen", json("thread_id" to thread.id))
    }

    private fun removeThread(id: String) {
        setThread(null, id)
        rightPanel.forget(id)
        if (selection == Selection.Thread(id)) openEmptyDraft()
    }

    private fun applyProjects(new: List<Project>, serverID: String) {
        val git = gitProject?.git
        val updated = (projects.filter { it.serverID != serverID } + new.filter { !it.noProject }).sortedBy { it.createdAt }
        // Every row of the sidebar looks at the projects, so they only change when they did.
        if (updated != projects) projects = updated
        val order = servers.map { it.id }
        val none = (noProjects.filter { it.serverID != serverID } + new.filter { it.noProject })
            .sortedBy { order.indexOf(it.serverID).let { index -> if (index < 0) order.size else index } }
        if (none != noProjects) noProjects = none
        ensureDraftProject()
        openAwaitedProject()
        pickAwaitedFolder()
        // A server that has just started hasn't read the repository yet.
        gitProject?.let { if (it.serverID == serverID && it.git == null) readGit() }
        if (gitProject?.git != git) workspaceVersion += 1
    }

    // Lookups

    private fun replaceThreads(new: Map<String, ThreadInfo>) {
        threads = new
        activeThreads = new.values.filter { !it.isDone }.sortedWith(ThreadInfo.order)
        doneThreads = new.values.filter { it.isDone }.sortedWith(ThreadInfo.order)
    }

    /** Changes one thread, and moves it in its list only when its place there changed. */
    private fun setThread(thread: ThreadInfo?, id: String) {
        val before = threads[id]
        threads = if (thread == null) threads - id else threads + (id to thread)
        if (before != null && thread != null && before.isDone == thread.isDone && before.listedAt == thread.listedAt) {
            changeList(thread.isDone) { list -> list.indexOfListed(before)?.let { list[it] = thread } }
            return
        }
        if (before != null) changeList(before.isDone) { list -> list.indexOfListed(before)?.let { list.removeAt(it) } }
        if (thread != null) changeList(thread.isDone) { list -> list.add(list.place(thread), thread) }
    }

    private fun changeList(done: Boolean, change: (MutableList<ThreadInfo>) -> Unit) {
        if (done) doneThreads = doneThreads.toMutableList().also(change) else activeThreads = activeThreads.toMutableList().also(change)
    }

    val selectedThread: ThreadInfo?
        get() = (selection as? Selection.Thread)?.let { threads[it.id] }

    val selectedDraft: ThreadDraft?
        get() = (selection as? Selection.Draft)?.let { selected -> threadDrafts.firstOrNull { it.id == selected.id } }

    /** Every draft with something written in it, newest first. The open one is listed as it was when it was opened, and one being sent once it has been left. */
    val listedDrafts: List<ListedDraft>
        get() = threadDrafts.reversed().mapNotNull { draft ->
            val sending = sendingDrafts[draft.id]
            if (sending != null) return@mapNotNull if (sending.left) ListedDraft(draft, sending.preview, sending = true) else null
            val written = if (selection == Selection.Draft(draft.id)) openedDraftPreview else preview(draft)
            written?.let { ListedDraft(draft, it) }
        }

    /** The first line of what was written in a draft, or what is attached to it. Nothing for a draft that is still empty. */
    private fun preview(draft: ThreadDraft): String? = preview(drafts[draft.id] ?: "", attachmentsByKey[draft.id]?.size ?: 0)

    fun isSending(draft: ThreadDraft): Boolean = sendingDrafts[draft.id] != null

    fun project(id: String?): Project? = id?.let { projectsByID[it] }

    /** The projects, the one a thread was last started in first. A project without threads counts from when it was added. */
    val recentProjects: List<Project>
        get() {
            val lastUsed = HashMap<String, Double>()
            for (thread in threads.values) lastUsed[thread.projectID] = maxOf(lastUsed[thread.projectID] ?: 0.0, thread.createdAt)
            return projects.sortedWith(compareByDescending<Project> { lastUsed[it.id] ?: it.createdAt }.thenByDescending { it.id })
        }

    fun server(id: String?): Server? = servers.firstOrNull { it.id == id }

    /** The project the composer's thread works in, or the one the open draft would start in. */
    val composerProject: Project?
        get() = threadProject ?: project(selectedDraft?.projectID)

    /** The line over the thread's title: its project, its branch, and whether it starts in a new worktree. */
    val composerProjectLine: List<String>?
        get() {
            val project = composerProject
            val folder = selectedThread?.cwd?.substringAfterLast('/')
            val name = project?.name ?: folder ?: return null
            val start = draftStart
            if (draftUsesWorktree && start != null) return listOf(name, start, "New worktree")
            val branch = project?.branch ?: return listOf(name)
            return listOf(name, branch)
        }

    /** The open thread's project, as the thread works in it. */
    val threadProject: Project?
        get() {
            val thread = selectedThread ?: return null
            return project(thread.projectID)?.seen(thread)
        }

    /** The project git works in from here: the open thread's, or the open draft's, unless its thread starts in a new worktree, where only its base can be pulled. */
    val gitProject: Project?
        get() {
            val project = threadProject ?: (if (draftUsesWorktree) null else project(selectedDraft?.projectID)) ?: return null
            return if (canUseGit(project)) project else null
        }

    /** The notice to draw here: a project's anywhere, a git one over the checkout git works in from here, or over the project the open draft starts a worktree in. */
    val openNotice: Notice?
        get() {
            val notice = notice ?: return null
            val wanted = notice.checkoutID ?: return notice
            val draftProject = if (draftUsesWorktree) project(selectedDraft?.projectID) else null
            val checkoutID = (gitProject ?: draftProject)?.checkoutID ?: return null
            return if (wanted == checkoutID) notice else null
        }

    /** The folder the right panel looks into: the one the open thread works in, or the project's for the open draft. */
    val panelTarget: PanelTarget?
        get() {
            val project = threadProject ?: project(selectedDraft?.projectID) ?: return null
            // A thread without a project has no folder until it starts.
            if (project.noProject && selectedThread == null) return null
            val folder = if (project.noProject) selectedThread?.cwd else project.worktree?.path
            val awaitsWorktree = selectedThread == null && draftUsesWorktree
            return PanelTarget(
                key = draftKey, serverID = project.serverID, projectID = project.id, threadID = selectedThread?.id,
                name = (folder ?: project.path).trimEnd('/').substringAfterLast('/'),
                repository = project.branch != null, worktree = project.worktree != null, awaitsWorktree = awaitsWorktree,
                pullRequest = if (awaitsWorktree) null else (selectedThread?.pullRequest ?: project.git?.pullRequest)?.number,
            )
        }

    /** Why no pull request can be shown here, when none can. */
    val pullRequestsUnavailable: String?
        get() {
            val target = panelTarget ?: return null
            val server = server(target.serverID) ?: return null
            if (server.protocolVersion < 8) return "Update ${server.name} to see pull requests here."
            return if (target.repository) null else "This folder isn't a git repository."
        }

    /** Why the thread's pull request tab has nothing to show here, when it hasn't. */
    val pullRequestUnavailable: String?
        get() {
            val target = panelTarget ?: return null
            return pullRequestsUnavailable ?: if (target.pullRequest == null) "This branch has no pull request yet." else null
        }

    /** The server lists, links, edits and watches pull requests, and reviews their lines. */
    val pullRequestsExtended: Boolean
        get() {
            val target = panelTarget ?: return false
            return (server(target.serverID)?.protocolVersion ?: 0) >= 9
        }

    /** What the server does with pull requests by itself. */
    fun setPullRequestSettings(server: Server, doneOnMerge: Boolean? = null, removeMergedWorktrees: Boolean? = null, pullDefaultBranch: Boolean? = null) {
        val settings = mapOf(
            "done_on_merge" to (doneOnMerge ?: server.doneOnMerge),
            "remove_merged_worktrees" to (removeMergedWorktrees ?: server.removeMergedWorktrees),
            "pull_default_branch" to (pullDefaultBranch ?: server.pullDefaultBranch),
        )
        core.send("set_pull_request_settings", mapOf("server_id" to server.id, "settings" to settings)) { alert(it) }
    }

    /** Opens the pull request's tab, or the pull request on GitHub where the tab can't show it. */
    fun showPullRequest(url: String) {
        notice = null
        if (panelUnavailable != null || pullRequestUnavailable != null) return Platform.open(url)
        rightPanel.open(app.motile.panel.PanelTab.PullRequest)
    }

    /** Puts a prompt that a pull request's tab wrote in the composer, under what is there, for the user to read and send. */
    fun handOff(prompt: String) {
        val written = draft.trim()
        draft = if (written.isEmpty()) prompt else "$written\n\n$prompt"
        if (rightPanel.isMaximized) rightPanel.toggleMaximized()
        if (Platform.panelCoversThread) rightPanel.isOpen = false
        composerFocus += 1
    }

    /** Puts the prompt in the composer of the open draft. From a thread it opens a draft in the project first, which takes the panel's open tab along. */
    fun startWork(prompt: String, projectID: String) {
        if (selectedDraft != null) return handOff(prompt)
        val project = project(projectID) ?: return
        val tab = rightPanel.tabs.active
        startNewThread(project)
        if (tab != null && tab.blankNumber == null) rightPanel.open(tab)
        handOff(prompt)
    }

    /** Why the right panel has nothing to show here, when it hasn't. */
    val panelUnavailable: String?
        get() {
            val target = panelTarget ?: return "Add a project to see its files."
            val server = server(target.serverID)
            if (server == null || server.state != Server.State.Connected) return "Your server isn't connected."
            return if (server.protocolVersion >= 7) null else "Update ${server.name} to see files and changes."
        }

    /** The server the composer is talking to: the open thread's, or the open draft's project's. */
    val composerServer: Server?
        get() {
            selectedThread?.let { return server(it.serverID) }
            return selectedDraft?.let(::serverOf) ?: servers.firstOrNull()
        }

    /** The server a draft starts its thread on: its project's, or the first one. */
    private fun serverOf(draft: ThreadDraft): Server? = server(project(draft.projectID)?.serverID) ?: servers.firstOrNull()

    /** The models the composer offers. An open thread on a server from before accounts stays with its agent. */
    val composerModels: List<ModelInfo>
        get() {
            val models = composerServer?.models ?: emptyList()
            val thread = selectedThread ?: return models
            if (composerServer?.switchesAccounts == true) return models
            return models.filter { it.agent == thread.agent }
        }

    /** The model of the thread or the draft, as its account lists it, or the first one offered. */
    val composerModel: ModelInfo?
        get() {
            val thread = selectedThread
            val id = if (thread != null) thread.model else selectedDraft?.model
            val agent = thread?.agent
            val accountID = thread?.agentAccount ?: selectedDraft?.agentAccount
            return Server.model(id, accountID, composerModels.filter { agent == null || it.agent == agent })
        }

    /** The agent a draft starts with: its model's, or its server's first model's. */
    fun agentOf(draft: ThreadDraft): Agent? {
        val server = serverOf(draft) ?: return null
        return (server.models.firstOrNull { it.id == draft.model } ?: server.models.firstOrNull())?.agent
    }

    /** The accounts the composer offers the models of, an agent's default one first. */
    val composerAccounts: List<AgentAccount>
        get() {
            val agents = composerModels.map { it.agent }.toSet()
            return Agent.entries.filter { it in agents }.flatMap { composerServer?.accounts(it) ?: emptyList() }
        }

    /** The account of the composer's model's agent the thread works with. */
    val composerAccount: AgentAccount?
        get() {
            val model = composerModel ?: return null
            val id = selectedThread?.agentAccount ?: selectedDraft?.agentAccount
            return composerServer?.account(id, model.agent)
        }

    /** The model the composer shows, with the account when its agent has more than one. */
    val composerModelLabel: String
        get() {
            val model = composerModel ?: return "No agent"
            val accountID = selectedThread?.agentAccount ?: selectedDraft?.agentAccount
            return composerServer?.label(model, accountID) ?: model.shortName
        }

    /** The thread's model as the composer shows it, with the account when its agent has more than one. */
    fun modelLabel(thread: ThreadInfo): String {
        val server = server(thread.serverID)
        val offered = server?.models?.filter { it.agent == thread.agent } ?: emptyList()
        val model = Server.model(thread.model, thread.agentAccount, offered)
        if (server == null || model == null) return thread.model ?: thread.agent.title
        return server.label(model, thread.agentAccount)
    }

    /** One account with its agent's models, as the composer's model menu lists them. */
    data class ModelChoice(val title: String, val account: AgentAccount, val models: List<ModelInfo>)

    /** What the composer's model menu lists: each account with its agent's models, titled with the account's name when its agent has more than one. */
    val composerChoices: List<ModelChoice>
        get() = composerAccounts.map { account ->
            val several = (composerServer?.accounts(account.agent)?.size ?: 0) > 1
            val title = if (several) "${account.agent.title} · ${account.name}" else account.agent.title
            ModelChoice(title, account, composerModels.filter { it.runs(account) })
        }

    /** The effort the thread or the draft runs at: "" where the agent applies its own default. */
    val composerEffort: String?
        get() {
            val thread = selectedThread
            val effort = if (thread != null) thread.effort else selectedDraft?.effort
            val model = composerModel ?: return null
            if (model.efforts.isEmpty()) return null
            if (effort != null && effort in model.efforts) return effort
            return model.defaultEffort ?: ""
        }

    val composerAccess: Access get() = selectedThread?.access ?: selectedDraft?.access ?: Access.Full

    /** What the composer's text and attachments are kept under: the open draft or thread. */
    val draftKey: String get() = selection.id

    var draft: String
        get() = drafts[draftKey] ?: ""
        set(value) = setText(value, draftKey)

    val attachments: List<Attachment> get() = attachmentsByKey[draftKey] ?: emptyList()

    private fun setText(text: String, key: String) {
        drafts = if (text.isEmpty()) drafts - key else drafts + (key to text)
        Preferences.set("drafts", drafts)
    }

    // Preferences

    private fun loadPreferences() {
        drafts = Preferences.strings("drafts")
        lastSelection = Preferences.string("selection")
        val saved = Preferences.string("threadDrafts")?.let { text ->
            runCatching { kotlinx.serialization.json.Json.parseToJsonElement(text) as? kotlinx.serialization.json.JsonArray }.getOrNull()
                ?.mapNotNull { (it as? JSON)?.let(ThreadDraft::of) }
        } ?: emptyList()
        threadDrafts = saved.filter { preview(it) != null || it.id == lastSelection }
        val restored = threadDrafts.firstOrNull { it.id == lastSelection }
        val restoredPreview = Preferences.string("openedDraftPreview")
        val opened = restored ?: emptyDraft()
        if (opened.model == null && opened.agentAccount == null && opened.effort == null) draftAwaitingServer = opened.id
        selection = Selection.Draft(opened.id)
        openedDraftPreview = if (restored == null) null else restoredPreview
    }

    private fun saveThreadDrafts() {
        Preferences.set("threadDrafts", jsonOf(threadDrafts.map { it.json }).toString())
    }

    /** A draft that starts with what the last one was set to. */
    private fun addDraft(): ThreadDraft {
        val draft = applyLastSettings(ThreadDraft(projectID = Preferences.string("new.project"), worktree = Preferences.bool("new.worktree")))
        threadDrafts = threadDrafts + draft
        saveThreadDrafts()
        return draft
    }

    /** The model, account and effort are the server's, so each server keeps what was last chosen on it. A draft whose server isn't known yet keeps what it had. */
    private fun applyLastSettings(draft: ThreadDraft): ThreadDraft {
        val reset = draft.copy(access = Access.of(Preferences.string("new.access")) ?: Access.Full, base = null)
        val server = serverOf(reset)
        if (server == null) {
            draftAwaitingServer = reset.id
            return reset
        }
        if (draftAwaitingServer == reset.id) draftAwaitingServer = null
        return reset.copy(
            model = Preferences.string("new.model-${server.id}"),
            agentAccount = Preferences.string("new.agentAccount-${server.id}"),
            effort = Preferences.string("new.effort-${server.id}"),
        )
    }

    /** Has the next draft start with what was just chosen, and only that. */
    private fun rememberChoices(draft: ThreadDraft, before: ThreadDraft) {
        if (draft.projectID != before.projectID) Preferences.set("new.project", draft.projectID)
        if (draft.worktree != before.worktree) Preferences.set("new.worktree", draft.worktree == true)
        if (draft.access != before.access) Preferences.set("new.access", draft.access.raw)
        val server = serverOf(draft) ?: return
        if (draft.model != before.model) Preferences.set("new.model-${server.id}", draft.model)
        if (draft.agentAccount != before.agentAccount) Preferences.set("new.agentAccount-${server.id}", draft.agentAccount)
        if (draft.effort != before.effort) Preferences.set("new.effort-${server.id}", draft.effort)
    }

    /** A draft with nothing in it: the one that is already there, or a new one. */
    private fun emptyDraft(): ThreadDraft {
        val index = threadDrafts.indexOfLast { preview(it) == null && !isSending(it) }
        if (index >= 0) {
            val empty = applyLastSettings(threadDrafts[index])
            threadDrafts = threadDrafts.toMutableList().also { it[index] = empty }
            saveThreadDrafts()
            return empty
        }
        return addDraft()
    }

    /** Changes the open draft, and has the next draft start with what changed. */
    private fun updateDraft(change: (ThreadDraft) -> ThreadDraft) {
        val index = threadDrafts.indexOfFirst { selection == Selection.Draft(it.id) }
        if (index < 0) return
        val before = threadDrafts[index]
        val after = change(before)
        threadDrafts = threadDrafts.toMutableList().also { it[index] = after }
        rememberChoices(after, before)
        saveThreadDrafts()
    }

    private fun removeDraft(id: String) {
        threadDrafts = threadDrafts.filter { it.id != id }
        attachmentsByKey = attachmentsByKey - id
        setText("", id)
        saveThreadDrafts()
    }

    /** Keeps the open draft pointing at a project that exists, once the projects are known. */
    private fun ensureDraftProject() {
        try {
            val draft = selectedDraft ?: return
            if (!ready || project(draft.projectID) != null) return
            val first = projects.firstOrNull() ?: noProjects.firstOrNull() ?: return
            moveDraft(first.id)
        } finally {
            applyLastSettingsOnceServerKnown()
        }
    }

    /** Gives the draft opened at launch without choices of its own its server's last ones, once the cache has said which server that is. */
    private fun applyLastSettingsOnceServerKnown() {
        val index = threadDrafts.indexOfFirst { it.id == draftAwaitingServer && selection == Selection.Draft(it.id) }
        if (index < 0) return
        val draft = threadDrafts[index]
        val known = if (draft.projectID == null) servers.isNotEmpty() else project(draft.projectID) != null
        if (!known) return
        threadDrafts = threadDrafts.toMutableList().also { it[index] = applyLastSettings(draft) }
        saveThreadDrafts()
    }

    /** Puts the open draft in a project. On another server, it starts with that server's last choices. */
    private fun moveDraft(projectID: String?) {
        val from = selectedDraft?.let(::serverOf)?.id
        updateDraft { draft ->
            val moved = draft.copy(projectID = projectID, base = null)
            if (serverOf(moved)?.id != from) applyLastSettings(moved) else moved
        }
        if (selectedDraft?.let(::serverOf)?.id != from) refreshComposerModels()
    }

    /** Has the open draft's server ask its agents what models they run, so that its menus offer what the thread would start with. */
    private fun refreshComposerModels() {
        val server = composerServer ?: return
        if (server.state != Server.State.Connected || !server.refreshesModels) return
        core.send("request", mapOf("server_id" to server.id, "request" to mapOf("type" to "refresh_models")))
    }

    /** Opens the thread that was open when the client was last closed, once it is known. */
    private fun restoreSelection() {
        val wanted = lastSelection ?: return
        if (threads[wanted] == null) return
        lastSelection = null
        val draft = selectedDraft ?: return
        if (preview(draft) != null || isSending(draft)) return
        select(Selection.Thread(wanted))
    }

    // Account

    fun signIn() {
        if (signingIn) return
        signingIn = true
        signInError = null
        core.send("begin_sign_in") { result ->
            val url = result.getOrNull()?.string("url")?.takeIf { it.isNotEmpty() }
            if (url == null) return@send signInFailed("The sign-in couldn't be started.")
            val session = SignInSession()
            signInSession = session
            session.start(context, url) { callback ->
                signInSession = null
                if (callback == null) {
                    signingIn = false
                    return@start
                }
                core.send("complete_sign_in", json("url" to callback)) { completed ->
                    val error = completed.coreError
                    if (error != null) signInFailed(error.message) else signingIn = false
                }
            }
        }
    }

    /** Signs in on an auth server that allows it without Google. Used by the demo and local work. */
    fun devSignIn(email: String, done: (() -> Unit)? = null) {
        signingIn = true
        core.send("dev_sign_in", json("email" to email)) { result ->
            val error = result.coreError
            if (error != null) signInFailed(error.message) else signingIn = false
            done?.invoke()
        }
    }

    private fun signInFailed(message: String) {
        signingIn = false
        signInError = message
    }

    fun signOut() {
        closeSettings()
        drafts = emptyMap()
        attachmentsByKey = emptyMap()
        threadDrafts = emptyList()
        Preferences.remove("drafts")
        openEmptyDraft()
        core.send("sign_out")
    }

    // Servers

    /** Asks for an install command and keeps looking for the server it will link. */
    fun prepareToAddServer() {
        addServerCount = servers.size
        core.send("watch_servers", json("on" to true))
        val token = enrollToken
        if (token != null && !token.isExpired()) return
        regenerateEnrollToken()
    }

    fun regenerateEnrollToken() {
        if (enrollTokenPending) return
        enrollTokenPending = true
        core.send("create_enroll_token") { result ->
            enrollTokenPending = false
            result.onSuccess { enrollToken = EnrollToken(it) }
            alert(result)
        }
    }

    fun stopAddingServer() {
        core.send("watch_servers", json("on" to false))
        // A token links one server; the next server gets a new one.
        if (servers.size != addServerCount) enrollToken = null
    }

    /** Whether the server runs an older version than the newest release. */
    fun isOutdated(server: Server): Boolean = server.state == Server.State.Connected && Version.isOlder(server.version, updater.latest)

    /** Opens the sidebar's servers on an update one of them needs: once for each release, or whenever the user looked for one. */
    private fun showServerUpdates(asked: Boolean = false) {
        val latest = updater.latest ?: return
        if (servers.none(::isOutdated)) return
        if (!asked && Preferences.string("sidebar.serversOpenedFor") == latest) return
        Preferences.set("sidebar.serversOpenedFor", latest)
        Preferences.set("sidebar.serversExpanded", true)
        serversExpandedRequests += 1
    }

    /** Counts up when the sidebar should open its servers. */
    var serversExpandedRequests by mutableIntStateOf(0)
        private set

    /** Whether an agent works on the server: in a turn, or watching something it left running. */
    fun isBusy(server: Server): Boolean = threads.values.any { it.serverID == server.id && it.busy }

    /** Whether the server can wait for its agents before it restarts, or stop them and go on with them after. */
    fun canChooseRestart(server: Server): Boolean = server.protocolVersion >= 14

    /** Has the server install the newest release and start it: `when` its agents work, once they have finished or at once. A server that can't choose refuses while they work. */
    fun update(server: Server, whenBusy: RestartWhen = RestartWhen.Idle) {
        val current = serverUpdate(server)
        if (current?.waiting == true && whenBusy == RestartWhen.Now) return restartNow(server)
        if (current != null) return
        serverUpdates[server.id] = ServerUpdate(from = server.version)
        val command = buildMap<String, Any?> {
            put("server_id", server.id)
            if (canChooseRestart(server)) put("when", whenBusy.raw)
        }
        core.send("update_server", command) { result ->
            if (result.isSuccess) {
                serverUpdates[server.id]?.let { serverUpdates[server.id] = it.copy(waiting = false, restarting = true) }
                // If the server never says it is back, the row stops waiting for it.
                main.postDelayed({ if (serverUpdates[server.id]?.restarting == true) serverUpdates.remove(server.id) }, 60_000)
            } else {
                serverUpdates.remove(server.id)
                alert(result)
            }
        }
    }

    /** The update a server is putting in place: as it says, or, from a server too old to say, as the request that began it goes. */
    fun serverUpdate(server: Server): ServerUpdate? = server.update ?: serverUpdates[server.id]

    /** Has a server that waits for its agents to update stop them and restart now. The request that started the update answers once it restarts. */
    private fun restartNow(server: Server) {
        core.send("update_server", json("server_id" to server.id, "when" to RestartWhen.Now.raw)) { alert(it) }
    }

    fun setContinueSettings(afterLimits: Boolean, afterRestarts: Boolean, server: Server) {
        val settings = mapOf("after_limits" to afterLimits, "after_restarts" to afterRestarts)
        core.send("set_continue_settings", mapOf("server_id" to server.id, "settings" to settings)) { alert(it) }
    }

    fun removeServer(server: Server) {
        core.send("remove_server", json("server_id" to server.id)) { alert(it) }
    }

    // Projects

    /** Adds the folder as a project and opens a thread in it. A folder that is a project already is opened where it was left. */
    fun addProject(serverID: String, path: String) {
        val trimmed = if (path.length > 1 && path.endsWith("/")) path.dropLast(1) else path
        projects.firstOrNull { it.serverID == serverID && it.path == trimmed }?.let { return goTo(it) }
        val name = trimmed.substringAfterLast('/')
        core.send("request", mapOf("server_id" to serverID, "request" to mapOf("type" to "add_project", "path" to path))) { result ->
            val error = result.coreError
            if (error != null) return@send show(Notice(title = "Couldn't add $name", description = error.message, failed = true))
            awaitedFolder = serverID to trimmed
            pickAwaitedFolder()
        }
    }

    private fun pickAwaitedFolder() {
        val (serverID, path) = awaitedFolder ?: return
        val project = projects.firstOrNull { it.serverID == serverID && it.path == path } ?: return
        awaitedFolder = null
        openThread(project)
    }

    /** Opens a thread in the project: the open draft, moved into it, or a new one. */
    private fun openThread(project: Project) {
        closeSettings()
        val draft = selectedDraft
        if (draft == null || isSending(draft)) return startNewThread(project)
        setNewThreadProject(project.id)
        threadsShown += 1
    }

    /** Goes to the project's latest thread that is still going, or opens a new one in it. */
    private fun goTo(project: Project) {
        val thread = activeThreads.firstOrNull { it.projectID == project.id } ?: return openThread(project)
        closeSettings()
        show(Selection.Thread(thread.id))
    }

    /** Opens the panel on the ways to add a project, after the servers when there is a choice. */
    fun addProject() = openPanel(addProjectPage)

    val addProjectPage: PanelPage
        get() {
            val connected = servers.filter { it.state == Server.State.Connected }
            val server = connected.singleOrNull() ?: return PanelPage.Servers
            return PanelPage.Sources(server.id)
        }

    /** Whether the server can start a project from a name or from GitHub. */
    fun startsProjects(server: Server?): Boolean = (server?.protocolVersion ?: 0) >= 5

    /** With `icons`, the images that can be a project's icon are listed too. */
    fun browse(serverID: String, query: String, icons: Boolean, done: (Result<FolderListing>) -> Unit) {
        core.send("browse", json("server_id" to serverID, "query" to query, "icons" to icons)) { result ->
            done(result.map(::FolderListing))
        }
    }

    fun readGitHub(serverID: String) {
        core.send("request", mapOf("server_id" to serverID, "request" to mapOf("type" to "github_status"))) { result ->
            result.getOrNull()?.let { heardGitHub(it, serverID) }
        }
    }

    private fun heardGitHub(answer: JSON, serverID: String) {
        val state = GitHubState.of(answer.string("state")) ?: return
        github = github + (serverID to state)
        Preferences.set("github-$serverID", state.raw)
    }

    fun isOn(integration: Integration): Boolean = integration in integrations

    fun setIntegration(integration: Integration, on: Boolean) {
        integrations = if (on) integrations + integration else integrations - integration
        Preferences.set(integration.preferencesKey, on)
    }

    /** Why Linear can't be shown here, when it can't. */
    val linearUnavailable: String?
        get() {
            val target = panelTarget ?: return null
            val server = server(target.serverID) ?: return null
            return if (server.protocolVersion >= 12) null else "Update ${server.name} to connect it to Linear."
        }

    /** The panel's server is connected to a Linear workspace. */
    val linearConnected: Boolean
        get() {
            if (!isOn(Integration.Linear)) return false
            val target = panelTarget ?: return false
            if (linearUnavailable != null) return false
            return linear.connected(target.serverID).isNotEmpty()
        }

    /** Why Ricrac can't be shown here, when it can't. */
    val ricracUnavailable: String?
        get() {
            val target = panelTarget ?: return null
            val server = server(target.serverID) ?: return null
            return if (server.protocolVersion >= 29) null else "Update ${server.name} to connect it to Ricrac."
        }

    /** The panel's server is connected to a Ricrac workspace. */
    val ricracConnected: Boolean
        get() {
            if (!isOn(Integration.Ricrac)) return false
            val target = panelTarget ?: return false
            if (ricracUnavailable != null) return false
            return ricrac.connected(target.serverID).isNotEmpty()
        }

    /** Asks the server for its GitHub repositories, unless it listed them in the last minute or `fresh` asks again anyway. The ones it listed before stay until it answers. */
    fun loadRepos(serverID: String, fresh: Boolean = false) {
        if (serverID in listingRepos) return
        val listed = reposListed[serverID]
        if (!fresh && listed != null && Time.now() - listed < 60) return
        listingRepos = listingRepos + serverID
        repoErrors = repoErrors - serverID
        core.send("request", mapOf("server_id" to serverID, "request" to mapOf("type" to "github_repos"))) { result ->
            listingRepos = listingRepos - serverID
            val answer = result.getOrNull()
            when {
                answer == null -> repoErrors = repoErrors + (serverID to (result.coreError?.message ?: ""))
                answer.string("type") == "repos" -> {
                    repos = repos + (serverID to answer.objects("repos").map(::Repo))
                    reposListed[serverID] = Time.now()
                }
                else -> {
                    repos = repos - serverID
                    heardGitHub(answer, serverID)
                }
            }
        }
    }

    /** Makes the project and opens a thread in it. The panel waits, so that a name that is taken can be changed. */
    fun newProject(name: String, serverID: String) {
        if (addingProject != null) return
        addingProject = AddingProject(serverID, name)
        panelNotice = null
        add(mapOf("type" to "new_project", "name" to name), serverID, arrived = { project ->
            addingProject = null
            openThread(project)
        }, failed = { error ->
            addingProject = null
            if (panel == null) alert(error) else panelNotice = error.message
        })
    }

    /** Clones the repository while the notice says so, and opens a thread in it once it is there. Someone who has gone elsewhere meanwhile is offered the thread instead. */
    fun clone(repo: String, serverID: String) {
        val into = server(serverID)?.let { " on ${it.name}" } ?: ""
        show(Notice(title = "Cloning $repo", description = "Into ~/projects$into", pending = true))
        val from = selection
        add(mapOf("type" to "clone_repo", "repo" to repo), serverID, arrived = { project ->
            if (selection != from) return@add show(Notice(title = "Cloned $repo", description = project.path, projectID = project.id))
            show(Notice(title = "Cloned $repo", description = project.path))
            openThread(project)
        }, failed = { error ->
            show(Notice(title = "Couldn't clone $repo", description = error.message, failed = true))
        })
    }

    private fun add(request: Map<String, Any?>, serverID: String, arrived: (Project) -> Unit, failed: (CoreError) -> Unit) {
        core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            val answer = result.getOrNull() ?: return@send failed(result.coreError!!)
            projectsAdded += 1
            awaitedProject = answer.string("project_id") to arrived
            openAwaitedProject()
        }
    }

    /** Does what was planned for the project that was just made, once the server has told about it. */
    private fun openAwaitedProject() {
        val (id, arrived) = awaitedProject ?: return
        val project = project(id) ?: return
        awaitedProject = null
        arrived(project)
    }

    // Branches

    /** Whether a turn is running in the project's folder, which is when its branch can't be switched. */
    fun isWorking(project: Project): Boolean = threads.values.any { it.projectID == project.id && it.running && it.cwd == project.path }

    /** Whether the project's server is new enough to start threads in worktrees of their own. */
    fun canUseWorktrees(project: Project): Boolean =
        project.branch != null && project.git?.hasCommit != false && (server(project.serverID)?.protocolVersion ?: 0) >= 6

    /** Whether the open draft starts its thread in a new worktree. */
    val draftUsesWorktree: Boolean
        get() {
            val draft = selectedDraft ?: return false
            val project = project(draft.projectID) ?: return false
            return draft.worktree == true && canUseWorktrees(project)
        }

    /** The branch the open draft's worktree starts from: the one picked, the one picked last in its project, or the default one. */
    val draftBase: String?
        get() {
            val draft = selectedDraft ?: return null
            val project = project(draft.projectID) ?: return null
            return draft.base ?: Preferences.string("new.base-${project.id}") ?: project.git?.defaultBranch ?: project.branch
        }

    /** The project and the base of the open draft's worktree, when its thread starts in one. */
    val draftWorktreeKey: String?
        get() {
            if (!draftUsesWorktree) return null
            val projectID = selectedDraft?.projectID ?: return null
            val base = draftBase ?: return null
            return "$projectID $base"
        }

    /** What the open draft's worktree starts at: its base, or the remote's when that has commits the local one lacks. */
    val draftStart: String?
        get() {
            val base = draftBase ?: return null
            return draftWorktreeKey?.let { worktreeStarts[it] } ?: base
        }

    /** Asks the project's server what the open draft's worktree would start at. With `fetch` the remote is asked first, and a fetch that fails is said. */
    fun readWorktreeStart(fetch: Boolean) {
        val key = draftWorktreeKey ?: return
        val base = draftBase ?: return
        val project = project(selectedDraft?.projectID) ?: return
        if ((server(project.serverID)?.protocolVersion ?: 0) < 13) return
        val request = mapOf("type" to "worktree_start", "project_id" to project.id, "base" to base, "fetch" to fetch)
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            val answer = result.getOrNull() ?: return@send
            worktreeStarts = worktreeStarts + (key to answer.string("start"))
            val problem = answer.optionalString("problem") ?: return@send
            show(Notice(checkoutID = project.checkoutID, title = "Couldn't fetch $base", description = problem, failed = true))
        }
    }

    /** The open draft's base, when its worktree would start from the remote's instead, which has commits the local one lacks. */
    val draftBaseToPull: String?
        get() {
            val base = draftBase ?: return null
            val start = draftStart ?: return null
            val project = project(selectedDraft?.projectID) ?: return null
            if (start == base || (server(project.serverID)?.protocolVersion ?: 0) < 20) return null
            return base
        }

    val pullingDraftBase: Boolean get() = draftWorktreeKey?.let { it in pullingBases } ?: false

    /** Brings the open draft's local base up to the remote's, for its worktree to start from it. */
    fun pullDraftBase() {
        val key = draftWorktreeKey ?: return
        val base = draftBaseToPull ?: return
        val project = project(selectedDraft?.projectID) ?: return
        if (key in pullingBases) return
        pullingBases = pullingBases + key
        notice = null
        val request = mapOf("type" to "update_base", "project_id" to project.id, "base" to base)
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            pullingBases = pullingBases - key
            val answer = result.getOrNull()
            if (answer != null) {
                worktreeStarts = worktreeStarts + (key to answer.string("start"))
                show(Notice(checkoutID = project.checkoutID, title = "Pulled $base"))
            } else {
                show(Notice(checkoutID = project.checkoutID, title = "Couldn't pull $base", description = result.coreError?.message, failed = true))
            }
        }
    }

    fun setDraftWorktree(worktree: Boolean) {
        updateDraft { it.copy(worktree = worktree) }
        readGit(fetch = true)
    }

    fun setDraftBase(branch: String) {
        updateDraft { it.copy(base = branch) }
        val projectID = selectedDraft?.projectID ?: return
        Preferences.set("new.base-$projectID", branch)
    }

    /** Whether the project's server is new enough to list and switch branches. */
    fun canSwitchBranches(project: Project): Boolean = project.branch != null && (server(project.serverID)?.protocolVersion ?: 0) >= 3

    /** Opens the branch picker at once, on the branches last listed or on placeholders, and lists them again. */
    fun showBranches(project: Project) {
        listedBranches = knownBranches[project.id]?.let { Result.success(it) }
        branchesProjectID = project.id
        showsBranches = true
        listBranches(project)
    }

    /** Lists the branches of the project the composer is on before the picker is opened. */
    private fun listComposerBranches() {
        val project = composerProject ?: return
        if (canSwitchBranches(project)) listBranches(project)
    }

    private fun listBranches(project: Project) {
        val request = mapOf("type" to "branches", "project_id" to project.id)
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            val listed = result.map { answer -> answer.objects("branches").map(::Branch) }
            listed.getOrNull()?.let { knownBranches[project.id] = it }
            if (branchesProjectID != project.id || !showsBranches) return@send
            listedBranches = listed
        }
    }

    /** Checks the branch out in the project's folder, making it first if asked to. `done` gets what went wrong, if anything. */
    fun switchBranch(project: Project, name: String, create: Boolean, done: (String?) -> Unit) {
        val request = mapOf("type" to "switch_branch", "project_id" to project.id, "branch" to name, "create" to create)
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            done(result.coreError?.message)
        }
    }

    // Git

    /** Whether the project's server is new enough to commit, push and open pull requests. */
    fun canUseGit(project: Project): Boolean = (server(project.serverID)?.protocolVersion ?: 0) >= 4

    /** Whether the project's folder is no git repository and its server can make it one. */
    fun canInitializeGit(project: Project): Boolean =
        !project.noProject && project.branch == null && project.git == null && (server(project.serverID)?.protocolVersion ?: 0) >= 16

    fun initializeGit(project: Project) {
        if (project.id in initializingGit) return
        initializingGit = initializingGit + project.id
        val request = mapOf("type" to "init_repository", "project_id" to project.id)
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            initializingGit = initializingGit - project.id
            alert(result)
        }
    }

    /** Has the server read the repository git works in from here again, which the project then arrives with. With `fetch` the remote is asked first, and a fetch that fails is said. */
    private fun readGit(fetch: Boolean = false, done: ((List<ChangedFile>) -> Unit)? = null) {
        if (fetch) listComposerBranches()
        val project = gitProject ?: return
        val request = buildMap<String, Any?> {
            put("type", "git_status")
            put("project_id", project.id)
            put("fetch", fetch)
            selectedThread?.let { put("thread_id", it.id) }
        }
        core.send("request", mapOf("server_id" to project.serverID, "request" to request)) { result ->
            val answer = result.getOrNull()
            if (answer != null) {
                answer.optionalString("problem")?.let { problem ->
                    show(Notice(checkoutID = project.checkoutID, title = "Couldn't fetch from the remote", description = problem, failed = true))
                }
                done?.invoke(answer.objects("files").map(::ChangedFile))
            } else if (done != null) {
                // Only said when the user is waiting for the answer.
                alert(result)
            }
        }
    }

    /** What the run in the project's checkout is at, started here or from another client. */
    fun gitStage(project: Project): GitStage? {
        gitStages[project.checkoutID]?.let { return it }
        val thread = selectedThread ?: return null
        return if (thread.projectID == project.id) thread.gitStage else null
    }

    /** What a run starts with, until your server says what it is at. */
    private fun firstStage(action: String, project: Project, written: Boolean): GitStage = when (action) {
        "pull" -> GitStage.Pull
        "push" -> GitStage.Push
        "create_pr" -> {
            val pushes = project.gitControl?.menu?.any { it.action == "push" && it.reason == null } ?: false
            if (pushes) GitStage.Push else GitStage.PullRequestText
        }
        else -> if (written) GitStage.Commit else GitStage.Message
    }

    /** What a tap on the git button does: the one action the repository calls for, at once. */
    fun runQuickGit(project: Project) {
        val quick = project.gitControl?.quick ?: return
        if (gitStage(project) != null) return
        quick.url?.let { return showPullRequest(it) }
        val action = quick.action ?: return show(Notice(checkoutID = project.checkoutID, title = quick.hint ?: quick.label))
        startGit(action, project, quick.confirm)
    }

    /** What a pick from the menu does: a commit opens its sheet, the others happen at once. */
    fun chooseGit(item: GitMenuItem, project: Project) {
        if (item.reason != null || gitStage(project) != null) return
        if (item.action != "commit") return startGit(item.action, project, item.confirm)
        // A sheet takes its size from what it opens with, so the files come first.
        readGit { files ->
            gitFiles = files
            committingProject = project
        }
    }

    /** Runs the action, after asking where when it would push from the default branch. */
    private fun startGit(action: String, project: Project, confirm: GitConfirm?) {
        if (confirm == null) return runGit(action, project)
        pendingGit = PendingGit(project, action, confirm)
    }

    /** Carries on with the action that waited, on the default branch or on a branch made for it. */
    fun confirmGit(pending: PendingGit, onNewBranch: Boolean) {
        runGit(pending.action, pending.project, pending.message, pending.paths, onNewBranch)
    }

    /** Has the project's server carry the action out. It writes the commit message when there is none, and the pull request. What it did, or what git refused, shows under the button. */
    fun runGit(action: String, project: Project, message: String? = null, paths: List<String> = emptyList(), newBranch: Boolean = false) {
        val checkoutID = project.checkoutID
        if (gitStage(project) != null) return
        gitStages = gitStages + (checkoutID to firstStage(action, project, written = !message.isNullOrEmpty() && !newBranch))
        notice = null
        val command = buildMap<String, Any?> {
            put("server_id", project.serverID)
            put("project_id", project.id)
            put("action", action)
            put("paths", paths)
            put("new_branch", newBranch)
            if (!message.isNullOrEmpty()) put("message", message)
            selectedThread?.let { if (it.projectID == project.id) put("thread_id", it.id) }
        }
        core.send("git_run", command) { result ->
            gitStages = gitStages - checkoutID
            val done = result.getOrNull()
            if (done != null) {
                show(Notice(
                    checkoutID = checkoutID, title = done.string("title"), description = done.optionalString("description"),
                    url = done.optionalString("url"), next = done.optionalString("next"),
                ))
            } else {
                show(Notice(checkoutID = checkoutID, title = "Git stopped", description = result.coreError?.message, failed = true))
            }
        }
    }

    /** What worked goes away by itself; what failed or is still under way stays until it is closed or replaced. */
    private fun show(notice: Notice) {
        this.notice = notice
        if (notice.failed || notice.pending) return
        main.postDelayed({ if (this.notice == notice) this.notice = null }, 10_000)
    }

    fun dismissNotice() {
        notice = null
    }

    /** The action a notice says comes next, like the push after a commit. */
    fun runNextGit() {
        val notice = notice ?: return
        val next = notice.next ?: return
        val project = gitProject ?: return
        if (project.checkoutID != notice.checkoutID) return
        val confirm = project.gitControl?.menu?.firstOrNull { it.action == next }?.confirm
        this.notice = null
        startGit(next, project, confirm)
    }

    /** Opens a thread in the project a notice is about. */
    fun openNoticeProject() {
        val project = project(notice?.projectID) ?: return
        notice = null
        openThread(project)
    }

    /** What the agents spent on the connected `servers`, or all of them, in the last `buckets` spans of `bucketSeconds`, by this device's clock. `kept` answers at once with what was last read. */
    fun loadUsage(bucketSeconds: Int, buckets: Int, kept: Boolean, servers: Set<String>?, reply: (Result<UsageReport>) -> Unit) {
        val command = buildMap<String, Any?> {
            put("bucket_secs", bucketSeconds)
            put("buckets", buckets)
            put("utc_offset_secs", TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000)
            put("kept", kept)
            servers?.let { put("servers", it.toList()) }
        }
        core.send("usage", command, read = { UsageReport(it) }, reply = reply)
    }

    /** How much of their plans the agents' logins on the connected `servers`, or all of them, have used. `refresh` has the servers read it anew, `kept` answers at once with what was last read. */
    fun loadLimits(refresh: Boolean, kept: Boolean, servers: Set<String>?, reply: (Result<LimitsReport>) -> Unit) {
        val command = buildMap<String, Any?> {
            put("refresh", refresh)
            put("kept", kept)
            servers?.let { put("servers", it.toList()) }
        }
        core.send("limits", command, read = { LimitsReport(it) }, reply = reply)
    }

    /** Picks the model that writes titles, commit messages and pull requests on the server. Without one, the lightest model of the thread's agent writes. */
    fun setTextModel(model: String?, server: Server) {
        core.send("set_text_model", buildMap { put("server_id", server.id); model?.let { put("model", it) } }) { alert(it) }
    }

    /** Says how the server's writer names branches. Without instructions the server goes back to its own. */
    fun setBranchInstructions(instructions: String?, server: Server) {
        core.send("set_branch_instructions", buildMap { put("server_id", server.id); instructions?.let { put("instructions", it) } }) { alert(it) }
    }

    /** Sets the shell script that runs in every new worktree of the project, or takes it away. */
    fun setSetup(project: Project, script: String) {
        val change = buildMap<String, Any?> {
            put("type", "set_project_setup")
            put("project_id", project.id)
            if (script.isNotBlank()) put("script", script)
        }
        request(project.serverID, change)
    }

    fun removeProject(project: Project) = request(project.serverID, mapOf("type" to "remove_project", "project_id" to project.id))

    /** Makes an image on the project's server its icon. Without one, the project goes back to the icon found in its folder. */
    fun setIcon(project: Project, path: String?) {
        val command = buildMap<String, Any?> {
            put("server_id", project.serverID)
            put("project_id", project.id)
            path?.let { put("path", it) }
        }
        core.send("set_project_icon", command) { alert(it) }
    }

    // Settings

    fun openSettings(section: SettingsSection = SettingsSection.General, target: String? = null) {
        if (!account.signedIn) return
        showsUsage = false
        settings = section
        settingsTarget = target
    }

    fun closeSettings() {
        settings = null
        settingsTarget = null
        settingsQuery = ""
    }

    fun openUsage() {
        if (!account.signedIn) return
        closeSettings()
        showsUsage = true
        usage.start(this)
    }

    /** What the back gesture does when nothing else claims it: leaves a route, restores the maximized right panel, or hides a right panel that only has its blank tab. Nothing while the command panel, the viewer or the branches have it. */
    val escapes: (() -> Unit)?
        get() {
            if (panel != null || viewing != null || showsBranches) return null
            if (settings != null || showsUsage) return { closeRoute() }
            if (rightPanel.isMaximized) return { rightPanel.toggleMaximized() }
            if (rightPanel.isOpen && rightPanel.tabs.isBlank) return { rightPanel.isOpen = false }
            return null
        }

    /** Leaves the settings or the usage for the threads. */
    fun closeRoute() {
        closeSettings()
        showsUsage = false
    }

    // Command panel

    fun openPanel(page: PanelPage) {
        if (!account.signedIn || servers.isEmpty()) return
        panel = page
        readGitHubOfServers()
    }

    fun readGitHubOfServers() {
        for (server in servers) {
            if (server.state != Server.State.Connected || !startsProjects(server)) continue
            if (github[server.id] == null) GitHubState.of(Preferences.string("github-${server.id}"))?.let { github = github + (server.id to it) }
            readGitHub(server.id)
        }
    }

    fun closePanel() {
        if (panel == null) return
        panel = null
        panelNotice = null
        composerFocus += 1
    }

    /** Goes to another page of the open command panel. */
    fun goToPanelPage(page: PanelPage) {
        if (panel == null) return openPanel(page)
        panel = page
    }

    // Threads

    fun select(new: Selection) {
        lastSelection = null
        showsUsage = false
        val reopens = selectedThread != null && openThreadID == null
        if (new == selection && !reopens) return
        openThreadID?.let {
            core.send("close_thread", json("thread_id" to it))
            openThreadID = null
        }
        val left = selectedDraft
        selection = new
        if (new != revealedThreadID?.let { Selection.Thread(it) }) revealedThreadID = null
        if (left != null && isSending(left)) sendingDrafts[left.id]?.let { sendingDrafts = sendingDrafts + (left.id to it.copy(left = true)) }
        if (left != null && preview(left) == null && !isSending(left)) removeDraft(left.id)
        openedDraftPreview = selectedDraft?.let(::preview)
        activity = Activity()
        transcriptIsEmpty = true
        rightPanel.turns = emptyList()
        rightPanel.showAgents()
        agents = emptyList()
        val thread = (new as? Selection.Thread)?.let { threads[it.id] }
        if (thread == null) {
            transcript.begin(null)
            showSendingMessage()
            ensureDraftProject()
            readGit(fetch = true)
            refreshComposerModels()
            return
        }
        open(thread)
    }

    /** A draft whose first message is still on its way shows it as it was sent. */
    private fun showSendingMessage() {
        val sending = selectedDraft?.let { sendingDrafts[it.id] } ?: return
        activity = sending.activity
        transcript.changeActivity(activity)
        transcript.setPending(sending.text, sending.attachments)
        transcriptIsEmpty = false
    }

    private fun open(thread: ThreadInfo) {
        openThreadID = thread.id
        activity = Activity.of(thread)
        transcript.begin(thread.id, keepingRows = true)
        core.send("open_thread", json("server_id" to thread.serverID, "thread_id" to thread.id)) { result ->
            if (result.isFailure && transcript.threadID == thread.id) transcript.dropKeptRows()
        }
        core.send("mark_seen", json("thread_id" to thread.id))
        readGit(fetch = true)
    }

    /** What the new thread button does: with one project there is nothing to pick and the draft opens at once, without one it starts without a project, and with more the panel asks which. */
    fun newThread() {
        if (projects.size <= 1) return startNewThread(projects.firstOrNull() ?: noProjects.firstOrNull())
        openPanel(PanelPage.Projects)
    }

    /** Opens an empty draft. The one that was open stays in the sidebar if something was written in it. */
    fun startNewThread(project: Project? = null) {
        openEmptyDraft()
        project?.let { setNewThreadProject(it.id) }
        threadsShown += 1
    }

    /** Opens a thread or a draft and brings it to the front. */
    fun show(selection: Selection) {
        select(selection)
        threadsShown += 1
    }

    /** Where the client goes when what was open is gone. */
    private fun openEmptyDraft() = select(Selection.Draft(emptyDraft().id))

    fun setNewThreadProject(id: String?) {
        moveDraft(id)
        uploadToComposerServer()
        readGit(fetch = true)
    }

    fun discard(draft: ThreadDraft) {
        if (isSending(draft)) return
        val wasOpen = selection == Selection.Draft(draft.id)
        removeDraft(draft.id)
        if (!wasOpen) return
        threadDrafts.lastOrNull { !isSending(it) }?.let { return select(Selection.Draft(it.id)) }
        activeThreads.firstOrNull()?.let { return select(Selection.Thread(it.id)) }
        openEmptyDraft()
    }

    val canSend: Boolean
        get() {
            if (sendingDrafts[draftKey] != null) return false
            val server = composerServer ?: return false
            if (server.state != Server.State.Connected) return false
            if (selectedThread == null && project(selectedDraft?.projectID) == null) return false
            if (!attachments.all { it.state == Attachment.State.Ready && it.serverID == server.id }) return false
            return draft.isNotBlank() || attachments.isNotEmpty()
        }

    /** Why a message with these attachments can't be sent yet. */
    val attachmentsHold: String?
        get() {
            if (attachments.any { it.state is Attachment.State.Failed }) return "Try the attachment that failed again, or remove it"
            return if (attachments.any { it.state != Attachment.State.Ready }) "Waiting for the attachments to upload" else null
        }

    /** Sends what is written. While the agent works, an `alternate` message steers when the setting queues, and queues when it steers. */
    fun send(alternate: Boolean = false) {
        if (!canSend) return
        val text = draft.trim()
        val attached = attachments
        val key = draftKey
        val command = mutableMapOf<String, Any?>("text" to text, "attachments" to attached.mapNotNull { it.path })
        val existing = selectedThread
        val steers = existing != null && (Preferences.optionalBool(STEERS_KEY) ?: STEERS_BY_DEFAULT) != alternate
        if (existing != null) {
            command["server_id"] = existing.serverID
            command["thread_id"] = existing.id
            command["now"] = steers
        } else {
            val draft = selectedDraft
            val project = project(draft?.projectID)
            val model = composerModel
            if (draft == null || project == null || model == null) {
                errorMessage = "No agent is installed. Install Claude Code or Codex on your server and try again."
                return
            }
            val settings = buildMap<String, Any?> {
                put("project_id", project.id)
                put("agent", model.agent.raw)
                put("model", model.id)
                put("agent_account", composerAccount?.id ?: model.agent.raw)
                put("access", draft.access.raw)
                composerEffort?.let { put("effort", it) }
                val base = draftBase
                if (draftUsesWorktree && base != null) put("worktree", mapOf("base" to base))
            }
            command["server_id"] = project.serverID
            command["new_thread"] = settings
            activity = Activity.starting
            transcript.changeActivity(activity)
            sendingDrafts = sendingDrafts + (draft.id to SendingDraft(text, attached.map { it.attached }, activity))
        }
        val serverID = command["server_id"] as String
        this.draft = ""
        attachmentsByKey = attachmentsByKey - key
        // The server queues what is sent while the agent works, unless the message steers it.
        val queued = existing != null && activity.running && !steers
        val shown = attached.map { it.attached }
        transcript.setPending(text, shown, queued)
        readSkillSpans(text) { spans -> transcript.setPendingSkills(spans, text, shown, queued) }
        transcriptIsEmpty = false

        core.send("send", command) { result ->
            if (existing == null) sendingDrafts = sendingDrafts - key
            val error = result.coreError
            if (error != null) {
                // The message goes back to where it was written, wherever the client is now.
                setText(text, key)
                attachmentsByKey = if (attached.isEmpty()) attachmentsByKey - key else attachmentsByKey + (key to attached)
                alert(error)
                if (draftKey != key) return@send
                transcript.setPending(null)
                transcriptIsEmpty = transcript.isEmpty
                if (existing == null) activity = Activity()
                transcript.changeActivity(activity)
                return@send
            }
            if (existing != null) return@send
            openNewThread(result.getOrNull()!!.string("thread_id"), serverID, key)
        }
    }

    /** Replaces a draft with the thread its first message created. If the draft is still open, the thread opens in its place with the message kept on screen. */
    private fun openNewThread(id: String, serverID: String, draftID: String) {
        val wasOpen = selection == Selection.Draft(draftID)
        removeDraft(draftID)
        rightPanel.move(draftID, id)
        if (!wasOpen) return
        selection = Selection.Thread(id)
        openThreadID = id
        transcript.adopt(id)
        core.send("open_thread", json("server_id" to serverID, "thread_id" to id))
        core.send("mark_seen", json("thread_id" to id))
    }

    // Images and videos

    /** The file of an image or a video the open thread shows. The core fetches it from the thread's server if this device doesn't have it. */
    fun media(id: String, done: (File?) -> Unit) = fetchMedia(id) { done(it.getOrNull()) }

    /** The file, or why it can't be had. */
    fun fetchMedia(id: String, done: (Result<File>) -> Unit) {
        val serverID = selectedThread?.serverID ?: composerServer?.id
            ?: return done(Result.failure(CoreError("That server isn't connected. Try again when it is back.")))
        core.send("media", json("server_id" to serverID, "media_id" to id)) { result ->
            done(result.mapCatching { value -> File(value.optionalString("path") ?: throw CoreError("Something went wrong.")) })
        }
    }

    fun cancelMedia(id: String) {
        core.send("cancel_media", json("media_id" to id))
    }

    fun refreshMediaStorage() {
        core.send("storage") { result ->
            val value = result.getOrNull() ?: return@send
            mediaStorage = MediaStorage(value.long("media_bytes"), value.long("media_limit"))
        }
    }

    /** Removes the images and videos kept on this device. The servers still have them. */
    fun clearMedia() {
        core.send("clear_media") { refreshMediaStorage() }
    }

    fun stop() {
        val thread = selectedThread ?: return
        request(thread.serverID, mapOf("type" to "stop", "thread_id" to thread.id))
    }

    /** Allows or refuses a tool call the agent waits with. `answers` is what was chosen, by question, when the call asks questions. */
    fun answer(approval: Approval, allow: Boolean, answers: Map<String, String> = emptyMap()) {
        val thread = selectedThread ?: return
        request(thread.serverID, mapOf("type" to "answer", "thread_id" to thread.id, "approval_id" to approval.id, "allow" to allow, "answers" to answers))
    }

    /** Gives the agent a queued message now, in the turn that runs. */
    fun sendNow(queued: String) {
        val thread = selectedThread ?: return
        request(thread.serverID, mapOf("type" to "send_queued", "thread_id" to thread.id, "message_id" to queued))
    }

    /** Takes a queued message back into the composer of its thread, after what is written there. */
    fun takeBack(queued: String) {
        val thread = selectedThread ?: return
        val message = activity.queued.firstOrNull { it.id == queued } ?: return
        val key = thread.id
        request(thread.serverID, mapOf("type" to "cancel_queued", "thread_id" to thread.id, "message_id" to queued), done = {
            val written = listOf(drafts[key] ?: "", message.text).filter { it.isNotEmpty() }
            setText(written.joinToString("\n\n"), key)
            val back = message.attachments.map { Attachment.of(it, message.media[it], thread.serverID) }
            val attached = (attachmentsByKey[key] ?: emptyList()) + back
            attachmentsByKey = if (attached.isEmpty()) attachmentsByKey - key else attachmentsByKey + (key to attached)
            composerFocus += 1
        })
    }

    fun rename(thread: ThreadInfo, title: String) {
        val trimmed = title.trim()
        if (trimmed.isEmpty() || trimmed == thread.title) return
        update(thread, mapOf("title" to trimmed)) { it.copy(title = trimmed) }
    }

    fun delete(thread: ThreadInfo) = request(thread.serverID, mapOf("type" to "delete", "thread_id" to thread.id))

    /** Has the thread or the draft work with `model` under `account`. A thread that moves to an account without its session goes on in a new one, which is told what was said. */
    fun setModel(model: ModelInfo, account: AgentAccount) {
        val thread = selectedThread ?: return updateDraft { it.copy(model = model.id, agentAccount = account.id) }
        val effort = thread.effort?.takeIf { it in model.efforts }
        val change = buildMap<String, Any?> {
            put("model", model.id)
            put("effort", effort ?: "")
            if (account.id != thread.agentAccount) put("agent_account", account.id)
        }
        update(thread, change) { it.copy(model = model.id, effort = effort, agent = model.agent, agentAccount = account.id) }
    }

    /** Adds an account of an agent on the server, or changes one. `done` says whether it was kept. */
    fun saveAgentAccount(account: AgentAccount, server: Server, done: (Boolean) -> Unit) {
        request(server.id, mapOf("type" to "save_agent_account", "account" to account.json), done = { done(true) }, failed = { done(false) })
    }

    /** Removes an account. Its threads go on with the agent's default account. */
    fun removeAgentAccount(account: AgentAccount, server: Server) = request(server.id, mapOf("type" to "remove_agent_account", "id" to account.id))

    fun setEffort(effort: String) {
        val thread = selectedThread ?: return updateDraft { it.copy(effort = effort) }
        update(thread, mapOf("effort" to effort)) { it.copy(effort = effort) }
    }

    fun setAccess(access: Access) {
        val thread = selectedThread ?: return updateDraft { it.copy(access = access) }
        update(thread, mapOf("access" to access.raw)) { it.copy(access = access) }
    }

    /** Marks threads done or brings them back. A thread that is working or monitoring can't be marked done. */
    fun setDone(ids: List<String>, done: Boolean, fromSidebar: Boolean = false) {
        val changed = ids.mapNotNull { threads[it] }.filter { it.isDone != done && !(done && it.busy) }
        if (changed.isEmpty()) return
        // Leaving the thread that was just put away, for the next one that is still active.
        val open = (selection as? Selection.Thread)?.id
        if (done && fromSidebar && open != null && changed.any { it.id == open }) {
            val active = activeThreads
            val position = active.indexOfFirst { it.id == open }.coerceAtLeast(0)
            val remaining = active.filter { thread -> changed.none { it.id == thread.id } }
            val next = if (remaining.isEmpty()) null else remaining[minOf(position, remaining.size - 1)]
            if (next != null) select(Selection.Thread(next.id)) else openEmptyDraft()
        }
        val now = Time.now()
        for (thread in changed) update(thread, mapOf("done" to done)) { it.copy(doneAt = if (done) now else null) }
        if (!done) {
            undo = null
            return
        }
        showUndo(UndoNotice(changed.map { it.id }, if (changed.size == 1) "Done" else "Marked ${changed.size} threads done"))
    }

    /** Has the agent of the open thread go on with what it was doing when it was interrupted. */
    fun continueThread() {
        val thread = selectedThread ?: return
        request(thread.serverID, mapOf("type" to "continue", "thread_id" to thread.id))
    }

    /** Whether the open thread goes on by itself once its usage limit resets. */
    fun setContinues(on: Boolean) {
        val thread = selectedThread ?: return
        val limit = thread.interruption as? Interruption.Limit ?: return
        update(thread, mapOf("continues" to on)) { it.copy(interruption = Interruption.Limit(limit.resetsAt, on)) }
    }

    fun toggleDone() {
        val thread = selectedThread ?: return
        setDone(listOf(thread.id), !thread.isDone)
    }

    /** Whether the user can move the thread among the active ones. */
    fun moves(thread: ThreadInfo): Boolean = !thread.isDone && server(thread.serverID)?.movesThreads == true

    /** Puts the thread where `index` is among the sidebar's rows without it: `rows` is the active thread of each, or null for a row that isn't one. */
    fun move(thread: ThreadInfo, index: Int, rows: List<ThreadInfo?>) {
        val others = rows.filter { it?.id != thread.id }
        move(thread, others.getOrNull(index - 1), others.getOrNull(index))
    }

    /** Puts the thread between two active ones in the sidebar, or at the top or the bottom when one of them is missing. Its position is kept on its server. */
    private fun move(thread: ThreadInfo, above: ThreadInfo?, below: ThreadInfo?) {
        if (!moves(thread)) return
        val position = when {
            above == null && below == null -> return
            above == null -> maxOf(Time.now(), below!!.position + 1)
            below == null -> above.position - 1
            else -> (above.position + below.position) / 2
        }
        if (position == thread.position) return
        update(thread, mapOf("position" to position)) { it.copy(position = position) }
    }

    private fun showUndo(notice: UndoNotice) {
        undo = notice
        scheduleUndoDismissal()
    }

    /** Keeps the offer to undo while a finger is on it, and gives it its full time again once it lets go. */
    fun holdUndo(held: Boolean) {
        undoHeld = held
        scheduleUndoDismissal()
    }

    private fun scheduleUndoDismissal() {
        undoTimer?.let(main::removeCallbacks)
        if (undo == null || undoHeld) return
        val timer = Runnable { undo = null }
        undoTimer = timer
        main.postDelayed(timer, 5000)
    }

    fun performUndo() {
        val notice = undo ?: return
        undo = null
        setDone(notice.threadIDs, false)
    }

    /** Changes a thread on its server, and here at once so the client doesn't wait for the answer. */
    private fun update(thread: ThreadInfo, change: Map<String, Any?>, locally: (ThreadInfo) -> ThreadInfo) {
        setThread(locally(thread), thread.id)
        request(thread.serverID, mapOf("type" to "update", "thread_id" to thread.id, "change" to change), failed = { setThread(thread, thread.id) })
    }

    private fun request(serverID: String, request: Map<String, Any?>, done: (() -> Unit)? = null, failed: (() -> Unit)? = null) {
        core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            if (result.isSuccess) {
                done?.invoke()
            } else {
                failed?.invoke()
                alert(result)
            }
        }
    }

    // Attachments

    /** Adds the files to what is being written and starts sending them to its server, so they are there when the message is sent. */
    fun attach(files: List<File>) {
        val server = composerServer ?: return
        val key = draftKey
        for (file in files) {
            if (attachments.any { it.file == file }) continue
            val attachment = Attachment.of(file, server.id)
            attachmentsByKey = attachmentsByKey + (key to (attachmentsByKey[key] ?: emptyList()) + attachment)
            upload(attachment.id)
        }
        composerFocus += 1
    }

    fun removeAttachment(id: String) {
        if (attachments.firstOrNull { it.id == id }?.state is Attachment.State.Uploading) core.send("cancel_upload", json("key" to id))
        val left = attachments.filter { it.id != id }
        attachmentsByKey = if (left.isEmpty()) attachmentsByKey - draftKey else attachmentsByKey + (draftKey to left)
    }

    fun retryAttachment(id: String) = upload(id)

    private fun changeAttachment(id: String, change: (Attachment) -> Attachment) {
        for ((key, attached) in attachmentsByKey) {
            val index = attached.indexOfFirst { it.id == id }
            if (index < 0) continue
            attachmentsByKey = attachmentsByKey + (key to attached.toMutableList().also { it[index] = change(it[index]) })
            return
        }
    }

    private fun upload(id: String) {
        val attachment = attachmentsByKey.values.flatten().firstOrNull { it.id == id } ?: return
        val file = attachment.file ?: return
        val serverID = attachment.serverID
        changeAttachment(id) { it.copy(state = Attachment.State.Uploading(0.0)) }
        core.send("upload", json("server_id" to serverID, "key" to id, "file" to file.path)) { result ->
            val value = result.getOrNull()
            if (value == null) {
                changeAttachment(id) { current ->
                    // An upload that was stopped to go to another server is on its way there.
                    if (current.serverID != serverID) current else current.copy(state = Attachment.State.Failed(result.coreError?.message ?: ""))
                }
                return@send
            }
            val path = value.string("path")
            val media = value.optionalString("media")
            changeAttachment(id) { it.copy(path = path, media = media) }
            if (!attachment.video || media == null) return@send changeAttachment(id) { it.copy(state = Attachment.State.Ready) }
            uploadPoster(id, file, path, serverID)
        }
    }

    /** Sends the first frame of a video after it, which stands for it wherever it isn't played. A video without one is sent all the same. */
    private fun uploadPoster(id: String, file: File, path: String, serverID: String) {
        val ready = { poster: String? ->
            changeAttachment(id) { if (it.path != path) it else it.copy(poster = poster, state = Attachment.State.Ready) }
        }
        thread(isDaemon = true) {
            val poster = Pictures.firstFrame(file, 1280)?.let { Pictures.writeJPEG(it, File(context.cacheDir, "posters")) }
            main.post {
                if (poster == null) return@post ready(null)
                val upload = json("server_id" to serverID, "key" to "$id.poster", "file" to poster.path, "poster_of" to path)
                core.send("upload", upload) { result ->
                    poster.delete()
                    ready(result.getOrNull()?.optionalString("media"))
                }
            }
        }
    }

    /** Sends the open draft's files again when its project is on another server than they are. */
    private fun uploadToComposerServer() {
        val server = composerServer ?: return
        for (attachment in attachments) {
            if (attachment.serverID == server.id || attachment.file == null) continue
            core.send("cancel_upload", json("key" to attachment.id))
            changeAttachment(attachment.id) { it.copy(serverID = server.id, path = null, media = null, poster = null) }
            upload(attachment.id)
        }
    }

    // Viewer

    fun view(media: List<ViewedMedia>, index: Int) {
        if (index !in media.indices) return
        viewing = Viewing(media, index)
    }

    /** Shows the image or video before or after the one that is shown, around the ends. */
    fun viewNext(step: Int) {
        val viewing = viewing ?: return
        val count = viewing.items.size
        if (count <= 1) return
        this.viewing = viewing.copy(index = (viewing.index + step + count) % count)
    }

    fun closeViewer() {
        viewing = null
        composerFocus += 1
    }

    companion object {
        /** Whether a message sent while the agent works steers the turn that runs instead of waiting for it. */
        const val STEERS_KEY = "send.steers"
        /** It steers until the setting says otherwise. */
        const val STEERS_BY_DEFAULT = true

        fun preview(text: String, attachments: Int): String? {
            val trimmed = text.trim()
            trimmed.split('\n').firstOrNull()?.takeIf { it.isNotEmpty() }?.let { return it }
            if (attachments <= 0) return null
            return if (attachments == 1) "1 attachment" else "$attachments attachments"
        }
    }
}
