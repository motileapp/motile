import SwiftUI

/// A word or two that says what something is: a state, a label, a name. With a `tone` it is in
/// that colour on a wash of it; with a `dot` the colour is the dot's and the words stay plain.
struct Chip: View {
    static let height = scaled(20)

    let title: String
    var icon: Symbol?
    var dot: Color?
    var tone: Color?
    var monospaced = false
    @Environment(\.surface) private var surface

    init(_ title: String, icon: Symbol? = nil, dot: Color? = nil, tone: Color? = nil, monospaced: Bool = false) {
        self.title = title
        self.icon = icon
        self.dot = dot
        self.tone = tone
        self.monospaced = monospaced
    }

    var body: some View {
        HStack(spacing: 5) {
            if let dot {
                Circle()
                    .fill(dot)
                    .frame(width: 7, height: 7)
            }
            if let icon {
                Image(icon, size: 11)
            }
            Text(title)
                .font(.ui(size: 11.5, weight: .medium, design: monospaced ? .monospaced : .default))
                .lineLimit(1)
        }
        .foregroundStyle(dot == nil ? tone ?? Color.themeText : Color.themeText)
        .padding(.horizontal, 7)
        .frame(height: Self.height)
        .background(tone?.opacity(0.14) ?? surface.next.color, in: Capsule())
    }
}
