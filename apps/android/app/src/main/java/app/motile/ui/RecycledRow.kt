package app.motile.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.theme.LocalSurface
import app.motile.theme.Surface

/** A row drawn outside the hierarchy that provides what rows read, handed the store and its surface. */
@Composable
fun RecycledRow(store: AppStore, surface: Surface, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalStore provides store, LocalSurface provides surface, content = content)
}
