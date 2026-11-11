import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The box around code inside a list or a quote. Its paragraph leaves the room the box and its
/// header take; the layout manager draws the box.
final class CodeBox: NSObject {
    static let marginBottom: CGFloat = 12
    static let padding: CGFloat = 14
    static let paddingBottom: CGFloat = 12

    let language: String
    /// Which code block of the row it is, so that two in a row stay two boxes.
    let index: Int
    let indent: CGFloat

    init(language: String, index: Int, indent: CGFloat) {
        self.language = language
        self.index = index
        self.indent = indent
    }

    // The same block typeset again is equal, so streamed text keeps the layout before it.
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? CodeBox else { return false }
        return other.index == index && other.language == language && other.indent == indent
    }

    override var hash: Int { index }

    /// From the top of its header to under its last line. The bottom is measured from the last
    /// line's text, not its paragraph's spacing, which TextKit leaves off the last paragraph of a text.
    func frame(of glyphs: NSRange, in layout: NSLayoutManager) -> CGRect {
        guard let container = layout.textContainers.first, glyphs.length > 0 else { return .zero }
        var top: CGFloat?
        var bottom: CGFloat = 0
        layout.enumerateLineFragments(forGlyphRange: glyphs) { rect, used, _, _, _ in
            if top == nil { top = rect.minY }
            bottom = used.maxY
        }
        guard let top else { return .zero }
        let padding = container.lineFragmentPadding
        return CGRect(x: padding + indent, y: top, width: container.size.width - 2 * padding - indent, height: bottom + Self.paddingBottom - top)
    }
}

enum CodeBoxes {
    static func make(language: String, index: Int, indent: CGFloat, style: NSMutableParagraphStyle) -> CodeBox {
        style.firstLineHeadIndent = indent + CodeBox.padding
        style.headIndent = indent + CodeBox.padding
        style.tailIndent = -CodeBox.padding
        style.paragraphSpacingBefore = CodeHeader.height
        style.paragraphSpacing = CodeBox.paddingBottom + CodeBox.marginBottom
        return CodeBox(language: language, index: index, indent: indent)
    }
}

/// Keeps a shell command's words whole: its lines break only after a space, or inside a word
/// longer than the line. A skill's chip never breaks.
final class ShellWords: NSObject, NSLayoutManagerDelegate {
    static let shared = ShellWords()

    func layoutManager(_ layoutManager: NSLayoutManager, shouldBreakLineByWordBeforeCharacterAt charIndex: Int) -> Bool {
        guard let storage = layoutManager.textStorage, charIndex > 0, charIndex < storage.length else { return true }
        var chip = NSRange()
        if storage.attribute(.motileSkill, at: charIndex, effectiveRange: &chip) != nil, charIndex > chip.location { return false }
        guard storage.attribute(.motileShellWords, at: charIndex, effectiveRange: nil) != nil else { return true }
        let before = (storage.string as NSString).character(at: charIndex - 1)
        return before == 0x20 || before == 0x09
    }
}

/// Draws what attributes alone can't: rounded backgrounds behind inline code, the bar beside a
/// quote and horizontal rules.
final class DecoratingLayoutManager: NSLayoutManager {
    override func fillBackgroundRectArray(
        _ rectArray: UnsafePointer<CGRect>,
        count rectCount: Int,
        forCharacterRange charRange: NSRange,
        color: PlatformColor
    ) {
        // Inline code's background is drawn in `drawBackground`.
        guard color !== Typesetter.inlineCodeBackground else { return }
        super.fillBackgroundRectArray(rectArray, count: rectCount, forCharacterRange: charRange, color: color)
    }

    // The selection is drawn by `super`, so it goes over the boxes.
    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: CGPoint) {
        defer { super.drawBackground(forGlyphRange: glyphsToShow, at: origin) }
        guard let storage = textStorage, glyphsToShow.length > 0 else { return }
        let characters = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)
        drawCodeBoxes(in: characters, storage: storage, at: origin)

        storage.enumerateAttribute(.backgroundColor, in: characters) { value, range, _ in
            guard (value as? PlatformColor) === Typesetter.inlineCodeBackground,
                let font = storage.attribute(.font, at: range.location, effectiveRange: nil) as? PlatformFont
            else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let unselected = NSRange(location: NSNotFound, length: 0)
            Typesetter.inlineCodeBackground.setFill()
            // A line's rect is as tall as the line and the space under it, which the last line
            // doesn't have. The font's height around the baseline is the same on every line.
            enumerateLineFragments(forGlyphRange: glyphs) { line, _, container, lineGlyphs, _ in
                guard let onLine = lineGlyphs.intersection(glyphs), onLine.length > 0 else { return }
                let baseline = line.minY + self.location(forGlyphAt: onLine.location).y
                let top = (baseline - font.ascender).rounded()
                let bottom = (baseline - font.descender).rounded()
                self.enumerateEnclosingRects(forGlyphRange: onLine, withinSelectedGlyphRange: unselected, in: container) { rect, _ in
                    let box = CGRect(x: rect.minX - RowTextView.overhang, y: top, width: rect.width + 2 * RowTextView.overhang, height: bottom - top)
                    RoundedBox.fill(box.offsetBy(dx: origin.x, dy: origin.y), radius: Radius.xs)
                }
            }
        }

        drawFound(in: characters, storage: storage, at: origin)

        storage.enumerateAttribute(.motileSkill, in: characters) { value, range, _ in
            guard value != nil else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let unselected = NSRange(location: NSNotFound, length: 0)
            let font = Typesetter.messageFont
            enumerateLineFragments(forGlyphRange: glyphs) { line, _, container, lineGlyphs, _ in
                guard let onLine = lineGlyphs.intersection(glyphs), onLine.length > 0 else { return }
                let baseline = line.minY + self.location(forGlyphAt: onLine.location).y
                self.enumerateEnclosingRects(forGlyphRange: onLine, withinSelectedGlyphRange: unselected, in: container) { rect, _ in
                    let box = SkillChip.box(from: rect.minX, to: rect.maxX, baseline: baseline, beside: font)
                    SkillChip.drawBox(in: box.offsetBy(dx: origin.x, dy: origin.y))
                }
            }
        }

        storage.enumerateAttribute(.motileQuote, in: characters) { value, range, _ in
            guard let depth = (value as? NSNumber)?.intValue, depth > 0 else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            Theme.border.setFill()
            enumerateLineFragments(forGlyphRange: glyphs) { rect, _, container, _, _ in
                for level in 0..<depth {
                    let bar = CGRect(x: rect.minX + container.lineFragmentPadding + CGFloat(level) * 14 + 1, y: rect.minY, width: 2, height: rect.height)
                    bar.offsetBy(dx: origin.x, dy: origin.y).fillCurrent()
                }
            }
        }

        storage.enumerateAttribute(.motileRule, in: characters) { value, range, _ in
            guard value != nil else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            Theme.border.setFill()
            enumerateLineFragments(forGlyphRange: glyphs) { rect, _, container, _, _ in
                let padding = container.lineFragmentPadding
                let line = CGRect(x: rect.minX + padding, y: rect.midY, width: rect.width - 2 * padding, height: 1)
                line.offsetBy(dx: origin.x, dy: origin.y).fillCurrent()
            }
        }
    }

    /// A box behind each match of finding in the thread, as tall as its font around the baseline.
    private func drawFound(in characters: NSRange, storage: NSTextStorage, at origin: CGPoint) {
        storage.enumerateAttribute(.motileFound, in: characters) { value, range, _ in
            guard let current = value as? Bool,
                let font = storage.attribute(.font, at: range.location, effectiveRange: nil) as? PlatformFont
            else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let unselected = NSRange(location: NSNotFound, length: 0)
            (current ? Theme.findCurrent : Theme.findMatch).setFill()
            enumerateLineFragments(forGlyphRange: glyphs) { line, _, container, lineGlyphs, _ in
                guard let onLine = lineGlyphs.intersection(glyphs), onLine.length > 0 else { return }
                let baseline = line.minY + self.location(forGlyphAt: onLine.location).y
                let top = max(line.minY, (baseline - font.ascender - 1).rounded())
                let bottom = min(line.maxY, (baseline - font.descender + 1).rounded())
                self.enumerateEnclosingRects(forGlyphRange: onLine, withinSelectedGlyphRange: unselected, in: container) { rect, _ in
                    let box = CGRect(x: rect.minX - 1, y: top, width: rect.width + 2, height: bottom - top)
                    RoundedBox.fill(box.offsetBy(dx: origin.x, dy: origin.y), radius: Radius.xs)
                }
            }
        }
    }

    private func drawCodeBoxes(in characters: NSRange, storage: NSTextStorage, at origin: CGPoint) {
        storage.enumerateAttribute(.motileCode, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let box = value as? CodeBox, NSIntersectionRange(range, characters).length > 0 else { return }
            let frame = box.frame(of: glyphRange(forCharacterRange: range, actualCharacterRange: nil), in: self)
                .offsetBy(dx: origin.x, dy: origin.y)
                .insetBy(dx: 0.5, dy: 0.5)
            Theme.backgroundSecondary.setFill()
            RoundedBox.fill(frame, radius: Radius.lg)
            Theme.border.setStroke()
            RoundedBox.stroke(frame, radius: Radius.lg)
        }
    }
}

/// The text system of a row's text: TextKit 1, whose layout can be asked for directly. A row's
/// view and `TextMeasure` use the same one, so both lay text out alike.
final class TextSystem {
    let storage = NSTextStorage()
    let layout: NSLayoutManager = DecoratingLayoutManager()
    let container = NSTextContainer(size: CGSize(width: 100, height: CGFloat.greatestFiniteMagnitude))

    init() {
        layout.delegate = ShellWords.shared
        layout.allowsNonContiguousLayout = true
        storage.addLayoutManager(layout)
        container.lineFragmentPadding = RowTextView.overhang
        container.widthTracksTextView = false
        container.heightTracksTextView = false
        layout.addTextContainer(container)
    }

    /// Lets every line be as long as it is. Its height is counted in lines and never laid out, so
    /// a part that is drawn lays out the lines before it too, rather than guessing where it is.
    func unwrap() {
        container.size = CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        container.lineFragmentPadding = 0
        layout.allowsNonContiguousLayout = false
    }

    /// Replaces the text, touching only what follows the part that stayed the same. Streamed
    /// text only ever grows at its end, so most of the layout is kept.
    func update(to new: NSAttributedString) {
        if storage.isEqual(to: new) { return }
        let old = storage.string as NSString
        let common = old.commonPrefix(with: new.string, options: .literal).utf16.count
        // Back to the start of the paragraph, whose style may have changed with its end.
        let kept = common == 0 ? 0 : old.paragraphRange(for: NSRange(location: min(common, max(0, old.length - 1)), length: 0)).location
        let sameStart = kept > 0
            && kept <= new.length
            && storage.attributedSubstring(from: NSRange(location: 0, length: kept))
                .isEqual(to: new.attributedSubstring(from: NSRange(location: 0, length: kept)))
        guard sameStart else {
            storage.setAttributedString(new)
            return
        }
        let tail = new.attributedSubstring(from: NSRange(location: kept, length: new.length - kept))
        storage.replaceCharacters(in: NSRange(location: kept, length: storage.length - kept), with: tail)
    }

    /// Lays the text out at `width` and returns how tall it is.
    /// Where each of its first `count` lines starts, from its top, laid out `width` wide.
    func lineTops(_ count: Int, forWidth width: CGFloat) -> [CGFloat] {
        guard storage.length > 0 else { return [] }
        let size = CGSize(width: width + 2 * container.lineFragmentPadding, height: CGFloat.greatestFiniteMagnitude)
        if container.size != size { container.size = size }
        var tops: [CGFloat] = []
        layout.enumerateLineFragments(forGlyphRange: NSRange(location: 0, length: layout.numberOfGlyphs)) { rect, _, _, _, stop in
            tops.append(rect.minY)
            if tops.count == count { stop.pointee = true }
        }
        return tops
    }

    /// The first character of the first line that starts at or below `y`, from the container's
    /// top, or the text's length when no line does.
    func firstCharacter(below y: CGFloat) -> Int {
        guard storage.length > 0, y > 0 else { return 0 }
        let glyph = layout.glyphIndex(for: CGPoint(x: 0, y: y), in: container)
        var line = NSRange()
        let rect = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: &line)
        let first = rect.minY >= y ? line.location : NSMaxRange(line)
        guard first < layout.numberOfGlyphs else { return storage.length }
        return layout.characterIndexForGlyph(at: first)
    }

    /// Where that part of the text is laid out, from the container's corner.
    func rect(of range: NSRange) -> CGRect? {
        guard range.length > 0, NSMaxRange(range) <= storage.length else { return nil }
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return layout.boundingRect(forGlyphRange: glyphs, in: container)
    }

    func height(forWidth width: CGFloat) -> CGFloat {
        guard storage.length > 0 else { return 0 }
        // A text view on iOS makes its container as tall as itself, which would cut off what a
        // reused view's new text has more.
        let size = CGSize(width: width + 2 * container.lineFragmentPadding, height: CGFloat.greatestFiniteMagnitude)
        if container.size != size { container.size = size }
        layout.ensureLayout(for: container)
        var height = layout.usedRect(for: container).height
        // A code box that ends the text reaches below its last line, with nothing after it to
        // make the room.
        var range = NSRange()
        if let box = storage.attribute(.motileCode, at: storage.length - 1, effectiveRange: &range) as? CodeBox {
            height = max(height, box.frame(of: layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil), in: layout).maxY)
        }
        return ceil(height)
    }

    /// Where each code block's box is, `origin` from the view's corner, with its language and
    /// its code.
    func codeBoxes(origin: CGPoint) -> [(frame: CGRect, language: String, code: String)] {
        var boxes: [(frame: CGRect, language: String, code: String)] = []
        storage.enumerateAttribute(.motileCode, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let block = value as? CodeBox else { return }
            let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let frame = block.frame(of: glyphs, in: layout).offsetBy(dx: origin.x, dy: origin.y)
            var code = Self.withLineBreaks((storage.string as NSString).substring(with: range))
            if code.hasSuffix("\n") { code.removeLast() }
            boxes.append((frame: frame, language: block.language, code: code))
        }
        return boxes
    }

    /// Lines broken inside a paragraph are joined by line separators, which other apps don't
    /// take for line breaks.
    static func withLineBreaks(_ text: String) -> String {
        text.replacingOccurrences(of: "\u{2028}", with: "\n")
    }
}

/// Fades in the part of a layer under a height, which was just added, and leaves the rest.
enum GrowthFade {
    static func run(on layer: CALayer, size: CGSize, below oldHeight: CGFloat) {
        guard size.height > oldHeight,
            let before = mask(height: size.height, opaqueTop: oldHeight),
            let after = mask(height: 1, opaqueTop: 1)
        else { return }
        // An image in a layer is upright whichever way the layer's geometry runs.
        let mask = CALayer()
        mask.frame = CGRect(origin: .zero, size: size)
        mask.contentsGravity = .resize
        mask.contents = after
        layer.mask = mask
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak layer] in
            guard let layer, layer.mask === mask else { return }
            layer.mask = nil
        }
        let fade = CABasicAnimation(keyPath: "contents")
        fade.fromValue = before
        fade.toValue = after
        fade.duration = 0.35
        fade.timingFunction = CAMediaTimingFunction(name: .easeOut)
        mask.add(fade, forKey: "fade")
        CATransaction.commit()
    }

    /// A column of pixels, opaque from the top down to `opaqueTop`.
    private static func mask(height: CGFloat, opaqueTop: CGFloat) -> CGImage? {
        let rows = max(1, Int(height.rounded(.up)))
        let context = CGContext(
            data: nil,
            width: 1,
            height: rows,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
        guard let context else { return nil }
        context.clear(CGRect(x: 0, y: 0, width: 1, height: rows))
        context.setFillColor(CGColor(gray: 0, alpha: 1))
        context.fill(CGRect(x: 0, y: CGFloat(rows) - opaqueTop, width: 1, height: opaqueTop))
        return context.makeImage()
    }
}

/// Says how tall text is in a `RowTextView` of some width, without a view, so that rows can be
/// measured off the main thread. Its one text system is used by a thread at a time.
enum TextMeasure {
    private static let lock = NSLock()
    private static let system: TextSystem = {
        let system = TextSystem()
        #if os(macOS)
        // Background layout runs on the main thread, where this text system must not be touched.
        system.layout.backgroundLayoutEnabled = false
        #endif
        return system
    }()

    static func height(of text: NSAttributedString, width: CGFloat) -> CGFloat {
        guard text.length > 0 else { return 0 }
        lock.lock()
        defer { lock.unlock() }
        system.storage.setAttributedString(text)
        return system.height(forWidth: width)
    }

    static func lineTops(of text: NSAttributedString, count: Int, width: CGFloat) -> [CGFloat] {
        guard text.length > 0 else { return [] }
        lock.lock()
        defer { lock.unlock() }
        system.storage.setAttributedString(text)
        return system.lineTops(count, forWidth: width)
    }
}

extension RowTextView {
    /// How far inline code's box reaches past its text. The text's container is this much wider
    /// than its text on each side, so that a box at the start or the end of a line isn't cut off.
    static let overhang: CGFloat = 2

    /// Places the view so that its text takes `frame`.
    func place(_ frame: CGRect) {
        self.frame = frame.insetBy(dx: -Self.overhang, dy: 0)
    }

    /// Lays out a little of every kind of text once, at launch, so that the first reply doesn't
    /// pay for loading the fonts and the text system while it streams.
    static func warmUp() {
        let view = make()
        let sample = NSMutableAttributedString(attributedString: Typesetter.plain("Warm up", color: Theme.foreground))
        sample.append(Typesetter.code("let warm = true", spans: [0, 3, 2]))
        sample.append(Typesetter.mono("up", color: Theme.mutedForeground))
        sample.append(NSAttributedString(string: "bold", attributes: [.font: Theme.proseBold]))
        sample.append(NSAttributedString(string: "heading", attributes: [.font: Theme.heading(2)]))
        view.content = sample
        _ = view.height(forWidth: 400)
    }
}

/// A row's text with what finding in the thread found drawn over it.
struct FoundText {
    private var base = NSAttributedString()
    private var marks: FoundMarks?

    /// Shows the row's text, without the marks of the row the view showed before.
    mutating func show(_ text: NSAttributedString, in view: RowTextView) {
        base = text
        marks = nil
        view.content = text
    }

    mutating func mark(_ marks: FoundMarks?, in view: RowTextView) {
        guard marks != self.marks else { return }
        self.marks = marks
        view.content = marks.map { Typesetter.found(base, $0) } ?? base
        view.redraw()
    }

    /// Where that part of the text the core sent is in the view.
    func rect(of range: NSRange, in view: RowTextView) -> CGRect? {
        Typesetter.shownRange(range, in: base).flatMap { view.rect(of: $0) }
    }

    /// Where the first whole line at or below `y` in the view starts in the text the core sent.
    func offset(below y: CGFloat, in view: RowTextView) -> Int {
        Typesetter.sentLocation(view.firstCharacter(below: y), in: base)
    }
}
