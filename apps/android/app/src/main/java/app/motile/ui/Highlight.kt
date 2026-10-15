package app.motile.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue

/** The row the arrow keys are on. An object, so that a key handler made in an earlier composition reads the row as it is now. */
class Highlight {
    var index by mutableIntStateOf(0)
}
