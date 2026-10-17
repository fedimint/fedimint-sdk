import SwiftUI

import FedimintSdk

/// 17 Ecash ready: the notes an ecash send made, to hand to the recipient, and
/// what has become of them since.
///
/// The notes are bearer value: whoever redeems them first gets the money. So
/// they leave this screen only deliberately, through the copy and share
/// buttons, and the copy is the local-only, expiring one `Pasteboard` makes.
struct EcashReadyView: View {
    @ObservedObject var model: DemoModel
    let done: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: Metrics.sectionGap) {
                if let notes = model.lastNotes {
                    ready(notes)
                } else if let failure = model.failures[.ecashSend] {
                    Text(failure)
                        .font(Typeface.callout)
                        .foregroundStyle(Palette.danger)
                        .textSelection(.enabled)
                } else {
                    ProgressView("Making notes")
                        .padding(.top, 80)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Metrics.gap) {
                if let notes = model.lastNotes?.display() {
                    ShareLink(item: notes) { Label("Share notes", systemImage: "square.and.arrow.up") }
                        .buttonStyle(.primary)
                    Button("Reclaim notes") { model.reclaimEcash() }
                        .buttonStyle(.secondary)
                        .disabled(!canReclaim)
                }
                Button("Done", action: done)
                    .font(Typeface.button)
                    .foregroundStyle(Palette.primary)
                    .padding(.top, 4)
            }
            .padding(Metrics.screenPadding)
            .background(Palette.background)
        }
        .navigationTitle("Ecash notes")
    }

    private func ready(_ notes: Notes) -> some View {
        let token = notes.display()
        return VStack(spacing: Metrics.sectionGap) {
            VStack(spacing: 4) {
                AmountView(msats: notes.value())
                Text("From \(model.federationName ?? "this federation")")
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.muted)
            }
            VStack(spacing: Metrics.gap) {
                QRCodeView(text: token).frame(maxWidth: 240)
                HStack {
                    Text(middleTruncated(token, keep: 12))
                        .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                        .foregroundStyle(Palette.muted)
                    CopyIconButton(value: token, bearer: true)
                }
                status
            }
            .frame(maxWidth: .infinity)
            .card(padding: 20)

            Text("Anyone who has these notes can spend them, so share them only with the person you're paying. If no one redeems them, you can reclaim them back to your balance.")
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
                .multilineTextAlignment(.center)
            if let failure = model.failures[.ecashReclaim] {
                Text(failure).font(Typeface.footnote).foregroundStyle(Palette.danger)
            }
        }
    }

    /// Reclaiming is a request the receiver's redeem can still beat, so it is
    /// offered only while the notes are outstanding.
    private var canReclaim: Bool {
        switch model.ecashSendState {
        case nil, .created: !model.isWorking(.ecashReclaim)
        case .cancelRequested, .canceled, .redeemed: false
        }
    }

    @ViewBuilder private var status: some View {
        switch model.ecashSendState {
        case nil, .created: StatusPill(text: "Waiting for the recipient to redeem", tone: .warning)
        case .cancelRequested: StatusPill(text: "Reclaiming", tone: .warning)
        case .canceled: StatusPill(text: "Reclaimed · back in your balance", tone: .success)
        case .redeemed: StatusPill(text: "Redeemed by the recipient", tone: .success)
        }
    }
}
