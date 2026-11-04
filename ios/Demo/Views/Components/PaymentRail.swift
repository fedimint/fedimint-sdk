import FedimintSdk

/// The three ways money moves, as Send and Receive offer them.
enum PaymentRail: Hashable {
    case lightning, ecash, onchain

    var title: String {
        switch self {
        case .lightning: "Lightning"
        case .ecash: "Ecash"
        case .onchain: "Onchain"
        }
    }

    /// Only the rails the federation has, in the design's order: a rail the
    /// federation cannot use is hidden, never shown disabled.
    static func available(_ capabilities: Capabilities?) -> [PaymentRail] {
        guard let capabilities else { return [] }
        return [
            capabilities.lightning ? .lightning : nil,
            capabilities.ecash ? .ecash : nil,
            capabilities.onchain ? .onchain : nil,
        ].compactMap { $0 }
    }
}
