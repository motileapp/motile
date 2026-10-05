import SwiftUI

/// A button in the window's toolbar. The buttons touch: the space seen between them is theirs.
struct ToolbarButton: View {
    static let margin: CGFloat = 2
    static let width = ControlSize.regular.height + 2 * margin

    let symbol: Symbol
    let help: String
    let action: () -> Void

    var body: some View {
        ActionButton(
            icon: symbol, help: help, margin: EdgeInsets(top: Self.margin, leading: Self.margin, bottom: Self.margin, trailing: Self.margin),
            action: action)
    }
}
