import SwiftUI

/// An item of a menu. On iOS a destructive one is red, its symbol too, which takes its item's tint.
/// The Mac's menus write it as any other, white where it is highlighted, as macOS does.
struct MenuItem: View {
    let title: String
    let symbol: Symbol
    var role: ButtonRole?
    let action: () -> Void

    init(_ title: String, symbol: Symbol, role: ButtonRole? = nil, action: @escaping () -> Void) {
        self.title = title
        self.symbol = symbol
        self.role = role
        self.action = action
    }

    var body: some View {
        Button(role: role, action: action) {
            Label(title, symbol: symbol)
        }
        #if os(iOS)
        .tint(role == .destructive ? .red : nil)
        #endif
    }
}
