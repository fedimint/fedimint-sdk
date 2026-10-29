import SwiftUI
import UIKit
import UniformTypeIdentifiers

enum Pasteboard {
    /// How long bearer value stays on the pasteboard.
    private static let bearerLifetime: TimeInterval = 120

    /// Copies `value` to the general pasteboard.
    ///
    /// A `bearer` value is money to whoever reads it: e-cash notes, not an
    /// invoice or an address, which are useless to anyone but the payer. Those
    /// stay on this device, so Universal Clipboard never syncs them to the
    /// user's other devices, and they expire after two minutes rather than
    /// sitting there for any app that reads the pasteboard later. The other
    /// values keep the plain copy, because pasting an invoice or address on
    /// another device is the point.
    static func copy(_ value: String, bearer: Bool) {
        if bearer {
            UIPasteboard.general.setItems(
                [[UTType.plainText.identifier: value]],
                options: [
                    .localOnly: true,
                    .expirationDate: Date().addingTimeInterval(bearerLifetime),
                ]
            )
        } else {
            UIPasteboard.general.string = value
        }
    }
}

/// The copy glyph beside an invoice, address or note bundle, which confirms the
/// copy in place for a moment.
struct CopyIconButton: View {
    let value: String
    var bearer = false

    @State private var copied = false

    var body: some View {
        Button {
            Pasteboard.copy(value, bearer: bearer)
            copied = true
            Task {
                try? await Task.sleep(for: .seconds(2))
                copied = false
            }
        } label: {
            Image(systemName: copied ? "checkmark" : "doc.on.doc")
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(copied ? Palette.success : Palette.ink)
                .frame(width: 32, height: 32)
        }
        .accessibilityLabel(copied ? "Copied" : "Copy")
    }
}
