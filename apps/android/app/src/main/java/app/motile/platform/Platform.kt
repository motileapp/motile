package app.motile.platform

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import kotlin.math.roundToInt

/** What Android does that the Apple clients do differently, behind one name each. */
object Platform {
    const val name = "android"

    /** How much larger than on the Mac text and the controls around it are, as on iOS. */
    const val scale = 1.14f

    /** What only matters under the pointer is hidden until the pointer is over it: never on a touch screen. */
    const val hoverReveals = false

    /** The least a control is tall and wide for a finger to press it. */
    const val minimumPress = 48f

    lateinit var context: Context
        private set

    fun start(context: Context) {
        this.context = context.applicationContext
    }

    /** The device is a tablet: a screen at least 600 wide at its narrowest. */
    val isTablet: Boolean
        get() = context.resources.configuration.smallestScreenWidthDp >= 600

    /** What the device is called in what the user reads. */
    val device: String
        get() = if (isTablet) "tablet" else "phone"

    /** The right panel is a screen of its own over the thread, as on a phone. */
    val panelCoversThread: Boolean
        get() = !isTablet

    val deviceName: String
        get() = Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
            ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"

    /** Whether the client is in front, which is when a reply counts as seen. */
    val isActive: Boolean
        get() = ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)

    val reducesMotion: Boolean
        get() = Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

    val isDark: Boolean
        get() = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

    fun open(url: String) {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            context.startActivity(intent)
        } catch (_: ActivityNotFoundException) {
        }
    }

    fun copy(text: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText("Motile", text))
    }

    /** Takes the keyboard away from whatever has it. The root view sets what does it. */
    var endEditing: () -> Unit = {}

    /** Hands a file of this device to another app: to open it, or to share it. */
    fun share(file: java.io.File, mime: String?, title: String = file.name) {
        val uri = androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        val send = Intent(Intent.ACTION_SEND)
            .setType(mime ?: "*/*")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        context.startActivity(Intent.createChooser(send, title).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/** A size as it is on the Mac, larger here, where text is read from further and hit by fingers. */
fun scaled(value: Float): Float = (value * Platform.scale).roundToInt().toFloat()
