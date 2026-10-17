import SwiftUI

/// 11 Restore: takes a recovery phrase and opens the wallet it belongs to.
struct RestoreView: View {
    @ObservedObject var model: DemoModel

    /// The phrase lengths BIP-39 defines, all of which the SDK accepts
    /// (`every_bip39_word_count_is_accepted` in rust/fedimint-sdk's
    /// `types/mnemonic.rs`). The picker offers the two in common use; a pasted
    /// phrase of any of these lengths resizes the grid to fit it. The SDK still
    /// does the real validation, checksum included.
    private static let validWordCounts: Set<Int> = [12, 15, 18, 21, 24]

    @State private var words = Array(repeating: "", count: 12)
    @FocusState private var focused: Int?

    private var complete: Bool { !words.contains { $0.isEmpty } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.gap) {
                Caption("Restore")
                Text("Enter your recovery phrase")
                    .font(Typeface.title)
                    .foregroundStyle(Palette.ink)
                Text("Type the \(words.count) words in order. Restoring starts recovery for every federation you join.")
                    .font(Typeface.callout)
                    .foregroundStyle(Palette.muted)

                lengthPicker.padding(.top, 4)

                SeedGrid(count: words.count, highlighted: focused) { index in
                    TextField("", text: binding(index), prompt: Text("·····").foregroundStyle(Palette.border))
                        .font(Typeface.geist(.medium, 16, relativeTo: .body))
                        .foregroundStyle(focused == index ? Palette.primary : Palette.ink)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.asciiCapable)
                        .submitLabel(index == words.count - 1 ? .done : .next)
                        .focused($focused, equals: index)
                        .onSubmit { focused = index + 1 < words.count ? index + 1 : nil }
                        .accessibilityLabel("Word \(index + 1)")
                }

                // Wrong checksum, unknown word, or storage that already holds a
                // different seed: the SDK's reason, shown where it can be fixed.
                if !model.isAttaching, let failure = model.results[.wallet] {
                    Text(failure)
                        .font(Typeface.footnote)
                        .foregroundStyle(Palette.danger)
                }
            }
            .padding(Metrics.screenPadding)
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) {
            Button {
                focused = nil
                model.openWallet()
            } label: {
                BusyLabel(title: "Restore wallet", busy: model.isAttaching)
            }
            .buttonStyle(.primary)
            .disabled(!complete || model.isAttaching)
            .padding(Metrics.screenPadding)
            .background(Palette.background)
        }
        .screenBackground()
        .navigationTitle("Restore")
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: words) { _, words in
            model.restoreWords = words.joined(separator: " ")
        }
    }

    private var lengthPicker: some View {
        HStack(spacing: 8) {
            ForEach([12, 24], id: \.self) { count in
                let selected = words.count == count
                Button("\(count) words") { resize(to: count) }
                    .font(Typeface.geist(.medium, 15, relativeTo: .subheadline))
                    .foregroundStyle(selected ? .white : Palette.ink)
                    .padding(.horizontal, 16)
                    .frame(height: 36)
                    .background(selected ? Palette.navy : Palette.surface, in: Capsule())
                    .overlay(Capsule().strokeBorder(selected ? .clear : Palette.border))
                    .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
    }

    /// One cell's text. A space or a paste moves on: whatever follows the first
    /// word spills into the next cells, so pasting the whole phrase into any
    /// cell fills the grid from there.
    private func binding(_ index: Int) -> Binding<String> {
        Binding {
            words[index]
        } set: { typed in
            let parts = typed.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
            guard parts.count > 1 || typed.last?.isWhitespace == true else {
                words[index] = typed.lowercased()
                return
            }
            // A whole phrase pasted into the first cell sets the length too.
            if index == 0, parts.count != words.count, Self.validWordCounts.contains(parts.count) {
                resize(to: parts.count)
            }
            for (offset, part) in parts.enumerated() where index + offset < words.count {
                words[index + offset] = part
            }
            let next = index + max(parts.count, 1)
            focused = next < words.count ? next : nil
        }
    }

    /// Keeps the words already typed, dropping only those past the new length.
    private func resize(to count: Int) {
        words = Array((words + Array(repeating: "", count: count)).prefix(count))
        if let focused, focused >= count { self.focused = nil }
    }
}
