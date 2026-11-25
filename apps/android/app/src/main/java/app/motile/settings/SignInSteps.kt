package app.motile.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.motile.core.AccountSignIn
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.CopyButton
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.delay

/** The code Codex's sign-in shows, to enter on its page, with the way to copy it. */
@Composable
fun DeviceCode(code: String, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
        SelectionContainer {
            Text(code, style = Theme.uiMono(TextSize.Xxl, FontWeight.SemiBold), color = LocalPalette.current.foreground, maxLines = 1)
        }
        CopyButton("Copy the code") { Platform.copy(code) }
    }
}

/** What a sign-in that finishes by itself shows while it waits. */
@Composable
fun WaitingForCode() {
    val muted = LocalPalette.current.mutedForeground
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Spinner(IconSize.Md, color = muted)
        Text("Waiting for you to enter the code", size = TextSize.Sm, color = muted)
    }
}

/**
 * Signing an account in, as rows of a group: the page to sign in on and the code to paste or to
 * enter there, or who is signed in with the way to sign in again. `canFinish` is whatever else the
 * code waits for besides being typed; once signed in, it offers to sign in again if `signsInAgain`.
 */
fun SettingsRows.signInRows(
    signIn: AccountSignIn,
    signedInAs: String?,
    canFinish: Boolean = true,
    signsInAgain: Boolean = true,
    start: () -> Unit,
    finish: () -> Unit,
) {
    val step = signIn.step
    if (step is AccountSignIn.Step.Waiting) {
        row {
            SettingsRow {
                Text(step.url, Modifier.weight(1f), style = Theme.uiMono(TextSize.Sm), color = LocalPalette.current.mutedForeground, maxLines = 1)
            }
        }
        row { SettingsPressRow({ Platform.open(step.url) }) { SettingsActionLabel("Open the Sign-In Page", Symbol.SquareArrowOutUpRight) } }
        row { CopyLinkRow(step.url) }
        val deviceCode = step.deviceCode
        if (deviceCode != null) {
            row { SettingsRow(Modifier.padding(vertical = 4.dp)) { DeviceCode(deviceCode, Modifier.weight(1f)) } }
            row { SettingsRow { WaitingForCode() } }
            return
        }
        row { CodeRow(signIn, canFinish, finish) }
        return
    }
    signedInAs?.let { who -> row { SettingsRow { RowText("Signed in as $who", color = LocalPalette.current.foreground) } } }
    if (signedInAs != null && !signsInAgain) return
    row {
        SettingsPressRow(start, enabled = step != AccountSignIn.Step.Starting && canFinish) {
            SettingsActionLabel(if (signedInAs == null) "Sign In" else "Sign In Again", Symbol.CircleUser)
            Spacer(Modifier.weight(1f))
            if (step == AccountSignIn.Step.Starting) Spinner(IconSize.Xl)
        }
    }
}

@Composable
private fun CopyLinkRow(url: String) {
    var copied by remember { mutableStateOf(false) }
    var copies by remember { mutableIntStateOf(0) }
    LaunchedEffect(copies) {
        if (!copied) return@LaunchedEffect
        delay(1200)
        copied = false
    }
    SettingsPressRow({
        Platform.copy(url)
        copied = true
        copies += 1
    }) {
        SettingsActionLabel(if (copied) "Copied" else "Copy the Link", if (copied) Symbol.Check else Symbol.Copy)
    }
}

@Composable
private fun CodeRow(signIn: AccountSignIn, canFinish: Boolean, finish: () -> Unit) {
    SettingsRow {
        Box(Modifier.weight(1f).height(ControlSize.Large.height)) {
            InputField(
                "Paste the code", signIn.code, { signIn.code = it }, variant = InputVariant.Bare, size = ControlSize.Large,
                monospaced = true, keyboardOptions = plainKeyboard.copy(imeAction = ImeAction.Done), onSubmit = finish,
            )
        }
        if (signIn.finishing) {
            Spinner(IconSize.Xl)
        } else {
            ActionButton("Sign In", variant = ButtonVariant.Link, disabled = !signIn.canFinish || !canFinish, onClick = finish)
        }
    }
}
