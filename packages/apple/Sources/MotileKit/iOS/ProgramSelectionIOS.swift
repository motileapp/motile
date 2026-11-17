#if os(iOS)
import UIKit

/// A place in a program's joined text.
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

/// A program row's texts as one read-only text, which the system's text interaction selects in
/// with a long press, handles and the edit menu. It lies over the row's views, as large as the
/// row, to draw the selection over them; it only takes the touches the row hands it and those on
/// its handles, and hides from accessibility, which reads each text on its own.
final class ProgramTextInput: UIView, UITextInput {
    weak var row: ProgramRowView?
    weak var inputDelegate: UITextInputDelegate?
    lazy var tokenizer: UITextInputTokenizer = UITextInputStringTokenizer(textInput: self)
    private static let empty = ProgramSelection()

    override init(frame: CGRect) {
        super.init(frame: frame)
        accessibilityElementsHidden = true
        let interaction = UITextInteraction(for: .nonEditable)
        interaction.textInput = self
        addInteraction(interaction)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var selection: ProgramSelection { row?.selection ?? Self.empty }

    private func collectTexts() {
        row?.collectTexts()
    }

    func clear() {
        inputDelegate?.selectionWillChange(self)
        selection.select(nil)
        inputDelegate?.selectionDidChange(self)
        if isFirstResponder { resignFirstResponder() }
    }

    override var canBecomeFirstResponder: Bool { true }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === self ? nil : hit
    }

    override func copy(_ sender: Any?) {
        guard let text = selection.copied() else { return }
        Platform.copy(text)
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        guard action == #selector(copy(_:)) else { return false }
        return selection.hasSelection
    }

    /// The text interaction starts a selection only on text.
    override func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        collectTexts()
        return selection.hasSelection || selection.piece(under: inRow(recognizer.location(in: self))) != nil
    }

    private func inRow(_ point: CGPoint) -> CGPoint {
        row.map { convert(point, to: $0) } ?? point
    }

    private func fromRow(_ rect: CGRect) -> CGRect {
        row.map { convert(rect, from: $0) } ?? rect
    }

    private var length: Int { selection.text.length }

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
        get { selection.range.map(ProgramTextRange.init) }
        set {
            selection.select(newValue.map(nsRange))
            guard selection.hasSelection, let row else { return }
            row.owner?.rowWillSelect(row)
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

    var endOfDocument: UITextPosition {
        collectTexts()
        return ProgramTextPosition(length)
    }

    var textInputView: UIView { self }

    func text(in range: UITextRange) -> String? {
        let range = NSIntersectionRange(nsRange(range), NSRange(location: 0, length: length))
        return selection.text.substring(with: range)
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
        selection.rects(of: nsRange(range)).first.map(fromRow) ?? .zero
    }

    func caretRect(for position: UITextPosition) -> CGRect {
        fromRow(selection.caret(at: clamped(offset(position))))
    }

    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        let rects = selection.rects(of: nsRange(range)).map(fromRow)
        return rects.enumerated().map { index, rect in
            ProgramSelectionRect(rect, starts: index == 0, ends: index == rects.count - 1)
        }
    }

    func closestPosition(to point: CGPoint) -> UITextPosition? {
        collectTexts()
        return selection.offset(at: inRow(point)).map(ProgramTextPosition.init)
    }

    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        let within = nsRange(range)
        let at = selection.offset(at: inRow(point)) ?? within.location
        return ProgramTextPosition(max(within.location, min(NSMaxRange(within), at)))
    }

    func characterRange(at point: CGPoint) -> UITextRange? {
        guard let at = selection.offset(at: inRow(point)) else { return nil }
        let character = at < length ? selection.text.rangeOfComposedCharacterSequence(at: at) : NSRange(location: at, length: 0)
        return ProgramTextRange(character)
    }

    var hasText: Bool {
        collectTexts()
        return length > 0
    }

    func insertText(_ text: String) {}

    func deleteBackward() {}
}
#endif
