import SwiftUI

/// The design's full-width buttons, as a `ButtonStyle` so a plain `Button`
/// picks them up with `.buttonStyle(.primary)` and keeps its own semantics
/// (role, disabled state, accessibility).
struct DemoButtonStyle: ButtonStyle {
    enum Kind {
        /// The one action a screen exists for.
        case primary
        /// The alternative beside or below it.
        case secondary
        /// An action that gives something up: reclaiming notes, cancelling.
        case destructive
        /// `primary` and `secondary` for the welcome screen's dark ground.
        case primaryOnDark
        case secondaryOnDark
    }

    let kind: Kind

    func makeBody(configuration: Configuration) -> some View {
        StyledLabel(kind: kind, label: configuration.label, isPressed: configuration.isPressed)
    }

    /// A view rather than inline in `makeBody`, because the disabled state is
    /// only readable from the environment of a view.
    private struct StyledLabel<Label: View>: View {
        let kind: Kind
        let label: Label
        let isPressed: Bool

        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            let shape = RoundedRectangle(cornerRadius: Metrics.controlRadius, style: .continuous)
            label
                .font(Typeface.button)
                .foregroundStyle(foreground)
                .frame(maxWidth: .infinity, minHeight: Metrics.buttonHeight)
                .padding(.horizontal, Metrics.gap)
                .background(background, in: shape)
                .overlay {
                    if let border { shape.strokeBorder(border, lineWidth: Metrics.borderWidth) }
                }
                .contentShape(shape)
                .opacity(isEnabled ? (isPressed ? 0.8 : 1) : 0.4)
        }

        private var foreground: Color {
            switch kind {
            case .primary, .secondaryOnDark: .white
            case .secondary: Palette.ink
            case .destructive: Palette.danger
            case .primaryOnDark: Palette.navy
            }
        }

        private var background: Color {
            switch kind {
            case .primary: Palette.primary
            case .secondary, .primaryOnDark: Palette.surface
            case .destructive: Palette.dangerTint
            case .secondaryOnDark: .clear
            }
        }

        private var border: Color? {
            switch kind {
            case .secondary: Palette.border
            case .secondaryOnDark: .white.opacity(0.35)
            case .primary, .destructive, .primaryOnDark: nil
            }
        }
    }
}

extension ButtonStyle where Self == DemoButtonStyle {
    static var primary: DemoButtonStyle { DemoButtonStyle(kind: .primary) }
    static var secondary: DemoButtonStyle { DemoButtonStyle(kind: .secondary) }
    static var destructive: DemoButtonStyle { DemoButtonStyle(kind: .destructive) }
    static var primaryOnDark: DemoButtonStyle { DemoButtonStyle(kind: .primaryOnDark) }
    static var secondaryOnDark: DemoButtonStyle { DemoButtonStyle(kind: .secondaryOnDark) }
}

/// A button label with a spinner in front of it while `busy`, for an action
/// whose second tap would be a second real spend.
struct BusyLabel: View {
    let title: String
    let busy: Bool
    /// The spinner's colour, which has to match the button's text: white on
    /// `primary`, navy on `primaryOnDark`.
    var spinner: Color = .white

    var body: some View {
        HStack(spacing: 8) {
            if busy { ProgressView().tint(spinner) }
            Text(title)
        }
    }
}
