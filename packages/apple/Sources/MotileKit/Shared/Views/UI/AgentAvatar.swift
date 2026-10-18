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
            if let agent {
                AgentIcon(agent: agent, size: (circle * 0.56).rounded())
            } else {
                Image(.bot, size: (circle * 0.48).rounded())
                    .foregroundStyle(Color.themeMutedForeground)
            }
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

/// Avatars side by side, each over the last, and how many more there are.
struct AgentAvatars: View {
    let avatars: [(agent: Agent?, state: AgentState?)]
    var more = 0
    var side: CGFloat = AgentAvatarDrawing.side

    var body: some View {
        HStack(spacing: -side / 4) {
            ForEach(Array(avatars.enumerated()), id: \.offset) { _, avatar in
                AgentAvatar(agent: avatar.agent, state: avatar.state, side: side)
            }
            if more > 0 {
                Text("+\(more)")
                    .font(.ui(size: 10, weight: .medium))
                    .foregroundStyle(Color.themeMutedForeground)
                    .padding(.horizontal, AgentAvatarDrawing.morePadding)
                    .frame(minWidth: side)
                    .frame(height: side)
                    .background(Capsule().fill(Color(platform: Theme.borderAccent)))
            }
        }
    }
}

/// The avatar drawn in the transcript's rows, which aren't SwiftUI.
final class AgentAvatarView: FlippedView {
    var agent: Agent? { didSet { if agent != oldValue { redraw() } } }
    var state: AgentState? { didSet { if state != oldValue { redraw() } } }
    var ground = Theme.background { didSet { if ground != oldValue { redraw() } } }
    /// It lies over another, from which a ring of the ground sets it apart.
    var ringed = false { didSet { if ringed != oldValue { redraw() } } }

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
        AgentAvatarDrawing.draw(agent: agent, state: state, ground: ground, ringed: ringed, in: bounds)
    }
}

enum AgentAvatarDrawing {
    static let side = scaled(24)
    /// The dot is as large on every system as the composer strips' are, and cut out of what it lies on.
    static let dot: CGFloat = 6
    static let cut: CGFloat = 2
    /// The room either side of how many more there are, in its pill of the avatars' outline colour.
    static let morePadding: CGFloat = 3
    private static let logos: [Agent: PlatformImage] = Dictionary(uniqueKeysWithValues: Agent.allCases.compactMap { agent in
        agent.logo.map { (agent, ImageFiles.tinted(ImageFiles.sized($0, 64), agent.platformColor)) }
    })

    /// Draws the avatar in `rect`, a square, in the current context.
    static func draw(agent: Agent?, state: AgentState?, ground: PlatformColor, ringed: Bool = false, in rect: CGRect) {
        let box = min(rect.width, rect.height)
        // The circle leaves the box's corner to the dot, which sits on its edge.
        let side = state == nil ? box : box - cut
        let whole = CGRect(x: rect.minX, y: rect.minY, width: side, height: side)
        if ringed {
            ground.setFill()
            RoundedBox.fill(whole, radius: side / 2)
        }
        let circle = whole.insetBy(dx: ringed ? 2 : 0.5, dy: ringed ? 2 : 0.5)
        Theme.backgroundAccent.setFill()
        RoundedBox.fill(circle, radius: side / 2)
        Theme.borderAccent.setStroke()
        RoundedBox.stroke(circle, radius: side / 2)
        let mark = (circle.width * 0.56).rounded()
        let markRect = CGRect(x: circle.midX - mark / 2, y: circle.midY - mark / 2, width: mark, height: mark)
        if let agent, let logo = logos[agent] {
            logo.drawUpright(in: markRect)
        } else {
            TintedSymbol.draw(.bot, size: mark * 0.85, color: Theme.mutedForeground, in: markRect)
        }
        guard let state, let context = Platform.drawing else { return }
        let outer = CGRect(x: rect.minX + box - dot - cut, y: rect.minY + box - dot - cut, width: dot, height: dot)
        context.saveGState()
        context.setBlendMode(.clear)
        RoundedBox.fill(outer.insetBy(dx: -cut, dy: -cut), radius: dot)
        context.restoreGState()
        state.platformColor.setFill()
        RoundedBox.fill(outer, radius: dot / 2)
    }
}
