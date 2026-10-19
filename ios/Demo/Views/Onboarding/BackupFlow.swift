import SwiftUI

/// Back up, then Verify, straight after a wallet is created and before it is
/// used.
struct BackupFlow: View {
    @ObservedObject var model: DemoModel
    let done: () -> Void

    @State private var verifying = false

    var body: some View {
        NavigationStack {
            BackupView(model: model, next: { verifying = true }, later: finish)
                .navigationDestination(isPresented: $verifying) {
                    VerifyView(words: model.seedWords, finish: finish)
                }
        }
        // The open that led here left "wallet opened" in this line; from here on
        // it is for why the phrase stayed hidden.
        .onAppear { model.results[.wallet] = nil }
    }

    /// Hides the phrase on the way out, so the words do not stay in memory
    /// behind the wallet once this flow is gone.
    private func finish() {
        model.hideSeed()
        done()
    }
}

/// The recovery phrase again, later: the same screen as the backup step, from
/// the wallet's menu, for a phrase that was not written down at creation.
struct RecoveryPhraseSheet: View {
    @ObservedObject var model: DemoModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            BackupView(model: model, next: { dismiss() }, later: { dismiss() }, step: nil)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Close") { dismiss() }
                    }
                }
        }
        .onAppear { model.results[.wallet] = nil }
        // Every way off the sheet, Close and swiping it away alike, so the
        // phrase is never still revealed the next time it opens.
        .onDisappear { model.hideSeed() }
    }
}
