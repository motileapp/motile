import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A text of a program. On the Mac the program's one selection runs through them all; on iOS
/// each is selected on its own, as a reply's text is.
protocol ProgramSelectable: PlatformView {
    #if os(macOS)
    var selectableText: NSAttributedString { get }
    /// Where the text is laid out in the view.
    var textGeometry: TextGeometry? { get }
    /// Shows the part of the text selected, or none.
    func showSelected(_ range: NSRange?)
    #else
    func clearSelection()
    #endif
}

/// Where the characters of a laid out text are, in the view that draws it.
struct TextGeometry {
    let layout: NSLayoutManager
    let container: NSTextContainer
    let origin: CGPoint
    /// How far down a line's box is from where the text system lays it, for text drawn raised
    /// into the middle of a line taller than its font.
    var lineShift: CGFloat = 0

    /// The place between characters nearest the point.
    func index(nearest point: CGPoint) -> Int {
        let length = layout.textStorage?.length ?? 0
        guard length > 0 else { return 0 }
        let at = CGPoint(x: point.x - origin.x, y: point.y - origin.y)
        layout.ensureLayout(for: container)
        let used = layout.usedRect(for: container)
        guard at.y >= used.minY else { return 0 }
        guard at.y < used.maxY else { return length }
        var fraction: CGFloat = 0
        let glyph = layout.glyphIndex(for: at, in: container, fractionOfDistanceThroughGlyph: &fraction)
        let index = layout.characterIndexForGlyph(at: glyph)
        guard index < length else { return length }
        let text = layout.textStorage?.string as NSString?
        let character = text?.rangeOfComposedCharacterSequence(at: index) ?? NSRange(location: index, length: 1)
        let isBreak = text.map { CharacterSet.newlines.contains(Unicode.Scalar($0.character(at: index)) ?? " ") } ?? false
        guard fraction > 0.5, !isBreak else { return character.location }
        return min(length, NSMaxRange(character))
    }

    /// The character drawn under the point, if any.
    func character(under point: CGPoint) -> Int? {
        let at = CGPoint(x: point.x - origin.x, y: point.y - origin.y)
        let glyph = layout.glyphIndex(for: at, in: container)
        guard glyph < layout.numberOfGlyphs else { return nil }
        let drawn = layout.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: container)
        guard drawn.insetBy(dx: -2, dy: -2).contains(at) else { return nil }
        let index = layout.characterIndexForGlyph(at: glyph)
        return index < (layout.textStorage?.length ?? 0) ? index : nil
    }

    func link(at point: CGPoint) -> URL? {
        guard let index = character(under: point), let storage = layout.textStorage else { return nil }
        let value = storage.attribute(.link, at: index, effectiveRange: nil)
        return value as? URL ?? (value as? String).flatMap(URL.init(string:))
    }

    /// The rectangles the range covers, one a line.
    func rects(of range: NSRange) -> [CGRect] {
        guard range.length > 0 else { return [] }
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        var rects: [CGRect] = []
        layout.enumerateEnclosingRects(forGlyphRange: glyphs, withinSelectedGlyphRange: glyphs, in: container) { rect, _ in
            rects.append(rect.offsetBy(dx: origin.x, dy: origin.y + lineShift))
        }
        return rects
    }

    /// Where a caret before the character at `index` stands.
    func caret(at index: Int) -> CGRect {
        let length = layout.textStorage?.length ?? 0
        guard length > 0 else { return CGRect(x: origin.x, y: origin.y, width: 2, height: 16) }
        let character = min(index, length - 1)
        let glyph = layout.glyphIndexForCharacter(at: character)
        let line = layout.lineFragmentUsedRect(forGlyphAt: glyph, effectiveRange: nil)
        let drawn = layout.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: container)
        let x = index >= length ? drawn.maxX : drawn.minX
        return CGRect(x: origin.x + x, y: origin.y + lineShift + line.minY, width: 2, height: line.height)
    }
}

#if os(macOS)
/// One selection across every text of a program, in the order they read, as text on a page is
/// selected. The texts are joined into one, a tab between those side by side and a new line
/// between the others, and a place in the selection is a place in that text.
final class ProgramSelection {
    struct Piece {
        let view: ProgramSelectable
        let start: Int
        let length: Int
        var end: Int { start + length }
    }

    private(set) var pieces: [Piece] = []
    private(set) var text = NSMutableString()
    private weak var host: PlatformView?
    /// What is selected, in the joined text.
    private(set) var range: NSRange?

    var hasSelection: Bool { (range?.length ?? 0) > 0 }

    /// Takes the texts as they are now, in the order they read.
    func collect(_ views: [ProgramSelectable], in host: PlatformView) {
        self.host = host
        pieces = []
        text = NSMutableString()
        var last: CGRect?
        for view in views where view.shown(in: host) {
            let string = view.selectableText.string
            let frame = view.convert(view.bounds, to: host)
            if let last {
                text.append(last.minY < frame.maxY && frame.minY < last.maxY ? "\t" : "\n")
            }
            pieces.append(Piece(view: view, start: text.length, length: (string as NSString).length))
            text.append(string)
            last = frame
        }
        guard let range else { return }
        let clamped = NSIntersectionRange(range, NSRange(location: 0, length: text.length))
        self.range = range.location > text.length ? nil : clamped
    }

    func select(_ range: NSRange?) {
        self.range = range.map { NSIntersectionRange($0, NSRange(location: 0, length: text.length)) }
        show()
    }

    func selectAll() {
        select(NSRange(location: 0, length: text.length))
    }

    /// Has every text show its part of the selection.
    func show() {
        for piece in pieces {
            guard let range, range.length > 0 else {
                piece.view.showSelected(nil)
                continue
            }
            let part = NSIntersectionRange(range, NSRange(location: piece.start, length: piece.length))
            let touches = part.length > 0 || piece.length == 0 && range.location <= piece.start && NSMaxRange(range) > piece.start
            piece.view.showSelected(touches ? NSRange(location: part.location - piece.start, length: part.length) : nil)
        }
    }

    /// The piece a place in the joined text is in, and the place in it.
    func piece(at offset: Int) -> (piece: Piece, index: Int)? {
        guard let piece = pieces.last(where: { $0.start <= offset }) ?? pieces.first else { return nil }
        return (piece, max(0, min(offset - piece.start, piece.length)))
    }

    /// The place in the joined text nearest a point in the host.
    func offset(at point: CGPoint) -> Int? {
        guard let host, let piece = nearest(to: point) else { return nil }
        guard let geometry = piece.view.textGeometry else { return piece.start }
        let inside = piece.view.convert(point, from: host)
        guard inside.y >= 0 else { return piece.start }
        guard inside.y < piece.view.bounds.height else { return piece.end }
        return piece.start + min(piece.length, geometry.index(nearest: inside))
    }

    /// The text under the point, or the one nearest it.
    func nearest(to point: CGPoint) -> Piece? {
        guard let host else { return nil }
        var best: (piece: Piece, distance: CGFloat)?
        for piece in pieces {
            let frame = piece.view.convert(piece.view.bounds, to: host)
            let dx = max(frame.minX - point.x, 0, point.x - frame.maxX)
            let dy = max(frame.minY - point.y, 0, point.y - frame.maxY)
            let distance = dx * dx + dy * dy
            if distance == 0 { return piece }
            if best.map({ distance < $0.distance }) ?? true { best = (piece, distance) }
        }
        return best?.piece
    }

    /// The text right under the point, if there is one.
    func piece(under point: CGPoint) -> Piece? {
        guard let host else { return nil }
        return pieces.first { $0.view.convert($0.view.bounds, to: host).contains(point) }
    }

    func link(at point: CGPoint) -> URL? {
        guard let host, let piece = piece(under: point) else { return nil }
        return piece.view.textGeometry?.link(at: piece.view.convert(point, from: host))
    }

    /// The word around a place in the joined text, as a double click takes it.
    func word(at offset: Int) -> NSRange {
        guard let (piece, index) = piece(at: offset) else { return NSRange(location: offset, length: 0) }
        let string = piece.view.selectableText.string as NSString
        guard index < string.length || index > 0 else { return NSRange(location: offset, length: 0) }
        var found = NSRange(location: index, length: 0)
        let at = min(index, string.length - 1)
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: .byWords) { _, word, _, stop in
            guard word.location <= at, at < NSMaxRange(word) else { return }
            found = word
            stop.pointee = true
        }
        if found.length == 0 { found = string.rangeOfComposedCharacterSequence(at: at) }
        return NSRange(location: piece.start + found.location, length: found.length)
    }

    /// The paragraph around a place in the joined text, as a triple click takes it.
    func paragraph(at offset: Int) -> NSRange {
        guard let (piece, index) = piece(at: offset) else { return NSRange(location: offset, length: 0) }
        let string = piece.view.selectableText.string as NSString
        let found = string.paragraphRange(for: NSRange(location: min(index, string.length), length: 0))
        return NSRange(location: piece.start + found.location, length: found.length)
    }

    /// The rectangles the range covers in the host, a line each.
    func rects(of range: NSRange) -> [CGRect] {
        guard let host else { return [] }
        return pieces.flatMap { piece -> [CGRect] in
            let part = NSIntersectionRange(range, NSRange(location: piece.start, length: piece.length))
            guard part.length > 0, let geometry = piece.view.textGeometry else { return [] }
            let rects = geometry.rects(of: NSRange(location: part.location - piece.start, length: part.length))
            return rects.map { piece.view.convert($0, to: host) }
        }
    }

    /// Where a caret at a place in the joined text stands in the host.
    func caret(at offset: Int) -> CGRect {
        guard let host, let (piece, index) = piece(at: offset), let geometry = piece.view.textGeometry else { return .zero }
        return piece.view.convert(geometry.caret(at: index), to: host)
    }

    /// The selected text as it is copied.
    func copied(_ range: NSRange? = nil) -> String? {
        guard let range = range ?? self.range, range.length > 0 else { return nil }
        var copied = ""
        for (number, piece) in pieces.enumerated() {
            let part = NSIntersectionRange(range, NSRange(location: piece.start, length: piece.length))
            if number > 0, range.location < piece.start, piece.start < NSMaxRange(range) {
                copied += text.substring(with: NSRange(location: piece.start - 1, length: 1))
            }
            guard part.length > 0 else { continue }
            let words = piece.view.selectableText.attributedSubstring(from: NSRange(location: part.location - piece.start, length: part.length))
            copied += TextSystem.withLineBreaks(Typesetter.words(of: words))
        }
        return copied
    }
}

extension PlatformView {
    /// Whether neither the view nor any view between it and `host` is hidden.
    fileprivate func shown(in host: PlatformView) -> Bool {
        var view: PlatformView? = self
        while let current = view, current !== host {
            if current.isHidden { return false }
            view = current.superview
        }
        return true
    }
}
#endif

extension ProgramNodeView {
    /// The texts of the node and its children, in the order they read: line by line, and along
    /// each line. A child is read whole where it lies, so columns side by side read one after the other.
    var selectables: [ProgramSelectable] {
        let own = ownSelectables.map { (frame: $0.convert($0.bounds, to: self), texts: [$0]) }
        let nested = (box?.children ?? []).compactMap { children[$0.id] }.map { (frame: $0.convert($0.bounds, to: self), texts: $0.selectables) }
        var lines: [(bottom: CGFloat, units: [(frame: CGRect, texts: [ProgramSelectable])])] = []
        for unit in (own + nested).sorted(by: { $0.frame.minY < $1.frame.minY }) {
            guard let last = lines.last, unit.frame.minY < last.bottom else {
                lines.append((unit.frame.maxY, [unit]))
                continue
            }
            lines[lines.count - 1] = (max(last.bottom, unit.frame.maxY), last.units + [unit])
        }
        return lines.flatMap { line in
            line.units.sorted { ($0.frame.minX, $0.frame.minY) < ($1.frame.minX, $1.frame.minY) }.flatMap(\.texts)
        }
    }
}
