import SwiftUI
import UIKit

import FedimintSdk

/// Every SDK action on one scrolling screen, section by section, mirroring
/// `android/app`'s `activity_main.xml` and `js/examples/vite-core`: wallet
/// status, join, generate an invoice, redeem and send ecash, pay lightning,
/// parse an invite code, deposit, send on-chain, and recent activity.
struct ContentView: View {
    @StateObject private var model = DemoModel()

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    walletSection
                    joinSection
                    lightningInSection
                    ecashSection
                    lightningOutSection
                    parseInviteSection
                    onchainSection
                    activitySection
                }
                .padding()
            }
            .navigationTitle("Fedimint")
        }
    }

    // MARK: - Sections

    private var walletSection: some View {
        DemoSection("Wallet", result: model.results[.wallet]) {
            Text(model.walletStatus).font(.subheadline)
            Text(model.balance).font(.headline.monospacedDigit())

            TextField("restore from seed words (optional)", text: $model.restoreWords, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)

            HStack {
                // Disabled mid-send for the same reason as Join/Recover below:
                // reopening reattaches a federation, and `attach` clears the
                // notes of a send that has already committed. An open started
                // first disables Quote + Send instead (`isAttaching`).
                Button("Open Wallet") { model.openWallet() }
                    .disabled(model.isSendingEcash)
                Button(model.seed == nil ? "Show seed" : "Hide seed") { model.toggleSeed() }
                    .disabled(!model.hasSdk)
                Button("Refresh") { model.refreshBalance() }
                    .disabled(!model.hasFederation)
            }
            .buttonStyle(.bordered)

            if let seed = model.seed {
                // The one place the phrase leaves the SDK's care. A real wallet
                // would gate this behind device authentication.
                Text(seed).font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
            }
        }
    }

    private var joinSection: some View {
        DemoSection("Join a federation", result: model.results[.join]) {
            TextField("invite code", text: $model.invite, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)

            HStack {
                Button("Preview") { model.preview() }
                Button("Join") { model.join() }
                Button("Recover") { model.recover() }
            }
            .buttonStyle(.bordered)
            // `isSendingEcash` blocks the whole row while a send is committing:
            // Join and Recover call `attach`, which cancels the send's run and
            // clears its notes — losing the only record of value that has
            // already moved. Preview is harmless but rides along rather than
            // splitting the row for one button. The reverse ordering, a join
            // started before the send, is gated on Quote + Send via
            // `isAttaching`.
            .disabled(!model.hasSdk || model.isSendingEcash)
        }
    }

    private var lightningInSection: some View {
        DemoSection("Lightning — receive", result: model.results[.lnReceive]) {
            TextField("amount (msats)", text: $model.lnReceiveAmount)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
                .autocorrectionDisabled()
            TextField("description", text: $model.lnReceiveDescription)
                .textFieldStyle(.roundedBorder)

            HStack {
                Button("Create invoice") { model.lnReceive() }
                    .disabled(!model.hasFederation || model.lnReceiveAmount.trimmed.isEmpty)
                CopyButton("Copy invoice", value: model.lastInvoice)
            }
            .buttonStyle(.bordered)
        }
    }

    private var ecashSection: some View {
        DemoSection("Ecash", result: model.results[.ecashReceive]) {
            TextField("paste ecash notes", text: $model.ecashNotes, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            Button("Redeem") { model.ecashReceive() }
                .buttonStyle(.bordered)
                .disabled(!model.hasFederation)

            Divider()

            TextField("amount to send (msats)", text: $model.ecashSendAmount)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
            HStack {
                // Disabled while a send is committing: two taps are two real
                // spends. Also disabled while an Open Wallet, Join or Recover
                // is in flight: its `attach` would clear this send's notes when
                // it lands, even after the send has finished. This is the mirror
                // of the gate on those buttons. The model refuses both as well,
                // since this state lags the tap by a frame.
                Button(model.isSendingEcash ? "Sending…" : "Quote + Send") { model.ecashSend() }
                    .disabled(!model.hasFederation || model.isSendingEcash || model.isAttaching)
                // `notes` is an opaque handle, so nothing prints the token by
                // accident; `display()` is the deliberate way to take it out.
                CopyButton("Copy notes", value: model.lastNotes?.display())
            }
            .buttonStyle(.bordered)

            ResultText(model.results[.ecashSend])
        }
    }

    private var lightningOutSection: some View {
        DemoSection("Lightning — pay", result: model.results[.lnPay]) {
            TextField("bolt11 invoice", text: $model.lnInvoice, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)

            HStack {
                Button("Quote") { model.lnQuoteInvoice() }
                    .disabled(!model.hasFederation)
                // Enabled only once a quote exists: the user sees the fee before
                // anything is paid, which is the whole point of the two-step.
                Button("Pay") { model.lnPay() }
                    .disabled(model.lnQuote == nil)
            }
            .buttonStyle(.bordered)
        }
    }

    private var parseInviteSection: some View {
        DemoSection("Parse an invite code", result: model.results[.parseInvite]) {
            TextField("invite code", text: $model.parseInviteText, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            // No SDK instance needed: parsing is a pure operation on the type.
            Button("Parse") { model.parseInvite() }
                .buttonStyle(.bordered)
        }
    }

    private var onchainSection: some View {
        DemoSection("On-chain", result: model.results[.deposit]) {
            HStack {
                Button("Deposit address") { model.deposit() }
                    .disabled(!model.hasFederation)
                CopyButton("Copy address", value: model.lastAddress)
            }
            .buttonStyle(.bordered)

            Divider()

            TextField("destination address", text: $model.onchainAddress)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            // Sats, not msats — the one call in the SDK that differs.
            TextField("amount (sats)", text: $model.onchainAmount)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)

            HStack {
                Button("Quote") { model.onchainQuoteAddress() }
                    .disabled(!model.hasFederation)
                Button("Send") { model.onchainSend() }
                    .disabled(model.onchainQuote == nil)
            }
            .buttonStyle(.bordered)

            ResultText(model.results[.onchainSend])
        }
    }

    private var activitySection: some View {
        DemoSection("Recent activity", result: model.results[.activity]) {
            Button("Load") { model.activity() }
                .buttonStyle(.bordered)
                .disabled(!model.hasFederation)
        }
    }
}

// MARK: - Building blocks

/// One titled card with its own result line, matching `section_background.xml`
/// in the Android demo.
private struct DemoSection<Content: View>: View {
    private let title: String
    private let result: String?
    private let content: Content

    init(_ title: String, result: String?, @ViewBuilder content: () -> Content) {
        self.title = title
        self.result = result
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.headline)
            content
            ResultText(result)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(Color.secondary.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}

private struct ResultText: View {
    private let value: String?

    init(_ value: String?) { self.value = value }

    var body: some View {
        if let value, !value.isEmpty {
            Text(value)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct CopyButton: View {
    private let title: String
    private let value: String?

    init(_ title: String, value: String?) {
        self.title = title
        self.value = value
    }

    var body: some View {
        Button(title) {
            if let value { UIPasteboard.general.string = value }
        }
        .disabled(value == nil)
    }
}
