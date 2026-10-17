import SwiftUI

/// A label above an input, the way every form in the design is laid out.
struct Field<Content: View>: View {
    let label: String
    let content: Content

    init(_ label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label)
                .font(Typeface.geist(.medium, 14, relativeTo: .subheadline))
                .foregroundStyle(Palette.muted)
            content
        }
    }
}

extension View {
    /// The design's input box, for a `TextField` or a `TextEditor`.
    func inputStyle(monospaced: Bool = false) -> some View {
        self
            .font(monospaced ? Typeface.code : Typeface.body)
            .foregroundStyle(Palette.ink)
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .frame(minHeight: Metrics.buttonHeight)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.controlRadius, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: Metrics.controlRadius, style: .continuous)
                    .strokeBorder(Palette.border, lineWidth: Metrics.borderWidth)
            )
    }
}

/// The Lightning / Ecash / Onchain control at the top of Send and Receive.
///
/// Callers pass only the rails the attached federation supports: the design's
/// rule is to hide a rail the federation cannot use, never to show it disabled.
struct RailPicker<Rail: Hashable>: View {
    let rails: [Rail]
    @Binding var selection: Rail
    let title: (Rail) -> String

    var body: some View {
        HStack(spacing: 4) {
            ForEach(rails, id: \.self) { rail in
                Button {
                    selection = rail
                } label: {
                    Text(title(rail))
                        .font(Typeface.geist(.medium, 15, relativeTo: .subheadline))
                        .foregroundStyle(rail == selection ? Palette.ink : Palette.muted)
                        .frame(maxWidth: .infinity, minHeight: 40)
                        .background {
                            if rail == selection {
                                RoundedRectangle(cornerRadius: 10, style: .continuous)
                                    .fill(Palette.surface)
                                    .shadow(color: .black.opacity(0.06), radius: 2, y: 1)
                            }
                        }
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(rail == selection ? .isSelected : [])
            }
        }
        .padding(4)
        .background(Palette.subtle, in: RoundedRectangle(cornerRadius: 13, style: .continuous))
    }
}
