package org.fedimint.demo.ui.payments

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import kotlinx.coroutines.delay
import org.fedimint.sdk.OperationId

/** The frame every payment screen shares: title, back, scrolling content that clears the keyboard. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PaymentScreen(title: String, onBack: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            content = content,
        )
    }
}

/** A whole-sats amount. The screens convert to msats, the SDK's unit, where it asks for them. */
@Composable
fun SatsField(value: String, onValueChange: (String) -> Unit, enabled: Boolean, label: String = "Amount (sats)") {
    OutlinedTextField(
        value = value,
        onValueChange = { text -> onValueChange(text.filter(Char::isDigit).take(15)) },
        label = { Text(label) },
        singleLine = true,
        enabled = enabled,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
fun TextInput(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    enabled: Boolean,
    minLines: Int = 1,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = minLines == 1,
        minLines = minLines,
        enabled = enabled,
        keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, enabled: Boolean, working: Boolean) {
    Button(onClick = onClick, enabled = enabled && !working, modifier = Modifier.fillMaxWidth()) {
        if (working) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) else Text(text)
    }
}

@Composable
fun ErrorText(error: String?) {
    error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

/**
 * A quote for the user to approve: what they pay and what it costs, and how
 * long the offer stands. Past its expiry the SDK refuses it (QUOTE_EXPIRED),
 * so the countdown tells the user before they tap.
 */
@Composable
fun ReviewCard(review: PaymentViewModel.Review) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Review", style = MaterialTheme.typography.titleMedium)
            review.rows.forEach { (label, value) -> Line(label, value) }
            HorizontalDivider(Modifier.padding(vertical = 4.dp))
            Line("Total", review.total, bold = true)
            review.expiresAtMillis?.let { Countdown(it) }
        }
    }
}

@Composable
private fun Line(label: String, value: String, bold: Boolean = false) {
    Row(Modifier.fillMaxWidth()) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, fontWeight = if (bold) FontWeight.Bold else null)
    }
}

@Composable
private fun Countdown(expiresAtMillis: Long) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(expiresAtMillis) {
        while (now < expiresAtMillis) {
            delay(1_000)
            now = System.currentTimeMillis()
        }
    }
    val left = (expiresAtMillis - now) / 1_000
    Text(
        if (left > 0) "Quote valid for ${left / 60}:${"%02d".format(left % 60)}" else "Quote expired. Get a new one.",
        style = MaterialTheme.typography.bodySmall,
        color = if (left > 0) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error,
    )
}

/** Text to hand to the other party, as a QR code and a copy button. */
@Composable
fun OutputCard(output: PaymentViewModel.Output) {
    val clipboard = LocalClipboardManager.current
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text(output.label, style = MaterialTheme.typography.titleMedium, modifier = Modifier.fillMaxWidth())
            output.qr?.let { content ->
                val qr = remember(content) { qrBitmap(content) }
                if (qr != null) {
                    Image(
                        painter = BitmapPainter(qr),
                        contentDescription = "QR code",
                        modifier = Modifier.padding(vertical = 12.dp).size(240.dp).background(Color.White).padding(8.dp),
                    )
                } else {
                    Text(
                        "Too long for a QR code. Copy it instead.",
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
            }
            Text(
                output.text,
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 4,
                overflow = TextOverflow.Ellipsis,
            )
            OutlinedButton(
                onClick = { clipboard.setText(AnnotatedString(output.text)) },
                modifier = Modifier.padding(top = 8.dp),
            ) { Text("Copy") }
        }
    }
}

/** The live state of the operation this screen started. */
@Composable
fun ProgressCard(progress: OpProgress) {
    val scheme = MaterialTheme.colorScheme
    val container = when {
        !progress.settled -> scheme.secondaryContainer
        progress.ok -> scheme.primaryContainer
        else -> scheme.errorContainer
    }
    Card(colors = CardDefaults.cardColors(containerColor = container), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(progress.label, style = MaterialTheme.typography.titleMedium)
            progress.detail?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
            if (!progress.settled) {
                val fraction = progress.fraction
                if (fraction != null) {
                    LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth().height(4.dp))
                } else {
                    LinearProgressIndicator(Modifier.fillMaxWidth().height(4.dp))
                }
            }
        }
    }
}

/** A QR code, or null when the text is past what a scannable code can hold. */
private fun qrBitmap(text: String, size: Int = 512): ImageBitmap? {
    if (text.length > MAX_QR_CHARS) return null
    val matrix = runCatching {
        QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, mapOf(EncodeHintType.MARGIN to 0))
    }.getOrNull() ?: return null
    val pixels = IntArray(matrix.width * matrix.height) { i ->
        if (matrix[i % matrix.width, i / matrix.width]) android.graphics.Color.BLACK else android.graphics.Color.WHITE
    }
    return Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888).asImageBitmap()
}

/** Well inside QR's ~2.9 KB binary limit, past which phone cameras struggle anyway. */
private const val MAX_QR_CHARS = 1_500

/** Back to where the user came from, once the operation has settled. */
@Composable
fun DoneButton(state: PaymentViewModel.UiState, onDone: () -> Unit) {
    if (state.progress?.settled == true) {
        OutlinedButton(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text("Done") }
    }
}

/**
 * Shown once an operation exists but this screen has no state for it yet, or
 * lost its updates. It points at the existing operation; it never offers
 * anything that could send again.
 */
@Composable
fun SubmittedNotice(state: PaymentViewModel.UiState, onOpenOperation: (OperationId) -> Unit) {
    val id = state.operationId ?: return
    if (!state.needsActivityLink) return
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                if (state.followFailed) {
                    "This was submitted, but its live status couldn't be loaded here. It carries on regardless."
                } else {
                    "Submitted. Waiting for its first status update…"
                },
                style = MaterialTheme.typography.bodyMedium,
            )
            OutlinedButton(onClick = { onOpenOperation(id) }) { Text("View in Activity") }
        }
    }
}
