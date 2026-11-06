import SwiftUI

/// The SDK harness the cross-platform suite in js/e2e drives: one console
/// screen speaking js/e2e/PROTOCOL.md. Every SDK call goes through
/// `Dispatcher`; the state lives in `HarnessSession` for the life of the
/// process, which is what a restart ends.
@main
struct HarnessApp: App {
    @StateObject private var session = HarnessSession()

    var body: some Scene {
        WindowGroup {
            ConsoleView(session: session)
        }
    }
}
