import QuartzCore

/// A scroll view's mask that fades its content out at an edge with more behind it, the further
/// the more is hidden, up to `length`.
final class EdgeFade: CAGradientLayer {
    static let length = scaled(24)

    override init() {
        super.init()
        actions = ["bounds": NSNull(), "position": NSNull(), "colors": NSNull(), "locations": NSNull()]
        anchorPoint = .zero
        update(frame: .zero, above: 0, below: 0)
    }

    override init(layer: Any) {
        super.init(layer: layer)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// `above` and `below` are how much of the content is hidden past the top and the bottom.
    func update(frame: CGRect, above: CGFloat, below: CGFloat, length: CGFloat = EdgeFade.length) {
        self.frame = frame
        guard frame.height > 0 else { return }
        let edge = min(0.5, length / frame.height)
        locations = [0, NSNumber(value: Double(edge)), NSNumber(value: Double(1 - edge)), 1]
        colors = [Self.black(shown: 1 - above / length), Self.black(shown: 1), Self.black(shown: 1), Self.black(shown: 1 - below / length)]
    }

    /// Resizes along with the layer it masks while that layer's size animates, so that what the
    /// layer still shows isn't cut to its new size early.
    func follow(_ masked: CALayer) {
        guard let resize = masked.animation(forKey: "bounds.size") else { return }
        add(resize, forKey: "bounds.size")
    }

    private static func black(shown: CGFloat) -> CGColor {
        CGColor(gray: 0, alpha: min(1, max(0, shown)))
    }
}
