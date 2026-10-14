package app.motile.core

/** What is read from a server: still on its way, there, or why it couldn't be. */
sealed interface Loaded<out Value> {
    data object Loading : Loaded<Nothing>
    data class Ready<Value>(override val value: Value) : Loaded<Value>
    data class Failed(val message: String) : Loaded<Nothing>

    val value: Value? get() = null

    companion object {
        /** What was read, or what is shown when reading it failed. */
        fun <Value> of(result: Result<Value>, over: Loaded<Value>): Loaded<Value> = result.fold(
            onSuccess = { Ready(it) },
            onFailure = { error -> if (over.value != null) over else Failed(error.message ?: "Something went wrong.") },
        )
    }
}
