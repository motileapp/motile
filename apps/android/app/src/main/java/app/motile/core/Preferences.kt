package app.motile.core

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** What the client keeps between launches, as UserDefaults does on the Apple clients. */
object Preferences {
    private lateinit var shared: SharedPreferences

    fun start(context: Context) {
        shared = context.getSharedPreferences("motile", Context.MODE_PRIVATE)
    }

    fun string(key: String): String? = shared.getString(key, null)

    fun bool(key: String): Boolean = shared.getBoolean(key, false)

    /** The boolean, or nothing when it was never set. */
    fun optionalBool(key: String): Boolean? = if (shared.contains(key)) shared.getBoolean(key, false) else null

    fun set(key: String, value: String?) = shared.edit { if (value == null) remove(key) else putString(key, value) }

    fun set(key: String, value: Boolean) = shared.edit { putBoolean(key, value) }

    fun remove(key: String) = shared.edit { remove(key) }

    /** Whether a new thread starts on the account of its agent that has used the least of its limits, rather than the one last chosen. */
    var managesNewThreads: Boolean
        get() = bool("providerLimits.manageNewThreads")
        set(on) = set("providerLimits.manageNewThreads", on)

    /** A map of strings, kept as JSON. */
    fun strings(key: String): Map<String, String> {
        val json = string(key)?.let(::parseJSON) ?: return emptyMap()
        return json.mapNotNull { (name, value) -> (value as? JsonPrimitive)?.takeIf { it.isString }?.content?.let { name to it } }.toMap()
    }

    fun set(key: String, value: Map<String, String>) = set(key, JsonObject(value.mapValues { JsonPrimitive(it.value) }).toString())

    fun stringList(key: String): List<String> = string(key)?.let { text ->
        runCatching { kotlinx.serialization.json.Json.parseToJsonElement(text) as? kotlinx.serialization.json.JsonArray }.getOrNull()
            ?.mapNotNull { (it as? JsonPrimitive)?.content }
    } ?: emptyList()

    fun set(key: String, value: List<String>) = set(key, jsonOf(value).toString())
}
