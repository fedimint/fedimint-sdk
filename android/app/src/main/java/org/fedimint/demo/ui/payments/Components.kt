package org.fedimint.demo.ui.payments

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.delay
import org.fedimint.demo.R
import org.fedimint.demo.ui.common.AmountHero
import org.fedimint.demo.ui.common.ButtonStyle
import org.fedimint.demo.ui.common.BottomActions
import org.fedimint.demo.ui.common.DetailRow
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.ErrorMessage
import org.fedimint.demo.ui.common.MutedText
import org.fedimint.demo.ui.common.Overline
import org.fedimint.demo.ui.common.ProgressBar
import org.fedimint.demo.ui.common.QrScanner
import org.fedimint.demo.ui.common.StatusPill
import org.fedimint.demo.ui.common.StepState
import org.fedimint.demo.ui.common.TimelineStep
import org.fedimint.demo.ui.common.Tone
import org.fedimint.demo.ui.common.WalletButton
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.common.WalletIcon
import org.fedimint.demo.ui.common.copySecret
import org.fedimint.demo.ui.common.formatSats
import org.fedimint.demo.ui.theme.WalletText
import org.fedimint.sdk.Amount
import org.fedimint.sdk.OperationId

/**
 * One rail's body inside the Send or Receive screen: scrolling content, and
 * its actions pinned at the bottom.
 */
@Composable
fun PaymentPane(bottom: @Composable ColumnScope.() -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            content = content,
        )
        BottomActions(bottom)
    }
}

/** An operation state as a pill: amber while it runs, green when it went well, red when not. */
@Composable
fun ProgressPill(progress: OpProgress, modifier: Modifier = Modifier) =
    StatusPill(progress.label, progress.tone(), modifier)

fun OpProgress.tone(): Tone = when {
    !settled -> Tone.Warning
    ok -> Tone.Success
    else -> Tone.Danger
}

/**
 * The live state of an operation: its pill and detail, a bar while it runs,
 * and the states this screen has watched it pass through.
 */
@Composable
fun ProgressCard(progress: OpProgress, steps: List<PaymentViewModel.Step> = emptyList()) {
    WalletCard(padding = PaddingValues(16.dp), spacing = 12.dp) {
        ProgressPill(progress)
        progress.detail?.let { MutedText(it) }
        if (!progress.settled) ProgressBar(progress.fraction)
        if (steps.size > 1) {
            CardDivider()
            Timeline(steps)
        }
    }
}

/** The observed states as a list, the last one current unless it is final. */
@Composable
fun Timeline(steps: List<PaymentViewModel.Step>) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        steps.forEachIndexed { i, step ->
            val last = i == steps.lastIndex
            val state = when {
                !last -> StepState.Done
                !step.progress.settled -> StepState.Current
                step.progress.ok -> StepState.Done
                else -> StepState.Failed
            }
            TimelineStep(step.progress.label, if (state == StepState.Current) null else clockTime(step.atMillis), state)
        }
    }
}

private fun clockTime(millis: Long): String = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date(millis))

/**
 * A quote for the user to approve: the amount large, then what it costs and
 * the total, and how long the offer stands. Past its expiry the SDK refuses it
 * (QUOTE_EXPIRED), so the countdown tells the user before they tap.
 */
@Composable
fun ReviewContent(review: PaymentViewModel.Review, heading: String = "You're sending") {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Overline(heading)
        AmountHero(review.amount ?: review.total.removeSuffix(" sats"))
    }
    WalletCard(padding = PaddingValues(horizontal = 16.dp)) {
        review.rows.forEach { (label, value) ->
            DetailRow(label, value, mono = label in MONO_ROWS)
            CardDivider()
        }
        DetailRow("Total", review.total, strong = true)
    }
    review.expiresAtMillis?.let { QuoteCountdown(it) }
}

private val MONO_ROWS = setOf("To", "Gateway", "Fee", "Network fee", "Fees")

@Composable
private fun QuoteCountdown(expiresAtMillis: Long) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(expiresAtMillis) {
        while (now < expiresAtMillis) {
            delay(1_000)
            now = System.currentTimeMillis()
        }
    }
    val left = (expiresAtMillis - now) / 1_000
    val expired = left <= 0
    val color = if (expired) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        WalletIcon(R.drawable.ic_refresh, contentDescription = null, size = 16.dp, tint = color)
        Text(
            if (expired) "Quote expired. Cancel and review again." else "Quote locked · expires in ${left / 60}:${"%02d".format(left % 60)}",
            style = WalletText.mono,
            color = color,
        )
    }
}

/** "Available: 21,500 sats", under a send's amount. */
@Composable
fun AvailableBalance(balance: Amount?) {
    balance?.let {
        Text(
            "Available: ${formatSats(it)} sats",
            style = WalletText.mono,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/** Paste and Scan, side by side, under an input that takes a code. */
@Composable
fun PasteScanRow(enabled: Boolean, onText: (String) -> Unit) {
    val clipboard = LocalClipboardManager.current
    var scanning by remember { mutableStateOf(false) }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        WalletButton(
            "Paste",
            onClick = { clipboard.getText()?.text?.trim()?.takeIf { it.isNotEmpty() }?.let(onText) },
            style = ButtonStyle.Secondary,
            enabled = enabled,
            icon = R.drawable.ic_copy,
            modifier = Modifier.weight(1f),
        )
        WalletButton(
            "Scan",
            onClick = { scanning = true },
            style = ButtonStyle.Secondary,
            enabled = enabled,
            icon = R.drawable.ic_scan,
            modifier = Modifier.weight(1f),
        )
    }
    if (scanning) {
        QrScanner(
            onResult = {
                scanning = false
                onText(it)
            },
            onDismiss = { scanning = false },
        )
    }
}

/** Copies text that isn't a secret (an invoice, an address, an id) and says so. */
@Composable
fun rememberCopy(): (String) -> Unit {
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    return { text ->
        clipboard.setText(AnnotatedString(text))
        Toast.makeText(context, "Copied", Toast.LENGTH_SHORT).show()
    }
}

/**
 * Copies ecash notes the way the recovery phrase is copied (SECURITY.md, rule
 * 4): they are a bearer instrument, so flagged sensitive and cleared a minute
 * later if still on the clipboard.
 */
@Composable
fun rememberCopyNotes(): (String) -> Unit {
    val context = LocalContext.current
    return { notes ->
        copySecret(context, "Ecash notes", notes)
        Toast.makeText(context, "Copied. Cleared from the clipboard after a minute.", Toast.LENGTH_SHORT).show()
    }
}

@Composable
fun ErrorText(error: String?) = ErrorMessage(error)

/** The settled state's detail and a way back, once the operation needs nothing more. */
@Composable
fun ColumnScope.DoneButton(state: PaymentViewModel.UiState, onDone: () -> Unit) {
    if (state.progress?.settled == true) WalletButton("Done", onDone, style = ButtonStyle.Secondary)
}

/**
 * Shown once an operation exists but this screen has no state for it yet, or
 * lost its updates. It points at the existing operation; it never offers
 * anything that could send again.
 */
@Composable
fun SubmittedNotice(
    state: PaymentViewModel.UiState,
    onOpenOperation: (OperationId) -> Unit,
    onCheckAgain: () -> Unit,
) {
    when {
        state.checkingSubmission -> {
            WalletCard(padding = PaddingValues(16.dp), spacing = 12.dp) {
                MutedText(
                    if (state.submissionCheckFailed) {
                        "The app closed while this was being submitted, and the wallet couldn't " +
                            "check yet whether it went through. Nothing more will be sent from here."
                    } else {
                        "The app closed while this was being submitted. Checking whether it went through…"
                    },
                )
                if (state.submissionCheckFailed) WalletButton("Check again", onCheckAgain, style = ButtonStyle.Secondary)
            }
            return
        }
        state.submissionUnclear -> {
            WalletCard(padding = PaddingValues(16.dp), spacing = 12.dp) {
                MutedText(
                    "The app closed while this was being submitted, and the wallet can't tell from " +
                        "its history whether it went through. Check the Activity tab before paying " +
                        "again. Nothing more will be sent from here.",
                )
                WalletButton("Check again", onCheckAgain, style = ButtonStyle.Secondary)
            }
            return
        }
        state.notSubmitted -> {
            MutedText("The app closed before this was submitted. Nothing was sent.")
            return
        }
    }
    val id = state.operationId ?: return
    if (!state.needsActivityLink) return
    WalletCard(padding = PaddingValues(16.dp), spacing = 12.dp) {
        MutedText(
            when {
                state.restored ->
                    "This was submitted before the app was closed. It carries on regardless; " +
                        "follow it in Activity."
                state.followFailed ->
                    "This was submitted, but its live status couldn't be loaded here. It carries on regardless."
                else -> "Submitted. Waiting for its first status update…"
            },
        )
        WalletButton("View in Activity", { onOpenOperation(id) }, style = ButtonStyle.Secondary)
    }
}
