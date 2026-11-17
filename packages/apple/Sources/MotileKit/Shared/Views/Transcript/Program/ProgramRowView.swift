import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A program in the transcript: the views of its tree, what keeps it from running under them,
/// and its source while it has no tree. Its texts are selected as one, as text on a page is.
final class ProgramRowView: RowView, ProgramHost {
    private static let errorGap: CGFloat = 8
    /// The room over the program and under it, as a code box has.
    private static let above: CGFloat = 4
    private static let below: CGFloat = 10
    private var content: UiContent?
    private var root: ProgramNodeView?
    private let source = ProgramCodeBox()
    private lazy var errors = ErrorBox(in: self)
    private let images = NSHashTable<ProgramImageView>.weakObjects()
    private var lookingSoon = false
    let selection = ProgramSelection()
    #if os(iOS)
    private let textInput = ProgramTextInput()
    #endif

    override init(frame: CGRect) {
        super.init(frame: frame)
        source.text.selectsWithProgram = true
        addSubview(source)
        #if os(macOS)
        focusRingType = .none
        #endif
        #if os(iOS)
        textInput.row = self
        addSubview(textInput)
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .ui(let content) = row.kind else { return }
        self.content = content
    }

    /// How tall the row is in a column `width` wide; it lays the tree out, off the main thread.
    static func height(_ content: UiContent, width: CGFloat) -> CGFloat {
        let program = content.box(width: width)?.size.height ?? sourceHeight(content, width: width)
        guard let message = errorMessage(content) else { return margins(program) }
        return margins(program + (program > 0 ? errorGap : 0) + ErrorBox.height(message, width: width))
    }

    private static func margins(_ height: CGFloat) -> CGFloat {
        height > 0 ? above + height + below : 0
    }

    private static func sourceHeight(_ content: UiContent, width: CGFloat) -> CGFloat {
        content.complete && !content.code.code.isEmpty ? ProgramCode.height(content.code, width: width) : 0
    }

    private static func errorMessage(_ content: UiContent) -> NSAttributedString? {
        guard content.complete, !content.errors.isEmpty else { return nil }
        return ErrorBox.message(content.errors.joined(separator: "\n"))
    }

    override func layout(width: CGFloat) -> CGFloat {
        guard let content else { return 0 }
        defer {
            keepSelection()
            #if os(iOS)
            bringSubviewToFront(textInput)
            #endif
        }
        var y = Self.above
        if let box = content.box(width: width) {
            let view = root.flatMap { ProgramViews.fits($0, box.node) ? $0 : nil } ?? {
                root?.removeFromSuperview()
                let view = ProgramViews.make(for: box.node)
                addSubview(view)
                return view
            }()
            root = view
            view.frame = CGRect(origin: CGPoint(x: 0, y: y), size: box.size)
            view.show(box, host: self)
            y += box.size.height
            source.isHidden = true
        } else {
            root?.removeFromSuperview()
            root = nil
            let height = Self.sourceHeight(content, width: width)
            source.isHidden = height == 0
            if height > 0 { source.show(content.code, surface: .background) }
            source.frame = CGRect(x: 0, y: y, width: width, height: height)
            y += height
        }
        guard let message = Self.errorMessage(content) else {
            errors.isHidden = true
            return y > Self.above ? y + Self.below : 0
        }
        if y > Self.above { y += Self.errorGap }
        errors.text.content = message
        errors.isHidden = false
        return y + errors.layout(y: y, width: width) + Self.below
    }

    // MARK: ProgramHost

    func send(_ node: String, _ event: JSON) {
        owner?.programEvent(rowID: rowID, node: node, event: event)
    }

    func media(_ id: String, done: @escaping (URL?) -> Void) {
        guard let owner else { return done(nil) }
        owner.media(id: id, done: done)
    }

    var streaming: Bool { content?.complete == false }

    func watchViewport(_ image: ProgramImageView) {
        images.add(image)
        guard !lookingSoon else { return }
        lookingSoon = true
        DispatchQueue.main.async { [weak self] in
            self?.lookingSoon = false
            self?.viewportMoved()
        }
    }

    override func viewportMoved() {
        for image in images.allObjects { image.viewportMoved() }
    }

    func resized(_ node: String) {
        content?.cache.forget(node)
        owner?.rowResized(id: rowID)
    }

    override func recycled() {
        root?.leave()
    }

    // MARK: Selection

    /// Takes the program's texts as they are now, in the order they read.
    func collectTexts() {
        var views = root?.selectables ?? []
        if !source.isHidden { views.append(source.text) }
        selection.collect(views, in: self)
    }

    /// Shows what stays selected of the texts once they changed.
    private func keepSelection() {
        guard selection.range != nil else { return }
        collectTexts()
        selection.show()
    }

    var selectedText: String? { selection.copied() }

    override func clearSelection() {
        guard selection.range != nil else { return }
        #if os(iOS)
        textInput.clear()
        #else
        selection.select(nil)
        #endif
    }

    #if os(macOS)
    override var acceptsFirstResponder: Bool { true }
    override var canBecomeKeyView: Bool { false }

    /// A drag selects from where it starts to where it is, a double click a word and a triple
    /// click a paragraph; a click on a link opens it.
    override func mouseDown(with event: NSEvent) {
        owner?.rowWillSelect(self)
        window?.makeFirstResponder(self)
        collectTexts()
        let start = convert(event.locationInWindow, from: nil)
        guard let at = selection.offset(at: start) else { return selection.select(nil) }
        let extends = event.modifierFlags.contains(.shift)
        let anchor = extends ? selection.range ?? NSRange(location: at, length: 0) : granule(at: at, clicks: event.clickCount)
        selection.select(extends ? Self.spanning(anchor, at) : anchor)
        var moved = false
        while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]), next.type == .leftMouseDragged {
            let point = convert(next.locationInWindow, from: nil)
            moved = moved || hypot(point.x - start.x, point.y - start.y) > 2
            guard moved, let focus = selection.offset(at: point) else { continue }
            selection.select(Self.spanning(anchor, focus))
        }
        guard !moved, !extends, event.clickCount == 1, let url = selection.link(at: start) else { return }
        Platform.open(url)
    }

    private func granule(at offset: Int, clicks: Int) -> NSRange {
        switch clicks {
        case 2: selection.word(at: offset)
        case 3...: selection.paragraph(at: offset)
        default: NSRange(location: offset, length: 0)
        }
    }

    private static func spanning(_ anchor: NSRange, _ focus: Int) -> NSRange {
        let start = min(anchor.location, focus)
        return NSRange(location: start, length: max(NSMaxRange(anchor), focus) - start)
    }

    @objc func copy(_ sender: Any?) {
        guard let text = selection.copied() else { return }
        Platform.copy(text)
    }

    override func selectAll(_ sender: Any?) {
        owner?.rowWillSelect(self)
        collectTexts()
        selection.selectAll()
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        guard selection.hasSelection else { return super.menu(for: event) }
        return NSMenu([MenuAction(title: "Copy", symbol: .copy) { [weak self] in self?.copy(nil) }])
    }
    #else
    override func layoutNow() {
        super.layoutNow()
        textInput.frame = bounds
    }

    /// The text interaction only takes touches on its own view, so the row hands it those on its
    /// texts. A control, a link and what scrolls sideways keep theirs, and accessibility still
    /// finds each text.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        guard event?.type == .touches, let hit, !hit.isDescendant(of: textInput) else { return hit }
        if onSelectionEnd(point) { return textInput }
        guard !Self.isControl(hit, below: self) else { return hit }
        if let text = hit as? RowTextView, text.textGeometry?.link(at: convert(point, to: text)) != nil { return hit }
        collectTexts()
        return selection.piece(under: point) != nil ? textInput : hit
    }

    /// Whether the point is on a handle at either end of the selection, which reaches past the text.
    private func onSelectionEnd(_ point: CGPoint) -> Bool {
        guard selection.hasSelection, let range = selection.range else { return false }
        return [range.location, NSMaxRange(range)].contains { selection.caret(at: $0).insetBy(dx: -22, dy: -22).contains(point) }
    }

    private static func isControl(_ view: UIView?, below row: UIView) -> Bool {
        var current = view
        while let view = current, view !== row {
            if view is UIControl || (view as? UITextView)?.isEditable == true { return true }
            if let scroll = view as? UIScrollView, scroll.contentSize.width > scroll.bounds.width { return true }
            if let flipped = view as? FlippedView, flipped.onPress != nil || flipped.onDrag != nil { return true }
            current = view.superview
        }
        return false
    }
    #endif
}

#if os(macOS)
extension ProgramRowView: NSMenuItemValidation {
    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        item.action == #selector(copy(_:)) ? selection.hasSelection : true
    }
}
#endif

#if DEBUG
/// The frame of every node of a program laid out in a column of a width, relative to the row, as
/// the views are placed: `{"<node id>": [x, y, width, height]}`. For the test that compares the
/// clients' layouts.
enum ProgramFrames {
    static func frames(tree: JSON, width: CGFloat, window: CGSize = CGSize(width: 1200, height: 800)) -> [String: [Double]] {
        guard let node = UiNode(json: tree) else { return [:] }
        let box = ProgramLayout.layout(node, width: width, env: ProgramEnv(column: width, window: window))
        var frames: [String: [Double]] = [:]
        collect(box, at: .zero, into: &frames)
        return frames
    }

    /// The frames as JSON, with its keys in order.
    static func json(tree: JSON, width: CGFloat) -> String {
        let frames = frames(tree: tree, width: width)
        guard let data = try? JSONSerialization.data(withJSONObject: frames, options: [.sortedKeys, .withoutEscapingSlashes]) else { return "{}" }
        return String(data: data, encoding: .utf8) ?? "{}"
    }

    private static func collect(_ box: ProgramBox, at origin: CGPoint, into frames: inout [String: [Double]]) {
        frames[box.id] = [origin.x, origin.y, box.size.width, box.size.height].map { (Double($0) * 100).rounded() / 100 }
        for (child, place) in zip(box.children, box.origins) {
            collect(child, at: CGPoint(x: origin.x + place.x, y: origin.y + place.y), into: &frames)
        }
    }
}
#endif
