package app.motile.transcript.openui

import androidx.compose.runtime.Composable
import app.motile.core.UiCanvas
import app.motile.core.UiColor
import app.motile.core.UiFit
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiPhotos
import app.motile.core.UiRadius

@Composable fun UiFlashcard(node: UiNode, kind: UiKind.Flashcard) {}
@Composable fun UiCelebration(node: UiNode, kind: UiKind.Celebration) {}
@Composable fun UiCanvasView(node: UiNode, canvas: UiCanvas) {}
@Composable fun UiButton(node: UiNode, kind: UiKind.Button) {}
@Composable fun UiIconButton(node: UiNode, kind: UiKind.IconButton) {}
@Composable fun UiPressable(node: UiNode, kind: UiKind.Pressable) {}
@Composable fun UiInput(node: UiNode, kind: UiKind.Input) {}
@Composable fun UiTextArea(node: UiNode, kind: UiKind.TextArea) {}
@Composable fun UiSelect(node: UiNode, kind: UiKind.Select) {}
@Composable fun UiSegmented(node: UiNode, kind: UiKind.SegmentedControl) {}
@Composable fun UiSlider(node: UiNode, kind: UiKind.Slider) {}
@Composable fun UiCheckbox(node: UiNode, kind: UiKind.Checkbox) {}
@Composable fun UiRadioGroup(node: UiNode, kind: UiKind.RadioGroup) {}
@Composable fun UiDatePicker(node: UiNode, kind: UiKind.DatePicker) {}
@Composable fun UiTimer(node: UiNode, kind: UiKind.Timer) {}
@Composable fun UiImage(node: UiNode, name: String, src: String?, aspect: Float, radius: UiRadius, fit: UiFit, photos: UiPhotos) {}
@Composable fun UiIcon(node: UiNode, kind: UiKind.Icon) {}
@Composable fun UiTable(node: UiNode, kind: UiKind.Table) {}
@Composable fun UiYouTube(node: UiNode, kind: UiKind.YouTube) {}
@Composable fun UiHtmlCard(node: UiNode, kind: UiKind.HtmlView) {}
@Composable fun UiMapCard(node: UiNode, kind: UiKind.Map) {}
