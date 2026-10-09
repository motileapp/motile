#if os(iOS)
import SwiftUI

extension View {
    /// The surface of a button that floats over the client: Liquid Glass where the system has it,
    /// and the system's material with a hairline around it before that.
    @ViewBuilder
    func glassButton<S: Shape>(in shape: S) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular.interactive(), in: shape)
        } else {
            background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeBorder, lineWidth: 1) }
        }
    }
}

extension View {
    /// The search field in the bottom bar, in glass, where the system has it there.
    @ViewBuilder
    func searchAtBottom() -> some View {
        if #available(iOS 26.0, *) {
            toolbar { DefaultToolbarItem(kind: .search, placement: .bottomBar) }
        } else {
            self
        }
    }

    /// The system's search field with its placeholder in the colour every field's has. The field
    /// ignores the colour of its prompt, so it is set on the field once it is in the window.
    func searchField(text: Binding<String>, prompt: String) -> some View {
        searchable(text: text, prompt: prompt)
            .background(SearchPlaceholderColor(prompt: prompt))
    }
}

private struct SearchPlaceholderColor: UIViewRepresentable {
    let prompt: String

    func makeUIView(context: Context) -> Finder { Finder() }

    func updateUIView(_ view: Finder, context: Context) {
        view.prompt = prompt
    }

    final class Finder: UIView {
        var prompt = "" {
            didSet { color() }
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            color()
        }

        private func color(attempt: Int = 0) {
            guard attempt < 10 else { return }
            guard let field = UIView.first(UISearchTextField.self, in: window) else {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.color(attempt: attempt + 1) }
                return
            }
            field.attributedPlaceholder = NSAttributedString(string: prompt, attributes: [.foregroundColor: Theme.mutedStrongerForeground])
        }
    }
}

extension UIView {
    static func first<Found: UIView>(_ kind: Found.Type, in view: UIView?) -> Found? {
        guard let view else { return nil }
        if let found = view as? Found { return found }
        return view.subviews.lazy.compactMap { first(kind, in: $0) }.first
    }
}

/// What closes a sheet: the system's round glass button with an x, or "Done" before iOS 26.
struct SheetCloseButton: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        if #available(iOS 26, *) {
            Button(role: .close) { dismiss() }
        } else {
            Button("Done") { dismiss() }
        }
    }
}
#endif
