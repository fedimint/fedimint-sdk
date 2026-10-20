import SwiftUI
import UIKit

// The unified demo design's tokens: the one place a colour, typeface, size or
// spacing is decided. The same design drives the Android and web demos, so a
// value changed here should change there too.
//
// Colours were sampled from the design's rendered screens; the mark's own
// gradient (#181884 to #4AD6FF, Resources/Assets.xcassets) confirms `primary`
// and `accent`. Sizes come from the design's layer geometry.

// MARK: - Colour

enum Palette {
    /// Titles and body text.
    static let ink = Color(hex: 0x0B0B1F)
    /// The darkest surface: the welcome screen, avatars, a selected chip.
    static let navy = Color(hex: 0x0A0A2A)
    /// Primary buttons, links, the active tab.
    static let primary = Color(hex: 0x181884)
    /// The brand highlight, from the Fedimint mark.
    static let accent = Color(hex: 0x4AD6FE)
    /// Secondary text, labels, inactive tabs.
    static let muted = Color(hex: 0x65657A)

    /// Every screen's ground.
    static let background = Color(hex: 0xF6F6F2)
    /// Cards, inputs, secondary buttons.
    static let surface = Color.white
    static let border = Color(hex: 0xE4E4DC)
    /// Icon tiles and the neutral pill.
    static let subtle = Color(hex: 0xEEEEE8)

    // Status. Each tone is a foreground with the tint it sits on in a pill.
    static let success = Color(hex: 0x0E7A4B)
    static let successTint = Color(hex: 0xDDF1E6)
    static let warning = Color(hex: 0x9A5800)
    static let warningTint = Color(hex: 0xFAEED7)
    static let danger = Color(hex: 0xC0302B)
    static let dangerTint = Color(hex: 0xFAE3E1)
}

extension Color {
    /// A colour from a `0xRRGGBB` literal, so a token reads the same here as it
    /// does in the design file.
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255
        )
    }
}

// MARK: - Type

/// Geist for text and Geist Mono for figures and labels, both bundled
/// (Resources/Fonts, registered in project.yml's `UIAppFonts`).
///
/// Every style is built with `relativeTo:`, so it still follows the user's
/// Dynamic Type setting: the design's size is the default, not a ceiling.
enum Typeface {
    /// Screen titles: "Write down your recovery phrase", "Federations".
    static let title = geist(.bold, 30, relativeTo: .largeTitle)
    /// Navigation titles and row titles.
    static let headline = geist(.semibold, 17, relativeTo: .headline)
    static let body = geist(.regular, 16, relativeTo: .body)
    /// Explanatory copy under a title.
    static let callout = geist(.regular, 15, relativeTo: .callout)
    static let footnote = geist(.regular, 13, relativeTo: .footnote)
    /// Button labels.
    static let button = geist(.medium, 17, relativeTo: .body)

    /// The small uppercase labels: "STEP 1 OF 2", "TOTAL BALANCE". Pair with
    /// `.tracking(Metrics.labelTracking)` and `.textCase(.uppercase)`.
    static let label = mono(.medium, 11, relativeTo: .caption)
    /// Invoices, addresses, ids and seed words.
    static let code = mono(.regular, 15, relativeTo: .body)

    /// The headline figure on Home and the send and receive screens.
    static let amount = mono(.semibold, 44, relativeTo: .largeTitle)
    /// The unit beside it.
    static let amountUnit = geist(.semibold, 20, relativeTo: .title3)
    /// A figure inside a row or a quote summary.
    static let figure = mono(.medium, 15, relativeTo: .body)

    enum Weight: String {
        case regular = "Regular"
        case medium = "Medium"
        case semibold = "SemiBold"
        case bold = "Bold"
    }

    static func geist(_ weight: Weight, _ size: CGFloat, relativeTo style: Font.TextStyle) -> Font {
        .custom("Geist-\(weight.rawValue)", size: size, relativeTo: style)
    }

    static func mono(_ weight: Weight, _ size: CGFloat, relativeTo style: Font.TextStyle) -> Font {
        .custom("GeistMono-\(weight.rawValue)", size: size, relativeTo: style)
    }

    /// Every PostScript name the styles above ask for. Nothing is set in bold
    /// mono, so that weight is not bundled.
    static let postScriptNames =
        [Weight.regular, .medium, .semibold, .bold].map { "Geist-\($0.rawValue)" }
        + [Weight.regular, .medium, .semibold].map { "GeistMono-\($0.rawValue)" }

    /// Crashes a debug build whose bundle is missing one of the fonts.
    ///
    /// `Font.custom` falls back to the system font without a word when a name
    /// does not resolve, so a typo in `UIAppFonts` or a font left out of the
    /// bundle would otherwise ship as a screen that looks almost right.
    static func assertRegistered() {
        #if DEBUG
        let missing = postScriptNames.filter { UIFont(name: $0, size: 12) == nil }
        assert(missing.isEmpty, "fonts not registered: \(missing). Check UIAppFonts in project.yml")
        #endif
    }
}

// MARK: - Size and space

enum Metrics {
    /// Side padding on every screen.
    static let screenPadding: CGFloat = 20
    /// Between related elements: two buttons side by side, a label and its field.
    static let gap: CGFloat = 12
    /// Between the blocks of a screen.
    static let sectionGap: CGFloat = 24

    static let buttonHeight: CGFloat = 52
    static let rowHeight: CGFloat = 68
    static let seedWordHeight: CGFloat = 46

    static let cardRadius: CGFloat = 16
    static let controlRadius: CGFloat = 14
    static let borderWidth: CGFloat = 1

    static let labelTracking: CGFloat = 0.8
}
