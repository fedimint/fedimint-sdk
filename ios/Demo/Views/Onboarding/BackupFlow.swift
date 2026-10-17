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
        if model.seed != nil { model.toggleSeed() }
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
            BackupView(model: model, next: close, later: close, step: nil)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Close", action: close)
                    }
                }
        }
        .onAppear { model.results[.wallet] = nil }
    }

    /// Hides the phrase on the way out, as the backup flow does.
    private func close() {
        if model.seed != nil { model.toggleSeed() }
        dismiss()
    }
}
