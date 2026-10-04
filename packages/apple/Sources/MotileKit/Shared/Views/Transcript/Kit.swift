import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A filled, rounded rectangle whose colours follow the appearance.
final class SurfaceView: LayerView {
    var fill: PlatformColor = .clear { didSet { repaint() } }
    var stroke: PlatformColor? { didSet { repaint() } }
    var dotted = false { didSet { repaint() } }
    var radius: CGFloat = 0 { didSet { repaint() } }
    private var dots: CAShapeLayer?

    override var frame: CGRect {
        didSet {
            guard dotted, frame.size != oldValue.size else { return }
            repaint()
        }
    }

    /// A clickable surface takes the clicks on the labels and icons inside it.
    var onClick: (() -> Void)? {
        didSet { onPress = onClick.map { action in { _ in action() } } }
    }

    override func paint(_ layer: CALayer) {
        layer.backgroundColor = resolved(fill)
        layer.cornerRadius = radius
        layer.cornerCurve = .continuous
        layer.borderColor = dotted ? nil : stroke.map(resolved)
        layer.borderWidth = stroke == nil || dotted ? 0 : 1
        paintDots(layer)
    }

    private func paintDots(_ layer: CALayer) {
        guard dotted, let stroke else {
            dots?.removeFromSuperlayer()
            dots = nil
            return
        }
        let dots = self.dots ?? CAShapeLayer()
        self.dots = dots
        if dots.superlayer !== layer { layer.addSublayer(dots) }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let rect = layer.bounds.insetBy(dx: 0.5, dy: 0.5)
        let corner = max(0, min(radius - 0.5, rect.width / 2, rect.height / 2))
        dots.frame = layer.bounds
        dots.contentsScale = layer.contentsScale
        dots.path = rect.isEmpty ? nil : CGPath(roundedRect: rect, cornerWidth: corner, cornerHeight: corner, transform: nil)
        dots.fillColor = nil
        dots.strokeColor = resolved(stroke)
        dots.lineWidth = 1
        dots.lineDashPattern = [2, 3]
        CATransaction.commit()
    }
}

/// A button of words inside a row. All of its frame takes the click, and what lights up under
/// the pointer is inset from it, so buttons that touch each other and the row's edge look apart.
final class RowButton: FlippedView {
    private let highlight = SurfaceView()
    private let title = TextLabel(font: Theme.smallFont, color: Theme.secondary)
    private let insets: PlatformEdgeInsets
    private static let titleHeight = scaled(16)

    /// The room between the words and the highlight's sides.
    static let padding: CGFloat = 8

    init(title: String, tooltip: String, radius: CGFloat, insets: PlatformEdgeInsets, action: @escaping () -> Void) {
        self.insets = insets
        super.init(frame: .zero)
        highlight.radius = radius
        addSubview(highlight)
        self.title.string = title
        self.title.centered = true
        highlight.addSubview(self.title)
        tip = tooltip
        describe(title, button: true)
        onPress = { _ in action() }
        onHover = { [weak self] point in self?.highlight.fill = point == nil ? .clear : Theme.hover }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    var width: CGFloat {
        let words = ceil(title.string.size(withAttributes: [.font: Theme.smallFont]).width)
        return words + 2 * Self.padding + insets.left + insets.right
    }

    override var frame: CGRect {
        didSet {
            let lit = CGSize(width: bounds.width - insets.left - insets.right, height: bounds.height - insets.top - insets.bottom)
            highlight.frame = CGRect(x: insets.left, y: insets.top, width: max(0, lit.width), height: max(0, lit.height))
            title.frame = CGRect(x: 0, y: ((lit.height - Self.titleHeight) / 2).rounded(), width: max(0, lit.width), height: Self.titleHeight)
        }
    }

    /// A button that is shown again starts unlit, wherever the pointer left it.
    func dim() {
        highlight.fill = .clear
    }
}

/// The box an image or a video is shown in: empty until the picture is there.
final class PictureView: LayerView {
    static let radius: CGFloat = 10

    var picture: CGImage? { didSet { repaint() } }
    /// Fills the box with the picture, cutting off what doesn't fit, instead of showing all of it.
    var fills = false { didSet { repaint() } }

    override func paint(_ layer: CALayer) {
        layer.contents = picture
        layer.contentsGravity = fills ? .resizeAspectFill : .resizeAspect
        layer.backgroundColor = resolved(Theme.bubble)
        layer.cornerRadius = Self.radius
        layer.cornerCurve = .continuous
        layer.masksToBounds = true
        layer.borderColor = resolved(Theme.border)
        layer.borderWidth = 1
    }
}

/// Symbols drawn in a colour that follows the appearance.
enum TintedSymbol {
    private static var cache: [String: PlatformImage] = [:]

    static func image(_ name: Symbol, size: CGFloat, color: PlatformColor) -> PlatformImage {
        let key = "\(name.rawValue)/\(size)/\(ObjectIdentifier(color).hashValue)"
        if let cached = cache[key] { return cached }
        let symbol = PlatformImage.symbol(name, size: size)
        #if os(macOS)
        let tinted = NSImage(size: symbol.size, flipped: false) { rect in
            symbol.draw(in: rect)
            color.set()
            rect.fill(using: .sourceAtop)
            return true
        }
        #else
        let tinted = symbol.withTintColor(color, renderingMode: .alwaysOriginal)
        #endif
        cache[key] = tinted
        return tinted
    }

    /// Draws the symbol in the middle of `rect`.
    static func draw(_ name: Symbol, size: CGFloat, color: PlatformColor, in rect: CGRect) {
        let image = image(name, size: size, color: color)
        let origin = CGPoint(x: (rect.midX - image.size.width / 2).rounded(), y: (rect.midY - image.size.height / 2).rounded())
        let frame = CGRect(origin: origin, size: image.size)
        #if os(macOS)
        image.draw(in: frame, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        #else
        image.draw(in: frame)
        #endif
    }
}

/// The lines added and removed, as they are written everywhere: `+12 −3`.
enum LineCountText {
    static let font = PlatformFont.uiDigits(11.5, weight: .medium)

    static func text(added: Int, removed: Int) -> NSAttributedString {
        let text = NSMutableAttributedString()
        if added > 0 || removed == 0 {
            text.append(NSAttributedString(string: "+\(added)", attributes: [.font: font, .foregroundColor: Theme.success]))
        }
        if removed > 0 || added == 0 {
            let space = text.length > 0 ? " " : ""
            text.append(NSAttributedString(string: "\(space)−\(removed)", attributes: [.font: font, .foregroundColor: Theme.danger]))
        }
        return text
    }
}

extension NSAttributedString {
    /// Draws one line in the rectangle, cut short where it doesn't fit.
    func drawTruncated(in rect: CGRect) {
        #if os(macOS)
        draw(with: rect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        #else
        draw(with: rect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine], context: nil)
        #endif
    }

    /// How large the text is when it wraps at `width`.
    func bounds(width: CGFloat) -> CGRect {
        let size = CGSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        #if os(macOS)
        return boundingRect(with: size, options: [.usesLineFragmentOrigin, .usesFontLeading])
        #else
        return boundingRect(with: size, options: [.usesLineFragmentOrigin, .usesFontLeading], context: nil)
        #endif
    }
}
