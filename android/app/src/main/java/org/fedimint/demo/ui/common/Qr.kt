package org.fedimint.demo.ui.common

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.EncodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import com.google.zxing.qrcode.QRCodeWriter
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import org.fedimint.demo.R
import org.fedimint.demo.ui.theme.WalletText

/**
 * The card that hands something over: a QR code, the text shortened beneath
 * it with a copy button, and an optional status under that.
 */
@Composable
fun QrCard(
    qr: String?,
    text: String,
    onCopy: () -> Unit,
    status: (@Composable () -> Unit)? = null,
) {
    WalletCard(padding = PaddingValues(20.dp), spacing = 16.dp) {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val bitmap = remember(qr) { qr?.let(::qrBitmap) }
            when {
                bitmap != null -> Image(
                    painter = BitmapPainter(bitmap),
                    contentDescription = "QR code",
                    modifier = Modifier.size(200.dp).background(Color.White),
                )
                qr != null -> MutedText("Too long for a QR code. Copy or share it instead.", textAlign = TextAlign.Center)
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    shortId(text, keep = 14),
                    style = WalletText.mono,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = onCopy) { WalletIcon(R.drawable.ic_copy, contentDescription = "Copy", size = 20.dp) }
            }
            status?.invoke()
        }
    }
}

/** A QR code, or null when the text is past what a scannable code can hold. */
fun qrBitmap(text: String, size: Int = 512): ImageBitmap? {
    if (text.length > MAX_QR_CHARS) return null
    val matrix = runCatching {
        QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, mapOf(EncodeHintType.MARGIN to 0))
    }.getOrNull() ?: return null
    val pixels = IntArray(matrix.width * matrix.height) { i ->
        if (matrix[i % matrix.width, i / matrix.width]) 0xFF0B0B1F.toInt() else android.graphics.Color.WHITE
    }
    return Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888).asImageBitmap()
}

/** Well inside QR's ~2.9 KB binary limit, past which phone cameras struggle anyway. */
private const val MAX_QR_CHARS = 1_500

/** The system share sheet, for text meant for someone else (an invoice, an address, notes). */
fun shareText(context: Context, text: String, title: String) {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
    context.startActivity(Intent.createChooser(send, title))
}

/**
 * A full-screen camera that reads one QR code and returns its text. Asks for
 * the camera permission first; refused, it says so and can be closed.
 *
 * Frames are decoded with zxing (already used to draw codes), so scanning
 * needs no Google Play services.
 */
@Composable
fun QrScanner(onResult: (String) -> Unit, onDismiss: () -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var refused by remember { mutableStateOf(false) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        granted = ok
        refused = !ok
    }
    LaunchedEffect(Unit) { if (!granted) permission.launch(Manifest.permission.CAMERA) }

    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            if (granted) CameraPreview(onResult)
            Box(
                Modifier
                    .align(Alignment.Center)
                    .size(260.dp)
                    .border(2.dp, Color.White.copy(alpha = 0.8f), RoundedCornerShape(24.dp)),
            )
            // A scrim, so the bar reads over a bright camera image.
            Row(
                Modifier.fillMaxWidth().background(Color.Black.copy(alpha = 0.55f)).statusBarsPadding().padding(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onDismiss) { WalletIcon(R.drawable.ic_x, contentDescription = "Close", tint = Color.White) }
                Text("Scan a QR code", style = MaterialTheme.typography.titleMedium, color = Color.White)
            }
            if (refused) {
                Text(
                    "The camera permission was refused. Allow it in Settings to scan, or paste instead.",
                    color = Color.White,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.align(Alignment.BottomCenter).padding(32.dp),
                )
            }
        }
    }
}

/**
 * The camera, bound for as long as this composable is shown, reporting at
 * most one code.
 *
 * Each instance has a lifetime flag, cleared on disposal and checked
 * everywhere work can arrive late: the provider listener (which may run after
 * the scanner closed, and then binds nothing), the analyzer, and the
 * main-thread hand-off of a result (a decode already running when the user
 * cancelled is dropped, so it can't navigate anywhere). Disposal unbinds only
 * this scanner's use cases and never blocks on the provider.
 */
@Composable
private fun CameraPreview(onResult: (String) -> Unit) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    // A TextureView, not the default SurfaceView, so the title bar and close
    // button drawn over the preview stay visible.
    val previewView = remember { PreviewView(context).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE } }
    val deliver by rememberUpdatedState(onResult)

    DisposableEffect(owner) {
        val alive = AtomicBoolean(true)
        val delivered = AtomicBoolean(false)
        val main = ContextCompat.getMainExecutor(context)
        val analysis = Executors.newSingleThreadExecutor()
        val preview = Preview.Builder().build().also { it.setSurfaceProvider(previewView.surfaceProvider) }
        val analyzer = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
        analyzer.setAnalyzer(analysis) { image ->
            val text = image.use { if (alive.get()) decode(it) else null }
            if (text != null && alive.get()) {
                main.execute { if (alive.get() && delivered.compareAndSet(false, true)) deliver(text) }
            }
        }
        var provider: ProcessCameraProvider? = null
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            if (!alive.get()) return@addListener
            provider = runCatching { future.get() }.getOrNull()
            provider?.let { runCatching { it.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analyzer) } }
        }, main)
        onDispose {
            alive.set(false)
            analyzer.clearAnalyzer()
            provider?.let { runCatching { it.unbind(preview, analyzer) } }
            analysis.shutdownNow()
        }
    }
    AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())
}

/** The luminance plane is all a QR reader needs. */
private fun decode(image: ImageProxy): String? {
    val plane = image.planes[0]
    val buffer = plane.buffer
    val bytes = ByteArray(buffer.remaining()).also { buffer.get(it) }
    val source = PlanarYUVLuminanceSource(bytes, plane.rowStride, image.height, 0, 0, image.width, image.height, false)
    return runCatching {
        QRCodeReader().decode(BinaryBitmap(HybridBinarizer(source)), mapOf(DecodeHintType.TRY_HARDER to true)).text
    }.getOrNull()
}
