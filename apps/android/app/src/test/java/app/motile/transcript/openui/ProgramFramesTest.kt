package app.motile.transcript.openui

import android.app.Application
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.Preferences
import app.motile.core.UiNode
import app.motile.platform.Platform
import app.motile.theme.LocalPalette
import com.github.takahirom.roborazzi.captureRoboImage
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Every program of the corpus (crates/core/fixtures/openui) laid out at 390 and 720 points as the
 * transcript lays it out, in the dark: its frames to build/frames/android and its picture to
 * build/programs/android, as `<program>-<width>`, for scripts/compare-frames.sh.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], application = Application::class, qualifiers = "w800dp-h6000dp-xhdpi")
class ProgramFramesTest {
    @get:Rule val compose = createComposeRule()

    private val root: File = generateSequence(File("").absoluteFile) { it.parentFile }
        .first { File(it, "crates/core/fixtures/openui").isDirectory }

    private class Shown(val tree: UiNode, val width: Float, val frames: UiFrames)

    @Test
    fun everyProgramOfTheCorpusLaysOutAtBothWidths() {
        val frames = File(root, "build/frames/android").apply { mkdirs() }
        val pictures = File(root, "build/programs/android").apply { mkdirs() }
        val programs = File(root, "crates/core/fixtures/openui").listFiles { file -> file.extension == "json" }!!.sortedBy { it.name }
        var shown by mutableStateOf<Shown?>(null)
        Platform.start(RuntimeEnvironment.getApplication())
        Preferences.start(RuntimeEnvironment.getApplication())
        val store = AppStore(RuntimeEnvironment.getApplication())
        compose.setContent {
            CompositionLocalProvider(LocalStore provides store) {
                shown?.let { program ->
                    key(program.frames) {
                        Box(Modifier.wrapContentSize(Alignment.TopStart, unbounded = true).testTag("program").background(LocalPalette.current.background)) {
                            UiFrameDump(program.tree, program.width, program.frames)
                        }
                    }
                }
            }
        }
        for (file in programs) {
            val tree = UiNode.from(Json.parseToJsonElement(file.readText()).jsonObject) ?: error("${file.name} is not a tree")
            for (width in listOf(390f, 720f)) {
                val dump = UiFrames()
                shown = Shown(tree, width, dump)
                val json = settled(dump)
                val name = "${file.nameWithoutExtension}-${width.toInt()}"
                assertEquals("$name fills the column", width, frame(json, "")[2], 0.5f)
                File(frames, "$name.json").writeText(json)
                compose.onNodeWithTag("program").captureRoboImage(File(pictures, "$name.png"))
            }
        }
    }

    /** The frames once the scenes laid out off the main thread have arrived: nothing moved for 200 ms. */
    private fun settled(dump: UiFrames): String {
        var last = ""
        var still = 0
        repeat(250) {
            compose.waitForIdle()
            val json = dump.json()
            still = if (json == last && json != "{}") still + 1 else 0
            if (still == 10) return json
            last = json
            Thread.sleep(20)
        }
        return last
    }

    private fun frame(json: String, id: String): List<Float> =
        Json.parseToJsonElement(json).jsonObject[id]?.jsonArray?.map { it.jsonPrimitive.float } ?: error("no frame for \"$id\"")
}
