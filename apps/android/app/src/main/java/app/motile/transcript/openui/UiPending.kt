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
@Composable fun UiImage(node: UiNode, name: String, src: String?, aspect: Float, radius: UiRadius, fit: UiFit, photos: UiPhotos) {}
@Composable fun UiIcon(node: UiNode, kind: UiKind.Icon) {}
@Composable fun UiTable(node: UiNode, kind: UiKind.Table) {}
@Composable fun UiYouTube(node: UiNode, kind: UiKind.YouTube) {}
@Composable fun UiHtmlCard(node: UiNode, kind: UiKind.HtmlView) {}
@Composable fun UiMapCard(node: UiNode, kind: UiKind.Map) {}
