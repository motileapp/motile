import SwiftUI

/// A line across the surface it lies on, in that surface's border.
struct ThemeDivider: View {
    var color: Color?
    @Environment(\.surface) private var surface

    var body: some View {
        Rectangle()
            .fill(color ?? surface.border)
            .frame(height: 1)
    }
}
