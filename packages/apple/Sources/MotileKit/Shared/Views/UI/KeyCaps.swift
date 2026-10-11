import SwiftUI

/// Keys to press, each on a cap of its own, or all on one when `joined`.
struct KeyCaps: View {
    static let height: CGFloat = 20

    let caps: [String]
    var joined = false
    @Environment(\.surface) private var surface

    init(_ caps: [String], joined: Bool = false) {
        self.caps = caps
        self.joined = joined
    }

    var body: some View {
        let shown = joined ? [caps.joined()] : caps
        HStack(spacing: 5) {
            ForEach(shown.indices, id: \.self) { index in
                Text(shown[index])
                    .font(.ui(size: 11, weight: .medium))
                    .padding(.horizontal, 6)
                    .frame(minWidth: 22, minHeight: Self.height)
                    .background(surface.color(.control), in: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous))
            }
        }
    }
}
