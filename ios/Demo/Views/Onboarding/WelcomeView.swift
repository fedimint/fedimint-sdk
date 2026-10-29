import SwiftUI

/// 01 Welcome: no wallet in storage yet, so the only two things that can happen
/// are creating one or restoring one from a recovery phrase.
struct WelcomeView: View {
    @ObservedObject var model: DemoModel
    let restore: () -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            Palette.navy.ignoresSafeArea()

            Image("FedimintMark")
                .resizable()
                .scaledToFit()
                .frame(width: 280)
                // Bleeds off the trailing edge, the way the design crops it.
                .frame(maxWidth: .infinity, alignment: .trailing)
                .offset(x: 48, y: 150)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 0) {
                Image("FedimintLogoOnDark")
                    .resizable()
                    .scaledToFit()
                    .frame(height: 26)
                    .accessibilityLabel("Fedimint")

                Spacer(minLength: Metrics.sectionGap)

                Caption("Fedimint SDK · Demo wallet", color: Palette.accent)
                Text("Bitcoin custody, shared by people you trust.")
                    .font(Typeface.title)
                    .foregroundStyle(.white)
                    .padding(.top, 10)
                Text("A reference wallet showing every SDK flow, identical on iOS, Android and the web.")
                    .font(Typeface.callout)
                    .foregroundStyle(.white.opacity(0.65))
                    .padding(.top, Metrics.gap)

                VStack(spacing: Metrics.gap) {
                    Button {
                        // Explicit rather than assumed: the field is shared with
                        // Restore, and a wallet created here must never pick up
                        // words left behind by an abandoned restore.
                        model.restoreWords = ""
                        model.openWallet()
                    } label: {
                        BusyLabel(title: "Create new wallet", busy: model.isAttaching, spinner: Palette.navy)
                    }
                    .buttonStyle(.primaryOnDark)

                    Button("Restore from recovery phrase", action: restore)
                        .buttonStyle(.secondaryOnDark)
                }
                .disabled(model.isAttaching)
                .padding(.top, 32)

                // An open that failed leaves the reason here; one that worked
                // replaces this screen.
                if !model.isAttaching, let failure = model.results[.wallet] {
                    Text(failure)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.dangerTint)
                        .padding(.top, Metrics.gap)
                }

                Text("Use test funds only")
                    .font(Typeface.mono(.regular, 12, relativeTo: .caption))
                    .foregroundStyle(.white.opacity(0.5))
                    .frame(maxWidth: .infinity)
                    .padding(.top, Metrics.sectionGap)
            }
            .padding(.horizontal, Metrics.screenPadding)
            .padding(.vertical, 16)
        }
        .toolbar(.hidden, for: .navigationBar)
    }
}
