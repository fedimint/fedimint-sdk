import SwiftUI

import FedimintSdk

/// 06 Review: the quote the SDK returned, exactly what executing it will cost,
/// and the time it stays valid. Approving executes this quote and nothing else;
/// the SDK refuses one that has expired or changed underneath it.
struct ReviewView: View {
    @ObservedObject var model: DemoModel
    let rail: PaymentRail
    let done: () -> Void

    @Environment(\.dismiss) private var back
    @State private var submitted = false

    var body: some View {
        Group {
            if submitted {
                switch rail {
                case .ecash: EcashReadyView(model: model, done: done)
                case .lightning, .onchain: SendResultView(model: model, rail: rail, done: done)
                }
            } else if let terms {
                review(terms)
            } else {
                unavailable
            }
        }
        .screenBackground()
        .navigationTitle(submitted ? "" : "Review payment")
        .navigationBarTitleDisplayMode(.inline)
        // Once sent there is no going back to the quote: it is spent.
        .navigationBarBackButtonHidden(submitted)
    }

    // MARK: - The quote

    /// One quote's figures, whichever rail it came from.
    private struct Terms {
        var amount: UInt64
        var notesValue: UInt64?
        var fee: UInt64
        var total: UInt64
        var expiresAt: Timestamp
        var destination: String?
        var route: LightningRoute?
    }

    private var terms: Terms? {
        switch rail {
        case .lightning:
            guard let quote = model.lnQuote else { return nil }
            return Terms(amount: quote.invoiceAmount(), fee: quote.fee(), total: quote.total(),
                         expiresAt: quote.expiresAt(), destination: model.lnInvoice.trimmed, route: quote.route())
        case .ecash:
            guard let quote = model.ecashQuote else { return nil }
            // A mint rounds a request up to the notes it can issue, so the
            // notes can be worth more than asked for; the review says so.
            let notes = quote.notesValue()
            return Terms(amount: quote.requestedAmount(), notesValue: notes == quote.requestedAmount() ? nil : notes,
                         fee: quote.fee(), total: quote.total(), expiresAt: quote.expiresAt())
        case .onchain:
            guard let quote = model.onchainQuote else { return nil }
            // Whole sats on chain, msats for the fee and total: the units the
            // SDK reports each in.
            let (msats, overflow) = quote.amount().multipliedReportingOverflow(by: 1_000)
            return Terms(amount: overflow ? UInt64.max : msats, fee: quote.fee(), total: quote.total(),
                         expiresAt: quote.expiresAt(), destination: model.onchainAddress.trimmed)
        }
    }

    private func review(_ terms: Terms) -> some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let secondsLeft = Int(ActivityTime.date(terms.expiresAt).timeIntervalSince(context.date))
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                    VStack(alignment: .leading, spacing: 8) {
                        Caption("You're sending")
                        AmountView(msats: terms.amount)
                    }
                    rows(terms)
                    expiry(secondsLeft)
                }
                .padding(Metrics.screenPadding)
            }
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: Metrics.gap) {
                    if secondsLeft > 0 {
                        Button("Approve & send", action: approve)
                            .buttonStyle(.primary)
                            .disabled(model.isAttaching || (rail == .ecash && model.isSendingEcash))
                    } else {
                        Button("Get a new quote", action: requote)
                            .disabled(model.isWorking(section))
                            .buttonStyle(.primary)
                    }
                    Button("Cancel") {
                        model.discardQuotes()
                        back()
                    }
                    .buttonStyle(.secondary)
                }
                .padding(Metrics.screenPadding)
                .background(Palette.background)
            }
        }
    }

    private func rows(_ terms: Terms) -> some View {
        VStack(spacing: 0) {
            if let destination = terms.destination {
                ReviewRow("To", middleTruncated(destination, keep: 10), monospaced: true)
                RowDivider()
            }
            ReviewRow("Via", rail.title)
            if let route = terms.route {
                RowDivider()
                ReviewRow("Route", route.label, monospaced: true)
            }
            if let notesValue = terms.notesValue {
                RowDivider()
                ReviewRow("Notes worth", SatsFormat.text(notesValue), monospaced: true)
            }
            RowDivider()
            // "Fee", not "network fee": it is every cost of the payment, the
            // gateway's and the federation's included.
            ReviewRow("Fee", SatsFormat.text(terms.fee), monospaced: true)
            RowDivider()
            ReviewRow("Total", SatsFormat.text(terms.total), monospaced: true, emphasised: true)
        }
        .card(padding: 0)
    }

    @ViewBuilder private func expiry(_ secondsLeft: Int) -> some View {
        if secondsLeft > 0 {
            Label("Quote locked · expires in \(secondsLeft / 60):\(String(format: "%02d", secondsLeft % 60))",
                  systemImage: "lock")
                .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                .foregroundStyle(Palette.muted)
        } else {
            StatusPill(text: "Quote expired · get a new one to send", tone: .warning)
        }
    }

    /// No quote to review: a new one is on its way, the last request for one
    /// failed, or it has been used or dropped.
    @ViewBuilder private var unavailable: some View {
        if model.isWorking(section) {
            ProgressView("Getting a new quote")
                .padding(Metrics.screenPadding)
        } else {
            VStack(spacing: Metrics.gap) {
                Text(model.failures[section] ?? "This quote is no longer available.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                Button("Back") { back() }.buttonStyle(.secondary)
            }
            .padding(Metrics.screenPadding)
        }
    }

    /// The model section this rail quotes and sends in.
    private var section: Section {
        switch rail {
        case .lightning: .lnPay
        case .ecash: .ecashSend
        case .onchain: .onchainSend
        }
    }

    // MARK: - Actions

    /// Moves on to the result only if the send started: one refused at the
    /// start, such as while a federation is being attached, has no result of
    /// its own to show. Never moves back: a second tap before the redraw is
    /// refused too, and must not hide the send the first one started.
    private func approve() {
        let started: Bool
        switch rail {
        case .lightning: started = model.lnPay()
        case .ecash: started = model.ecashSend()
        case .onchain: started = model.onchainSend()
        }
        if started { submitted = true }
    }

    private func requote() {
        switch rail {
        case .lightning: model.lnQuoteInvoice()
        case .ecash: model.ecashQuoteAmount()
        case .onchain: model.onchainQuoteAddress()
        }
    }
}

/// A label and its value on one line of a review card.
struct ReviewRow: View {
    let label: String
    let value: String
    var monospaced = false
    var emphasised = false

    init(_ label: String, _ value: String, monospaced: Bool = false, emphasised: Bool = false) {
        self.label = label
        self.value = value
        self.monospaced = monospaced
        self.emphasised = emphasised
    }

    var body: some View {
        HStack {
            Text(label).foregroundStyle(Palette.muted)
            Spacer(minLength: Metrics.gap)
            Text(value)
                .font(emphasised ? Typeface.geist(.semibold, 16, relativeTo: .body)
                                 : (monospaced ? Typeface.figure : Typeface.body))
                .foregroundStyle(Palette.ink)
                .multilineTextAlignment(.trailing)
        }
        .font(Typeface.body)
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .frame(minHeight: 52)
    }
}

extension LightningRoute {
    /// How a quote names the route: settled inside the federation, or through
    /// a gateway, identified by the start of its key.
    var label: String {
        switch self {
        case .internal: "Internal"
        case .gateway(let gatewayId): "Gateway \(gatewayId.prefix(8))…"
        }
    }
}
