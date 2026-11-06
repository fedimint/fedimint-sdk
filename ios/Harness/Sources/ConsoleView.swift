import SwiftUI
import UIKit

/// The console of js/e2e/PROTOCOL.md: `run` on top, where the keyboard never
/// covers it, then `request`, then `response` filling the rest.
struct ConsoleView: View {
    @ObservedObject var session: HarnessSession
    @State private var request = ""

    var body: some View {
        VStack(spacing: 8) {
            Button("Run") {
                // An empty field is a repeated tap on a request already taken.
                guard !request.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
                let raw = request
                request = ""
                session.submit(raw)
            }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity)
            .accessibilityIdentifier("run")

            ConsoleTextView(identifier: "request", text: $request, editable: true)
                .frame(height: 120)
            ConsoleTextView(identifier: "response", text: .constant(session.response), editable: false)
        }
        .padding(12)
    }
}

/// A plain `UITextView`, so a driver reads the whole text as the element's
/// value, and with every input assist off, so what is typed is what arrives.
struct ConsoleTextView: UIViewRepresentable {
    let identifier: String
    @Binding var text: String
    let editable: Bool

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.accessibilityIdentifier = identifier
        view.isEditable = editable
        view.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        view.autocorrectionType = .no
        view.autocapitalizationType = .none
        view.spellCheckingType = .no
        view.smartQuotesType = .no
        view.smartDashesType = .no
        view.smartInsertDeleteType = .no
        view.inlinePredictionType = .no
        if #available(iOS 18.0, *) { view.writingToolsBehavior = .none }
        view.keyboardType = .asciiCapable
        view.layer.borderColor = UIColor.separator.cgColor
        view.layer.borderWidth = 1
        view.layer.cornerRadius = 6
        view.delegate = context.coordinator
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        if view.text != text { view.text = text }
    }

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    final class Coordinator: NSObject, UITextViewDelegate {
        private let text: Binding<String>
        init(text: Binding<String>) { self.text = text }
        func textViewDidChange(_ view: UITextView) { text.wrappedValue = view.text }
    }
}
