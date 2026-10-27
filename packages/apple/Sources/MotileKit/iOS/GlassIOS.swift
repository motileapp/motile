#if os(iOS)
import SwiftUI

extension View {
    /// The surface of a button that floats over the client.
    func glassButton<S: Shape>(in shape: S) -> some View {
        glassSurface(in: shape, interactive: true)
    }

    /// The surface of what floats over the client, like the composer and the buttons: the system's
    /// Liquid Glass in its tint where it has it, and the system's material with a hairline around
    /// it before that.
    @ViewBuilder
    func glassSurface<S: Shape>(in shape: S, interactive: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            let glass = Glass.regular.tint(.themeGlassTint)
            glassEffect(interactive ? glass.interactive() : glass, in: shape)
        } else {
            background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeBorder, lineWidth: 1) }
        }
    }
}

/// Buttons in the top bar on one capsule of the client's glass in place of the system's, which
/// can't be tinted, so that they look like every other glass of the client.
struct GlassToolbarItem<Content: View>: ToolbarContent {
    let placement: ToolbarItemPlacement
    @ViewBuilder let content: Content

    var body: some ToolbarContent {
        ToolbarItem(placement: placement) {
            HStack(spacing: 0) { content }
                .buttonStyle(.glassSymbol)
                .foregroundStyle(Color.themeForeground)
                .glassButton(in: Capsule())
        }
        .withoutSystemGlass()
    }
}

extension View {
    /// A pushed page's way back on the client's glass in place of the system's, which keeps the
    /// swipe from the edge.
    func glassBackButton() -> some View {
        modifier(GlassBack())
    }
}

private struct GlassBack: ViewModifier {
    @Environment(\.dismiss) private var dismiss

    func body(content: Content) -> some View {
        content
            .navigationBarBackButtonHidden()
            .toolbar {
                GlassToolbarItem(placement: .topBarLeading) {
                    Button {
                        dismiss()
                    } label: {
                        Image(.chevronLeft, size: .lg)
                    }
                    .accessibilityLabel("Back")
                }
            }
            .background(EdgeSwipeBack())
    }
}

/// The stack's swipe from the edge, which it turns off once its back button is hidden.
private struct EdgeSwipeBack: UIViewRepresentable {
    func makeUIView(context: Context) -> Finder { Finder() }

    func updateUIView(_ view: Finder, context: Context) {}

    final class Finder: UIView {
        private let swipe = Swipe()

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard window != nil else { return }
            swipe.stack = sequence(first: self as UIResponder, next: \.next).lazy.compactMap { $0 as? UINavigationController }.first
            swipe.stack?.interactivePopGestureRecognizer?.delegate = swipe
        }
    }

    final class Swipe: NSObject, UIGestureRecognizerDelegate {
        weak var stack: UINavigationController?

        func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
            (stack?.viewControllers.count ?? 0) > 1
        }
    }
}

extension View {
    /// Glass shapes side by side that melt into each other as they come and go, where the system
    /// has it.
    @ViewBuilder
    func glassGroup(spacing: CGFloat) -> some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) { self }
        } else {
            self
        }
    }

    /// A search field at the bottom of the page on the client's glass, as the sidebar's, in place
    /// of the system's, which can't be tinted. `submit` is what return does in it.
    func bottomSearch(text: Binding<String>, prompt: String, focusesOnAppear: Bool = false, submit: @escaping () -> Void = {}) -> some View {
        modifier(BottomSearch(text: text, prompt: prompt, focusesOnAppear: focusesOnAppear, submit: submit))
    }
}

private struct BottomSearch: ViewModifier {
    @Binding var text: String
    let prompt: String
    let focusesOnAppear: Bool
    let submit: () -> Void
    @FocusState private var focused: Bool

    func body(content: Content) -> some View {
        content
            .safeAreaInset(edge: .bottom) {
                SearchField(text: $text, prompt: prompt, capsule: true, focus: $focused)
                    .onSubmit(submit)
                    .submitLabel(.search)
                    .padding(.leading, 6)
                    .frame(height: GlassButtonStyle.side)
                    .glassButton(in: Capsule())
                    .padding(.horizontal, sidebarEdge)
                    .padding(.vertical, 8)
            }
            .task {
                guard focusesOnAppear else { return }
                focused = true
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

/// What closes a sheet: an x, for a `GlassToolbarItem`.
struct SheetCloseButton: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Button {
            dismiss()
        } label: {
            Image(.x, size: .lg)
                .foregroundStyle(Color.themeForeground)
        }
        .accessibilityLabel("Close")
    }
}
#endif
