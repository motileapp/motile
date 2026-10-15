package app.motile.panel

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import app.motile.core.JSON
import app.motile.core.Loaded
import app.motile.core.Pictures
import app.motile.core.ViewedMedia
import app.motile.core.bool
import app.motile.core.coreError
import app.motile.core.long
import app.motile.core.objects
import app.motile.core.string
import app.motile.core.strings
import java.io.File

data class FileEntry(val name: String, val folder: Boolean, val ignored: Boolean)

/** A row of the files tab: a file or a folder, as deep as the folders above it. */
data class FileNode(val path: String, val name: String, val folder: Boolean, val ignored: Boolean, val depth: Int, val open: Boolean)

/** What a file's tab shows: its lines, or what a turn changed in it, an image, a video, or what it is when it is none of those. */
sealed interface FileContent {
    class Text(val document: CodeDocument, val truncated: Boolean) : FileContent
    class Image(val picture: ImageBitmap) : FileContent
    class Video(val file: File) : FileContent
    class Binary(val size: Long) : FileContent
}

/** The image or the video a diff shows, as fetched: where it is on this device and its picture. */
private class FetchedMedia(val file: File?, val picture: ImageBitmap?)

/** What the changes, files and file tabs of the open thread show, read from its server. All of it is used on the main thread. */
class PanelContent(val panel: RightPanel) {
    var diff by mutableStateOf<Loaded<CodeDocument>>(Loaded.Loading)
        private set
    /** The files of the diff that are closed, by path. */
    var collapsed by mutableStateOf(setOf<String>())
        private set

    /** What each folder that was looked into has in it, by its path. */
    var listings by mutableStateOf(mapOf<String, List<FileEntry>>())
        private set
    var openFolders by mutableStateOf(setOf<String>())
        private set
    var filesError by mutableStateOf<String?>(null)
        private set
    /** What the tabs of one file show: the file, or what a turn changed in it. */
    var contents by mutableStateOf(mapOf<PanelTab, Loaded<FileContent>>())
        private set
    /** The tabs whose content is being read, whose button to read it again turns meanwhile. */
    var reading by mutableStateOf(setOf<PanelTab>())
        private set

    /** The folder all of the above is of. */
    private var shown: PanelTarget? = null
    private var shownScope: DiffScope? = null
    /** Where the images and videos diffs show are on this device, by their blobs. */
    private val mediaFiles = HashMap<String, File>()
    /** The pictures of the images and videos diffs showed last, by their blobs. */
    private val pictures = object : LinkedHashMap<String, ImageBitmap>(16, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ImageBitmap>?) = size > 48
    }
    private var diffRequest = 0L
    private var fileRequests = mapOf<Long, PanelTab>()

    private val store get() = panel.store

    // Diff

    /** Asks the server for the diff. What is shown stays until the answer is there, unless it is of another folder or scope. */
    fun loadDiff(target: PanelTarget, scope: DiffScope) {
        val store = store ?: return
        look(target)
        if (shownScope != scope) {
            shownScope = scope
            diff = Loaded.Loading
            collapsed = emptySet()
        }
        val command = target.request + mapOf("server_id" to target.serverID, "scope" to scope.request)
        val id = "${target.key}/$scope"
        val fresh = diff.value?.id != id
        reading = reading + PanelTab.Diff
        diffRequest = store.core.send("diff", command, read = { CodeDocument.diff(it, id) }) { result ->
            if (shown != target || shownScope != scope) return@send
            reading = reading - PanelTab.Diff
            val document = result.getOrNull()
            if (document == null) {
                diff = Loaded.Failed(result.coreError?.message ?: "Something went wrong.")
                return@send
            }
            if (fresh) {
                val long = document.files.filter { it.lines.size > CodeFile.OPEN_UP_TO_LINES }.map { it.path }
                collapsed = long.toSet() + viewedPaths(scope)
            }
            diff = Loaded.Ready(document)
        }
    }

    /** The files of the pull request the user has marked as viewed, which its diff shows closed. */
    private fun viewedPaths(scope: DiffScope): Set<String> {
        if (scope !is DiffScope.PullRequest) return emptySet()
        val page = panel.pullRequests.page.value ?: return emptySet()
        if (page.number != scope.number) return emptySet()
        return page.viewed.filterValues { it }.keys
    }

    /** Marks the file viewed, or not, and closes or opens it in the diff with that. */
    fun setViewed(path: String, viewed: Boolean, target: PanelTarget, number: Int) {
        collapse(path, viewed)
        panel.pullRequests.edit(mapOf("kind" to "viewed", "path" to path, "viewed" to viewed), target = target, number = number)
    }

    private fun collapse(path: String, closed: Boolean) {
        collapsed = if (closed) collapsed + path else collapsed - path
    }

    fun toggleCollapsed(path: String) = collapse(path, path !in collapsed)

    fun setAllCollapsed(closed: Boolean) {
        collapsed = if (closed) diff.value?.files?.map { it.path }?.toSet() ?: emptySet() else emptySet()
    }

    // Files

    /** The rows of the files tab: every folder that is open with what is in it. */
    val nodes: List<FileNode>
        get() {
            val nodes = mutableListOf<FileNode>()
            fun list(folder: String, depth: Int) {
                for (entry in listings[folder] ?: return) {
                    val path = if (folder.isEmpty()) entry.name else "$folder/${entry.name}"
                    val open = entry.folder && path in openFolders
                    nodes += FileNode(path, entry.name, entry.folder, entry.ignored, depth, open)
                    if (open) list(path, depth + 1)
                }
            }
            list("", 0)
            return nodes
        }

    /** Reads the folders that have been looked into again, the folder itself first. */
    fun loadFiles(target: PanelTarget) {
        look(target)
        for (folder in setOf("") + listings.keys) list(folder, target)
    }

    fun toggleFolder(path: String) {
        if (path in openFolders) {
            openFolders = openFolders - path
            return
        }
        openFolders = openFolders + path
        val shown = shown ?: return
        if (listings[path] == null) list(path, shown)
    }

    private fun list(folder: String, target: PanelTarget) {
        val store = store ?: return
        val request = target.request + mapOf("type" to "list_files", "path" to folder)
        if (folder.isEmpty()) reading = reading + PanelTab.Files
        store.core.send("request", mapOf("server_id" to target.serverID, "request" to request)) { result ->
            if (shown != target) return@send
            if (folder.isEmpty()) reading = reading - PanelTab.Files
            val answer = result.getOrNull()
            if (answer == null) {
                // A folder that has gone closes; only the folder itself says what went wrong.
                listings = listings - folder
                openFolders = openFolders - folder
                if (folder.isEmpty()) filesError = result.coreError?.message
                return@send
            }
            val entries = answer.objects("entries").map { FileEntry(it.string("name"), it.bool("folder"), it.bool("ignored")) }
            if (listings[folder] != entries) listings = listings + (folder to entries)
            if (folder.isEmpty()) filesError = null
        }
    }

    /** Asks the server for the file. What is shown of it stays until the answer is there. */
    fun loadFile(path: String, target: PanelTarget) {
        val id = "${target.key}/$path"
        load(PanelTab.File(path), "file", mapOf("path" to path), target) { readFile(it, path, id) }
    }

    /** Fetches the image or the video a diff shows in place of the file's lines, and decodes its picture off the main thread. */
    fun loadMedia(file: CodeFile, target: PanelTarget) {
        val store = store ?: return
        val media = file.media ?: return
        if (media.asked) return
        media.asked = true
        pictures[media.blob]?.let {
            media.picture = it
            return
        }
        val command = target.request + mapOf("server_id" to target.serverID, "path" to file.path, "blob" to media.blob)
        store.core.send("file", command, read = { answer -> fetched(answer, media.video) }) { result ->
            val fetched = result.getOrNull()
            fetched?.file?.let { mediaFiles[media.blob] = it }
            val picture = fetched?.picture
            if (picture == null) {
                media.failed = true
                return@send
            }
            pictures[media.blob] = picture
            media.picture = picture
        }
    }

    private fun fetched(answer: JSON, video: Boolean): FetchedMedia {
        val path = answer.string("file")
        if (path.isEmpty()) return FetchedMedia(null, null)
        val file = File(path)
        val bitmap = if (video) Pictures.firstFrame(file, MEDIA_PIXELS) else Pictures.decode(file, MEDIA_PIXELS)
        return FetchedMedia(file, bitmap?.asImageBitmap())
    }

    /** Opens the image or the video a diff shows in the viewer, once it is on this device. */
    fun viewMedia(file: CodeFile) {
        val media = file.media ?: return
        val local = mediaFiles[media.blob] ?: return
        val name = file.path.substringAfterLast('/')
        store?.view(listOf(ViewedMedia(name, media.video, ViewedMedia.Source.LocalFile(local))), 0)
    }

    /** Asks the server for what the turn changed in the file. */
    fun loadChange(turn: String, path: String, target: PanelTarget) {
        val id = "${target.key}/$turn/$path"
        load(PanelTab.Change(turn, path), "diff", mapOf("scope" to DiffScope.Turn(turn).request, "path" to path), target) {
            FileContent.Text(CodeDocument.diff(it, id), it.bool("truncated"))
        }
    }

    private fun load(tab: PanelTab, type: String, fields: Map<String, Any?>, target: PanelTarget, read: (JSON) -> FileContent) {
        val store = store ?: return
        look(target)
        if (contents[tab] == null) contents = contents + (tab to Loaded.Loading)
        val command = target.request + fields + mapOf("server_id" to target.serverID)
        reading = reading + tab
        val request = store.core.send(type, command, read = read) { result ->
            if (shown != target) return@send
            reading = reading - tab
            val content = result.getOrNull()
            val loaded = if (content != null) Loaded.Ready(content) else Loaded.Failed(result.coreError?.message ?: "Something went wrong.")
            contents = contents + (tab to loaded)
        }
        fileRequests = fileRequests.filterValues { it != tab } + (request to tab)
    }

    /** What a closed tab showed is let go of. */
    fun forget(tab: PanelTab) {
        if (tab in contents) contents = contents - tab
    }

    /** The tabs of a draft went to the thread it became, whose folder is another. */
    fun moved(from: String) {
        if (shown?.key == from) shown = null
    }

    // Both

    /** Forgets what was shown when the folder is another one than before. */
    private fun look(target: PanelTarget) {
        if (shown == target) return
        val sameFolder = shown?.key == target.key
        shown = target
        // What is read for another target never answers here.
        reading = emptySet()
        if (sameFolder) return
        shownScope = null
        diff = Loaded.Loading
        collapsed = emptySet()
        listings = emptyMap()
        openFolders = emptySet()
        filesError = null
        contents = emptyMap()
        fileRequests = emptyMap()
    }

    /** The highlighting of what a request answered with has arrived. */
    fun colour(request: Long, file: Int, lines: List<IntArray>) {
        val document = if (request == diffRequest) {
            diff.value
        } else {
            val tab = fileRequests[request]
            (contents[tab]?.value as? FileContent.Text)?.document
        } ?: return
        val shown = document.files.getOrNull(file) ?: return
        if (shown.lines.size != lines.size) return
        shown.spans = lines
    }

    private companion object {
        /** The pictures of images and videos are decoded no larger than this on a side. */
        const val MEDIA_PIXELS = 1600
        /** An image a file tab shows is decoded no larger than this on a side. */
        const val IMAGE_PIXELS = 4096

        fun readFile(answer: JSON, path: String, id: String): FileContent = when (answer.string("kind")) {
            "text" -> {
                val file = CodeFile.whole(path, answer.strings("lines"))
                FileContent.Text(CodeDocument(id, listOf(file), truncated = false, headed = false), answer.bool("truncated"))
            }
            "image" -> Pictures.decode(File(answer.string("file")), IMAGE_PIXELS)?.let { FileContent.Image(it.asImageBitmap()) }
                ?: FileContent.Binary(answer.long("size"))
            "video" -> FileContent.Video(File(answer.string("file")))
            else -> FileContent.Binary(answer.long("size"))
        }
    }
}
