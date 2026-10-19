// The design tokens of the Android app. Generated from tokens.json by build.mjs: edit tokens.json and run `pnpm tokens`.
package app.motile.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** Every colour of the design, in one appearance. */
class Palette(
    val background: Color,
    val backgroundAccent: Color,
    val backgroundAccentStronger: Color,
    val backgroundAccentStrongest: Color,
    val backgroundAccentLarger: Color,
    val backgroundAccentLargerStronger: Color,
    val backgroundSecondary: Color,
    val backgroundSecondaryAccent: Color,
    val backgroundSecondaryAccentStronger: Color,
    val card: Color,
    val cardAccent: Color,
    val popover: Color,
    val popoverAccent: Color,
    val foreground: Color,
    val emphasizedForeground: Color,
    val mutedForeground: Color,
    val mutedStrongerForeground: Color,
    val mutedStrongestForeground: Color,
    val primary: Color,
    val primaryForeground: Color,
    val destructive: Color,
    val destructiveForeground: Color,
    val success: Color,
    val successForeground: Color,
    val warning: Color,
    val warningForeground: Color,
    val pending: Color,
    val pendingForeground: Color,
    val process: Color,
    val processForeground: Color,
    val merged: Color,
    val mergedForeground: Color,
    val pink: Color,
    val skill: Color,
    val border: Color,
    val borderCard: Color,
    val borderInput: Color,
    val borderPopover: Color,
    val input: Color,
    val composer: Color,
    val borderComposer: Color,
    val borderAccent: Color,
    val ring: Color,
    val overlay: Color,
    val shadow: Color,
    val chart1: Color,
    val chart2: Color,
    val chart3: Color,
    val chart4: Color,
    val chart5: Color,
    val chart6: Color,
    val mapLand: Color,
    val mapGreen: Color,
    val mapPark: Color,
    val mapCampus: Color,
    val mapWater: Color,
    val mapWaterway: Color,
    val mapBuilding: Color,
    val mapStreet: Color,
    val mapRoad: Color,
    val mapFerry: Color,
    val claude: Color,
    val openai: Color,
    val googleBlue: Color,
    val googleRed: Color,
    val googleYellow: Color,
    val googleGreen: Color,
    val linear: Color,
    val windowClose: Color,
    val windowMinimize: Color,
    val windowZoom: Color,
    val syntaxComment: Color,
    val syntaxKeyword: Color,
    val syntaxString: Color,
    val syntaxConstant: Color,
    val syntaxFunction: Color,
    val syntaxType: Color,
    val syntaxTag: Color,
) {
    companion object {
        val light = Palette(
            background = Color(0xFFF6F7F9),
            backgroundAccent = Color(0xFFE4E6EB),
            backgroundAccentStronger = Color(0xFFD9DCE3),
            backgroundAccentStrongest = Color(0xFFCED2DA),
            backgroundAccentLarger = Color(0xFFE8EAEE),
            backgroundAccentLargerStronger = Color(0xFFDFE2E8),
            backgroundSecondary = Color(0xFFEAECF0),
            backgroundSecondaryAccent = Color(0xFFDFE2E8),
            backgroundSecondaryAccentStronger = Color(0xFFD4D8E0),
            card = Color(0xFFFFFFFF),
            cardAccent = Color(0xFFE6E8EC),
            popover = Color(0xFFFFFFFF),
            popoverAccent = Color(0xFFEAECF0),
            foreground = Color(0xFF22242B),
            emphasizedForeground = Color(0xFF000000),
            mutedForeground = Color(0xFF606470),
            mutedStrongerForeground = Color(0xFF8E919D),
            mutedStrongestForeground = Color(0xFFADB1BC),
            primary = Color(0xFF2A5BD7),
            primaryForeground = Color(0xFFFFFFFF),
            destructive = Color(0xFFC62828),
            destructiveForeground = Color(0xFFFFFFFF),
            success = Color(0xFF047857),
            successForeground = Color(0xFFFFFFFF),
            warning = Color(0xFFC74F0A),
            warningForeground = Color(0xFFFFFFFF),
            pending = Color(0xFF947100),
            pendingForeground = Color(0xFFFFFFFF),
            process = Color(0xFF0284C7),
            processForeground = Color(0xFFFFFFFF),
            merged = Color(0xFF8250DF),
            mergedForeground = Color(0xFFFFFFFF),
            pink = Color(0xFFB7377C),
            skill = Color(0xFF47730D),
            border = Color(0xFFE2E3E5),
            borderCard = Color(0xFFE9EBF0),
            borderInput = Color(0xFFEAECF0),
            borderPopover = Color(0xFFE6E8EB),
            input = Color(0xFFE8EAEE),
            composer = Color(0xFFFFFFFF),
            borderComposer = Color(0xFFEAECF0),
            borderAccent = Color(0xFFD9DCE5),
            ring = Color(0xFF22242B),
            overlay = Color(0xFF000000),
            shadow = Color(0xFF000000),
            chart1 = Color(0xFF2A5BD7),
            chart2 = Color(0xFF047857),
            chart3 = Color(0xFFC74F0A),
            chart4 = Color(0xFF8250DF),
            chart5 = Color(0xFF0284C7),
            chart6 = Color(0xFFB7377C),
            mapLand = Color(0xFFEEF0F3),
            mapGreen = Color(0xFFBCE9B0),
            mapPark = Color(0xFFB7E7AA),
            mapCampus = Color(0xFFEDE8D7),
            mapWater = Color(0xFFA2DFF5),
            mapWaterway = Color(0xFF91D6EF),
            mapBuilding = Color(0xFFE3E6EB),
            mapStreet = Color(0xFFCDD1D9),
            mapRoad = Color(0xFFB1B9C8),
            mapFerry = Color(0xFF89CFE9),
            claude = Color(0xFFD97757),
            openai = Color(0xFF22242B),
            googleBlue = Color(0xFF4285F4),
            googleRed = Color(0xFFEA4335),
            googleYellow = Color(0xFFFBBC05),
            googleGreen = Color(0xFF34A853),
            linear = Color(0xFF22242B),
            windowClose = Color(0xFFFF5F57),
            windowMinimize = Color(0xFFFEBC2E),
            windowZoom = Color(0xFF28C840),
            syntaxComment = Color(0xFF6E7781),
            syntaxKeyword = Color(0xFFCF222E),
            syntaxString = Color(0xFF0A3069),
            syntaxConstant = Color(0xFF0550AE),
            syntaxFunction = Color(0xFF8250DF),
            syntaxType = Color(0xFF953800),
            syntaxTag = Color(0xFF116329),
        )

        val dark = Palette(
            background = Color(0xFF0A0B0F),
            backgroundAccent = Color(0xFF191A1F),
            backgroundAccentStronger = Color(0xFF212329),
            backgroundAccentStrongest = Color(0xFF2B2C32),
            backgroundAccentLarger = Color(0xFF14151A),
            backgroundAccentLargerStronger = Color(0xFF191A1F),
            backgroundSecondary = Color(0xFF111217),
            backgroundSecondaryAccent = Color(0xFF191A1F),
            backgroundSecondaryAccentStronger = Color(0xFF212227),
            card = Color(0xFF15161B),
            cardAccent = Color(0xFF1E1F26),
            popover = Color(0xFF191A1F),
            popoverAccent = Color(0xFF212329),
            foreground = Color(0xFFD0D2D9),
            emphasizedForeground = Color(0xFFFFFFFF),
            mutedForeground = Color(0xFF9A9EAB),
            mutedStrongerForeground = Color(0xFF646875),
            mutedStrongestForeground = Color(0xFF4A4E5A),
            primary = Color(0xFF4F7CFF),
            primaryForeground = Color(0xFFFFFFFF),
            destructive = Color(0xFFDF6C6A),
            destructiveForeground = Color(0xFFFFFFFF),
            success = Color(0xFF41B76F),
            successForeground = Color(0xFFFFFFFF),
            warning = Color(0xFFE98538),
            warningForeground = Color(0xFFFFFFFF),
            pending = Color(0xFFD3B142),
            pendingForeground = Color(0xFFFFFFFF),
            process = Color(0xFF2AA9E5),
            processForeground = Color(0xFFFFFFFF),
            merged = Color(0xFFB480FF),
            mergedForeground = Color(0xFFFFFFFF),
            pink = Color(0xFFF28CC0),
            skill = Color(0xFFA3E635),
            border = Color(0xFF191A1E),
            borderCard = Color(0xFF1E1F25),
            borderInput = Color(0xFF1A1B21),
            borderPopover = Color(0xFF222228),
            input = Color(0xFF14151A),
            composer = Color(0xFF111217),
            borderComposer = Color(0xFF1A1B21),
            borderAccent = Color(0xFF25262C),
            ring = Color(0xFFD0D2D9),
            overlay = Color(0xFF000000),
            shadow = Color(0xFF000000),
            chart1 = Color(0xFF4F7CFF),
            chart2 = Color(0xFF41B76F),
            chart3 = Color(0xFFE98538),
            chart4 = Color(0xFFB480FF),
            chart5 = Color(0xFF2AA9E5),
            chart6 = Color(0xFFF28CC0),
            mapLand = Color(0xFF18191E),
            mapGreen = Color(0xFF1F2B22),
            mapPark = Color(0xFF20301F),
            mapCampus = Color(0xFF26241F),
            mapWater = Color(0xFF1D3540),
            mapWaterway = Color(0xFF23414D),
            mapBuilding = Color(0xFF24252B),
            mapStreet = Color(0xFF34363D),
            mapRoad = Color(0xFF4A4E57),
            mapFerry = Color(0xFF2B5566),
            claude = Color(0xFFD97757),
            openai = Color(0xFFD0D2D9),
            googleBlue = Color(0xFF4285F4),
            googleRed = Color(0xFFEA4335),
            googleYellow = Color(0xFFFBBC05),
            googleGreen = Color(0xFF34A853),
            linear = Color(0xFFD0D2D9),
            windowClose = Color(0xFFFF5F57),
            windowMinimize = Color(0xFFFEBC2E),
            windowZoom = Color(0xFF28C840),
            syntaxComment = Color(0xFF8B949E),
            syntaxKeyword = Color(0xFFFF7B72),
            syntaxString = Color(0xFFA5D6FF),
            syntaxConstant = Color(0xFF79C0FF),
            syntaxFunction = Color(0xFFD2A8FF),
            syntaxType = Color(0xFFFFA657),
            syntaxTag = Color(0xFF7EE787),
        )
    }

    /** The colour of a token by its name in tokens.json, as OpenUI's scenes and trees name them. */
    fun named(name: String): Color? = when (name) {
        "background" -> background
        "background-accent" -> backgroundAccent
        "background-accent-stronger" -> backgroundAccentStronger
        "background-accent-strongest" -> backgroundAccentStrongest
        "background-accent-larger" -> backgroundAccentLarger
        "background-accent-larger-stronger" -> backgroundAccentLargerStronger
        "background-secondary" -> backgroundSecondary
        "background-secondary-accent" -> backgroundSecondaryAccent
        "background-secondary-accent-stronger" -> backgroundSecondaryAccentStronger
        "card" -> card
        "card-accent" -> cardAccent
        "popover" -> popover
        "popover-accent" -> popoverAccent
        "foreground" -> foreground
        "emphasized-foreground" -> emphasizedForeground
        "muted-foreground" -> mutedForeground
        "muted-stronger-foreground" -> mutedStrongerForeground
        "muted-strongest-foreground" -> mutedStrongestForeground
        "primary" -> primary
        "primary-foreground" -> primaryForeground
        "destructive" -> destructive
        "destructive-foreground" -> destructiveForeground
        "success" -> success
        "success-foreground" -> successForeground
        "warning" -> warning
        "warning-foreground" -> warningForeground
        "pending" -> pending
        "pending-foreground" -> pendingForeground
        "process" -> process
        "process-foreground" -> processForeground
        "merged" -> merged
        "merged-foreground" -> mergedForeground
        "pink" -> pink
        "skill" -> skill
        "border" -> border
        "border-card" -> borderCard
        "border-input" -> borderInput
        "border-popover" -> borderPopover
        "input" -> input
        "composer" -> composer
        "border-composer" -> borderComposer
        "border-accent" -> borderAccent
        "ring" -> ring
        "overlay" -> overlay
        "shadow" -> shadow
        "chart-1" -> chart1
        "chart-2" -> chart2
        "chart-3" -> chart3
        "chart-4" -> chart4
        "chart-5" -> chart5
        "chart-6" -> chart6
        "map-land" -> mapLand
        "map-green" -> mapGreen
        "map-park" -> mapPark
        "map-campus" -> mapCampus
        "map-water" -> mapWater
        "map-waterway" -> mapWaterway
        "map-building" -> mapBuilding
        "map-street" -> mapStreet
        "map-road" -> mapRoad
        "map-ferry" -> mapFerry
        "claude" -> claude
        "openai" -> openai
        "google-blue" -> googleBlue
        "google-red" -> googleRed
        "google-yellow" -> googleYellow
        "google-green" -> googleGreen
        "linear" -> linear
        "window-close" -> windowClose
        "window-minimize" -> windowMinimize
        "window-zoom" -> windowZoom
        "syntax-comment" -> syntaxComment
        "syntax-keyword" -> syntaxKeyword
        "syntax-string" -> syntaxString
        "syntax-constant" -> syntaxConstant
        "syntax-function" -> syntaxFunction
        "syntax-type" -> syntaxType
        "syntax-tag" -> syntaxTag
        else -> null
    }
}

/** How much of a colour shows: every opacity in the app is one of these. */
enum class Opacity(val light: Float, val dark: Float) {
    Overlay(0.5f, 0.6f),
    OverlayPulled(0.4f, 0.4f),
    Disabled(0.45f, 0.45f),
    Lit(0.88f, 0.88f),
    Shadow(0.06f, 0.2f),
    ShadowStronger(0.14f, 0.4f),
    ShadowStrongest(0.25f, 0.6f),
    ColorTint(0.12f, 0.1f),
    ColorTintStronger(0.2f, 0.16f),
    ColorTintChart(0.25f, 0.2f),
    InputGlassTint(0.8f, 0.8f),
    ShimmerBandEdge(0.12f, 0.12f),
    ShimmerBandMiddle(0.55f, 0.55f),
    Glow(0.4f, 0.4f),
    GlowFaint(0.3f, 0.3f),
    GlowBright(0.5f, 0.5f),
}

/** How round corners are: every radius is one of these. */
object Radius {
    val xs = 4.dp
    val sm = 6.dp
    val md = 8.dp
    val lg = 12.dp
    val xl = 16.dp
    val xxl = 24.dp
}

/** A shadow's size: how far down it falls and its blur, as CSS gives them. */
enum class ShadowSize(val down: Float, val blur: Float) {
    Sm(2.5f, 8f),
    Md(4f, 12f),
    Lg(8f, 24f),
    Xl(16f, 40f),
}
