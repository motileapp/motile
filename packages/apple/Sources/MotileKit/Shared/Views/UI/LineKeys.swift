import SwiftUI

extension EnvironmentValues {
    /// What Return does in the lines to type in inside a view, which `onReturn` says.
    @Entry var lineReturn: (() -> Void)?
    /// What the keys `onLineKey` names do in the lines to type in inside a view.
    @Entry var lineKey: ((KeyEquivalent) -> KeyPress.Result)?
}

extension View {
    /// What Return does in the lines to type in inside this view, after what an `onReturn` inside
    /// it does, as `onSubmit` is for SwiftUI's own fields. The Mac's `LineField` is AppKit's,
    /// which SwiftUI doesn't tell of `onSubmit`.
    func onReturn(_ action: @escaping () -> Void) -> some View {
        modifier(ReturnAction(action: action))
    }

    /// What a key does in the lines to type in inside this view, before the line itself and
    /// before an `onLineKey` around this view, as `onKeyPress` is for SwiftUI's own fields. The
    /// line only takes the key when `action` ignores it. The Mac's `LineField` is AppKit's, whose
    /// editor SwiftUI's `onKeyPress` doesn't keep a key from.
    func onLineKey(_ key: KeyEquivalent, action: @escaping () -> KeyPress.Result) -> some View {
        modifier(LineKeyAction(key: key, action: action))
    }
}

private struct ReturnAction: ViewModifier {
    @Environment(\.lineReturn) private var outer
    let action: () -> Void

    func body(content: Content) -> some View {
        let outer = outer
        content
            .onSubmit(action)
            .environment(\.lineReturn) {
                action()
                outer?()
            }
    }
}

private struct LineKeyAction: ViewModifier {
    @Environment(\.lineKey) private var outer
    let key: KeyEquivalent
    let action: () -> KeyPress.Result

    func body(content: Content) -> some View {
        let (outer, key, action) = (outer, key, action)
        #if os(macOS)
        content
            .environment(\.lineKey) { pressed in
                if pressed == key, action() == .handled { return .handled }
                return outer?(pressed) ?? .ignored
            }
        #else
        content.onKeyPress(key, action: action)
        #endif
    }
}
