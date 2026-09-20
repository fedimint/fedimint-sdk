import SwiftUI

/// The demo app's entry point.
///
/// The app exists to *run* the native library against a real federation, not
/// just to link it — the same role `android/app` plays. Every call it makes is
/// one this SDK's `uniffi` feature exports; none of it is hand-written glue.
@main
struct FedimintDemoApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
