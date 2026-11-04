import SwiftUI

/// A word or two that says what something is: a state, a label, a name. With a `tone` it is in
/// that colour on a wash of it; without, it is a step above the row it lies on, bordered but on glass. With a
/// `dot` the colour is the dot's and the words stay plain.
struct Chip: View {
    static let height = scaled(20)
    static let compactHeight = scaled(18)

    let title: String
    var icon: Symbol?
    var dot: Color?
    var tone: Color?
    var monospaced = false
    /// It marks quietly, as a default does: its words muted on the accent's tint, with no line.
    var muted = false
    /// Less tall, to sit on a line of text without making it taller.
    var compact = false
    @Environment(\.surface) private var surface
    @Environment(\.row) private var row

    init(_ title: String, icon: Symbol? = nil, dot: Color? = nil, tone: Color? = nil, monospaced: Bool = false, muted: Bool = false, compact: Bool = false) {
        self.title = title
        self.icon = icon
        self.dot = dot
        self.tone = tone
        self.monospaced = monospaced
        self.muted = muted
        self.compact = compact
    }

    var body: some View {
        HStack(spacing: 5) {
            if let dot {
                Circle()
                    .fill(dot)
                    .frame(width: 7, height: 7)
            }
            if let icon {
                Image(icon, size: .xs)
            }
            Text(title)
                .font(.ui(size: 11.5, weight: .medium, design: monospaced ? .monospaced : .default))
                .lineLimit(1)
        }
        .foregroundStyle(muted ? Color.themeMutedForeground : dot == nil ? tone ?? Color.themeForeground : Color.themeForeground)
        .padding(.horizontal, 7)
        .frame(height: compact ? Self.compactHeight : Self.height)
        .background(background, in: Capsule())
        .overlay {
            if tone == nil && !muted && surface != .glass {
                Capsule().strokeBorder(surface.border, lineWidth: 1)
            }
        }
    }

    private var background: AnyShapeStyle {
        if let tone { return AnyShapeStyle(tone.wash()) }
        if muted { return AnyShapeStyle(Color(platform: Theme.accentTint)) }
        return AnyShapeStyle(surface.color(layer))
    }

    private var layer: Surface.Layer {
        switch row {
        case .rowSelected, .controlLit, .controlLitOnSelected: .controlLitOnSelected
        case .row, .control, .chip: .controlLit
        case .box, nil: .control
        }
    }
}
