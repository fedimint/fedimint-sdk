import SwiftUI

/// The small uppercase label above a title, a balance or a section:
/// "STEP 1 OF 2", "TOTAL BALANCE", "RECENT ACTIVITY".
struct Caption: View {
    let text: String
    var color: Color = Palette.muted

    init(_ text: String, color: Color = Palette.muted) {
        self.text = text
        self.color = color
    }

    var body: some View {
        Text(text)
            .font(Typeface.label)
            .textCase(.uppercase)
            .tracking(Metrics.labelTracking)
            .foregroundStyle(color)
    }
}

/// A coloured dot and a word, for a federation's status or an operation's
/// state. The tone carries the meaning; the text says it.
struct StatusPill: View {
    enum Tone {
        case success, warning, danger, neutral

        var foreground: Color {
            switch self {
            case .success: Palette.success
            case .warning: Palette.warning
            case .danger: Palette.danger
            case .neutral: Palette.muted
            }
        }

        var tint: Color {
            switch self {
            case .success: Palette.successTint
            case .warning: Palette.warningTint
            case .danger: Palette.dangerTint
            case .neutral: Palette.subtle
            }
        }
    }

    let text: String
    let tone: Tone

    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(tone.foreground).frame(width: 6, height: 6)
            Text(text)
        }
        .font(Typeface.geist(.medium, 13, relativeTo: .footnote))
        .foregroundStyle(tone.foreground)
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(tone.tint, in: Capsule())
    }
}

/// A balance or payment amount the way the design sets it: the whole sats as
/// the figure, "sats" beside it, and a sub-sat remainder only when there is one.
struct AmountView: View {
    let msats: UInt64
    var sign: String = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(sign + SatsFormat.grouped(msats))
                    .font(Typeface.amount)
                    .monospacedDigit()
                    .foregroundStyle(Palette.ink)
                Text("sats")
                    .font(Typeface.amountUnit)
                    .foregroundStyle(Palette.muted)
            }
            if let remainder = SatsFormat.remainder(msats) {
                Text(remainder)
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.muted)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// Integer arithmetic throughout, for the reason `formatMsats` gives: the
/// binding hands back a full `UInt64` and going through `Double` would round
/// anything above 2^53 msats.
enum SatsFormat {
    /// Whole sats with grouping separators: "21,500".
    static func grouped(_ msats: UInt64) -> String {
        msats / 1_000 == 0 ? "0" : (msats / 1_000).formatted(.number.grouping(.automatic))
    }

    /// "+250 msat" when `msats` is not a whole number of sats, otherwise `nil`.
    static func remainder(_ msats: UInt64) -> String? {
        let rest = msats % 1_000
        return rest == 0 ? nil : "+\(rest) msat"
    }
}
