package app.motile.core

/** A draft or a thread the sidebar's search found, with the message it was found by when its title, its project and its pull request don't have what was searched for. */
sealed interface SearchResult {
    data class Draft(val listed: ListedDraft) : SearchResult
    data class Thread(val thread: ThreadInfo, val match: MessageMatch?) : SearchResult

    val id: String
        get() = when (this) {
            is Draft -> "draft:${listed.id}"
            is Thread -> thread.id
        }

    val selection: Selection
        get() = when (this) {
            is Draft -> Selection.Draft(listed.id)
            is Thread -> Selection.Thread(thread.id)
        }

    val threadInfo: ThreadInfo? get() = (this as? Thread)?.thread
}

val AppStore.isSearchingThreads: Boolean get() = threadSearch.isNotBlank()

/** Whether the thread's title, project or pull request has what is searched for. */
fun AppStore.matches(thread: ThreadInfo, search: String): Boolean {
    if (search.isEmpty()) return true
    val project = project(thread.projectID)
    val pullRequest = project?.pullRequest(thread)
    val names = listOf(thread.title, project?.name, pullRequest?.let { "#${it.number}" }, pullRequest?.title, pullRequest?.url)
    return names.any { it?.contains(search, ignoreCase = true) == true }
}

/** The drafts whose text or project matches what is being searched for. */
fun AppStore.searched(drafts: List<ListedDraft>, search: String): List<ListedDraft> {
    if (search.isEmpty()) return drafts
    return drafts.filter { listed ->
        val project = project(listed.draft.projectID)?.name ?: ""
        listed.preview.contains(search, ignoreCase = true) || project.contains(search, ignoreCase = true)
    }
}

/**
 * What the sidebar lists while it searches, in one list: the drafts, the threads found by their
 * title, project or pull request, active before done, then the ones found by a message. What the
 * servers find later only ever comes below.
 */
val AppStore.searchResults: List<SearchResult>
    get() {
        val search = threadSearch.trim()
        if (search.isEmpty()) return emptyList()
        val threads = activeThreads + doneThreads
        val named = threads.filter { matches(it, search) }
        val namedIDs = named.map { it.id }.toSet()
        val found = messageSearch.matches
        val byMessage = threads.filter { it.id !in namedIDs && found[it.id] != null }
        return searched(listedDrafts, search).map { SearchResult.Draft(it) } +
            named.map { SearchResult.Thread(it, null) } +
            byMessage.map { SearchResult.Thread(it, found[it.id]) }
    }

/** Opens what the search found and ends the search, then shows it in the sidebar. */
fun AppStore.openSearchResult(result: SearchResult) {
    show(result.selection)
    threadSearch = ""
    revealedThreadID = result.threadInfo?.id
}
