package org.fedimint.demo.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.fedimint.demo.R

/**
 * The shared design for the demo wallets (one Figma file for iOS, Android and
 * web): its colour, type and shape tokens mapped onto Material 3 roles, so the
 * screens keep using `MaterialTheme` and pick the design up from here.
 *
 * The design is light only. Dark mode keeps Material's baseline dark scheme
 * until the design has dark tokens.
 */
@Composable
fun FedimintTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    CompositionLocalProvider(LocalWalletColors provides if (dark) DarkWalletColors else LightWalletColors) {
        MaterialTheme(
            colorScheme = if (dark) darkColorScheme() else LightColors,
            typography = WalletTypography,
            shapes = WalletShapes,
            content = content,
        )
    }
}

// Figma variables (color/*), by name.
private val Bg = Color(0xFFF6F6F2)
private val Surface = Color.White
private val Surface2 = Color(0xFFEEEEE8)
private val Border = Color(0xFFE4E4DC)
private val Text = Color(0xFF0B0B1F)
private val TextMuted = Color(0xFF65657A)
private val Primary = Color(0xFF181884)
private val PrimarySoft = Color(0xFFE8E8F6)
private val Danger = Color(0xFFC0302B)
private val DangerSoft = Color(0xFFFAE3E1)
private val Success = Color(0xFF0E7A4B)
private val SuccessSoft = Color(0xFFDDF1E6)
private val Warning = Color(0xFF9A5800)
private val WarningSoft = Color(0xFFFAEED7)
private val Hero = Color(0xFF0A0A2A)
private val OnHeroMuted = Color(0xFF9EA2CC)
private val Accent = Color(0xFF4AD6FF)

private val LightColors = lightColorScheme(
    primary = Primary,
    onPrimary = Color.White,
    primaryContainer = PrimarySoft,
    onPrimaryContainer = Primary,
    secondaryContainer = PrimarySoft,
    onSecondaryContainer = Primary,
    tertiary = Success,
    tertiaryContainer = SuccessSoft,
    onTertiaryContainer = Success,
    error = Danger,
    errorContainer = DangerSoft,
    onErrorContainer = Danger,
    background = Bg,
    onBackground = Text,
    // Screens sit on the background colour; cards are white on it.
    surface = Bg,
    onSurface = Text,
    surfaceVariant = Surface2,
    onSurfaceVariant = TextMuted,
    surfaceContainerLowest = Surface,
    surfaceContainerLow = Surface,
    surfaceContainer = Surface,
    surfaceContainerHigh = Surface,
    surfaceContainerHighest = Surface,
    outline = Border,
    outlineVariant = Border,
)

/** The design's colours that Material has no role for. */
@Immutable
data class WalletColors(
    val success: Color,
    val successSoft: Color,
    val warning: Color,
    val warningSoft: Color,
    /** The dark brand surface: Welcome, federation avatars, selected chips. */
    val hero: Color,
    val onHero: Color,
    val onHeroMuted: Color,
    val accent: Color,
)

private val LightWalletColors = WalletColors(
    success = Success,
    successSoft = SuccessSoft,
    warning = Warning,
    warningSoft = WarningSoft,
    hero = Hero,
    onHero = Color.White,
    onHeroMuted = OnHeroMuted,
    accent = Accent,
)

private val DarkWalletColors = LightWalletColors.copy(
    success = Color(0xFF6FD3A0),
    successSoft = Color(0xFF123526),
    warning = Color(0xFFF2B866),
    warningSoft = Color(0xFF3A2A10),
    hero = Color(0xFF26264A),
)

val LocalWalletColors = staticCompositionLocalOf { LightWalletColors }

/** The design's extra colours: `MaterialTheme.walletColors.success`. */
val MaterialTheme.walletColors: WalletColors
    @Composable get() = LocalWalletColors.current

/** Geist (OFL, licence shipped in assets/licenses/GEIST_LICENSE.txt): every platform's demo uses the same faces. */
val UiFont = FontFamily(
    Font(R.font.geist_regular, FontWeight.Normal),
    Font(R.font.geist_medium, FontWeight.Medium),
    Font(R.font.geist_semibold, FontWeight.SemiBold),
)

/** Amounts, ids, seed-word indexes and overlines. */
val MonoFont = FontFamily(
    Font(R.font.geist_mono_regular, FontWeight.Normal),
    Font(R.font.geist_mono_medium, FontWeight.Medium),
)

private fun style(size: Int, line: Int, weight: FontWeight, tracking: Double = 0.0, family: FontFamily = UiFont) =
    TextStyle(fontFamily = family, fontSize = size.sp, lineHeight = line.sp, fontWeight = weight, letterSpacing = tracking.sp)

// Figma text styles, by name; tracking converted from percent to sp.
private val WalletTypography = Typography(
    displaySmall = style(44, 48, FontWeight.Medium, -1.76, MonoFont), // balance and amount heroes
    headlineLarge = style(30, 36, FontWeight.SemiBold, -0.75), // Title/Large
    headlineMedium = style(26, 32, FontWeight.SemiBold, -0.5),
    titleLarge = style(20, 26, FontWeight.SemiBold, -0.3), // Title/Medium
    titleMedium = style(16, 22, FontWeight.Medium, -0.08), // Body/Strong
    bodyLarge = style(16, 24, FontWeight.Normal, -0.08), // Body/Regular
    bodyMedium = style(14, 20, FontWeight.Normal), // Body/Small
    bodySmall = style(12, 16, FontWeight.Normal),
    labelLarge = style(16, 22, FontWeight.Medium, -0.08), // button labels (Body/Strong)
    labelMedium = style(14, 20, FontWeight.Medium, -0.03), // Label/Medium
    labelSmall = style(12, 16, FontWeight.Medium), // Caption
)

/** The design's text styles Material has no slot for. */
object WalletText {
    /** Section labels: upper case, spaced out. */
    val overline = style(11, 16, FontWeight.Medium, 0.66, MonoFont)

    /** Ids, addresses, notes, timestamps. */
    val mono = style(13, 20, FontWeight.Normal, family = MonoFont)

    /** Amounts in rows. */
    val amount = style(15, 20, FontWeight.Medium, -0.15, MonoFont)
}

// radius/sm 8, radius/md 12 (buttons, inputs), cards 16.
private val WalletShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(24.dp),
)
