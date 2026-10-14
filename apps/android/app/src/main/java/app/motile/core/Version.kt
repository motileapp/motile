package app.motile.core

object Version {
    /** Whether `version` comes before `other`, comparing their numbers from the left. */
    fun isOlder(version: String, than: String?): Boolean {
        if (than == null || version.isEmpty() || than.isEmpty()) return false
        val numbers = { text: String -> text.split('.').map { it.toIntOrNull() ?: 0 } }
        val ours = numbers(version)
        val theirs = numbers(than)
        for (index in 0 until maxOf(ours.size, theirs.size)) {
            val left = ours.getOrElse(index) { 0 }
            val right = theirs.getOrElse(index) { 0 }
            if (left != right) return left < right
        }
        return false
    }
}
