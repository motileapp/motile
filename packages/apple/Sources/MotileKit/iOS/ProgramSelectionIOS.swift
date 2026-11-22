#if os(iOS)
import UIKit

/// A place in a program's text.
final class ProgramTextPosition: UITextPosition {
    let offset: Int

    init(_ offset: Int) {
        self.offset = offset
    }
}

final class ProgramTextRange: UITextRange {
    let range: NSRange

    init(_ range: NSRange) {
        self.range = range
    }

    override var start: UITextPosition { ProgramTextPosition(range.location) }
    override var end: UITextPosition { ProgramTextPosition(NSMaxRange(range)) }
    override var isEmpty: Bool { range.length == 0 }
}

private final class ProgramSelectionRect: UITextSelectionRect {
    private let frame: CGRect
    private let starts: Bool
    private let ends: Bool

    init(_ frame: CGRect, starts: Bool, ends: Bool) {
        self.frame = frame
        self.starts = starts
        self.ends = ends
    }

    override var rect: CGRect { frame }
    override var writingDirection: NSWritingDirection { .natural }
    override var containsStart: Bool { starts }
    override var containsEnd: Bool { ends }
    override var isVertical: Bool { false }
}

/// A program's text is selected as a reply's is, with the system's text interaction: a long press
/// takes a word, its handles stretch it and the edit menu copies it.
extension ProgramTextView: UITextInput {
    func addTextInteraction() {
        let interaction = UITextInteraction(for: .nonEditable)
        interaction.textInput = self
        addInteraction(interaction)
    }

    func clearSelection() {
        guard selected != nil else { return }
        inputDelegate?.selectionWillChange(self)
        selected = nil
        inputDelegate?.selectionDidChange(self)
        if isFirstResponder { resignFirstResponder() }
    }

    private var length: Int { selectableText.length }

    private func offset(_ position: UITextPosition) -> Int {
        (position as? ProgramTextPosition)?.offset ?? 0
    }

    private func nsRange(_ range: UITextRange) -> NSRange {
        (range as? ProgramTextRange)?.range ?? NSRange(location: 0, length: 0)
    }

    private func clamped(_ offset: Int) -> Int {
        max(0, min(length, offset))
    }

    var selectedTextRange: UITextRange? {
        get { selected.map(ProgramTextRange.init) }
        set {
            selected = newValue.map(nsRange)
            guard let selected, selected.length > 0 else { return }
            programRow?.textWillSelect(self)
        }
    }

    var markedTextRange: UITextRange? { nil }

    var markedTextStyle: [NSAttributedString.Key: Any]? {
        get { nil }
        set {}
    }

    func setMarkedText(_ markedText: String?, selectedRange: NSRange) {}

    func unmarkText() {}

    var beginningOfDocument: UITextPosition { ProgramTextPosition(0) }

    var endOfDocument: UITextPosition { ProgramTextPosition(length) }

    var textInputView: UIView { self }

    func text(in range: UITextRange) -> String? {
        let range = NSIntersectionRange(nsRange(range), NSRange(location: 0, length: length))
        return (selectableText.string as NSString).substring(with: range)
    }

    func replace(_ range: UITextRange, withText text: String) {}

    func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        let from = offset(fromPosition)
        let to = offset(toPosition)
        return ProgramTextRange(NSRange(location: min(from, to), length: abs(to - from)))
    }

    func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        let moved = self.offset(position) + offset
        guard moved >= 0, moved <= length else { return nil }
        return ProgramTextPosition(moved)
    }

    func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int) -> UITextPosition? {
        switch direction {
        case .left: self.position(from: position, offset: -offset)
        case .right: self.position(from: position, offset: offset)
        default: nil
        }
    }

    func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let a = offset(position)
        let b = offset(other)
        return a < b ? .orderedAscending : a > b ? .orderedDescending : .orderedSame
    }

    func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        offset(toPosition) - offset(from)
    }

    func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        switch direction {
        case .left, .up: range.start
        default: range.end
        }
    }

    func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection) -> UITextRange? {
        let at = offset(position)
        switch direction {
        case .left, .up: return ProgramTextRange(NSRange(location: 0, length: at))
        default: return ProgramTextRange(NSRange(location: at, length: length - at))
        }
    }

    func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection) -> NSWritingDirection {
        .natural
    }

    func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {}

    func firstRect(for range: UITextRange) -> CGRect {
        textGeometry?.rects(of: nsRange(range)).first ?? .zero
    }

    func caretRect(for position: UITextPosition) -> CGRect {
        textGeometry?.caret(at: clamped(offset(position))) ?? .zero
    }

    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        let rects = textGeometry?.rects(of: nsRange(range)) ?? []
        return rects.enumerated().map { index, rect in
            ProgramSelectionRect(rect, starts: index == 0, ends: index == rects.count - 1)
        }
    }

    func closestPosition(to point: CGPoint) -> UITextPosition? {
        textGeometry.map { ProgramTextPosition($0.index(nearest: point)) }
    }

    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        let within = nsRange(range)
        let at = textGeometry?.index(nearest: point) ?? within.location
        return ProgramTextPosition(max(within.location, min(NSMaxRange(within), at)))
    }

    func characterRange(at point: CGPoint) -> UITextRange? {
        guard let at = textGeometry?.index(nearest: point) else { return nil }
        let character = at < length ? (selectableText.string as NSString).rangeOfComposedCharacterSequence(at: at) : NSRange(location: at, length: 0)
        return ProgramTextRange(character)
    }

    var hasText: Bool { length > 0 }

    func insertText(_ text: String) {}

    func deleteBackward() {}
}
#endif
