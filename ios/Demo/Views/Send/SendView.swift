import SwiftUI
import UIKit

/// 14 Send: what to pay, on which rail. Nothing is spent here; "Review quote"
/// asks the SDK what the payment would cost, and the review screen executes
/// that quote or nothing.
struct SendView: View {
    @ObservedObject var model: DemoModel
    @Environment(\.dismiss) private var dismiss

    @State private var rail: PaymentRail
    @State private var ecashSats = "1000"
    @State private var reviewing = false

    init(model: DemoModel) {
        self.model = model
        _rail = State(initialValue: PaymentRail.available(model.capabilities).first ?? .lightning)
    }

    private var section: Section {
        switch rail {
        case .lightning: .lnPay
        case .ecash: .ecashSend
        case .onchain: .onchainSend
        }
    }

    private var quoteReady: Bool {
        switch rail {
        case .lightning: model.lnQuote != nil
        case .ecash: model.ecashQuote != nil
        case .onchain: model.onchainQuote != nil
        }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.sectionGap) {
                    RailPicker(rails: PaymentRail.available(model.capabilities), selection: $rail) { $0.title }
                    switch rail {
                    case .lightning: lightning
                    case .ecash: ecash
                    case .onchain: onchain
                    }
                    if let balance = model.balanceMsats {
                        Text("\(model.balanceFailure == nil ? "Available" : "Last known"): \(SatsFormat.text(balance))")
                            .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                            .foregroundStyle(Palette.muted)
                            .frame(maxWidth: .infinity)
                    }
                    if let failure = model.failures[section] {
                        Text(failure)
                            .font(Typeface.footnote)
                            .foregroundStyle(Palette.danger)
                            .textSelection(.enabled)
                    }
                }
                .padding(Metrics.screenPadding)
            }
            .scrollDismissesKeyboard(.interactively)
            .safeAreaInset(edge: .bottom) {
                Button(action: requestQuote) {
                    BusyLabel(title: "Review quote", busy: model.isWorking(section))
                }
                .buttonStyle(.primary)
                .disabled(!canQuote || model.isWorking(section) || model.isSendingEcash)
                .padding(Metrics.screenPadding)
                .background(Palette.background)
            }
            .screenBackground()
            .navigationTitle("Send")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
            .navigationDestination(isPresented: $reviewing) {
                ReviewView(model: model, rail: rail) { dismiss() }
            }
            // The review opens when its quote lands, not when the button is
            // tapped: a quote that fails leaves the user here, with the reason.
            .onChange(of: quoteReady) { _, ready in
                if ready { reviewing = true }
            }
        }
    }

    // MARK: - Forms

    private var lightning: some View {
        Field("Invoice") {
            VStack(spacing: Metrics.gap) {
                TextEditor(text: $model.lnInvoice)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .frame(minHeight: 100)
                    .inputStyle(monospaced: true)
                Button {
                    if let pasted = UIPasteboard.general.string { model.lnInvoice = pasted.trimmed }
                } label: {
                    Label("Paste", systemImage: "doc.on.clipboard")
                }
                .buttonStyle(.secondary)
            }
        }
    }

    private var ecash: some View {
        VStack(alignment: .leading, spacing: Metrics.gap) {
            Field("Amount (sats)") {
                TextField("1000", text: $ecashSats)
                    .keyboardType(.numberPad)
                    .inputStyle()
            }
            Text("Sending ecash makes notes to hand to the recipient yourself, as a message or a QR code.")
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
        }
    }

    private var onchain: some View {
        VStack(spacing: Metrics.gap) {
            Field("Destination address") {
                TextField("tb1…", text: $model.onchainAddress)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .inputStyle(monospaced: true)
            }
            // Sats, not msats: `Onchain.quote` is the one SDK call that takes
            // whole satoshis.
            Field("Amount (sats)") {
                TextField("10000", text: $model.onchainAmount)
                    .keyboardType(.numberPad)
                    .inputStyle()
            }
        }
    }

    // MARK: - Quote

    private var canQuote: Bool {
        switch rail {
        case .lightning: !model.lnInvoice.trimmed.isEmpty
        case .ecash: ecashMsats != nil
        case .onchain: !model.onchainAddress.trimmed.isEmpty && (UInt64(model.onchainAmount.trimmed) ?? 0) > 0
        }
    }

    /// The ecash amount in the msats the SDK takes, or `nil` for anything that
    /// is not a whole, positive number of sats or would overflow.
    private var ecashMsats: UInt64? {
        guard let sats = UInt64(ecashSats.trimmed), sats > 0 else { return nil }
        let (msats, overflow) = sats.multipliedReportingOverflow(by: 1_000)
        return overflow ? nil : msats
    }

    private func requestQuote() {
        switch rail {
        case .lightning:
            model.lnQuoteInvoice()
        case .ecash:
            guard let msats = ecashMsats else { return }
            model.ecashSendAmount = String(msats)
            model.ecashQuoteAmount()
        case .onchain:
            model.onchainQuoteAddress()
        }
    }
}
