import SwiftUI

/// The root. It owns the model and picks the stage the app is in: reopening a
/// stored wallet, onboarding, or the wallet itself.
struct ContentView: View {
    @StateObject private var model = DemoModel()

    /// Read once, at launch: an earlier launch left a wallet behind, so start by
    /// reopening it rather than offering to create one.
    @State private var reopening = DemoModel.hasStoredWallet()

    /// Set once Back up and Verify are behind a newly created wallet. Not
    /// persisted: a wallet reopened later is `.reopened`, not `.created`, so
    /// the backup step is offered once, right after creation.
    @State private var backedUp = false

    var body: some View {
        Group {
            if model.hasSdk, model.walletOrigin == .created, !backedUp {
                BackupFlow(model: model) { backedUp = true }
            } else if model.hasSdk {
                MainView(model: model)
            } else if reopening {
                LaunchView()
            } else {
                OnboardingView(model: model)
            }
        }
        // SwiftUI draws the back chevron and other bar controls in the tint,
        // not in the UIKit appearance's tint colour.
        .tint(Palette.ink)
        .task {
            if reopening { model.openWallet() }
        }
        .onChange(of: model.isAttaching) { _, attaching in
            // A reopen that failed falls back to onboarding, where its reason
            // is shown.
            if !attaching, !model.hasSdk { reopening = false }
        }
    }
}

#Preview {
    ContentView()
}
