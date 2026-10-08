import SwiftUI

/// What the view that has focus does on Esc, while it has something to undo.
struct EscapeClaim {
    let action: (() -> Void)?
}

extension FocusedValues {
    @Entry var escape: EscapeClaim?
}

extension View {
    /// Claims Esc for the view while it has focus. With nil it lets Esc go, to what is around it.
    func onEscape(_ action: (() -> Void)?) -> some View {
        focusedValue(\.escape, EscapeClaim(action: action))
    }
}

/// Does what Esc means now: what the focused view claims, else what the store says. Without
/// either there is no key, so that Esc reaches whatever else listens for it. Each is a button of
/// its own: a shortcut keeps the action it was registered with.
struct EscapeKey: View {
    @Environment(AppStore.self) private var store
    @FocusedValue(\.escape) private var claim

    var body: some View {
        if let action = claim?.action {
            key(action)
        } else if store.escapes != nil {
            key { store.escapes?() }
        }
    }

    private func key(_ action: @escaping () -> Void) -> some View {
        Button("", action: action)
            .keyboardShortcut(.cancelAction)
            .frame(width: 0, height: 0)
            .opacity(0)
            .accessibilityHidden(true)
    }
}
