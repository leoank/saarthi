package me.leoank.saarthi

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The blue-flame palette (ADR-0068): near-black tinted blue, surfaces a few percent apart
 *  and set off by hairlines, a gas-flame blue with its pale core, and the flame's hot amber
 *  kept for what needs the owner. Dark only. The colours Material has no role for are here;
 *  the rest are the scheme's roles below. */
object Flame {
    val Bg = Color(0xFF080B11)
    /** Recessed: code, inputs, the bar. */
    val S0 = Color(0xFF0B1018)
    /** Cards and sheets. */
    val S1 = Color(0xFF111824)
    /** Raised: chips, prompts, the prompt bar. */
    val S2 = Color(0xFF172131)
    val S3 = Color(0xFF1D2838)
    /** Hairlines between surfaces. */
    val Line = Color(0xFF1F2A3B)
    val LineStrong = Color(0xFF2E3B50)

    val Text = Color(0xFFE8EEF7)
    val Soft = Color(0xFFB9C4D4)
    val Dim = Color(0xFF8794A8)

    /** The flame's blue, the accent. */
    val Blue = Color(0xFF3D8BFF)
    /** Its pale core: highlights and the hot end of gradients. */
    val Core = Color(0xFF8FD8FF)
    val OnBlue = Color(0xFF04101F)
    /** What needs the owner, and nothing else. */
    val Hot = Color(0xFFFFB547)
    val Ok = Color(0xFF4ED6A0)
    val Bad = Color(0xFFFF6B6B)

    /** A hue per workspace, for its tab, icons and highlights. */
    val Agents = Color(0xFF5AA0FF)
    val Notebooks = Color(0xFFF2A65A)
    val Terminals = Color(0xFF54E0A6)
    val Nitya = Color(0xFFB69CFF)

    /** `c` at `alpha` over a card, for tinted fills and borders. */
    fun soft(c: Color, alpha: Float = 0.14f) = c.copy(alpha = alpha)
}

/** JetBrains Mono, for structure and facts (ADR-0069): machines, paths, branches, counts,
 *  states, rules, the bar, the prompt bar and code. What people and Claude wrote stays in
 *  the phone's sans face. */
val Mono = FontFamily(
    Font(R.font.jetbrains_mono_regular, FontWeight.Normal),
    Font(R.font.jetbrains_mono_medium, FontWeight.Medium),
    Font(R.font.jetbrains_mono_semibold, FontWeight.SemiBold),
    Font(R.font.jetbrains_mono_bold, FontWeight.Bold),
)

/** Machine names, paths and branches under a title. */
val MonoMeta = TextStyle(fontFamily = Mono, fontSize = 11.5.sp, lineHeight = 15.sp)

/** Small corners, as a terminal's (ADR-0069); sheets a little rounder. */
private val Corners = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(6.dp),
    medium = RoundedCornerShape(6.dp),
    large = RoundedCornerShape(8.dp),
    extraLarge = RoundedCornerShape(12.dp),
)

private val Scheme = darkColorScheme(
    primary = Flame.Blue,
    onPrimary = Flame.OnBlue,
    primaryContainer = Color(0xFF13294A),
    onPrimaryContainer = Color(0xFFCFE3FF),
    inversePrimary = Color(0xFF1F5FC4),
    secondary = Flame.Core,
    onSecondary = Flame.OnBlue,
    secondaryContainer = Flame.S2,
    onSecondaryContainer = Flame.Text,
    tertiary = Flame.Hot,
    onTertiary = Color(0xFF231500),
    tertiaryContainer = Color(0xFF322E29),
    onTertiaryContainer = Color(0xFFFFDDA8),
    background = Flame.Bg,
    onBackground = Flame.Text,
    surface = Flame.Bg,
    onSurface = Flame.Text,
    surfaceVariant = Flame.S1,
    onSurfaceVariant = Flame.Dim,
    surfaceTint = Flame.Blue,
    inverseSurface = Flame.Text,
    inverseOnSurface = Flame.Bg,
    surfaceBright = Flame.S3,
    surfaceDim = Flame.Bg,
    surfaceContainerLowest = Color(0xFF06080D),
    surfaceContainerLow = Flame.S0,
    surfaceContainer = Flame.S1,
    surfaceContainerHigh = Flame.S2,
    surfaceContainerHighest = Flame.S3,
    outline = Flame.LineStrong,
    outlineVariant = Flame.Line,
    scrim = Color(0xFF000000),
    error = Flame.Bad,
    onError = Color(0xFF2A0505),
    errorContainer = Color(0xFF3A1518),
    onErrorContainer = Color(0xFFFFD6D6),
)

@Composable
fun SaarthiTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Scheme, shapes = Corners, content = content)
}
