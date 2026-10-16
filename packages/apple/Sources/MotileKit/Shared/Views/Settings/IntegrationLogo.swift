import SwiftUI

/// An integration's logo on a tile of its own, as an app's icon sits.
struct IntegrationLogo: View {
    static let size: CGFloat = 32
    let integration: Integration

    var body: some View {
        Image(integration.symbol, size: 18)
            .foregroundStyle(Color.themeForeground)
            .frame(width: scaled(Self.size), height: scaled(Self.size))
            .box(in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), bordered: true)
    }
}
