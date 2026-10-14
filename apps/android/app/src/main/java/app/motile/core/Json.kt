package app.motile.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

// What the core sends, read from its JSON. The shapes are defined in crates/core/src/api.rs and
// crates/protocol/src/wire.rs.

typealias JSON = JsonObject

val EmptyJSON = JsonObject(emptyMap())

fun parseJSON(text: String): JSON? = runCatching { Json.parseToJsonElement(text) as? JsonObject }.getOrNull()

private fun JSON.primitive(key: String): JsonPrimitive? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }

fun JSON.string(key: String): String = optionalString(key) ?: ""

fun JSON.optionalString(key: String): String? = primitive(key)?.takeIf { it.isString }?.content

fun JSON.bool(key: String): Boolean = primitive(key)?.booleanOrNull ?: false

fun JSON.optionalBool(key: String): Boolean? = primitive(key)?.booleanOrNull

fun JSON.long(key: String): Long = optionalLong(key) ?: 0

fun JSON.optionalLong(key: String): Long? = primitive(key)?.let { it.longOrNull ?: it.doubleOrNull?.toLong() }

fun JSON.int(key: String): Int = long(key).toInt()

fun JSON.optionalInt(key: String): Int? = optionalLong(key)?.toInt()

fun JSON.double(key: String): Double = optionalDouble(key) ?: 0.0

fun JSON.optionalDouble(key: String): Double? = primitive(key)?.doubleOrNull

fun JSON.obj(key: String): JSON? = this[key] as? JsonObject

fun JSON.objects(key: String): List<JSON> = (this[key] as? JsonArray)?.mapNotNull { it as? JsonObject } ?: emptyList()

fun JSON.strings(key: String): List<String> =
    (this[key] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content } ?: emptyList()

/** A list of numbers, like the core's `[start, length, palette index]` spans. */
fun JSON.numbers(key: String): List<Double> = (this[key] as? JsonArray)?.let(::numbersOf) ?: emptyList()

fun JSON.optionalNumbers(key: String): List<Double>? = (this[key] as? JsonArray)?.let(::numbersOf)

fun JSON.ints(key: String): IntArray = (this[key] as? JsonArray)?.let { array -> IntArray(array.size) { (array[it] as? JsonPrimitive)?.let { p -> p.longOrNull?.toInt() ?: p.doubleOrNull?.toInt() } ?: 0 } } ?: IntArray(0)

fun JSON.optionalInts(key: String): IntArray? = if (this[key] is JsonArray) ints(key) else null

/** Lists of numbers, like the lines of `code_spans`. */
fun JSON.intLists(key: String): List<IntArray> =
    (this[key] as? JsonArray)?.map { line -> (line as? JsonArray)?.let { array -> IntArray(array.size) { ((array[it] as? JsonPrimitive)?.longOrNull ?: 0).toInt() } } ?: IntArray(0) } ?: emptyList()

private fun numbersOf(array: JsonArray): List<Double> = array.mapNotNull { (it as? JsonPrimitive)?.doubleOrNull }

/** A JSON value of what Kotlin has: strings, numbers, booleans, lists, maps and nulls. */
fun jsonOf(value: Any?): JsonElement = when (value) {
    null -> JsonNull
    is JsonElement -> value
    is String -> JsonPrimitive(value)
    is Boolean -> JsonPrimitive(value)
    is Number -> JsonPrimitive(value)
    is Map<*, *> -> JsonObject(value.entries.associate { (key, item) -> key.toString() to jsonOf(item) })
    is Iterable<*> -> JsonArray(value.map(::jsonOf))
    is Array<*> -> JsonArray(value.map(::jsonOf))
    is IntArray -> JsonArray(value.map { JsonPrimitive(it) })
    else -> JsonPrimitive(value.toString())
}

/** An object of the pairs, as the core takes commands: `json("type" to "stop", "thread_id" to id)`. */
fun json(vararg pairs: Pair<String, Any?>): JSON = JsonObject(pairs.associate { (key, value) -> key to jsonOf(value) })
