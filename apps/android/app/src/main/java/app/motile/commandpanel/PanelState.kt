package app.motile.commandpanel

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.core.AppStore
import app.motile.core.FolderListing
import app.motile.core.GitHubState
import app.motile.core.PanelPage
import app.motile.core.Project
import app.motile.core.Repo
import app.motile.core.RestartWhen
import app.motile.core.Server
import app.motile.core.ThreadInfo
import app.motile.core.Time
import app.motile.core.coreError
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.ui.AgentLogo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The command panel's pages, what is typed, the highlight and the folders browsed, and what each
 * page offers. Pages are pushed on `pages`; outside a stack the store is told the one in front.
 */
internal class PanelState(start: PanelPage, private val store: AppStore, private val scope: CoroutineScope, val inStack: Boolean) {
    val pages = mutableStateListOf(start)
    var query by mutableStateOf("")
        private set
    var highlighted by mutableIntStateOf(0)
    /** Whether the keys have moved the highlight. A finger picks without one. */
    var steered by mutableStateOf(false)
        private set
    /** The folders under the path typed on the folder page. Missing while they are asked for. */
    var listing by mutableStateOf<FolderListing?>(null)
        private set
    var browseError by mutableStateOf<String?>(null)
        private set
    private var browsesAsked = 0
    private var browsesAnswered = 0
    var copied by mutableStateOf(false)
        private set
    /** The last move pushed a page rather than went back. */
    var forward by mutableStateOf(true)
        private set
    /** What closes the panel in a stack: leaves it for the page it was pushed from. */
    var leave: () -> Unit = {}

    val page: PanelPage get() = pages.lastOrNull() ?: PanelPage.Commands

    // Moving

    fun type(new: String) {
        if (new == query) return
        query = new
        highlighted = 0
        if (store.panelNotice != null) store.panelNotice = null
        browse()
    }

    fun open(page: PanelPage) {
        forward = true
        pages.add(page)
        arrive()
    }

    fun back() {
        if (pages.size <= 1) return
        forward = false
        pages.removeAt(pages.lastIndex)
        arrive()
    }

    /** Starts over on the page the store opened the panel on. */
    fun restart(page: PanelPage) {
        forward = true
        pages.clear()
        pages.add(page)
        arrive()
    }

    fun close() {
        if (!inStack) return store.closePanel()
        leave()
    }

    /** Moves between the repositories and what GitHub still needs as the server's answer changes. */
    fun followGitHub() {
        val replaced = when (val page = page) {
            is PanelPage.GitHub -> if (store.github[page.serverID] != GitHubState.Ready) PanelPage.GitHubSetup(page.serverID) else null
            is PanelPage.GitHubSetup -> if (store.github[page.serverID] == GitHubState.Ready) PanelPage.GitHub(page.serverID) else null
            else -> null
        } ?: return
        pages[pages.lastIndex] = replaced
        arrive()
    }

    fun arrive() {
        if (!inStack) store.goToPanelPage(page)
        highlighted = 0
        store.panelNotice = null
        copied = false
        listing = null
        browseError = null
        val page = page
        val start = when (page) {
            is PanelPage.Folder -> "~/"
            is PanelPage.Icon -> (store.project(page.projectID)?.path ?: "~") + "/"
            else -> ""
        }
        when (page) {
            is PanelPage.GitHub -> store.loadRepos(page.serverID)
            is PanelPage.Sources -> if (store.github[page.serverID] == GitHubState.Ready) store.loadRepos(page.serverID)
            else -> Unit
        }
        if (start == query) return
        query = start
        browse()
    }

    fun steer(step: Int) {
        val count = sections(page).flatMap { it.items }.count { it.selectable }
        highlighted = if (steered) (highlighted + step).coerceIn(0, maxOf(0, count - 1)) else 0
        steered = true
    }

    fun runHighlighted() {
        val item = sections(page).flatMap { it.items }.firstOrNull { it.index == highlighted } ?: return
        run(item)
    }

    fun run(item: PanelItem) {
        if (!item.selectable) return
        if (!item.keepsOpen) close()
        item.action()
    }

    /** Asks for the folders under the typed path. Placeholders take the place of the folders that are shown when the answer takes a while. */
    private fun browse() {
        val (id, icons) = browsed(page) ?: return
        browsesAsked += 1
        val asked = browsesAsked
        store.browse(id, query, icons) { result ->
            if (asked != browsesAsked) return@browse
            browsesAnswered = asked
            result.onSuccess {
                listing = it
                browseError = null
            }
            result.onFailure {
                listing = null
                browseError = result.coreError?.message ?: it.message
            }
        }
        scope.launch {
            delay(150)
            if (asked != browsesAsked || browsesAnswered >= asked) return@launch
            listing = null
            browseError = null
        }
    }

    /** The server whose folders the page lists, and whether with the images that can be an icon. */
    fun browsed(page: PanelPage): Pair<String, Boolean>? = when (page) {
        is PanelPage.Folder -> page.serverID to false
        is PanelPage.Icon -> store.project(page.projectID)?.let { it.serverID to true }
        else -> null
    }

    // What is shown

    fun title(page: PanelPage): String = when (page) {
        PanelPage.Commands -> "Commands"
        PanelPage.Projects -> "New Thread"
        PanelPage.DraftProject -> "Project"
        PanelPage.Threads -> "Threads"
        PanelPage.Servers, is PanelPage.Sources -> "Add a Project"
        is PanelPage.NewProject -> "New Project"
        is PanelPage.GitHub -> "Your GitHub"
        is PanelPage.GitHubSetup -> "GitHub"
        is PanelPage.Folder -> "Local Folder"
        is PanelPage.Icon -> "Project Icon"
    }

    fun prompt(page: PanelPage): String = when (page) {
        PanelPage.Commands -> "Search threads, projects and commands"
        PanelPage.Projects -> "Start a thread in"
        PanelPage.DraftProject -> "Search projects"
        PanelPage.Threads -> "Go to thread"
        PanelPage.Servers -> "Add a project on"
        is PanelPage.Sources -> "Add a project"
        is PanelPage.NewProject -> "Project name"
        is PanelPage.GitHub -> "Search your repositories"
        is PanelPage.GitHubSetup -> "Set up GitHub"
        is PanelPage.Folder -> "Path on ${serverName(page.serverID)}"
        is PanelPage.Icon -> "Path to an image on ${serverName(store.project(page.projectID)?.serverID ?: "")}"
    }

    /** What a server refused, or why it couldn't list the repositories again. */
    fun notice(page: PanelPage): String? {
        if (page !is PanelPage.GitHub || store.repos[page.serverID] == null) return store.panelNotice
        return store.panelNotice ?: store.repoErrors[page.serverID]
    }

    fun emptyText(page: PanelPage): String = when (page) {
        is PanelPage.Folder -> browseError ?: "No folders found"
        is PanelPage.Icon -> browseError ?: "No folders or images found"
        is PanelPage.GitHub -> store.repoErrors[page.serverID] ?: "No repository found"
        else -> "Nothing found"
    }

    // What is offered

    fun sections(page: PanelPage): List<PanelSection> {
        var narrows = true
        val sections = when (page) {
            PanelPage.Projects -> listOf(PanelSection("Projects", projectItems { store.startNewThread(it) } + addProject))
            PanelPage.DraftProject -> listOf(PanelSection("Projects", projectItems { store.setNewThreadProject(it.id) } + addProject))
            PanelPage.Threads -> listOf(PanelSection("Threads", threadItems))
            // The sheet hides the thread, so its actions stay on the thread's screen. Searching from here looks through everything.
            PanelPage.Commands -> buildList {
                add(PanelSection("Commands", commands))
                if (query.isEmpty()) return@buildList
                add(PanelSection("Threads", threadItems))
                add(PanelSection("Start a thread in", projectItems { store.startNewThread(it) }))
            }
            PanelPage.Servers -> listOf(PanelSection("Servers", serverItems))
            is PanelPage.Sources -> {
                val title = if (store.servers.size > 1) "Add a project on ${serverName(page.serverID)}" else "Add a project"
                listOf(PanelSection(title, sourceItems(page.serverID)))
            }
            is PanelPage.NewProject -> {
                narrows = false
                listOf(PanelSection("New project", listOf(newProject(page.serverID))))
            }
            is PanelPage.GitHub -> {
                narrows = false
                listOf(PanelSection("Your GitHub", repoItems(page.serverID)))
            }
            is PanelPage.GitHubSetup -> {
                val name = serverName(page.serverID)
                val missing = store.github[page.serverID] == GitHubState.Missing
                val title = if (missing) "GitHub's gh isn't installed on $name" else "GitHub isn't signed in on $name"
                listOf(PanelSection(title, setupItems(page.serverID)))
            }
            is PanelPage.Folder -> {
                narrows = false
                listOf(PanelSection("Folders on ${serverName(page.serverID)}", folderItems(page.serverID)))
            }
            is PanelPage.Icon -> {
                narrows = false
                listOf(PanelSection("Icon for ${store.project(page.projectID)?.name ?: "the project"}", iconItems(page.projectID)))
            }
        }
        var index = 0
        return sections.mapNotNull { section ->
            val items = if (narrows) section.items.filter(::matches) else section.items
            if (items.isEmpty()) return@mapNotNull null
            for (item in items) {
                if (!item.selectable) continue
                item.index = index
                index += 1
            }
            PanelSection(section.title, items)
        }
    }

    private fun matches(item: PanelItem): Boolean {
        if (query.isEmpty() || item.placeholderLines != 0) return true
        return item.title.contains(query, ignoreCase = true) || item.detail.contains(query, ignoreCase = true)
    }

    private fun serverName(id: String): String = store.server(id)?.name ?: "your server"

    /** The projects, the one the open thread works in first, then by when a thread last started, and "No project" of each server last. */
    private val projects: List<Project>
        get() {
            val recent = store.recentProjects
            val current = store.composerProject
            if (current == null || store.projects.none { it.id == current.id }) return recent + store.noProjects
            return listOf(current) + recent.filter { it.id != current.id } + store.noProjects
        }

    private fun projectItems(pick: (Project) -> Unit): List<PanelItem> = projects.map { project ->
        val server = store.server(project.serverID)?.shortName ?: ""
        val location = if (project.noProject) "A folder of its own for each thread" else project.path
        PanelItem("project-${project.id}", project.name, "$server $location", PanelIcon.Project(project)) { pick(project) }.apply {
            detailParts = if (server.isEmpty()) listOf(Symbol.Folder to location) else listOf(Symbol.Server to server, Symbol.Folder to location)
        }
    }

    private val addProject: PanelItem
        get() = PanelItem("add-project", "Add a project", "A new one, one of your GitHub's or a folder", PanelIcon.Glyph(Symbol.FolderPlus), keepsOpen = true) {
            open(store.addProjectPage)
        }

    /** The servers a project can be added on: the open thread's first, the offline ones last. */
    private val serverItems: List<PanelItem>
        get() {
            val current = store.composerServer?.id
            fun rank(server: Server) = when {
                server.state != Server.State.Connected -> 2
                server.id == current -> 0
                else -> 1
            }
            return store.servers.withIndex().sortedWith(compareBy({ rank(it.value) }, { it.index })).map { (_, server) ->
                val count = store.projects.count { it.serverID == server.id }
                val offline = server.state != Server.State.Connected
                val detail = if (offline) "Offline" else if (count == 1) "1 project" else "$count projects"
                PanelItem("server-${server.id}", server.name, detail, PanelIcon.Glyph(Symbol.Server), keepsOpen = true) {
                    open(PanelPage.Sources(server.id))
                }.apply { off = offline }
            }
        }

    /** GitHub comes last while it still needs setting up on the server. */
    private fun sourceItems(id: String): List<PanelItem> {
        val folder = PanelItem("source-folder", "Local folder", "Browse the folders on ${serverName(id)}", PanelIcon.Glyph(Symbol.Folder), keepsOpen = true) {
            open(PanelPage.Folder(id))
        }
        if (!store.startsProjects(store.server(id))) return listOf(folder)
        val new = PanelItem("source-new", "New project", "Start a new Git repository from a name", PanelIcon.Glyph(Symbol.SquarePlus), keepsOpen = true) {
            open(PanelPage.NewProject(id))
        }
        val state = store.github[id] ?: return listOf(new, PanelItem.placeholder(0, lines = 2), folder)
        val github = PanelItem("source-github", "Your GitHub", "Clone one of your repositories", PanelIcon.Logo(AgentLogo.github), keepsOpen = true) {
            open(if (state == GitHubState.Ready) PanelPage.GitHub(id) else PanelPage.GitHubSetup(id))
        }
        if (state == GitHubState.Ready) return listOf(new, github, folder)
        github.note = "Setup required"
        github.warns = true
        return listOf(new, folder, github)
    }

    private fun newProject(id: String): PanelItem {
        val name = query.trim()
        val title = if (name.isEmpty()) "Name the project" else "Create $name"
        return PanelItem("create", title, "A new Git repository in ~/projects on ${serverName(id)}", PanelIcon.Glyph(Symbol.SquarePlus), keepsOpen = true) {
            store.newProject(name, id)
        }.apply {
            off = name.isEmpty()
            busy = store.addingProject == app.motile.core.AddingProject(id, name)
        }
    }

    private fun repoItems(id: String): List<PanelItem> {
        val repos = store.repos[id] ?: return if (store.repoErrors[id] == null) (0 until 8).map { PanelItem.placeholder(it, lines = 2) } else emptyList()
        fun clone(name: String, title: String, detail: String, symbol: Symbol) =
            PanelItem("repo-$name", title, detail, PanelIcon.Glyph(symbol)) { store.clone(name, id) }
        val typed = query.trim()
        val items = found(repos, typed.lowercase()).map { repo ->
            clone(repo.name, repo.name, repo.description ?: "", Symbol.BookMarked).apply { note = if (repo.isPrivate) "Private" else null }
        }.toMutableList()
        // A repository that isn't listed is cloned by its name.
        val listed = repos.any { it.name.equals(typed, ignoreCase = true) }
        if (!listed && repoName.matches(typed)) {
            items.add(clone(typed, "Clone $typed", "A repository that isn't in your list", Symbol.CircleArrowDown))
        }
        return items
    }

    /** The repositories that answer the search, the best first and the last pushed among equals. */
    private fun found(repos: List<Repo>, search: String): List<Repo> {
        if (search.isEmpty()) return repos
        return repos.withIndex()
            .mapNotNull { (position, repo) -> repo.rank(search)?.let { Triple(it, position, repo) } }
            .sortedWith(compareBy({ it.first }, { it.second }))
            .map { it.third }
    }

    private fun setupItems(id: String): List<PanelItem> {
        val name = serverName(id)
        val check = PanelItem("check-github", "Check again", "Once that is done", PanelIcon.Glyph(Symbol.RotateCw), keepsOpen = true) {
            store.readGitHub(id)
        }
        if (store.github[id] == GitHubState.Missing) {
            val install = PanelItem(
                "install-gh", "Open cli.github.com", "Install gh on $name, then run gh auth login there", PanelIcon.Glyph(Symbol.SquareArrowOutUpRight), keepsOpen = true,
            ) { Platform.open("https://cli.github.com") }
            return listOf(install, check)
        }
        val copy = PanelItem(
            "copy-login", if (copied) "Copied" else "Copy gh auth login", "Run it in a terminal on $name", PanelIcon.Glyph(if (copied) Symbol.Check else Symbol.Copy), keepsOpen = true,
        ) {
            Platform.copy("gh auth login")
            copied = true
        }
        return listOf(copy, check)
    }

    /** The folder that is typed, the one above it, and the folders in it. */
    private fun folderItems(id: String): List<PanelItem> {
        val listing = listing ?: return if (browseError == null) (0 until 8).map { PanelItem.placeholder(it, lines = 1) } else emptyList()
        val projects = store.projects.filter { it.serverID == id }.map { it.path }.toSet()
        val items = mutableListOf<PanelItem>()
        if (query.endsWith("/") || query == "~") {
            items.add(PanelItem("add-here", "Add this folder", listing.typed, PanelIcon.Glyph(Symbol.FolderPlus)) { store.addProject(id, listing.path) })
            listing.parent?.let { parent -> items.add(PanelItem("parent", "..", "", PanelIcon.Glyph(Symbol.CornerLeftUp), keepsOpen = true) { type(parent) }) }
        }
        items += listing.folders.map { folder ->
            PanelItem(folder.path, folder.name, "", PanelIcon.Glyph(Symbol.Folder), keepsOpen = true) { type(folder.typed) }.apply {
                note = if (folder.path in projects) "Project" else null
            }
        }
        return items
    }

    /** The icon in the project's folder, the folder above the typed one, and the folders and images in it. */
    private fun iconItems(projectID: String): List<PanelItem> {
        val project = store.project(projectID) ?: return emptyList()
        val listing = listing ?: return if (browseError == null) (0 until 8).map { PanelItem.placeholder(it, lines = 1) } else emptyList()
        val items = mutableListOf<PanelItem>()
        if (query.endsWith("/") || query == "~") {
            items.add(PanelItem("folder-icon", "Use the icon in its folder", project.path, PanelIcon.Glyph(Symbol.Undo2)) { store.setIcon(project, null) })
            listing.parent?.let { parent -> items.add(PanelItem("parent", "..", "", PanelIcon.Glyph(Symbol.CornerLeftUp), keepsOpen = true) { type(parent) }) }
        }
        items += listing.folders.map { folder ->
            PanelItem(folder.path, folder.name, "", PanelIcon.Glyph(Symbol.Folder), keepsOpen = true) { type(folder.typed) }
        }
        items += listing.images.map { (name, path) ->
            PanelItem(path, name, "", PanelIcon.Glyph(Symbol.Image)) { store.setIcon(project, path) }
        }
        return items
    }

    /** The threads, then the ones agents started, each saying which thread started it. */
    private val threadItems: List<PanelItem>
        get() {
            val started = store.startedThreads.values.sortedByDescending { it.updatedAt }
            return (store.activeThreads + store.doneThreads + started).map(::threadItem)
        }

    private fun threadItem(thread: ThreadInfo): PanelItem {
        val project = store.project(thread.projectID)
        val name = project?.name ?: thread.cwd.trimEnd('/').substringAfterLast('/')
        val from = thread.parentID?.let { store.anyThread(it)?.title }?.let { " · from $it" } ?: ""
        val waitsFor = thread.waitsFor
        val state = when {
            thread.isDone -> "done"
            waitsFor != null -> waitsFor.phrase
            thread.working -> "working"
            thread.monitoring -> "monitoring"
            else -> thread.interruption?.word ?: Time.ago(thread.updatedAt)
        }
        return PanelItem("thread-${thread.id}", thread.title, "$name$from · $state", PanelIcon.Project(project)) {
            store.show(app.motile.core.Selection.Thread(thread.id))
        }
    }

    /** The servers that run an older version than the newest release. One whose agents work is updated once they finish, or now, with their threads going on after. */
    private val serverUpdates: List<PanelItem>
        get() = store.servers.filter { store.isOutdated(it) && store.serverUpdate(it) == null }.flatMap { server ->
            val detail = "From version ${server.version} to ${store.updater.latest ?: ""}"
            val symbol = PanelIcon.Glyph(Symbol.CircleArrowDown)
            if (!store.isBusy(server) || !store.canChooseRestart(server)) {
                return@flatMap listOf(PanelItem("update-${server.id}", "Update ${server.name}", detail, symbol) { store.update(server) })
            }
            listOf(
                PanelItem("update-${server.id}", "Update ${server.name} when agents finish", detail, symbol) { store.update(server, RestartWhen.Idle) },
                PanelItem("update-now-${server.id}", "Update ${server.name} now", "Its agents stop and continue once it is back", symbol) {
                    store.update(server, RestartWhen.Now)
                },
            )
        }

    private val commands: List<PanelItem>
        get() {
            fun PanelItem.keyed(command: String) = apply { keys = store.shortcuts.label(command) }
            val always = listOf(
                PanelItem("new-thread", "New thread", "Choose a project to start in", PanelIcon.Glyph(Symbol.SquarePen), keepsOpen = true) {
                    open(PanelPage.Projects)
                }.keyed("chat.new"),
                PanelItem("go-to-thread", "Go to thread", "${store.threads.size + store.startedThreads.size} threads", PanelIcon.Glyph(Symbol.MessageSquareText), keepsOpen = true) {
                    open(PanelPage.Threads)
                }.keyed("threadPicker.toggle"),
                addProject,
                PanelItem("add-server", "Add a server", "A machine that runs your agents", PanelIcon.Glyph(Symbol.Server)) {
                    store.showsAddServer = true
                },
            )
            val settings = PanelItem("settings", "Settings", "Appearance, servers and projects", PanelIcon.Glyph(Symbol.Settings)) {
                store.openSettings()
            }.keyed("settings.open")
            val usage = PanelItem("usage", "Usage", "Limits, cost and tokens", PanelIcon.Glyph(Symbol.ChartColumn)) {
                store.openUsage()
            }.keyed("usage.open")
            return serverUpdates + always + settings + usage
        }

    private companion object {
        val repoName = Regex("""[\w.-]+/[\w.-]+""")
    }
}

/** The server whose repositories the page lists. */
internal val PanelPage.githubID: String? get() = (this as? PanelPage.GitHub)?.serverID

/** It asks for something to be typed rather than searched. */
internal val PanelPage.isTyped: Boolean
    get() = this is PanelPage.NewProject || this is PanelPage.Folder || this is PanelPage.Icon
