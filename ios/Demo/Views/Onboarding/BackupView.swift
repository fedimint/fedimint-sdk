import SwiftUI

/// 02 Back up: the new wallet's recovery phrase, numbered, behind the same
/// device-owner authentication `toggleSeed()` imposes everywhere else.
struct BackupView: View {
    @ObservedObject var model: DemoModel
    let next: () -> Void
    let later: () -> Void
    /// "Step 1 of 2" during onboarding; none when opened from the wallet later.
    var step: String? = "Step 1 of 2"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.gap) {
                if let step { Caption(step) }
                Text("Write down your recovery phrase")
                    .font(Typeface.title)
                    .foregroundStyle(Palette.ink)
                Text("These words are the only way to restore your wallet. Write them on paper, in order.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)

                Group {
                    if model.seedWords.isEmpty {
                        reveal
                    } else {
                        SeedGrid(count: model.seedWords.count) { index in
                            Text(model.seedWords[index])
                                .font(Typeface.geist(.medium, 16, relativeTo: .body))
                                .foregroundStyle(Palette.ink)
                        }
                    }
                }
                .padding(.top, Metrics.gap)

                Label {
                    Text("Never share these words. No one from Fedimint will ever ask for them.")
                } icon: {
                    Image(systemName: "checkmark.shield")
                }
                .font(Typeface.footnote)
                .foregroundStyle(Palette.muted)
                .padding(.top, 4)
            }
            .padding(Metrics.screenPadding)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Metrics.gap) {
                Button("I've written it down", action: next)
                    .buttonStyle(.primary)
                    .disabled(model.seedWords.isEmpty)
                if model.seedWords.isEmpty {
                    // A device without a passcode can never show the phrase
                    // (`toggleSeed` fails closed), so this screen needs a way
                    // out. The phrase stays reachable from the wallet later.
                    Button("Back up later", action: later)
                        .font(Typeface.button)
                        .foregroundStyle(Palette.primary)
                }
            }
            .padding(Metrics.screenPadding)
            .background(Palette.background)
        }
        .screenBackground()
        .navigationBarBackButtonHidden()
    }

    private var reveal: some View {
        VStack(spacing: Metrics.gap) {
            Image(systemName: "lock")
                .font(.system(size: 28, weight: .medium))
                .foregroundStyle(Palette.muted)
            Text("Confirm it's you to see the words.")
                .font(Typeface.callout)
                .foregroundStyle(Palette.muted)
            Button("Show recovery phrase") { model.toggleSeed() }
                .buttonStyle(.secondary)
            // Why the phrase stayed hidden: no passcode, or authentication
            // declined.
            if let reason = model.results[.wallet], model.seed == nil {
                Text(reason)
                    .font(Typeface.footnote)
                    .foregroundStyle(Palette.danger)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
        .card(padding: 24)
    }
}
