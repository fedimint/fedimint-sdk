import SwiftUI

/// The wallet once one is open: the design's three tabs. Send and Receive are
/// sheets over them, the iOS form the design asks for.
struct MainView: View {
    @ObservedObject var model: DemoModel

    enum Tab: Hashable { case home, activity, federations }
    @State private var tab = Tab.home

    enum Sheet: Identifiable {
        case send, receive, recoveryPhrase
        var id: Self { self }
    }
    @State private var sheet: Sheet?

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack {
                HomeView(
                    model: model,
                    receive: { sheet = .receive },
                    send: { sheet = .send },
                    showActivity: { tab = .activity },
                    showFederations: { tab = .federations },
                    showRecoveryPhrase: { sheet = .recoveryPhrase }
                )
            }
            .tabItem { Label("Home", systemImage: "house") }
            .tag(Tab.home)

            NavigationStack {
                ActivityView(model: model)
            }
            .tabItem { Label("Activity", systemImage: "waveform.path.ecg") }
            .tag(Tab.activity)

            // Brings its own NavigationStack, for its typed routes.
            FederationsView(model: model)
            .tabItem { Label("Federations", systemImage: "building.columns") }
            .tag(Tab.federations)
        }
        .tint(Palette.primary)
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .receive: ReceiveView(model: model)
            case .send: SendView(model: model)
            case .recoveryPhrase: RecoveryPhraseSheet(model: model)
            }
        }
    }
}
