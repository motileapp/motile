package app.motile.onboarding

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.platform.Platform
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.ui.ActionButton
import app.motile.ui.LogoMark
import app.motile.ui.Text
import kotlin.math.min

@Composable
fun SignInView() {
    val store = LocalStore.current
    val palette = LocalPalette.current
    Column(Modifier.fillMaxSize().safeDrawingPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        Spacer(Modifier.weight(1f))
        MotileMark(color = palette.emphasizedForeground)
        Text("Motile", Modifier.padding(top = 14.dp), size = 30f, weight = FontWeight.SemiBold)
        Text("The command center for coding agents.", Modifier.padding(top = 6.dp), size = 15f, color = palette.mutedForeground)

        ActionButton(
            "Continue with Google",
            modifier = Modifier.padding(top = 34.dp).width(250.dp),
            picture = { GoogleMark() },
            size = ControlSize.Large,
            pending = store.signingIn,
            fills = true,
        ) { store.signIn() }
        if (store.signingIn) {
            Text("Waiting for the browser", Modifier.padding(top = 10.dp), size = 12f, color = palette.mutedStrongerForeground)
        }

        store.signInError?.let { error ->
            Text(
                error,
                Modifier.padding(top = 14.dp).widthIn(max = 360.dp),
                size = 13f,
                color = palette.destructive,
                align = TextAlign.Center,
            )
        }
        Spacer(Modifier.weight(1f))
        Text(
            "Signing in links this ${Platform.device} to your account. Your threads stay on your own machines.",
            Modifier.padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
            size = 12f,
            color = palette.mutedStrongerForeground,
            align = TextAlign.Center,
        )
    }
}

/** Motile's mark, as the app icon draws it (apps/macos/Resources/AppIcon.icon), in a square of 24. */
object MotileMark {
    val path: Path by lazy {
        PathParser().parsePathString(
            "M18.477 1.929c1.107.2 1.988 1.241 1.701 2.468-.281 1.205.603 2.372 1.883 2.488 1.863.167 2.616 2.39 1.212 3.577a1.994 1.994 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.56 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.508 1.128-1.81 1.484-2.818 1.068-.934-.385-.922-1.601-.598-2.559L16.52 3.63c.315-.929.993-1.874 1.958-1.7zm-8.437-.693C10.547.108 11.849-.248 12.857.168c.934.385.922 1.6.598 2.558L7.48 20.37c-.314.929-.992 1.874-1.957 1.7-1.107-.199-1.988-1.241-1.701-2.469.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.538.182 3.048-.95z",
        ).toPath()
    }
}

/** Motile's mark, filled in one colour at the side given. */
@Composable
fun MotileMark(color: Color, modifier: Modifier = Modifier, side: Float = 48f) {
    LogoMark(MotileMark.path, side, color, modifier)
}

/** Google's "G" in its four colours, from Google's own mark on its 48-point grid. */
private object GoogleMark {
    private fun path(data: String): Path = PathParser().parsePathString(data).toPath()

    val red by lazy { path("M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z") }
    val blue by lazy { path("M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z") }
    val yellow by lazy { path("M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z") }
    val green by lazy { path("M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z") }

    fun pieces(palette: Palette): List<Pair<Color, Path>> =
        listOf(palette.googleRed to red, palette.googleBlue to blue, palette.googleYellow to yellow, palette.googleGreen to green)
}

@Composable
private fun GoogleMark() {
    val pieces = GoogleMark.pieces(LocalPalette.current)
    Canvas(Modifier.fillMaxSize()) {
        val side = min(size.width, size.height)
        translate((size.width - side) / 2, (size.height - side) / 2) {
            scale(side / 48, side / 48, pivot = Offset.Zero) {
                for ((color, path) in pieces) drawPath(path, color)
            }
        }
    }
}
