import SwiftUI

extension AgentState {
    var platformColor: PlatformColor {
        switch self {
        case .working: Theme.process
        case .waiting, .limited: Theme.warning
        case .done: Theme.success
        case .failed: Theme.destructive
        case .stopped: Theme.mutedStrongerForeground
        }
    }

    var color: Color { Color(platform: platformColor) }
}

/// An agent's logo in a ring, with a dot on its corner in the colour of how it is doing, cut out
/// of the ring.
struct AgentAvatar: View {
    let agent: Agent?
    var state: AgentState?
    var side: CGFloat = AgentAvatarDrawing.side

    var body: some View {
        let dot = AgentAvatarDrawing.dot
        let cut = AgentAvatarDrawing.cut
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let circle = state == nil ? side : side - cut
        ZStack {
            Circle().fill(Color(platform: Theme.backgroundAccent))
            Circle().strokeBorder(Color(platform: Theme.borderAccent), lineWidth: 1)
            Image(agent?.symbol ?? .bot, side: circle * AgentAvatarDrawing.mark)
                .foregroundStyle(agent?.color ?? .themeMutedForeground)
        }
        .frame(width: circle, height: circle)
        .frame(width: side, height: side, alignment: .topLeading)
        .mask {
            Rectangle()
                .overlay(alignment: .bottomTrailing) {
                    if state != nil {
                        Circle().frame(width: dot + cut * 2, height: dot + cut * 2).blendMode(.destinationOut)
                    }
                }
                .compositingGroup()
        }
        .overlay(alignment: .bottomTrailing) {
            if let state {
                Circle().fill(state.color).frame(width: dot, height: dot).padding(cut)
            }
        }
    }
}

/// Avatars side by side, each over the next, which has it cut out with a ring around it, and an
/// ellipsis for the agents there is no room for.
struct AgentAvatars: View {
    let avatars: [(agent: Agent?, state: AgentState?)]
    var more = 0
    var side: CGFloat = AgentAvatarDrawing.side

    var body: some View {
        HStack(spacing: AgentAvatarDrawing.step(side) - side) {
            ForEach(Array(avatars.enumerated()), id: \.offset) { index, avatar in
                AgentAvatar(agent: avatar.agent, state: avatar.state, side: side)
                    .cutUnder(index > 0, side: side)
            }
            if more > 0 {
                MoreAvatar(side: side)
                    .cutUnder(!avatars.isEmpty, side: side)
            }
        }
    }
}

extension View {
    /// Cuts the avatar before it out of it, with a ring around it, where that one lies over it.
    fileprivate func cutUnder(_ cut: Bool, side: CGFloat) -> some View {
        let ring = AgentAvatarDrawing.ring(side)
        return mask {
            Rectangle()
                .overlay(alignment: .leading) {
                    if cut {
                        Circle()
                            .frame(width: side + ring * 2, height: side + ring * 2)
                            .offset(x: -AgentAvatarDrawing.step(side) - ring)
                            .blendMode(.destinationOut)
                    }
                }
                .compositingGroup()
        }
    }
}

/// An avatar's ring around an ellipsis, standing for the agents there is no room for.
private struct MoreAvatar: View {
    let side: CGFloat

    var body: some View {
        ZStack {
            Circle().fill(Color(platform: Theme.backgroundAccent))
            Circle().strokeBorder(Color(platform: Theme.borderAccent), lineWidth: 1)
            Image(.ellipsis, side: side * AgentAvatarDrawing.ellipsis)
                .foregroundStyle(Color.themeMutedForeground)
        }
        .frame(width: side, height: side)
    }
}

/// The avatar drawn in the transcript's rows, which aren't SwiftUI.
final class AgentAvatarView: FlippedView {
    var agent: Agent? { didSet { if agent != oldValue { redraw() } } }
    var state: AgentState? { didSet { if state != oldValue { redraw() } } }
    /// The avatar before it lies over it, which is cut out of it with a ring around it.
    var cut = false { didSet { if cut != oldValue { redraw() } } }
    /// It stands for the agents there is no room for.
    var more = false { didSet { if more != oldValue { redraw() } } }

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(iOS)
        backgroundColor = .clear
        isOpaque = false
        isUserInteractionEnabled = false
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    override func draw(_ dirtyRect: CGRect) {
        AgentAvatarDrawing.draw(agent: agent, state: state, cut: cut, more: more, in: bounds)
    }
}

enum AgentAvatarDrawing {
    static let side = scaled(24)
    /// The dot is as large on every system as the composer strips' are, and cut out of what it lies on.
    static let dot: CGFloat = 6
    static let cut: CGFloat = 2
    /// The square of the agent's logo, or the bot when it has none, as a share of its circle.
    static let mark: CGFloat = 0.6
    /// The ellipsis that stands for the agents there is no room for, as a share of its circle.
    static let ellipsis: CGFloat = 0.72

    /// Cuts the circle in `rect` out of what is drawn.
    private static func clear(_ rect: CGRect) {
        guard let context = Platform.drawing else { return }
        context.saveGState()
        context.setBlendMode(.clear)
        RoundedBox.fill(rect, radius: rect.width / 2)
        context.restoreGState()
    }

    /// How far each avatar side by side stands from the one before it: they overlap by a quarter.
    static func step(_ side: CGFloat) -> CGFloat { side * 3 / 4 }

    /// The ring around an avatar that is cut out of the one under it.
    static func ring(_ side: CGFloat) -> CGFloat { max(1, (side / 12).rounded(.down)) }

    /// Draws the avatar in `rect`, a square, in the current context, with the one before it cut
    /// out where it lies over it.
    static func draw(agent: Agent?, state: AgentState?, cut under: Bool = false, more: Bool = false, in rect: CGRect) {
        let box = min(rect.width, rect.height)
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let side = state == nil ? box : box - cut
        let whole = CGRect(x: rect.minX, y: rect.minY, width: side, height: side)
        let circle = whole.insetBy(dx: 0.5, dy: 0.5)
        Theme.backgroundAccent.setFill()
        RoundedBox.fill(circle, radius: side / 2)
        Theme.borderAccent.setStroke()
        RoundedBox.stroke(circle, radius: side / 2)
        if more {
            TintedSymbol.draw(.ellipsis, side: circle.width * ellipsis, color: Theme.mutedForeground, in: circle)
        } else {
            TintedSymbol.draw(agent?.symbol ?? .bot, side: circle.width * mark, color: agent?.platformColor ?? Theme.mutedForeground, in: circle)
        }
        if under {
            let ring = ring(box)
            let over = CGRect(x: rect.minX - step(box), y: rect.minY, width: box, height: box).insetBy(dx: -ring, dy: -ring)
            clear(over)
        }
        guard let state else { return }
        let outer = CGRect(x: rect.minX + box - dot - cut, y: rect.minY + box - dot - cut, width: dot, height: dot)
        clear(outer.insetBy(dx: -cut, dy: -cut))
        state.platformColor.setFill()
        RoundedBox.fill(outer, radius: dot / 2)
    }
}
