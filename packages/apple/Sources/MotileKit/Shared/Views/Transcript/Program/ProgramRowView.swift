import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A program in the transcript: the views of its tree, what keeps it from running under them,
/// and its source while it has no tree. On the Mac its texts are selected as one, as text on a
/// page is; on iOS each is selected on its own, as a reply's text is.
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
    #if os(macOS)
    let selection = ProgramSelection()
    #endif

    override init(frame: CGRect) {
        super.init(frame: frame)
        source.text.inProgram = true
        addSubview(source)
        #if os(macOS)
        focusRingType = .none
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
        #if os(macOS)
        defer { keepSelection() }
        #endif
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

    func expand(_ page: ExpandedPage) {
        owner?.expand(page)
    }

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

    /// The program's texts as they are now, in the order they read.
    private var texts: [ProgramSelectable] {
        var views = root?.selectables ?? []
        if !source.isHidden { views.append(source.text) }
        return views
    }

    #if os(iOS)
    /// One of its texts starts a selection, so the others, here and in other rows, let go of theirs.
    func textWillSelect(_ text: ProgramSelectable) {
        owner?.rowWillSelect(self)
        for other in texts where other !== text { other.clearSelection() }
    }

    override func clearSelection() {
        for text in texts { text.clearSelection() }
    }
    #else
    func collectTexts() {
        selection.collect(texts, in: self)
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
        selection.select(nil)
    }

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
    #endif
}

extension PlatformView {
    /// The program row the view is in, if it is in one.
    var programRow: ProgramRowView? {
        var view = superview
        while let current = view, !(current is ProgramRowView) { view = current.superview }
        return view as? ProgramRowView
    }
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
