package app.motile.transcript.canvas

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import app.motile.theme.Palette
import app.motile.theme.Surface
import com.github.takahirom.roborazzi.captureRoboImage
import java.io.File
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The painter against the reference painter's pictures in crates/core/fixtures/canvas/reference:
 * each case's scene at two pixels a point on the transcript's background, labels masked, as
 * build/canvas/android/<case>-<appearance>.png.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], application = Application::class)
class ScenePaintTest {
    private val root: File = generateSequence(File("").absoluteFile) { it.parentFile }
        .first { File(it, "crates/core/fixtures/canvas/reference/cases.json").exists() }
    private val reference = File(root, "crates/core/fixtures/canvas/reference")
    private val text by lazy { CanvasText(RuntimeEnvironment.getApplication(), Density(2f)) }

    private class Case(val name: String, val hover: Int?, val scale: Float, val masks: List<FloatArray>)

    private fun cases(): List<Case> = Json.parseToJsonElement(File(reference, "cases.json").readText()).jsonArray.map { element ->
        val case = element.jsonObject
        Case(
            name = (case["name"] as JsonPrimitive).content,
            hover = (case["hover"] as? JsonPrimitive)?.intOrNull,
            scale = (case["scale"] as JsonPrimitive).doubleOrNull?.toFloat() ?: 2f,
            masks = (case["masks"] as JsonArray).map { mask -> mask.jsonArray.map { (it as JsonPrimitive).doubleOrNull?.toFloat() ?: 0f }.toFloatArray() },
        )
    }

    private fun paint(scene: Scene, dark: Boolean, scale: Float, hover: Int?): Bitmap {
        val palette = if (dark) Palette.dark else Palette.light
        val colors = SceneColors(palette, dark, Surface.Background, palette.foreground)
        val canvas = CanvasScene(scene, CanvasText(RuntimeEnvironment.getApplication(), Density(scale)))
        val width = max(1, ceil(scene.width * scale).toInt())
        val height = max(1, ceil(scene.height * scale).toInt())
        val image = ImageBitmap(width, height)
        CanvasDrawScope().draw(Density(scale), LayoutDirection.Ltr, Canvas(image), Size(width.toFloat(), height.toFloat())) {
            drawRect(colors.of(SceneColor.Surface(SceneColor.Layer.Base)))
            paintScene(canvas, colors, hover)
        }
        return image.asAndroidBitmap()
    }

    /** The share of pixels whose channels differ by more than 40 of 255, outside the masks. */
    private fun differing(expected: Bitmap, actual: Bitmap, masks: List<FloatArray>, scale: Float): Double {
        val (width, height) = expected.width to expected.height
        val masked = BooleanArray(width * height)
        for ((x, y, w, h) in masks.map { it.toList() }) {
            val left = floor(x * scale).toInt().coerceIn(0, width)
            val right = ceil((x + w) * scale).toInt().coerceIn(0, width)
            val top = floor(y * scale).toInt().coerceIn(0, height)
            val bottom = ceil((y + h) * scale).toInt().coerceIn(0, height)
            for (row in top until bottom) for (column in left until right) masked[row * width + column] = true
        }
        val a = IntArray(width * height).also { expected.getPixels(it, 0, width, 0, 0, width, height) }
        val b = IntArray(width * height).also { actual.getPixels(it, 0, width, 0, 0, width, height) }
        val differ = a.indices.count { at ->
            !masked[at] && (0 until 4).any { channel -> abs((a[at] ushr (channel * 8) and 0xFF) - (b[at] ushr (channel * 8) and 0xFF)) > 40 }
        }
        return differ.toDouble() / (width * height)
    }

    @Test
    fun scenesPaintAsTheReferencePainterPaintsThem() {
        val out = File(root, "build/canvas/android").apply { mkdirs() }
        val failures = mutableListOf<String>()
        val worst = mutableMapOf<String, Pair<String, Double>>()
        for (case in cases()) {
            val scene = Scene.parse(File(reference, "${case.name}.json").readText())!!
            for ((theme, dark) in listOf("dark" to true, "light" to false)) {
                val actual = paint(scene, dark, case.scale, case.hover)
                actual.captureRoboImage(File(out, "${case.name}-$theme.png").path)
                val expected = BitmapFactory.decodeFile(File(reference, "${case.name}-$theme.png").path)
                assertEquals("${case.name}-$theme's size", expected.width to expected.height, actual.width to actual.height)
                val share = differing(expected, actual, case.masks, case.scale)
                println("canvas ${case.name}-$theme: %.3f%% of its pixels differ".format(share * 100))
                if (share > (worst[theme]?.second ?: -1.0)) worst[theme] = case.name to share
                if (share > 0.01) failures += "${case.name}-$theme (%.2f%%)".format(share * 100)
            }
        }
        worst.forEach { (theme, case) -> println("canvas worst $theme: ${case.first} %.3f%%".format(case.second * 100)) }
        assertTrue("Painted unlike the reference: $failures", failures.isEmpty())
    }

    private fun legendScene(legend: String, width: Int, height: Int) =
        Scene.parse("""{"width": $width, "height": $height, "items": [], "legend": $legend}""")!!

    private fun entries(count: Int, share: Boolean) = (1..count).joinToString(",") {
        """{"color": "chart-${(it - 1) % 6 + 1}", "label": "S$it"${if (share) ""","share": "$it%"""" else ""}}"""
    }

    @Test
    fun aChartLegendWrapsUnderThePlot() {
        val scene = legendScene("""{"entries": [${entries(3, share = false)}]}""", 390, 240)
        assertEquals(240f + 10 + 21, sceneHeight(scene, 390f, text), 0.01f)
        assertEquals(240f + 10 + 21 * 3 + 6 * 2, sceneHeight(scene, 1f, text), 0.01f)
    }

    @Test
    fun aPieLegendStandsBesideTheDiscOrUnderIt() {
        val beside = legendScene("""{"beside": true, "entries": [${entries(4, share = true)}]}""", 200, 200)
        assertEquals(200f, sceneHeight(beside, 720f, text), 0.01f)
        val tall = legendScene("""{"beside": true, "entries": [${entries(12, share = true)}]}""", 200, 200)
        assertEquals(23f * 12 + 9 * 11, sceneHeight(tall, 720f, text), 0.01f)
        val under = legendScene("""{"entries": [${entries(4, share = true)}]}""", 186, 186)
        assertEquals(186f + 24 + 23 * 4 + 9 * 3, sceneHeight(under, 300f, text), 0.01f)
    }
}
