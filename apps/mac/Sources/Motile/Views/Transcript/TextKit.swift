import AppKit

/// Draws what attributes alone can't: rounded backgrounds behind inline code, the bar beside a
/// quote, horizontal rules, and the box around code inside a list.
final class DecoratingLayoutManager: NSLayoutManager {
    override func fillBackgroundRectArray(
        _ rectArray: UnsafePointer<NSRect>,
        count rectCount: Int,
        forCharacterRange charRange: NSRange,
        color: NSColor
    ) {
        guard color === Typesetter.inlineCodeBackground else {
            super.fillBackgroundRectArray(rectArray, count: rectCount, forCharacterRange: charRange, color: color)
            return
        }
        color.setFill()
        for index in 0..<rectCount {
            let rect = rectArray[index].insetBy(dx: -2, dy: 1)
            NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4).fill()
        }
    }

    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
        guard let storage = textStorage, glyphsToShow.length > 0 else { return }
        let characters = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)

        storage.enumerateAttribute(.motilePre, in: characters) { value, range, _ in
            guard value != nil else { return }
            // The whole paragraph's box, even when only part of it is being drawn.
            let paragraph = (storage.string as NSString).paragraphRange(for: range)
            let glyphs = glyphRange(forCharacterRange: paragraph, actualCharacterRange: nil)
            var box = NSRect.null
            enumerateLineFragments(forGlyphRange: glyphs) { rect, _, _, _, _ in box = box.union(rect) }
            guard !box.isNull, let style = storage.attribute(.paragraphStyle, at: paragraph.location, effectiveRange: nil) as? NSParagraphStyle else {
                return
            }
            let left = style.headIndent - 10
            let frame = NSRect(x: box.minX + left, y: box.minY - 5, width: box.width - left, height: box.height + 10)
            Theme.codeBackground.setFill()
            NSBezierPath(roundedRect: frame.offsetBy(dx: origin.x, dy: origin.y), xRadius: 7, yRadius: 7).fill()
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

    static func make(wraps: Bool = true) -> RowTextView {
        let storage = NSTextStorage()
        let layout = DecoratingLayoutManager()
        layout.allowsNonContiguousLayout = true
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        container.widthTracksTextView = false
        container.heightTracksTextView = false
        layout.addTextContainer(container)

        let view = RowTextView(frame: .zero, textContainer: container)
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
        guard let container = textContainer, let layout = layoutManager else { return 0 }
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

    // The transcript decides the size; the text view must not grow itself to fit.
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: NSView.noIntrinsicMetric) }
}

/// Shows code that is wider than the column and moves it sideways under the pointer. It is a
/// plain clipping view rather than a scroll view: a scroll view inside the transcript's own
/// doesn't redraw what the transcript scrolls into view.
final class SidewaysClipView: NSView {
    private weak var content: NSView?
    private var contentWidth: CGFloat = 0
    private var offset: CGFloat = 0

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

    override func scrollWheel(with event: NSEvent) {
        let overflow = contentWidth - bounds.width
        let sideways = abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
        guard sideways, overflow > 0, let content else {
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
