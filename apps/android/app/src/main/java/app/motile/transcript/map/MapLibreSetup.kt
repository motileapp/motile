package app.motile.transcript.map

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.core.graphics.createBitmap
import app.motile.BuildConfig
import app.motile.core.UiMapPin
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Dispatcher
import okhttp3.OkHttpClient
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.Style
import org.maplibre.android.module.http.HttpRequestUtil
import org.maplibre.android.snapshotter.MapSnapshotter
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume

/** MapLibre, started once, and what every map shares: its styles and its pins' emoji. */
object MapLibreSetup {
    private var started = false
    private val styles = ConcurrentHashMap<Boolean, String>()

    /** Starts MapLibre, whose tiles and glyphs come from OpenFreeMap with Motile's User-Agent. On the main thread. */
    fun start(context: Context) {
        if (started) return
        started = true
        MapLibre.getInstance(context.applicationContext)
        val agent = "Motile/${BuildConfig.VERSION_NAME}"
        val client = OkHttpClient.Builder()
            .dispatcher(Dispatcher().apply { maxRequestsPerHost = 20 })
            .addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("User-Agent", agent).build()) }
            .build()
        HttpRequestUtil.setOkHttpClient(client)
    }

    /** The style generated from tokens.json, as the build bundles it. */
    fun cachedStyle(dark: Boolean): String? = styles[dark]

    suspend fun style(context: Context, dark: Boolean): String = styles[dark] ?: withContext(Dispatchers.IO) {
        val name = if (dark) "map-style-dark.json" else "map-style-light.json"
        context.assets.open(name).bufferedReader().use { it.readText() }.also { styles[dark] = it }
    }

    /** An emoji pin's image: the emoji at 18 points, centred in a clear 34-point square, at the screen's density. */
    fun emojiImage(context: Context, emoji: String): Bitmap {
        val metrics = context.resources.displayMetrics
        val side = (34 * metrics.density).toInt()
        val bitmap = createBitmap(side, side)
        bitmap.density = metrics.densityDpi
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            textSize = 18 * metrics.density
            textAlign = Paint.Align.CENTER
        }
        val font = paint.fontMetrics
        Canvas(bitmap).drawText(emoji, side / 2f, side / 2f - (font.ascent + font.descent) / 2, paint)
        return bitmap
    }

    /** A still picture of the map, taken where a live map would show it; none if it fails. On the main thread. */
    suspend fun snapshot(context: Context, style: String, pins: List<UiMapPin>, camera: MapCamera, width: Float, height: Float): Bitmap? {
        start(context)
        val builder = Style.Builder().fromJson(style)
        MapGeoJson.emoji(pins).forEach { builder.withImage(it, emojiImage(context, it)) }
        val options = MapSnapshotter.Options(width.toInt(), height.toInt())
            .withStyleBuilder(builder)
            .withCameraPosition(CameraPosition.Builder().target(LatLng(camera.lat, camera.lng)).zoom(camera.zoom).build())
            .withPixelRatio(context.resources.displayMetrics.density)
            .withLogo(false)
            .withAttribution(false)
        val snapshotter = MapSnapshotter(context, options)
        return suspendCancellableCoroutine { waiting ->
            waiting.invokeOnCancellation { snapshotter.cancel() }
            snapshotter.start({ waiting.resume(it.bitmap) }, { waiting.resume(null) })
        }
    }
}
