import SwiftUI

/// 03 Verify: asks for three of the phrase's words back, each picked from
/// three choices, before the wallet opens.
///
/// Entirely local. Nothing in the SDK records that a phrase was backed up, so
/// this proves only that the words were read, which is all a demo can claim.
struct VerifyView: View {
    let finish: () -> Void

    private let words: [String]
    @State private var challenges: [Challenge]
    @State private var picks: [Int: String] = [:]

    /// How many positions are checked.
    private static let challengeCount = 3

    init(words: [String], finish: @escaping () -> Void) {
        self.words = words
        self.finish = finish
        _challenges = State(initialValue: Self.makeChallenges(words))
    }

    private var confirmed: Int {
        challenges.filter { picks[$0.position] == words[$0.position] }.count
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.gap) {
                Caption("Step 2 of 2")
                Text("Confirm three words")
                    .font(Typeface.title)
                    .foregroundStyle(Palette.ink)
                Text("Pick the right word for each position.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)

                ForEach(challenges, id: \.position) { challenge in
                    VStack(alignment: .leading, spacing: 10) {
                        Caption(String(format: "Word %02d", challenge.position + 1))
                        HStack(spacing: 10) {
                            ForEach(challenge.options, id: \.self) { option in
                                chip(option, for: challenge)
                            }
                        }
                    }
                    .padding(.top, Metrics.sectionGap)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Metrics.gap) {
                Text("\(confirmed) / \(challenges.count) confirmed")
                    .font(Typeface.mono(.regular, 13, relativeTo: .footnote))
                    .foregroundStyle(Palette.muted)
                Button("Continue", action: finish)
                    .buttonStyle(.primary)
                    .disabled(confirmed < challenges.count)
            }
            .padding(Metrics.screenPadding)
            .background(Palette.background)
        }
        .screenBackground()
    }

    private func chip(_ option: String, for challenge: Challenge) -> some View {
        let picked = picks[challenge.position] == option
        let right = option == words[challenge.position]
        let shape = RoundedRectangle(cornerRadius: Metrics.controlRadius, style: .continuous)
        return Button {
            picks[challenge.position] = option
        } label: {
            Text(option)
                .font(Typeface.geist(.medium, 16, relativeTo: .body))
                .foregroundStyle(picked ? (right ? .white : Palette.danger) : Palette.ink)
                .frame(maxWidth: .infinity, minHeight: Metrics.buttonHeight)
                .background(picked ? (right ? Palette.navy : Palette.dangerTint) : Palette.surface, in: shape)
                .overlay(shape.strokeBorder(picked ? .clear : Palette.border, lineWidth: Metrics.borderWidth))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(picked ? .isSelected : [])
    }

    struct Challenge {
        /// Zero-based index into the phrase.
        let position: Int
        let options: [String]
    }

    /// Three positions in phrase order, each offered with two other words from
    /// the same phrase. Decoys come from the phrase itself so that none of
    /// them can be ruled out as never having been shown.
    private static func makeChallenges(_ words: [String]) -> [Challenge] {
        let positions = Array(words.indices.shuffled().prefix(challengeCount)).sorted()
        return positions.map { position in
            let answer = words[position]
            // Distinct from the answer by spelling, not by position: a phrase
            // can repeat a word, and two identical chips would be unanswerable.
            let decoys = Set(words).subtracting([answer]).shuffled().prefix(2)
            return Challenge(position: position, options: ([answer] + decoys).shuffled())
        }
    }
}
