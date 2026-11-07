package app.motile.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import app.motile.LocalStore
import app.motile.core.BackupBucket
import app.motile.platform.Platform
import app.motile.theme.IconSize
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.HoldsModal
import app.motile.ui.LocalCloseModal
import app.motile.ui.Spinner

/** The S3 bucket every server backs up to and syncs through, to set up or change. */
@Composable
fun BackupSheet() {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    val kept = store.backup.bucket
    var endpoint by remember { mutableStateOf(kept?.endpoint ?: "") }
    var region by remember { mutableStateOf(kept?.region ?: "") }
    var bucket by remember { mutableStateOf(kept?.bucket ?: "") }
    var accessKey by remember { mutableStateOf(kept?.accessKey ?: "") }
    var secretKey by remember { mutableStateOf(kept?.secretKey ?: "") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val canSave = listOf(bucket, accessKey, secretKey).all { it.isNotBlank() }

    HoldsModal(saving)

    fun save() {
        saving = true
        error = null
        val edited = BackupBucket(endpoint.trim(), region.trim(), bucket.trim(), accessKey.trim(), secretKey.trim())
        store.setBackup(edited) { failure ->
            saving = false
            error = failure
            if (failure == null) close()
        }
    }

    val trailing: @Composable () -> Unit = {
        if (saving) Spinner(IconSize.Xl)
        else ActionButton("Save", variant = ButtonVariant.Primary, disabled = !canSave) { save() }
    }
    Column(Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalBarLeading provides BarLeading.Close { close() }) { SettingsBar("S3 Backup", trailing) }
        SettingsList(Modifier.weight(1f)) {
            group(header = "Endpoint", footer = "Empty for AWS.") {
                row { SettingsField("https://<account>.r2.cloudflarestorage.com", endpoint, { endpoint = it }, monospaced = true) }
            }
            group(header = "Region", footer = "auto for Cloudflare R2.") {
                row { SettingsField("us-east-1", region, { region = it }, monospaced = true) }
            }
            group(header = "Bucket") {
                row { SettingsField("motile-backup", bucket, { bucket = it }, monospaced = true) }
            }
            group(
                header = "Keys",
                footer = "One of your servers checks the bucket first. Each server then backs up to it by itself and syncs through it. The keys stay on your servers and this ${Platform.device}.",
                failure = error,
            ) {
                row { SettingsField("Access key", accessKey, { accessKey = it }, monospaced = true) }
                row { SettingsField("Secret key", secretKey, { secretKey = it }, monospaced = true, secure = true) }
            }
        }
    }
}
