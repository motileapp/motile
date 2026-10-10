import QuartzCore

/// A scroll view's mask that fades its content out at an edge with more behind it, the further
/// the more is hidden, up to `length`.
final class EdgeFade: CAGradientLayer {
    static let length = scaled(24)

    override init() {
        super.init()
        actions = ["bounds": NSNull(), "position": NSNull(), "colors": NSNull(), "locations": NSNull()]
        update(frame: .zero, above: 0, below: 0)
    }

    override init(layer: Any) {
        super.init(layer: layer)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// `above` and `below` are how much of the content is hidden past the top and the bottom.
    func update(frame: CGRect, above: CGFloat, below: CGFloat) {
        self.frame = frame
        guard frame.height > 0 else { return }
        let edge = min(0.5, Self.length / frame.height)
        locations = [0, NSNumber(value: Double(edge)), NSNumber(value: Double(1 - edge)), 1]
        colors = [Self.black(shown: 1 - above / Self.length), Self.black(shown: 1), Self.black(shown: 1), Self.black(shown: 1 - below / Self.length)]
    }

    private static func black(shown: CGFloat) -> CGColor {
        CGColor(gray: 0, alpha: min(1, max(0, shown)))
    }
}
