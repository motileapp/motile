#if os(macOS)
import AppKit
import SwiftUI

/// Puts the code view in SwiftUI.
struct CodeViewRepresentable: NSViewRepresentable {
    let document: CodeDocument
    var collapsed: Set<String> = []
    var reveal: (path: String, count: Int)?
    var marks = CodeMarks()
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }
    var onViewed: (String) -> Void = { _ in }
    var onComment: (CodeSheet.Place) -> Void = { _ in }

    func makeNSView(context: Context) -> CodeView { CodeView() }

    func updateNSView(_ view: CodeView, context: Context) {
        view.onToggle = onToggle
        view.onOpenFile = onOpenFile
        view.onViewed = onViewed
        view.onComment = onComment
        view.mark(marks)
        view.show(document, collapsed: collapsed)
        if let reveal { view.reveal(reveal.path, count: reveal.count) }
    }
}

/// Shows the files of a diff one under the other, or one whole file. Only the lines on screen
/// are drawn, so a document of any length costs what is seen of it.
final class CodeView: NSView {
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }
    var onViewed: (String) -> Void = { _ in }
    var onComment: (CodeSheet.Place) -> Void = { _ in }

    private let scrollView = NSScrollView()
    private let canvas = CodeCanvas()
    private let pinned = PinnedHeading()
    private var revealed = 0
    private var wanted: String?
    private var lastX: CGFloat = 0

    override init(frame: NSRect) {
        super.init(frame: frame)
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.automaticallyAdjustsContentInsets = false
        scrollView.documentView = canvas
        scrollView.contentView.postsBoundsChangedNotifications = true
        addSubview(scrollView)
        pinned.canvas = canvas
        pinned.isHidden = true
        addSubview(pinned)
        canvas.owner = self
        NotificationCenter.default.addObserver(
            self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification, object: scrollView.contentView)
        NotificationCenter.default.addObserver(self, selector: #selector(coloured(_:)), name: .codeColoured, object: nil)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override var isFlipped: Bool { true }

    private var sheet: CodeSheet { canvas.sheet }

    override func layout() {
        super.layout()
        scrollView.frame = bounds
        canvas.fit(to: scrollView.contentSize)
        place()
    }

    func mark(_ marks: CodeMarks) {
        guard sheet.marks != marks else { return }
        sheet.marks = marks
        canvas.needsDisplay = true
        pinned.needsDisplay = true
    }

    func show(_ document: CodeDocument, collapsed: Set<String>) {
        guard sheet.document !== document || sheet.collapsed != collapsed else { return }
        let same = sheet.document?.id == document.id
        let anchor = same ? sheet.anchor(at: scrollView.contentView.bounds.minY) : nil
        canvas.set(document, collapsed: collapsed, size: scrollView.contentSize)
        let y = anchor.map { sheet.offset(of: $0) } ?? 0
        let x = same ? scrollView.contentView.bounds.minX : 0
        scroll(to: NSPoint(x: x, y: y))
        bringWantedIntoView()
    }

    /// Brings the file's heading to the top, now or once the document that has it is shown.
    func reveal(_ path: String, count: Int) {
        guard count != revealed else { return }
        revealed = count
        wanted = path
        bringWantedIntoView()
    }

    private func bringWantedIntoView() {
        guard let path = wanted, let top = sheet.top(ofFile: path) else { return }
        wanted = nil
        scroll(to: NSPoint(x: 0, y: top))
    }

    private func scroll(to point: NSPoint) {
        let limit = max(0, canvas.frame.height - scrollView.contentSize.height)
        scrollView.contentView.scroll(to: NSPoint(x: max(0, point.x), y: min(max(0, point.y), limit)))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        place()
    }

    @objc private func scrolled() {
        place()
        // What stays at the left edge is drawn again where the edge is now.
        let x = scrollView.contentView.bounds.minX
        guard x != lastX else { return }
        lastX = x
        canvas.needsDisplay = true
    }

    @objc private func coloured(_ notification: Notification) {
        guard notification.object as AnyObject? === sheet.document, let file = notification.userInfo?["file"] as? Int else { return }
        sheet.recolour(file: file)
        canvas.needsDisplay = true
    }

    /// Keeps the heading of the file whose lines are at the top in view, until the next file's
    /// heading pushes it out.
    private func place() {
        let visible = scrollView.contentView.bounds
        guard let heading = sheet.pinnedHeading(at: visible.minY) else {
            pinned.isHidden = true
            return
        }
        pinned.isHidden = false
        pinned.file = heading.file
        pinned.frame = NSRect(x: 0, y: heading.offset, width: scrollView.contentSize.width, height: CodeSheet.headingHeight)
        pinned.needsDisplay = true
    }

    fileprivate func pressedHeading(of file: Int, at x: CGFloat, width: CGFloat) {
        guard let document = sheet.document else { return }
        let path = document.files[file].path
        switch sheet.headingPress(at: x, width: width, file: file) {
        case .open: return onOpenFile(path)
        case .viewed: return onViewed(path)
        case .toggle: break
        }
        // A file that is closed from under its pinned heading leaves the view at its heading.
        if let top = sheet.top(ofFile: path), top < scrollView.contentView.bounds.minY {
            scroll(to: NSPoint(x: scrollView.contentView.bounds.minX, y: top))
        }
        onToggle(path)
    }
}

/// The heading of the file whose lines are at the top of the view, over them.
private final class PinnedHeading: NSView {
    weak var canvas: CodeCanvas?
    var file = 0

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        canvas?.sheet.drawHeading(of: file, in: bounds, lineAbove: false)
    }

    override func mouseDown(with event: NSEvent) {
        canvas?.owner?.pressedHeading(of: file, at: convert(event.locationInWindow, from: nil).x, width: bounds.width)
    }
}

/// Draws the sheet, and selects in it with the pointer.
private final class CodeCanvas: NSView, NSMenuItemValidation {
    let sheet = CodeSheet()
    weak var owner: CodeView?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // The numbers stay at the left edge while the lines move sideways, so every scroll draws anew.
    override class var isCompatibleWithResponsiveScrolling: Bool { false }

    override func viewDidChangeEffectiveAppearance() {
        sheet.forgetLines()
        needsDisplay = true
    }

    func set(_ document: CodeDocument, collapsed: Set<String>, size: NSSize) {
        sheet.set(document, collapsed: collapsed)
        fit(to: size)
        needsDisplay = true
    }

    func fit(to size: NSSize) {
        let wanted = NSSize(width: max(size.width, sheet.contentSize.width), height: max(size.height, sheet.contentSize.height))
        guard frame.size != wanted else { return }
        setFrameSize(wanted)
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        sheet.draw(visible: visibleRect, dirty: dirtyRect, in: context)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    /// The line under the pointer shows that it can be commented on, while the pointer is on
    /// the numbers.
    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        var hovered: (file: Int, line: Int)?
        if sheet.marks.commentable, sheet.inGutter(point.x - visibleRect.minX), let block = sheet.block(at: point.y),
            point.y >= block.linesTop, let place = sheet.place(at: point), sheet.commentable(place) != nil
        {
            hovered = (place.file, place.line)
        }
        guard hovered?.file != sheet.hovered?.file || hovered?.line != sheet.hovered?.line else { return }
        sheet.hovered = hovered
        needsDisplay = true
        if hovered != nil { NSCursor.pointingHand.set() } else { NSCursor.arrow.set() }
    }

    override func mouseExited(with event: NSEvent) {
        guard sheet.hovered != nil else { return }
        sheet.hovered = nil
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let point = convert(event.locationInWindow, from: nil)
        guard let block = sheet.block(at: point.y) else { return }
        if sheet.document?.headed == true, point.y < block.linesTop {
            owner?.pressedHeading(of: block.file, at: point.x - visibleRect.minX, width: visibleRect.width)
            return
        }
        if sheet.marks.commentable, sheet.inGutter(point.x - visibleRect.minX), let place = sheet.place(at: point),
            sheet.commentable(place) != nil
        {
            owner?.onComment(place)
            return
        }
        guard let start = sheet.place(at: point) else { return }
        sheet.selection = event.clickCount == 2 ? sheet.word(at: start) : nil
        needsDisplay = true
        guard event.clickCount == 1 else { return }
        while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]), next.type == .leftMouseDragged {
            autoscroll(with: next)
            guard let end = sheet.place(at: convert(next.locationInWindow, from: nil), within: start.file) else { continue }
            sheet.selection = (min(start, end), max(start, end))
            needsDisplay = true
        }
    }

    @objc func copy(_ sender: Any?) {
        guard let text = sheet.selectedText else { return }
        Platform.copy(text)
    }

    override func selectAll(_ sender: Any?) {
        sheet.selectAll()
        needsDisplay = true
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        item.action == #selector(copy(_:)) ? sheet.selectedText != nil : true
    }

    override func keyDown(with event: NSEvent) {
        guard event.keyCode == 53, sheet.selection != nil else { return super.keyDown(with: event) }
        sheet.selection = nil
        needsDisplay = true
    }
}
#endif
