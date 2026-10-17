import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

/// A QR code for an invoice, an address or a note bundle, drawn with
/// CoreImage's built-in generator.
struct QRCodeView: View {
    let text: String

    private static let context = CIContext()
    /// Pixels per QR module before SwiftUI scales the image to its frame.
    private static let moduleScale: CGFloat = 8

    var body: some View {
        if let image = Self.render(text) {
            Image(uiImage: image)
                // One pixel per module from the generator: smoothing it on the
                // way up blurs the module edges and costs scanners their read.
                .interpolation(.none)
                .resizable()
                .scaledToFit()
                .accessibilityLabel("QR code")
        } else {
            RoundedRectangle(cornerRadius: Metrics.controlRadius)
                .fill(Palette.subtle)
                .overlay(Text("Too long for a QR code").font(Typeface.footnote).foregroundStyle(Palette.muted))
        }
    }

    private static func render(_ text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        // Medium error correction keeps a long bolt11 at a density a phone can
        // still scan off another phone's screen.
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: moduleScale, y: moduleScale))
        guard let image = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}

/// An invoice, address or note bundle shortened to its two ends, the way the
/// design shows one under its QR code. The full value is what gets copied and
/// shared.
func middleTruncated(_ value: String, keep: Int = 14) -> String {
    value.count <= keep * 2 ? value : "\(value.prefix(keep))…\(value.suffix(keep))"
}
