package app.motile.core

import android.content.Context
import android.os.Handler
import android.os.Looper
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.thread

/** Why a command failed. `offline`: the server couldn't be reached, which its state in the sidebar already shows. */
class CoreError(override val message: String, val offline: Boolean = false) : Exception(message)

/**
 * The client's end of the Rust core: commands go in as JSON, events come out as JSON. Events are
 * decoded on a thread of their own, in the order they were sent, and handed to the main thread.
 */
class CoreBridge {
    /** Decodes an event off the main thread into the work to do on it. */
    var decode: ((JSON) -> (() -> Unit)?)? = null

    private val main = Handler(Looper.getMainLooper())
    private var nextID = 0L
    private val replies = HashMap<Long, (Result<JSON>) -> Unit>()
    /** What turns a large answer into what the client keeps, off the main thread, and where that goes. */
    private val readers = ConcurrentHashMap<Long, (JSON) -> Any>()
    private val readReplies = HashMap<Long, (Result<Any>) -> Unit>()
    private var listening = false

    fun start(context: Context, config: JSON): Boolean {
        val started = Native.start(context.applicationContext, config.toString())
        if (started && !listening) {
            listening = true
            thread(name = "motile-events", isDaemon = true) {
                while (true) received(Native.next())
            }
        }
        return started
    }

    /** Sends a command. `reply` is called on the main thread with the answer. Answers with the command's id, which the events that belong to it carry. */
    fun send(type: String, fields: Map<String, Any?> = emptyMap(), reply: ((Result<JSON>) -> Unit)? = null): Long {
        nextID += 1
        if (reply != null) replies[nextID] = reply
        post(type, fields, nextID)
        return nextID
    }

    /** Sends a command whose answer is large: `read` turns it into what the client keeps, off the main thread, and `reply` gets that on the main thread. */
    @Suppress("UNCHECKED_CAST")
    fun <R : Any> send(type: String, fields: Map<String, Any?>, read: (JSON) -> R, reply: (Result<R>) -> Unit): Long {
        nextID += 1
        readers[nextID] = read
        readReplies[nextID] = { result -> reply(result.map { it as R }) }
        post(type, fields, nextID)
        return nextID
    }

    private fun post(type: String, fields: Map<String, Any?>, id: Long) {
        val command = jsonOf(fields + mapOf("type" to type, "id" to id))
        Native.send(command.toString())
    }

    private fun received(text: String) {
        val event = parseJSON(text) ?: return
        if (event.string("type") == "reply") {
            val id = event.long("id")
            val ok = event.bool("ok")
            val value = event.obj("value") ?: EmptyJSON
            val reader = readers.remove(id)
            val read = if (ok) reader?.let { runCatching { it(value) }.getOrNull() } else null
            main.post { answer(id, ok, value, read) }
            return
        }
        val apply = decode?.invoke(event) ?: return
        main.post(apply)
    }

    private fun answer(id: Long, ok: Boolean, value: JSON, read: Any?) {
        val failure = CoreError(value.optionalString("error") ?: "Something went wrong.", value.bool("offline"))
        readReplies.remove(id)?.let { reply ->
            reply(if (read != null) Result.success(read) else Result.failure(failure))
            return
        }
        val reply = replies.remove(id) ?: return
        reply(if (ok) Result.success(value) else Result.failure(failure))
    }
}

/** The error of a failed result as the core said it. */
val Result<*>.coreError: CoreError?
    get() = exceptionOrNull()?.let { it as? CoreError ?: CoreError(it.message ?: "Something went wrong.") }
