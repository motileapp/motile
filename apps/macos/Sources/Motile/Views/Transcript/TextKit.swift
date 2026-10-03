import AppKit
import QuartzCore

/// The box around code inside a list or a quote. The text system leaves room for its padding,
/// so nothing overlaps it; the prose row puts the code's header in the room at its top.
final class CodeTextBlock: NSTextBlock {
    private static let marginBottom: CGFloat = 12

    let language: String
    /// Which code block of the row it is, so that two in a row stay two boxes.
    let index: Int
    let indent: CGFloat

    init(language: String, index: Int, indent: CGFloat) {
        self.language = language
        self.index = index
        self.indent = indent
        super.init()
        setWidth(indent, type: .absoluteValueType, for: .margin, edge: .minX)
        setWidth(Self.marginBottom, type: .absoluteValueType, for: .margin, edge: .maxY)
        setWidth(CodeHeader.height, type: .absoluteValueType, for: .padding, edge: .minY)
        setWidth(14, type: .absoluteValueType, for: .padding, edge: .minX)
        setWidth(14, type: .absoluteValueType, for: .padding, edge: .maxX)
        setWidth(12, type: .absoluteValueType, for: .padding, edge: .maxY)
    }

    // The text system may archive attributes; what comes back is drawn as a plain block.
    required init?(coder: NSCoder) {
        language = ""
        index = -1
        indent = 0
        super.init(coder: coder)
    }

    // It never changes once made.
    override func copy(with zone: NSZone? = nil) -> Any { self }

    // The same block typeset again is equal, so streamed text keeps the layout before it.
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? CodeTextBlock else { return false }
        return other.index == index && other.language == language && other.indent == indent
    }

    override var hash: Int { index }

    /// The box inside the margins of the block's frame.
    func box(in frame: NSRect) -> NSRect {
        NSRect(x: frame.minX + indent, y: frame.minY, width: frame.width - indent, height: frame.height - Self.marginBottom)
    }

    override func drawBackground(
        withFrame frameRect: NSRect,
        in controlView: NSView?,
        characterRange charRange: NSRange,
        layoutManager: NSLayoutManager
    ) {
        let path = NSBezierPath(roundedRect: box(in: frameRect).insetBy(dx: 0.5, dy: 0.5), xRadius: 10, yRadius: 10)
        Theme.codeBackground.setFill()
        path.fill()
        Theme.border.setStroke()
        path.lineWidth = 1
        path.stroke()
    }
}

/// Draws what attributes alone can't: rounded backgrounds behind inline code, the bar beside a
/// quote and horizontal rules.
final class DecoratingLayoutManager: NSLayoutManager {
    override func fillBackgroundRectArray(
        _ rectArray: UnsafePointer<NSRect>,
        count rectCount: Int,
        forCharacterRange charRange: NSRange,
        color: NSColor
    ) {
        // Inline code's background is drawn in `drawBackground`.
        guard color !== Typesetter.inlineCodeBackground else { return }
        super.fillBackgroundRectArray(rectArray, count: rectCount, forCharacterRange: charRange, color: color)
    }

    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
        guard let storage = textStorage, glyphsToShow.length > 0 else { return }
        let characters = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)

        storage.enumerateAttribute(.backgroundColor, in: characters) { value, range, _ in
            guard (value as? NSColor) === Typesetter.inlineCodeBackground,
                let font = storage.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont
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
                    let box = NSRect(x: rect.minX - 2, y: top, width: rect.width + 4, height: bottom - top)
                    NSBezierPath(roundedRect: box.offsetBy(dx: origin.x, dy: origin.y), xRadius: 4, yRadius: 4).fill()
                }
            }
        }

        storage.enumerateAttribute(.motileQuote, in: characters) { value, range, _ in
            guard let depth = (value as? NSNumber)?.intValue, depth > 0 else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            Theme.strongBorder.setFill()
            enumerateLineFragments(forGlyphRange: glyphs) { rect, _, _, _, _ in
                for level in 0..<depth {
                    let bar = NSRect(x: rect.minX + CGFloat(level) * 14 + 1, y: rect.minY, width: 2, height: rect.height)
                    bar.offsetBy(dx: origin.x, dy: origin.y).fill()
                }
            }
        }

        storage.enumerateAttribute(.motileRule, in: characters) { value, range, _ in
            guard value != nil else { return }
            let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            Theme.border.setFill()
            enumerateLineFragments(forGlyphRange: glyphs) { rect, _, _, _, _ in
                let line = NSRect(x: rect.minX, y: rect.midY, width: rect.width, height: 1)
                line.offsetBy(dx: origin.x, dy: origin.y).fill()
            }
        }
    }
}

/// A read-only text view that the transcript sizes itself. It never scrolls; wheel events go on
/// to the transcript.
final class RowTextView: NSTextView {
    /// Called when the user starts selecting here, so other rows can let go of their selection.
    var onSelect: (() -> Void)?

    /// The text system of a row's text. `TextMeasure` uses the same one, so both lay text out alike.
    fileprivate static func textSystem() -> (storage: NSTextStorage, layout: NSLayoutManager, container: NSTextContainer) {
        let storage = NSTextStorage()
        let layout = DecoratingLayoutManager()
        layout.allowsNonContiguousLayout = true
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        container.widthTracksTextView = false
        container.heightTracksTextView = false
        layout.addTextContainer(container)
        return (storage, layout, container)
    }

    static func make(wraps: Bool = true) -> RowTextView {
        let system = textSystem()
        let container = system.container
        // The view takes the storage as its own while it is made.
        let view = withExtendedLifetime(system.storage) { RowTextView(frame: .zero, textContainer: container) }
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = false
        view.textContainerInset = .zero
        view.isVerticallyResizable = false
        view.isHorizontallyResizable = false
        view.isAutomaticLinkDetectionEnabled = false
        view.usesFontPanel = false
        view.usesFindBar = false
        view.isRichText = true
        view.linkTextAttributes = [.foregroundColor: Theme.link, .cursor: NSCursor.pointingHand]
        if !wraps {
            container.size = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        }
        return view
    }

    /// Lays out a little of every kind of text once, at launch, so that the first reply doesn't
    /// pay for loading the fonts and the text system while it streams.
    static func warmUp() {
        let view = make()
        let sample = NSMutableAttributedString(attributedString: Typesetter.plain("Warm up", color: Theme.text))
        sample.append(Typesetter.code("let warm = true", spans: [0, 3, 2]))
        sample.append(Typesetter.mono("up", color: Theme.secondary))
        sample.append(NSAttributedString(string: "bold", attributes: [.font: Theme.proseBold]))
        sample.append(NSAttributedString(string: "heading", attributes: [.font: Theme.heading(2)]))
        view.content = sample
        _ = view.height(forWidth: 400)
    }

    var content: NSAttributedString {
        get { textStorage ?? NSAttributedString() }
        set { update(to: newValue) }
    }

    /// Replaces the text, touching only what follows the part that stayed the same. Streamed
    /// text only ever grows at its end, so most of the layout is kept.
    private func update(to new: NSAttributedString) {
        guard let storage = textStorage else { return }
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
    func height(forWidth width: CGFloat) -> CGFloat {
        guard let container = textContainer, let layout = layoutManager, content.length > 0 else { return 0 }
        if container.size.width != width {
            container.size = NSSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        }
        layout.ensureLayout(for: container)
        return ceil(layout.usedRect(for: container).height)
    }

    /// How wide and tall the text is when it doesn't wrap.
    func naturalSize() -> NSSize {
        guard let container = textContainer, let layout = layoutManager else { return .zero }
        layout.ensureLayout(for: container)
        let used = layout.usedRect(for: container)
        return NSSize(width: ceil(used.width), height: ceil(used.height))
    }

    /// Fades in the text under `oldHeight`, which was just added, and leaves the rest as it is.
    func fadeIn(below oldHeight: CGFloat) {
        wantsLayer = true
        guard let layer, bounds.height > oldHeight,
            let before = Self.mask(height: bounds.height, opaqueTop: oldHeight),
            let after = Self.mask(height: 1, opaqueTop: 1)
        else { return }
        // An image in a layer is upright whichever way the layer's geometry runs.
        let mask = CALayer()
        mask.frame = CGRect(origin: .zero, size: bounds.size)
        mask.contentsGravity = .resize
        mask.contents = after
        layer.mask = mask
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak self] in
            guard let self, self.layer?.mask === mask else { return }
            self.layer?.mask = nil
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

    /// Where each code block's box is in the view, with its language and its code.
    func codeBoxes() -> [(frame: NSRect, language: String, code: String)] {
        guard let storage = textStorage, let layout = layoutManager else { return [] }
        var boxes: [(frame: NSRect, language: String, code: String)] = []
        storage.enumerateAttribute(.motileCode, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let block = value as? CodeTextBlock else { return }
            let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let frame = layout.boundsRect(for: block, glyphRange: glyphs)
                .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
            var code = Self.withLineBreaks((storage.string as NSString).substring(with: range))
            if code.hasSuffix("\n") { code.removeLast() }
            boxes.append((frame: block.box(in: frame), language: block.language, code: code))
        }
        return boxes
    }

    /// Lines broken inside a paragraph are joined by line separators, which other apps don't
    /// take for line breaks.
    static func withLineBreaks(_ text: String) -> String {
        text.replacingOccurrences(of: "\u{2028}", with: "\n")
    }

    override func writeSelection(to pboard: NSPasteboard, types: [NSPasteboard.PasteboardType]) -> Bool {
        guard super.writeSelection(to: pboard, types: types) else { return false }
        guard let text = pboard.string(forType: .string), text.contains("\u{2028}") else { return true }
        pboard.setString(Self.withLineBreaks(text), forType: .string)
        return true
    }

    func clearSelection() {
        guard selectedRange().length > 0 else { return }
        setSelectedRange(NSRange(location: 0, length: 0))
    }

    override func mouseDown(with event: NSEvent) {
        onSelect?()
        super.mouseDown(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        nextResponder?.scrollWheel(with: event)
    }

    // What floats over the text, like the jump button, keeps its own cursor.
    override func mouseMoved(with event: NSEvent) {
        guard isUnderPointer(event) else { return }
        super.mouseMoved(with: event)
    }

    override func cursorUpdate(with event: NSEvent) {
        guard isUnderPointer(event) else { return }
        super.cursorUpdate(with: event)
    }

    private func isUnderPointer(_ event: NSEvent) -> Bool {
        window?.contentView?.hitTest(event.locationInWindow)?.isDescendant(of: self) ?? false
    }

    // The transcript decides the size; the text view must not grow itself to fit.
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: NSView.noIntrinsicMetric) }
}

/// Says how tall text is in a `RowTextView` of some width, without a view, so that rows can be
/// measured off the main thread. Its one text system is used by a thread at a time.
enum TextMeasure {
    private static let lock = NSLock()
    private static let system: (storage: NSTextStorage, layout: NSLayoutManager, container: NSTextContainer) = {
        let system = RowTextView.textSystem()
        // Background layout runs on the main thread, where this text system must not be touched.
        system.layout.backgroundLayoutEnabled = false
        return system
    }()

    static func height(of text: NSAttributedString, width: CGFloat) -> CGFloat {
        guard text.length > 0 else { return 0 }
        lock.lock()
        defer { lock.unlock() }
        system.storage.setAttributedString(text)
        system.container.size = NSSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        system.layout.ensureLayout(for: system.container)
        return ceil(system.layout.usedRect(for: system.container).height)
    }
}

/// Shows code that is wider than the column and moves it sideways under the pointer. It is a
/// plain clipping view rather than a scroll view: a scroll view inside the transcript's own
/// doesn't redraw what the transcript scrolls into view.
final class SidewaysClipView: NSView {
    private weak var content: NSView?
    private var contentWidth: CGFloat = 0
    private var offset: CGFloat = 0
    /// Whether the gesture under way moves the code sideways. A gesture keeps the way it set out,
    /// as it does in a scroll view, so one that scrolls the transcript never drags the code along.
    private var gestureIsSideways: Bool?

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func setContent(_ view: NSView, size: NSSize) {
        content = view
        contentWidth = size.width
        offset = min(offset, max(0, contentWidth - bounds.width))
        view.frame = NSRect(x: -offset, y: 0, width: size.width, height: size.height)
    }

    private func isSideways(_ event: NSEvent) -> Bool {
        let sideways = abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
        // A wheel that isn't a gesture says so with every notch.
        guard !event.phase.isEmpty || !event.momentumPhase.isEmpty else { return sideways }
        if event.phase.contains(.began) || event.phase.contains(.mayBegin) { gestureIsSideways = nil }
        if gestureIsSideways == nil, event.scrollingDeltaX != 0 || event.scrollingDeltaY != 0 { gestureIsSideways = sideways }
        return gestureIsSideways ?? false
    }

    override func scrollWheel(with event: NSEvent) {
        let overflow = contentWidth - bounds.width
        guard isSideways(event), overflow > 0, let content else {
            nextResponder?.scrollWheel(with: event)
            return
        }
        offset = min(max(0, offset - event.scrollingDeltaX), overflow)
        content.frame.origin.x = -offset
    }
}

/// A view whose origin is its top left corner, like the text inside it.
class FlippedView: NSView {
    override var isFlipped: Bool { true }
}
