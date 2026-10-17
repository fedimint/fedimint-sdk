import SwiftUI

import FedimintSdk

/// One entry of the federation's local history, as Home and Activity show it.
///
/// Everything here is read off the `ActivityItem`; nothing is inferred beyond
/// what the SDK reports. In particular a `refunded` payment reads as such, a
/// state rather than an error, and only a settled credit is coloured, since a
/// pending one in green would read as money that has arrived.
struct ActivityRow: View {
    let item: ActivityItem

    var body: some View {
        ListRow(icon: icon, title: title, subtitle: subtitle) {
            if let amount = item.amount {
                Text(signedAmount(amount))
                    .font(Typeface.figure)
                    .monospacedDigit()
                    .foregroundStyle(settledCredit ? Palette.success : Palette.ink)
            }
        }
    }

    private var settledCredit: Bool {
        item.direction == .incoming && item.status == .success
    }

    private var title: String { Self.title(item) }

    /// "Sent", "Receiving", "Deposit": the row's title, also the detail
    /// screen's.
    static func title(_ item: ActivityItem) -> String {
        switch item.kind {
        case .onchainReceive: return "Deposit"
        case .onchainSend: return "Withdraw"
        case .recovery:
            switch item.status {
            case .success: return "Recovered"
            case .pending: return "Recovering"
            default: return "Recovery"
            }
        case .unknown: return "Operation"
        case .ecashReceive, .lnReceive:
            return item.status == .pending ? "Receiving" : "Received"
        case .ecashSend, .lnSend:
            return item.status == .pending ? "Sending" : "Sent"
        }
    }

    private var subtitle: String {
        let detail: String
        switch item.status {
        case .success: detail = ActivityTime.short(item.time)
        case .pending: detail = "Pending"
        case .failed: detail = "Failed"
        case .refunded: detail = "Refunded"
        case .canceled: detail = "Canceled"
        case .unknown: detail = "Unknown"
        }
        return "\(Self.rail(item.kind)) · \(detail)"
    }

    static func rail(_ kind: OperationKind) -> String {
        switch kind {
        case .lnSend, .lnReceive: return "Lightning"
        case .ecashSend, .ecashReceive: return "Ecash"
        case .onchainSend, .onchainReceive: return "Onchain"
        case .recovery, .unknown: return "Wallet"
        }
    }

    private var icon: String { Self.icon(item.kind) }

    static func icon(_ kind: OperationKind) -> String {
        switch kind {
        case .lnReceive: return "arrow.down.left"
        case .lnSend: return "bolt"
        case .ecashSend, .ecashReceive: return "banknote"
        case .onchainSend, .onchainReceive: return "link"
        case .recovery: return "arrow.clockwise"
        case .unknown: return "questionmark"
        }
    }

    private func signedAmount(_ msats: UInt64) -> String {
        Self.sign(item.direction) + SatsFormat.grouped(msats)
    }

    static func sign(_ direction: Direction?) -> String {
        switch direction {
        case .incoming: return "+"
        // U+2212 MINUS SIGN, the width of the plus.
        case .outgoing: return "\u{2212}"
        case nil: return ""
        }
    }
}

/// How the history renders an SDK `Timestamp` (epoch milliseconds, recorded
/// locally by this device).
enum ActivityTime {
    static func date(_ epochMillis: UInt64) -> Date {
        Date(timeIntervalSince1970: Double(epochMillis) / 1_000)
    }

    /// "09:41" today, "Yesterday", otherwise the date.
    static func short(_ epochMillis: UInt64) -> String {
        let date = date(epochMillis)
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return date.formatted(date: .omitted, time: .shortened) }
        if calendar.isDateInYesterday(date) { return "Yesterday" }
        return date.formatted(date: .abbreviated, time: .omitted)
    }
}
