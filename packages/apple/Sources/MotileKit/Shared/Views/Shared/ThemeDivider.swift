import SwiftUI

/// A line across the surface it lies on, in that surface's border.
struct ThemeDivider: View {
    var color: Color?
    var dashed = false
    @Environment(\.surface) private var surface

    var body: some View {
        if dashed {
            HorizontalLine()
                .stroke(color ?? surface.border, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
                .frame(height: 1)
        } else {
            Rectangle()
                .fill(color ?? surface.border)
                .frame(height: 1)
        }
    }
}

private struct HorizontalLine: Shape {
    func path(in rect: CGRect) -> Path {
        Path { path in
            path.move(to: CGPoint(x: rect.minX, y: rect.midY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
        }
    }
}
