package org.fedimint.demo.ui.common

import androidx.annotation.DrawableRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.fedimint.demo.R
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.demo.ui.theme.walletColors

// The shared demo design's components (Figma: "Shared components"), built once
// here so every screen uses the same ones, as the iOS and web demos do.

/** One of the design's 24 px line icons (res/drawable/ic_*), tinted like text. */
@Composable
fun WalletIcon(
    @DrawableRes id: Int,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    size: Dp = 24.dp,
    tint: Color = LocalContentColor.current,
) = Icon(painterResource(id), contentDescription, modifier.size(size), tint)

/**
 * A full screen: optional header with back, a scrolling body, and actions
 * pinned to the bottom that ride above the keyboard.
 */
@Composable
fun WalletScreen(
    title: String? = null,
    onBack: (() -> Unit)? = null,
    backEnabled: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
    scroll: Boolean = true,
    spacing: Dp = 16.dp,
    bottom: (@Composable ColumnScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .imePadding(),
    ) {
        if (title != null || onBack != null || trailing != null) NavHeader(title.orEmpty(), onBack, backEnabled, trailing)
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .then(if (scroll) Modifier.verticalScroll(rememberScrollState()) else Modifier)
                .padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(spacing),
            content = content,
        )
        if (bottom != null) BottomActions(bottom)
    }
}

/** Actions under a screen's content, above the navigation bar. */
@Composable
fun BottomActions(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

/** The header of a pushed screen: back, a centred title, an optional action. */
@Composable
fun NavHeader(
    title: String,
    onBack: (() -> Unit)?,
    backEnabled: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
            if (onBack != null) {
                IconButton(onClick = onBack, enabled = backEnabled) {
                    WalletIcon(R.drawable.ic_chevron_left, contentDescription = "Back")
                }
            }
        }
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) { trailing?.invoke() }
    }
}

/** A tab's own large title, with an optional action on the right. */
@Composable
fun TabTitle(text: String, action: (@Composable () -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = MaterialTheme.typography.headlineLarge, modifier = Modifier.weight(1f))
        action?.invoke()
    }
}

@Composable
fun ScreenTitle(text: String, modifier: Modifier = Modifier) =
    Text(text, style = MaterialTheme.typography.headlineLarge, modifier = modifier)

@Composable
fun MutedText(text: String, modifier: Modifier = Modifier, textAlign: TextAlign? = null) = Text(
    text,
    style = MaterialTheme.typography.bodyMedium,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
    textAlign = textAlign,
    modifier = modifier,
)

@Composable
fun ErrorMessage(text: String?) {
    text?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error) }
}

/** A section label: upper case, monospaced, spaced out. */
@Composable
fun Overline(text: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) =
    Text(text.uppercase(), style = WalletText.overline, color = color, modifier = modifier)

/** An overline with a link on the right ("Recent activity · See all"). */
@Composable
fun SectionHeader(title: String, action: String? = null, onAction: () -> Unit = {}) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Overline(title, Modifier.weight(1f))
        if (action != null) {
            Text(
                action,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.clip(MaterialTheme.shapes.small).clickable(onClick = onAction).padding(4.dp),
            )
        }
    }
}

/** White, bordered, 16 dp corners: the container for rows and details. */
@Composable
fun WalletCard(
    modifier: Modifier = Modifier,
    padding: PaddingValues = PaddingValues(0.dp),
    color: Color = MaterialTheme.colorScheme.surfaceContainerLowest,
    border: Color? = MaterialTheme.colorScheme.outline,
    spacing: Dp = 0.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val shape = MaterialTheme.shapes.large
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(color)
            .then(if (border != null) Modifier.border(1.dp, border, shape) else Modifier)
            .padding(padding),
        verticalArrangement = Arrangement.spacedBy(spacing),
        content = content,
    )
}

@Composable
fun CardDivider() = HorizontalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)

/** Key on the left, value on the right; long values wrap under their own width. */
@Composable
fun DetailRow(
    key: String,
    value: String,
    mono: Boolean = false,
    strong: Boolean = false,
    valueColor: Color = Color.Unspecified,
    onCopy: (() -> Unit)? = null,
) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(key, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(
            value,
            style = when {
                mono -> WalletText.mono
                strong -> MaterialTheme.typography.titleMedium
                else -> MaterialTheme.typography.labelMedium
            },
            color = valueColor,
            textAlign = TextAlign.End,
            modifier = Modifier.weight(1f),
        )
        if (onCopy != null) {
            WalletIcon(
                R.drawable.ic_copy,
                contentDescription = "Copy $key",
                size = 18.dp,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.clip(CircleShape).clickable(onClick = onCopy),
            )
        }
    }
}

/** Rows in a card, divided: the design's detail list. */
@Composable
fun DetailCard(rows: List<Pair<String, String>>, mono: Boolean = false, strongLast: Boolean = false) {
    WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
        rows.forEachIndexed { i, (key, value) ->
            if (i > 0) CardDivider()
            DetailRow(key, value, mono = mono, strong = strongLast && i == rows.lastIndex)
        }
    }
}

enum class ButtonStyle { Primary, Secondary, Soft, Danger, Inverse, OutlineOnDark }

/** The design's 52 dp button, in its six styles, with an optional icon and a busy state. */
@Composable
fun WalletButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth(),
    style: ButtonStyle = ButtonStyle.Primary,
    enabled: Boolean = true,
    working: Boolean = false,
    @DrawableRes icon: Int? = null,
) {
    val scheme = MaterialTheme.colorScheme
    val wallet = MaterialTheme.walletColors
    val (container, content) = when (style) {
        ButtonStyle.Primary -> scheme.primary to scheme.onPrimary
        ButtonStyle.Secondary -> scheme.surfaceContainerLowest to scheme.onSurface
        ButtonStyle.Soft -> scheme.primaryContainer to scheme.primary
        ButtonStyle.Danger -> scheme.errorContainer to scheme.error
        ButtonStyle.Inverse -> wallet.onHero to wallet.hero
        ButtonStyle.OutlineOnDark -> Color.Transparent to wallet.onHero
    }
    val border = when (style) {
        ButtonStyle.Secondary -> BorderStroke(1.dp, scheme.outline)
        ButtonStyle.OutlineOnDark -> BorderStroke(1.dp, wallet.onHeroMuted)
        else -> null
    }
    Button(
        onClick = onClick,
        enabled = enabled && !working,
        shape = MaterialTheme.shapes.medium,
        border = border,
        colors = ButtonDefaults.buttonColors(
            containerColor = container,
            contentColor = content,
            disabledContainerColor = if (style == ButtonStyle.Primary) scheme.primary.copy(alpha = 0.35f) else container,
            disabledContentColor = if (style == ButtonStyle.Primary) scheme.onPrimary else content.copy(alpha = 0.4f),
        ),
        contentPadding = PaddingValues(horizontal = 20.dp),
        modifier = modifier.height(52.dp),
    ) {
        if (working) {
            CircularProgressIndicator(Modifier.size(20.dp), color = content, strokeWidth = 2.dp)
        } else {
            if (icon != null) {
                WalletIcon(icon, contentDescription = null, size = 20.dp)
                Spacer(Modifier.width(8.dp))
            }
            Text(text, style = MaterialTheme.typography.labelLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

enum class Tone { Success, Warning, Neutral, Danger }

/** A small rounded status label with a dot: federation and operation states. */
@Composable
fun StatusPill(text: String, tone: Tone, modifier: Modifier = Modifier) {
    val scheme = MaterialTheme.colorScheme
    val wallet = MaterialTheme.walletColors
    val (container, content) = when (tone) {
        Tone.Success -> wallet.successSoft to wallet.success
        Tone.Warning -> wallet.warningSoft to wallet.warning
        Tone.Neutral -> scheme.surfaceVariant to scheme.onSurfaceVariant
        Tone.Danger -> scheme.errorContainer to scheme.error
    }
    Row(
        modifier.clip(CircleShape).background(container).padding(horizontal = 10.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(content))
        Text(text, style = MaterialTheme.typography.labelSmall, color = content)
    }
}

/** An icon on a soft square: the leading slot of activity rows and the operation header. */
@Composable
fun IconTile(@DrawableRes icon: Int, size: Dp = 40.dp, iconSize: Dp = 20.dp, radius: Dp = 10.dp) {
    Box(
        Modifier.size(size).clip(RoundedCornerShape(radius)).background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) { WalletIcon(icon, contentDescription = null, size = iconSize) }
}

/** A federation's initials on the dark brand square. */
@Composable
fun FederationAvatar(name: String?, size: Dp = 40.dp) {
    val wallet = MaterialTheme.walletColors
    Box(
        Modifier.size(size).clip(RoundedCornerShape(size / 4)).background(wallet.hero),
        contentAlignment = Alignment.Center,
    ) {
        Text(initials(name), style = WalletText.amount, color = wallet.onHero)
    }
}

/** "E-Cash Club" → "EC": the first letters of the first two words. */
fun initials(name: String?): String =
    name?.split(Regex("[\\s\\-_]+"))?.filter { it.isNotEmpty() }?.take(2)?.joinToString("") { it.take(1) }
        ?.uppercase()?.ifEmpty { null } ?: "?"

/** The big number: `21,500 sats`, optionally signed and coloured. */
@Composable
fun AmountHero(amount: String, modifier: Modifier = Modifier, color: Color = Color.Unspecified, unit: String = "sats") {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(amount, style = MaterialTheme.typography.displaySmall, color = color, modifier = Modifier.alignByBaseline())
        Text(
            unit,
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.alignByBaseline(),
        )
    }
}

/**
 * The big amount, editable: whole sats, digits only. Grows with what is typed
 * and shows a muted 0 when empty.
 */
@Composable
fun AmountInput(value: String, onValueChange: (String) -> Unit, enabled: Boolean, modifier: Modifier = Modifier) {
    val style = MaterialTheme.typography.displaySmall.copy(color = MaterialTheme.colorScheme.onSurface)
    // Sized to what is typed (or the 0 placeholder), measured: intrinsic sizing
    // clips the first digit under the style's negative tracking.
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val width = with(density) { measurer.measure(value.ifEmpty { "0" }, style).size.width.toDp() } + 4.dp
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        BasicTextField(
            value = value,
            onValueChange = { text -> onValueChange(text.filter(Char::isDigit).take(15)) },
            enabled = enabled,
            singleLine = true,
            textStyle = style,
            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.width(width).alignByBaseline(),
            decorationBox = { field ->
                Box(contentAlignment = Alignment.Center) {
                    if (value.isEmpty()) Text("0", style = style.copy(color = MaterialTheme.colorScheme.outline))
                    field()
                }
            },
        )
        Text(
            "sats",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.alignByBaseline(),
        )
    }
}

/** Label above a white, bordered field; [mono] for codes, invoices and addresses. */
@Composable
fun WalletTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String? = null,
    enabled: Boolean = true,
    placeholder: String? = null,
    minLines: Int = 1,
    maxLines: Int = if (minLines == 1) 1 else 6,
    mono: Boolean = false,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    supportingText: String? = null,
) {
    val scheme = MaterialTheme.colorScheme
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        label?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = scheme.onSurfaceVariant) }
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = enabled,
            singleLine = minLines == 1 && maxLines == 1,
            minLines = minLines,
            maxLines = maxLines,
            textStyle = if (mono) WalletText.mono.copy(fontSize = MaterialTheme.typography.bodyMedium.fontSize) else MaterialTheme.typography.bodyLarge,
            placeholder = placeholder?.let { { Text(it, color = scheme.onSurfaceVariant) } },
            supportingText = supportingText?.let { { Text(it) } },
            shape = MaterialTheme.shapes.medium,
            keyboardOptions = keyboardOptions,
            colors = OutlinedTextFieldDefaults.colors(
                focusedContainerColor = scheme.surfaceContainerLowest,
                unfocusedContainerColor = scheme.surfaceContainerLowest,
                disabledContainerColor = scheme.surfaceContainerLowest,
                focusedBorderColor = scheme.primary,
                unfocusedBorderColor = scheme.outline,
                disabledBorderColor = scheme.outline,
                disabledTextColor = scheme.onSurface.copy(alpha = 0.7f),
            ),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/**
 * Lightning / Ecash / Onchain. Only the options passed are shown: the design
 * hides a module the federation doesn't run rather than disabling it.
 */
@Composable
fun <T> SegmentedControl(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    onSelect: (T) -> Unit,
    enabled: Boolean = true,
) {
    val scheme = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().height(44.dp).clip(MaterialTheme.shapes.medium).background(scheme.surfaceVariant).padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        options.forEach { option ->
            val isSelected = option == selected
            val shape = RoundedCornerShape(9.dp)
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clip(shape)
                    .then(
                        if (isSelected) {
                            Modifier.background(scheme.surfaceContainerLowest).border(1.dp, scheme.outline, shape)
                        } else {
                            Modifier
                        },
                    )
                    .clickable(enabled = enabled && !isSelected) { onSelect(option) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    label(option),
                    style = MaterialTheme.typography.labelMedium,
                    color = if (isSelected) scheme.onSurface else scheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** A pill-shaped toggle, dark when selected: filters and the 12/24-word switch. */
@Composable
fun ChoicePill(text: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val scheme = MaterialTheme.colorScheme
    val wallet = MaterialTheme.walletColors
    Box(
        modifier
            .clip(CircleShape)
            .background(if (selected) wallet.hero else scheme.surfaceContainerLowest)
            .then(if (selected) Modifier else Modifier.border(1.dp, scheme.outline, CircleShape))
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = MaterialTheme.typography.labelMedium, color = if (selected) wallet.onHero else scheme.onSurface)
    }
}

/** A 52 dp tile to pick, dark when selected: the backup check's word choices. */
@Composable
fun ChoiceTile(text: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val scheme = MaterialTheme.colorScheme
    val wallet = MaterialTheme.walletColors
    val shape = MaterialTheme.shapes.medium
    Box(
        modifier
            .height(52.dp)
            .clip(shape)
            .background(if (selected) wallet.hero else scheme.surfaceContainerLowest)
            .then(if (selected) Modifier else Modifier.border(1.dp, scheme.outline, shape))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = MaterialTheme.typography.titleMedium, color = if (selected) wallet.onHero else scheme.onSurface)
    }
}

/** A thin rounded bar; [fraction] null runs indeterminate. */
@Composable
fun ProgressBar(fraction: Float?, color: Color = MaterialTheme.colorScheme.primary) {
    val modifier = Modifier.fillMaxWidth().height(6.dp).clip(CircleShape)
    val track = MaterialTheme.colorScheme.surfaceVariant
    if (fraction != null) {
        LinearProgressIndicator(progress = { fraction }, modifier = modifier, color = color, trackColor = track, drawStopIndicator = {}, gapSize = 0.dp)
    } else {
        LinearProgressIndicator(modifier = modifier, color = color, trackColor = track, gapSize = 0.dp)
    }
}

/** A step in a progress list: a dot, what happened, and when. */
@Composable
fun TimelineStep(label: String, time: String?, state: StepState) {
    val wallet = MaterialTheme.walletColors
    val scheme = MaterialTheme.colorScheme
    val color = when (state) {
        StepState.Done -> wallet.success
        StepState.Current -> wallet.warning
        StepState.Failed -> scheme.error
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(
            Modifier
                .size(10.dp)
                .clip(CircleShape)
                .then(
                    if (state == StepState.Current) Modifier.border(2.dp, color, CircleShape) else Modifier.background(color),
                ),
        )
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge,
            color = if (state == StepState.Done) scheme.onSurface else color,
            modifier = Modifier.weight(1f),
        )
        Text(time ?: "…", style = WalletText.mono, color = scheme.onSurfaceVariant)
    }
}

enum class StepState { Done, Current, Failed }

/** A centred spinner for a loading screen or section. */
@Composable
fun Loading(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
    }
}

