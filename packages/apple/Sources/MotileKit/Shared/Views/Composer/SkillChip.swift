import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A skill a message starts, drawn as a chip in the composer and in the transcript: the skill's
/// symbol and its name in the skill colour, on its wash.
enum SkillChip {
    static let font = PlatformFont.ui(13, weight: .regular)
    static let symbol = Symbol.box
    /// Smaller than the icon for its text, to keep clear of the box's edges, which hug the text.
    static let symbolSize = IconSize.sm
    static let padding: CGFloat = 5
    static let gap: CGFloat = 4
    /// The room between the chip and the text on either side.
    static let margin: CGFloat = 1
    static let radius = Radius.xs
    static let wash = tinted(.colorTint)
    static let edge = tinted(.colorTintChart)

    private static func tinted(_ opacity: Opacity) -> PlatformColor {
        .dynamic(
            Theme.resolved(Theme.skill, dark: false).withAlphaComponent(opacity.light),
            Theme.resolved(Theme.skill, dark: true).withAlphaComponent(opacity.dark)
        )
    }

    static var symbolSide: CGFloat { PlatformImage.symbolSide(symbolSize) }

    /// The room before the name: the padding, the symbol and the gap after it.
    static var lead: CGFloat { padding + symbolSide + gap }

    /// How tall the chip is beside text of `font`: from its ascender to its descender, as inline
    /// code's box is, so that it never rises above its line, where the first line's is cut off.
    static func height(beside font: PlatformFont) -> CGFloat {
        ceil(font.ascender - font.descender)
    }

    /// Where the chip's box is on a line of text of `font` whose baseline is at `baseline`, in a
    /// view whose y grows downwards.
    static func box(from left: CGFloat, to right: CGFloat, baseline: CGFloat, beside font: PlatformFont) -> CGRect {
        let top = (baseline - font.ascender).rounded()
        let bottom = (baseline - font.descender).rounded()
        return CGRect(x: left + margin, y: top, width: right - left - margin * 2, height: bottom - top)
    }

    /// The chip's box, its edge and its symbol in `rect`, in a view whose y grows downwards.
    static func drawBox(in rect: CGRect) {
        let box = rect.insetBy(dx: 0.5, dy: 0.5)
        wash.drawn.setFill()
        RoundedBox.fill(box, radius: radius)
        edge.drawn.setStroke()
        RoundedBox.stroke(box, radius: radius)
        guard let context = Platform.drawing, let path = PlatformImage.symbolPath(symbol, side: symbolSide) else { return }
        let side = symbolSide
        let origin = CGPoint(x: rect.minX + padding, y: (rect.midY - side / 2).rounded())
        context.saveGState()
        context.translateBy(x: origin.x, y: origin.y + side)
        context.scaleBy(x: 1, y: -1)
        context.addPath(path)
        context.setFillColor(Theme.skill.drawn.cgColor)
        context.fillPath()
        context.restoreGState()
    }

    /// The whole chip, its name too, as the composer draws it.
    static func draw(_ label: String, in rect: CGRect) {
        let box = rect.insetBy(dx: margin, dy: 0)
        drawBox(in: box)
        let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: Theme.skill.drawn]
        let lineHeight = font.ascender - font.descender
        let origin = CGPoint(x: box.minX + lead, y: (box.midY - lineHeight / 2).rounded())
        (label as NSString).draw(at: origin, withAttributes: attributes)
    }

    static func size(_ label: String, beside text: PlatformFont) -> CGSize {
        let width = (label as NSString).size(withAttributes: [.font: font]).width
        return CGSize(width: ceil(lead + width + padding + margin * 2), height: height(beside: text))
    }

    /// A message's text with its skills' `$name`s as chips, which the transcript draws: the name
    /// in the skill colour after room for the symbol, and the box behind it drawn by its layout
    /// manager.
    static func set(_ spans: [SkillSpan], in text: NSMutableAttributedString) {
        for span in spans.sorted(by: { $0.range.location > $1.range.location }) {
            guard span.range.location >= 0, NSMaxRange(span.range) <= text.length else { continue }
            var attributes = text.attributes(at: span.range.location, effectiveRange: nil)
            let textFont = attributes[.font] as? PlatformFont ?? font
            attributes[.font] = font
            attributes[.foregroundColor] = Theme.skill
            attributes[.motileSkill] = span.name
            attributes[.link] = nil
            // A no-break space keeps the room for the symbol on the line of the name. It keeps the
            // text's font, so that a line of chips alone is as tall as their boxes.
            let chip = NSMutableAttributedString(string: "\u{00A0}" + span.label, attributes: attributes)
            let space = ("\u{00A0}" as NSString).size(withAttributes: [.font: textFont]).width
            chip.addAttributes([.font: textFont, .kern: max(0, margin + lead - space)], range: NSRange(location: 0, length: 1))
            chip.addAttribute(.kern, value: padding + margin, range: NSRange(location: chip.length - 1, length: 1))
            text.replaceCharacters(in: span.range, with: chip)
        }
    }
}

extension NSAttributedString.Key {
    /// A skill's chip in a message, by the skill's name.
    static let motileSkill = NSAttributedString.Key("motile.skill")
}

extension NSString {
    /// The word that ends at `cursor` when it starts with `$` or `/`, which a skill picked from
    /// the menu replaces: what is typed now, which the menu may not have caught up with.
    func skillWord(endingAt cursor: Int) -> NSRange? {
        let before = NSRange(location: 0, length: cursor)
        let space = rangeOfCharacter(from: .whitespacesAndNewlines, options: .backwards, range: before)
        let start = space.location == NSNotFound ? 0 : NSMaxRange(space)
        guard start < cursor, ["$", "/"].contains(substring(with: NSRange(location: start, length: 1))) else { return nil }
        return NSRange(location: start, length: cursor - start)
    }
}

extension NSAttributedString {
    /// The text with each skill's chip written as its `$name` again: the composer's, and a
    /// message's.
    var withSkillNames: String {
        let text = NSMutableString(string: string)
        var chips: [(NSRange, String)] = []
        let all = NSRange(location: 0, length: length)
        enumerateAttribute(.attachment, in: all) { value, range, _ in
            guard let chip = value as? SkillChipAttachment else { return }
            chips.append((range, chip.name))
        }
        enumerateAttribute(.motileSkill, in: all) { value, range, _ in
            guard let name = value as? String else { return }
            chips.append((range, name))
        }
        for (range, name) in chips.sorted(by: { $0.0.location > $1.0.location }) {
            text.replaceCharacters(in: range, with: "$" + name)
        }
        return text as String
    }
}

/// A skill's chip in the composer: one character, which the cursor steps over and a delete takes
/// whole.
final class SkillChipAttachment: NSTextAttachment {
    let name: String
    let label: String

    init(name: String, label: String, beside text: PlatformFont) {
        self.name = name
        self.label = label
        super.init(data: nil, ofType: nil)
        #if os(macOS)
        attachmentCell = SkillChipCell(label: label, beside: text)
        #else
        let size = SkillChip.size(label, beside: text)
        bounds = CGRect(x: 0, y: text.descender, width: size.width, height: size.height)
        #endif
    }

    required init?(coder: NSCoder) { nil }

    #if os(iOS)
    // Drawn each time it shows, in the light or the dark it shows in.
    override func image(for bounds: CGRect, attributes: [NSAttributedString.Key: Any], location: any NSTextLocation, textContainer: NSTextContainer?) -> UIImage? {
        drawn()
    }

    override func image(forBounds imageBounds: CGRect, textContainer: NSTextContainer?, characterIndex charIndex: Int) -> UIImage? {
        drawn()
    }

    private func drawn() -> UIImage {
        let traits = UITraitCollection.current
        let size = bounds.size
        return UIGraphicsImageRenderer(size: size, format: UIGraphicsImageRendererFormat(for: traits)).image { _ in
            traits.performAsCurrent { SkillChip.draw(label, in: CGRect(origin: .zero, size: size)) }
        }
    }
    #endif
}

#if os(macOS)
private final class SkillChipCell: NSTextAttachmentCell {
    let label: String
    let text: NSFont

    init(label: String, beside text: NSFont) {
        self.label = label
        self.text = text
        super.init(textCell: "")
    }

    required init(coder: NSCoder) { fatalError() }

    override func cellSize() -> NSSize { SkillChip.size(label, beside: text) }

    override func cellBaselineOffset() -> NSPoint { NSPoint(x: 0, y: text.descender) }

    override func draw(withFrame cellFrame: NSRect, in controlView: NSView?) {
        SkillChip.draw(label, in: cellFrame)
    }

    override func wantsToTrackMouse() -> Bool { false }
}
#endif
