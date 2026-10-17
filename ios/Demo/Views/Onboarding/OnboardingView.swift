import SwiftUI

/// Everything before a wallet is open: Welcome, and Restore pushed from it.
struct OnboardingView: View {
    @ObservedObject var model: DemoModel

    enum Route: Hashable { case restore }
    @State private var path: [Route] = []

    var body: some View {
        NavigationStack(path: $path) {
            WelcomeView(model: model) { path.append(.restore) }
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .restore: RestoreView(model: model)
                    }
                }
        }
        .onChange(of: path) { _, newPath in
            // A failure from the screen just left is about that screen, not the
            // one returned to.
            if newPath.isEmpty, !model.isAttaching { model.results[.wallet] = nil }
        }
    }
}

/// Shown while a wallet left by an earlier launch is reopened, so the first
/// frame is not a Welcome screen offering to create a wallet that already
/// exists.
struct LaunchView: View {
    var body: some View {
        ZStack {
            Palette.navy.ignoresSafeArea()
            VStack(spacing: Metrics.sectionGap) {
                Image("FedimintLogoOnDark")
                    .resizable()
                    .scaledToFit()
                    .frame(height: 32)
                    .accessibilityLabel("Fedimint")
                ProgressView().tint(.white)
            }
        }
    }
}
