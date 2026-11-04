import SwiftUI

import FedimintSdk

/// 05 / 15 / 16 Receive: Lightning, Ecash and Onchain behind one control.
struct ReceiveView: View {
    @ObservedObject var model: DemoModel
    @Environment(\.dismiss) private var dismiss

    @State private var rail: PaymentRail
    @State private var sats = "5000"
    @State private var composing = false

    init(model: DemoModel) {
        self.model = model
        _rail = State(initialValue: PaymentRail.available(model.capabilities).first ?? .lightning)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: Metrics.sectionGap) {
                    RailPicker(rails: PaymentRail.available(model.capabilities), selection: $rail) { $0.title }
                    switch rail {
                    case .lightning: lightning
                    case .ecash: ecash
                    case .onchain: onchain
                    }
                }
                .padding(Metrics.screenPadding)
            }
            .scrollDismissesKeyboard(.interactively)
            .screenBackground()
            .navigationTitle("Receive")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
    }

    // MARK: - Lightning

    /// An invoice is on screen unless the user has asked for a new one. While
    /// it is, the amount shown is the invoice's own, read back from the SDK,
    /// and the amount field is out of the way: an edited field next to an
    /// unchanged invoice would claim a different amount than the QR code asks
    /// for.
    private var showingInvoice: Bool { model.lastInvoice != nil && !composing }

    private var lightning: some View {
        VStack(spacing: Metrics.sectionGap) {
            if showingInvoice {
                if let msats = model.lnReceiveInvoiceMsats {
                    AmountView(msats: msats).frame(maxWidth: .infinity)
                }
            } else {
                VStack(spacing: 4) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        TextField("0", text: $sats)
                            .keyboardType(.numberPad)
                            .font(Typeface.amount)
                            .foregroundStyle(Palette.ink)
                            .multilineTextAlignment(.trailing)
                            .fixedSize()
                        Text("sats").font(Typeface.amountUnit).foregroundStyle(Palette.muted)
                    }
                    Text("Tap to edit").font(Typeface.footnote).foregroundStyle(Palette.muted)
                }
                .frame(maxWidth: .infinity)

                Field("Note (optional)") {
                    TextField("Coffee", text: $model.lnReceiveDescription).inputStyle()
                }
            }

            if showingInvoice, let invoice = model.lastInvoice {
                VStack(spacing: Metrics.gap) {
                    QRCodeView(text: invoice).frame(maxWidth: 240)
                    HStack {
                        Text(middleTruncated(invoice)).font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                            .foregroundStyle(Palette.muted)
                        CopyIconButton(value: invoice)
                    }
                    lightningStatus
                    if let lost = model.watchFailures[.lnReceive] { TrackingStopped(reason: lost) }
                }
                .frame(maxWidth: .infinity)
                .card(padding: 20)

                ShareLink(item: invoice) { Label("Share invoice", systemImage: "square.and.arrow.up") }
                    .buttonStyle(.primary)
                Button("New invoice") { composing = true }
                    .buttonStyle(.secondary)
            } else {
                Button("Create invoice", action: createInvoice)
                    .buttonStyle(.primary)
                    .disabled(satsToMsats == nil || model.isWorking(.lnReceive))
            }

            failure(model.failures[.lnReceive], shown: !showingInvoice)
        }
        // A new invoice landing ends composing, and shows that invoice.
        .onChange(of: model.lastInvoice) { composing = false }
    }

    /// The amount entered, in the msats the SDK takes. `nil` for anything that
    /// is not a whole, positive number of sats or would overflow.
    private var satsToMsats: UInt64? {
        guard let value = UInt64(sats.trimmed), value > 0 else { return nil }
        let (msats, overflow) = value.multipliedReportingOverflow(by: 1_000)
        return overflow ? nil : msats
    }

    private func createInvoice() {
        guard let msats = satsToMsats else { return }
        model.lnReceiveAmount = String(msats)
        model.lnReceive()
    }

    @ViewBuilder private var lightningStatus: some View {
        switch model.lnReceiveState {
        case nil, .created, .waitingForPayment:
            if let expiresAt = model.lnReceiveExpiresAt {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    StatusPill(text: "Waiting for payment · \(remaining(until: expiresAt, now: context.date))",
                               tone: .warning)
                }
            } else {
                StatusPill(text: "Waiting for payment", tone: .warning)
            }
        case .funded: StatusPill(text: "Paid · claiming", tone: .warning)
        case .claimed: StatusPill(text: "Received", tone: .success)
        case .expired: StatusPill(text: "Expired", tone: .neutral)
        case .canceled: StatusPill(text: "Canceled", tone: .neutral)
        case .failed: StatusPill(text: "Failed", tone: .danger)
        }
    }

    /// "59:12" until the invoice expires, "0:00" once it has.
    private func remaining(until epochMillis: Timestamp, now: Date) -> String {
        let seconds = max(0, Int(ActivityTime.date(epochMillis).timeIntervalSince(now)))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }

    // MARK: - Ecash

    private var ecash: some View {
        VStack(spacing: Metrics.sectionGap) {
            Field("Paste ecash notes") {
                TextEditor(text: $model.ecashNotes)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .frame(minHeight: 120)
                    .inputStyle(monospaced: true)
            }

            // Read locally from the notes themselves: it says what they claim to
            // be worth, not that they are still unspent. Only Redeem asks the
            // federation.
            if let value = notesValue {
                VStack(alignment: .leading, spacing: Metrics.gap) {
                    Caption("Preview")
                    VStack(spacing: 0) {
                        ReviewRow("Amount", SatsFormat.text(value), monospaced: true)
                        RowDivider()
                        ReviewRow("Unspent", "Checked when you redeem")
                    }
                    .card(padding: 0)
                }
            } else if !model.ecashNotes.trimmed.isEmpty {
                Text("These don't look like ecash notes.")
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.danger)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Button("Redeem") { model.ecashReceive() }
                .buttonStyle(.primary)
                .disabled(notesValue == nil || model.isWorking(.ecashReceive))

            failure(model.results[.ecashReceive], shown: true)
        }
    }

    private var notesValue: UInt64? {
        let text = model.ecashNotes.trimmed
        guard !text.isEmpty else { return nil }
        return try? Notes.parse(notes: text).value()
    }

    // MARK: - Onchain

    private var onchain: some View {
        VStack(spacing: Metrics.sectionGap) {
            if let address = model.lastAddress {
                VStack(spacing: Metrics.gap) {
                    QRCodeView(text: address).frame(maxWidth: 240)
                    HStack {
                        Text(middleTruncated(address, keep: 10)).font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                            .foregroundStyle(Palette.muted)
                        CopyIconButton(value: address)
                    }
                    depositStatus
                    if let lost = model.watchFailures[.deposit] { TrackingStopped(reason: lost) }
                }
                .frame(maxWidth: .infinity)
                .card(padding: 20)

                Text("Send only \(networkName) bitcoin to this address. Deposits can take several minutes to appear after they confirm.")
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.muted)
                    .frame(maxWidth: .infinity, alignment: .leading)

                ShareLink(item: address) { Label("Share address", systemImage: "square.and.arrow.up") }
                    .buttonStyle(.primary)
                // One payment per address: a second one to the same address may
                // go unnoticed for a long time on the v1 wallet module.
                Button { model.deposit() } label: {
                    BusyLabel(title: "New address", busy: model.isWorking(.deposit))
                }
                .buttonStyle(.secondary)
                .disabled(model.isWorking(.deposit))
            } else {
                Text("Get an address this federation watches, then send \(networkName) bitcoin to it.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button("Get deposit address") { model.deposit() }
                    .buttonStyle(.primary)
                    .disabled(model.isWorking(.deposit))
            }

            failure(model.failures[.deposit], shown: true)
        }
    }

    @ViewBuilder private var depositStatus: some View {
        if !model.depositFound {
            StatusPill(text: "Waiting for deposit", tone: .warning)
        } else {
            switch model.depositState {
            case nil, .confirmed: StatusPill(text: "Confirmed · claiming", tone: .warning)
            case .claimed: StatusPill(text: "Deposited", tone: .success)
            case .failed: StatusPill(text: "Failed", tone: .danger)
            }
        }
    }

    private var networkName: String {
        switch model.federationNetwork {
        case .bitcoin: "mainnet"
        case .testnet: "testnet"
        case .testnet4: "testnet4"
        case .signet: "signet"
        case .regtest: "regtest"
        case nil: "the federation's"
        }
    }

    // MARK: - Shared

    /// The section's result line while it explains something: an error, or an
    /// ecash redeem's outcome. "working…" shows as itself, since an action is
    /// under way.
    @ViewBuilder private func failure(_ text: String?, shown: Bool) -> some View {
        if shown, let text, !text.isEmpty {
            Text(text)
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
        }
    }
}
