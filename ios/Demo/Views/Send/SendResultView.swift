import SwiftUI

import FedimintSdk

/// After a Lightning or Onchain send: the payment's state as the SDK reports
/// it, until it is final.
///
/// The SDK's rule shows here: an error means the call failed, while a payment
/// that did not go through is a state. So `refunded` reads as an outcome,
/// money back in the balance, and not as a failure.
struct SendResultView: View {
    @ObservedObject var model: DemoModel
    let rail: PaymentRail
    let done: () -> Void

    private var section: Section { rail == .lightning ? .lnPay : .onchainSend }

    var body: some View {
        VStack(spacing: Metrics.sectionGap) {
            Spacer()
            outcome
            Spacer()
        }
        .padding(Metrics.screenPadding)
        .frame(maxWidth: .infinity)
        .safeAreaInset(edge: .bottom) {
            Button("Done", action: done)
                .buttonStyle(.primary)
                .padding(Metrics.screenPadding)
        }
    }

    @ViewBuilder private var outcome: some View {
        if let failure = model.failures[section] {
            // The call itself failed, so there may be no payment at all. The
            // SDK's reason is shown as it came.
            Outcome(icon: "exclamationmark.triangle", tone: .danger, title: "Couldn't send", detail: failure)
        } else if let lost = model.watchFailures[section] {
            // The send was submitted, and then reading its state failed. That
            // says nothing about how the payment ended, so neither outcome is
            // claimed. On a federation running lnv1, a payment that succeeded
            // ends up here: the SDK cannot decode the preimage the gateway
            // reported (`map_ln_pay` in rust/fedimint-sdk/src/lightning/v1.rs).
            Outcome(icon: "questionmark", tone: .neutral, title: "Status unknown",
                    detail: "The payment was submitted, but the SDK stopped reporting its state: \(lost). Check your balance and Activity to see how it ended.")
        } else {
            switch rail {
            case .lightning: lightning
            case .ecash, .onchain: onchain
            }
        }
    }

    @ViewBuilder private var lightning: some View {
        switch model.lnSendState {
        case nil, .created, .funded:
            Outcome(icon: "bolt", tone: .warning, title: "Sending", detail: "Waiting for the payment to settle.", busy: true)
        case let .success(preimage, fee, route):
            Outcome(icon: "checkmark", tone: .success, title: "Sent", detail: "Fee \(formatMsats(fee)) · \(route.label)") {
                Receipt(label: "Receipt (preimage)", value: preimage)
            }
        case .refunded:
            Outcome(icon: "arrow.uturn.backward", tone: .neutral, title: "Not sent",
                   detail: "The payment didn't go through. The amount is back in your balance.")
        case let .failed(reason):
            Outcome(icon: "xmark", tone: .danger, title: "Payment failed", detail: reason)
        }
    }

    @ViewBuilder private var onchain: some View {
        switch model.onchainSendState {
        case nil, .created:
            Outcome(icon: "link", tone: .warning, title: "Sending",
                   detail: "The federation is building and signing the transaction.", busy: true)
        case let .succeeded(txid):
            Outcome(icon: "checkmark", tone: .success, title: "Sent", detail: "The transaction is on its way.") {
                Receipt(label: "Transaction", value: txid)
            }
        case let .refunded(reason):
            Outcome(icon: "arrow.uturn.backward", tone: .neutral, title: "Not sent",
                   detail: "The amount is back in your balance. \(reason)")
        case let .failed(reason):
            Outcome(icon: "xmark", tone: .danger, title: "Withdrawal failed", detail: reason)
        }
    }
}

/// The large icon, title and explanation an outcome screen is built from.
private struct Outcome<Extra: View>: View {
    let icon: String
    let tone: StatusPill.Tone
    let title: String
    let detail: String
    var busy = false
    let extra: Extra

    init(icon: String, tone: StatusPill.Tone, title: String, detail: String, busy: Bool = false,
         @ViewBuilder extra: () -> Extra = { EmptyView() }) {
        self.icon = icon
        self.tone = tone
        self.title = title
        self.detail = detail
        self.busy = busy
        self.extra = extra()
    }

    var body: some View {
        VStack(spacing: Metrics.gap) {
            Image(systemName: icon)
                .font(.system(size: 26, weight: .semibold))
                .foregroundStyle(tone.foreground)
                .frame(width: 64, height: 64)
                .background(tone.tint, in: Circle())
            Text(title).font(Typeface.title).foregroundStyle(Palette.ink)
            Text(detail)
                .font(Typeface.callout)
                .foregroundStyle(Palette.muted)
                .multilineTextAlignment(.center)
                .textSelection(.enabled)
            if busy { ProgressView() }
            extra
        }
    }
}

/// A long value worth keeping, a preimage or a txid, shortened with a copy.
private struct Receipt: View {
    let label: String
    let value: String

    var body: some View {
        VStack(spacing: 6) {
            Caption(label)
            HStack {
                Text(middleTruncated(value, keep: 10))
                    .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                    .foregroundStyle(Palette.ink)
                CopyIconButton(value: value)
            }
        }
        .card()
        .padding(.top, Metrics.gap)
    }
}
