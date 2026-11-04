import UIKit

enum NavigationAppearance {
    /// Puts the navigation bar in the design's type and colours.
    ///
    /// UIKit appearance rather than a SwiftUI modifier: SwiftUI has no way to
    /// set the font of a navigation title. The back gesture and the bar itself
    /// stay native, which the design allows.
    static func apply() {
        let appearance = UINavigationBarAppearance()
        appearance.configureWithOpaqueBackground()
        appearance.backgroundColor = UIColor(Palette.background)
        appearance.shadowColor = .clear
        if let font = UIFont(name: "Geist-SemiBold", size: 17) {
            appearance.titleTextAttributes = [
                .font: UIFontMetrics(forTextStyle: .headline).scaledFont(for: font),
                .foregroundColor: UIColor(Palette.ink),
            ]
        }
        // The design's back control is the chevron alone, without the
        // previous screen's title beside it.
        let back = UIBarButtonItemAppearance()
        back.normal.titleTextAttributes = [.foregroundColor: UIColor.clear]
        appearance.backButtonAppearance = back

        let bar = UINavigationBar.appearance()
        bar.standardAppearance = appearance
        bar.scrollEdgeAppearance = appearance
        bar.compactAppearance = appearance
        bar.tintColor = UIColor(Palette.ink)
    }
}
