package app.motile

import android.os.Handler
import android.os.Looper
import app.motile.core.Access
import app.motile.core.AgentAccount
import app.motile.core.Agent
import app.motile.core.AppStore
import app.motile.core.Appearance
import app.motile.core.PanelPage
import app.motile.core.Preferences
import app.motile.core.Selection
import app.motile.core.SettingsEntry
import app.motile.core.SettingsSection
import app.motile.core.SkillMenuKey
import app.motile.core.skillKey
import app.motile.layout.LocalDrawerHolder
import app.motile.panel.PanelTab
import app.motile.platform.Platform
import app.motile.transcript.RowModel
import java.io.File
import java.util.concurrent.Executors

/**
 * Drives the client without a finger, for looking at it in the emulator: `MOTILE_DEMO_SCRIPT`
 * names a file of steps, one a line, that is read again whenever it grows. scripts/do.sh writes
 * them; the steps are those of DemoDriverIOS.
 *
 *     sidebar open | sidebar close      shows or hides the sidebar
 *     thread <title>                    opens the thread with that title
 *     new                               starts a new thread
 *     type <text>                       writes in the composer
 *     send                              sends what is written
 *     panel diff|files|agents|close     opens a tab of the panel, or closes the panel
 *     agent <n>                         opens the nth agent of the agents tab, counting from 0
 *     child                             opens the first agent's thread that waits for the user
 *     parent                            goes back from an agent to the thread that started it
 *     group                             opens or closes the first row of agents started together
 *     file <path>                       opens a file in the panel
 *     change <path>                     opens what the latest turn changed in a file
 *     sheet commands|projects|settings|usage|server|thread|close
 *     settings <section>|search <text>|pick <group>|close
 *     appearance light|dark|system
 *     answer <allow|refuse>             answers the first approval
 *     view                              opens the thread's images in the viewer
 *     branches                          opens the branch picker in the thread's settings
 *     top                               scrolls the transcript to its start
 *     focus | blur                      gives the composer the keyboard, or takes it away
 *     keys <text>                       types in the composer where its cursor is, as a keyboard does
 *     skill up|down|pick|dismiss        presses a key of the skills' menu
 *     search [text]                     gives the front field the keyboard, and types in it
 *     access <supervised|…>             sets how much the agent may do without asking
 *     slide <label> <share>             drags the program's slider with that label to a share of its track
 *     press <label>                     presses the program's button with that label
 */
object DemoDriver {
    private val main = Handler(Looper.getMainLooper())
    private val reader = Executors.newSingleThreadScheduledExecutor()
    private var done = 0

    fun startIfRequested(store: AppStore, environment: Map<String, String>) {
        environment["MOTILE_DEMO_SIGN_IN"]?.let { signIn(it, store) }
        val script = environment["MOTILE_DEMO_SCRIPT"] ?: return
        DemoHooks.programFrames = mutableMapOf()
        reader.scheduleWithFixedDelay({
            val steps = runCatching { File(script).readLines() }.getOrNull()?.filter { it.isNotEmpty() } ?: return@scheduleWithFixedDelay
            if (steps.size <= done) return@scheduleWithFixedDelay
            val fresh = steps.drop(done)
            done = steps.size
            main.post { fresh.forEach { run(it, store) } }
        }, 250, 250, java.util.concurrent.TimeUnit.MILLISECONDS)
    }

    /** Signs in on an auth server with the dev login, once the core has said who it is. */
    private fun signIn(email: String, store: AppStore) {
        if (!store.ready) {
            main.postDelayed({ signIn(email, store) }, 200)
            return
        }
        if (store.account.signedIn) return
        store.devSignIn(email)
    }

    /** Where the latest program of the open thread laid out the node with the label, in the window's pixels. */
    private fun programBounds(store: AppStore, label: String): androidx.compose.ui.geometry.Rect? {
        val row = store.transcript.rows.lastOrNull { it.kind is RowModel.Kind.Ui } ?: return null
        return DemoHooks.programFrames?.get(row.id)?.bounds(label)
    }

    /** A finger put down at `from`, moved to `to` in steps, and lifted, as a drag or, in place, a tap. */
    private fun drag(from: Float, to: Float, y: Float) {
        val start = android.os.SystemClock.uptimeMillis()
        val steps = if (from == to) 0 else 10
        fun touch(action: Int, at: Int) {
            val x = from + (to - from) * at / maxOf(steps, 1)
            val event = android.view.MotionEvent.obtain(start, start + at * 16L, action, x, y, 0)
            main.postDelayed({
                DemoHooks.touch?.invoke(event)
                event.recycle()
            }, at * 16L)
        }
        touch(android.view.MotionEvent.ACTION_DOWN, 0)
        for (at in 1..steps) touch(android.view.MotionEvent.ACTION_MOVE, at)
        touch(android.view.MotionEvent.ACTION_UP, steps + 1)
    }

    private fun run(step: String, store: AppStore) {
        val verb = step.substringBefore(' ')
        val rest = step.substringAfter(' ', "")
        when (verb) {
            "sidebar" -> LocalDrawerHolder.drawer?.isOpen = rest == "open"
            "thread" -> {
                val thread = (store.threads.values + store.startedThreads.values).firstOrNull { it.title.contains(rest, ignoreCase = true) } ?: return
                store.select(Selection.Thread(thread.id))
            }
            "new" -> store.startNewThread(store.composerProject ?: store.projects.firstOrNull())
            "type" -> store.draft = rest
            "send" -> store.send()
            "stop" -> store.stop()
            "agent" -> {
                val agent = rest.toIntOrNull()?.let { store.agents.getOrNull(it) } ?: return
                val thread = agent.threadID
                if (thread != null) store.openStartedThread(thread) else store.rightPanel.showAgent(agent.id)
            }
            "child" -> {
                val waiting = store.selectedThread?.children?.waiting?.firstOrNull() ?: return
                store.openStartedThread(waiting.threadID)
            }
            "parent" -> {
                if (store.shownAgent != null) return store.rightPanel.showAgents()
                val parent = store.parentThread ?: return
                store.openStartedThread(parent.id)
            }
            "group" -> {
                val threadID = store.transcript.threadID ?: return
                val row = store.transcript.rows.firstOrNull { it.kind is RowModel.Kind.Agents } ?: return
                store.core.send("toggle_row", mapOf("thread_id" to threadID, "row_id" to row.id))
            }
            "panel" -> when (rest) {
                "diff" -> store.rightPanel.showDiff()
                "files" -> store.rightPanel.open(PanelTab.Files)
                "agents" -> store.rightPanel.open(PanelTab.Agents)
                "pull request" -> store.rightPanel.open(PanelTab.PullRequest)
                "pull requests" -> store.rightPanel.open(PanelTab.PullRequests)
                "linear" -> store.rightPanel.open(PanelTab.Linear)
                "ricrac" -> store.rightPanel.open(PanelTab.Ricrac)
                "max" -> store.rightPanel.toggleMaximized()
                "open" -> store.rightPanel.isOpen = true
                else -> store.rightPanel.isOpen = false
            }
            "file" -> store.rightPanel.open(PanelTab.File(rest))
            "issue" -> {
                // "issue motile issue-1 ENG-1": the workspace, the issue's id and its identifier.
                val named = rest.split(' ')
                if (named.size != 3) return
                store.rightPanel.open(PanelTab.LinearIssue(named[0], named[1], named[2]))
            }
            "ricrac-issue" -> {
                val named = rest.split(' ')
                if (named.size != 3) return
                store.rightPanel.open(PanelTab.RicracIssue(named[0], named[1], named[2]))
            }
            "change" -> {
                val turn = store.rightPanel.turns.lastOrNull() ?: return
                store.rightPanel.showChange(turn.id, rest)
            }
            "sheet" -> when (rest) {
                "commands" -> store.openPanel(PanelPage.Commands)
                "projects" -> store.openPanel(PanelPage.Projects)
                "threads" -> store.openPanel(PanelPage.Threads)
                "add" -> store.addProject()
                "settings" -> store.openSettings()
                "usage" -> store.openUsage()
                "server" -> store.showsAddServer = true
                "icon" -> {
                    val project = store.projects.firstOrNull { !it.noProject } ?: return
                    store.openPanel(PanelPage.Icon(project.id))
                }
                "thread" -> store.showsThreadSettings = true
                "commit" -> {
                    val project = store.gitProject ?: return
                    val item = project.gitControl?.menu?.firstOrNull { it.action == "commit" } ?: return
                    store.chooseGit(item, project)
                }
                else -> {
                    store.closePanel()
                    store.closeSettings()
                    store.showsUsage = false
                    store.showsAddServer = false
                    store.showsThreadSettings = false
                    store.committingProject = null
                }
            }
            "settings" -> {
                val action = rest.substringBefore(' ')
                val text = rest.substringAfter(' ', "")
                when (action) {
                    "close" -> store.closeSettings()
                    "search" -> store.settingsQuery = text
                    "pick" -> {
                        val entry = SettingsEntry.all.firstOrNull { it.id == text } ?: return
                        store.openSettings(entry.section, entry.id)
                    }
                    else -> store.openSettings(SettingsSection.entries.firstOrNull { it.raw == action } ?: return)
                }
            }
            "appearance" -> Appearance.choose(Appearance.of(rest))
            "answer" -> {
                val approval = store.activity.approvals.firstOrNull() ?: return
                store.answer(approval, allow = rest == "allow")
            }
            "done" -> store.toggleDone()
            "tmpaccount" -> {
                val server = store.servers.firstOrNull() ?: return
                val account = AgentAccount.new(Agent.Claude).copy(name = rest, folder = "/tmp/claude-${rest.lowercase()}-9de1015d")
                store.saveAgentAccount(account, server) {}
            }
            "tmpeffort" -> store.setEffort(if (rest == "default") "" else rest)
            "tmpwork" -> {
                val choice = store.composerChoices.firstOrNull { it.account.name == "Work" } ?: return
                val model = choice.models.firstOrNull { it.id == store.composerModel?.id } ?: choice.models.firstOrNull() ?: return
                store.setModel(model, choice.account)
            }
            "top" -> DemoHooks.scrollTranscriptToTop?.invoke()
            "focus" -> DemoHooks.focusComposer?.invoke()
            "search" -> DemoHooks.search?.invoke(rest)
            "keys" -> {
                DemoHooks.focusComposer?.invoke()
                DemoHooks.typeInComposer?.invoke(rest)
            }
            "skill" -> {
                val keys = mapOf("up" to SkillMenuKey.Up, "down" to SkillMenuKey.Down, "pick" to SkillMenuKey.Pick, "dismiss" to SkillMenuKey.Dismiss)
                keys[rest]?.let(store::skillKey)
            }
            "blur" -> Platform.endEditing()
            "access" -> store.setAccess(Access.of(rest) ?: Access.Full)
            "view" -> {
                val shown = store.transcript.rows.mapNotNull { it.mediaShown }
                store.view(shown, 0)
            }
            "branches" -> {
                val project = store.composerProject ?: return
                store.showsThreadSettings = true
                store.showBranches(project)
            }
            "slide" -> {
                val share = rest.substringAfterLast(' ').toFloatOrNull() ?: return
                val track = programBounds(store, rest.substringBeforeLast(' ')) ?: return
                val y = track.bottom - 8 * android.content.res.Resources.getSystem().displayMetrics.density
                drag(track.left + track.width / 2, track.left + track.width * share, y)
            }
            "press" -> {
                val button = programBounds(store, rest) ?: return
                drag(button.center.x, button.center.x, button.center.y)
            }
            "steers" -> Preferences.set(AppStore.STEERS_KEY, rest == "on")
            "drop" -> store.dropTargeted = rest == "on"
        }
    }
}
