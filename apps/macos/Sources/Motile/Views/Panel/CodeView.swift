import AppKit
import SwiftUI

/// The lines of one file: of its diff, or all of them.
final class CodeFile {
    /// A file with more lines than this starts closed in a diff.
    static let openUpToLines = 1500

    enum Kind: UInt8 {
        case unchanged, added, removed
        /// The heading of a hunk, or a note in place of the lines.
        case note
    }

    let path: String
    /// Where a renamed file was.
    let from: String?
    let change: String
    let added: Int
    let removed: Int
    let lines: [String]
    let kinds: [UInt8]
    /// Each line's number in the file as it was and as it is, 0 where it isn't in that one.
    let old: [Int32]
    let new: [Int32]
    /// The length of the longest line, in columns.
    let columns: Int
    /// The highlighting of each line once it has arrived, as the core's span triples.
    var spans: [[Int32]] = []

    init(diff json: JSON) {
        path = json.string("path")
        from = json.optionalString("from")
        change = json.string("change")
        added = json.int("added")
        removed = json.int("removed")
        let lines = json.strings("lines")
        guard !lines.isEmpty else {
            let note = json.bool("binary") ? "Binary file" : change == "renamed" ? "Renamed without changes" : "Empty file"
            self.lines = [note]
            kinds = [Kind.note.rawValue]
            old = [0]
            new = [0]
            columns = note.count
            return
        }
        self.lines = lines
        kinds = (json["kinds"] as? [NSNumber] ?? []).map(\.uint8Value)
        old = (json["old"] as? [NSNumber] ?? []).map(\.int32Value)
        new = (json["new"] as? [NSNumber] ?? []).map(\.int32Value)
        columns = Self.widest(lines)
    }

    /// A whole file, every line as it is.
    init(path: String, lines: [String]) {
        self.path = path
        from = nil
        change = ""
        added = 0
        removed = 0
        self.lines = lines
        kinds = [UInt8](repeating: Kind.unchanged.rawValue, count: lines.count)
        old = [Int32](repeating: 0, count: lines.count)
        new = (0..<lines.count).map { Int32($0 + 1) }
        columns = Self.widest(lines)
    }

    func kind(_ line: Int) -> Kind {
        line < kinds.count ? Kind(rawValue: kinds[line]) ?? .unchanged : .unchanged
    }

    /// Counts what is wider than a letter as two columns, and a tab as four.
    private static func widest(_ lines: [String]) -> Int {
        var widest = 0
        for line in lines {
            var columns = 0
            for unit in line.utf16 {
                switch unit {
                case 9: columns += 4
                case 0..<0x2e80: columns += 1
                default: columns += 2
                }
            }
            widest = max(widest, columns)
        }
        return widest
    }
}

final class CodeDocument {
    /// Names what it shows. A document with the same name takes the place of the one before it
    /// without the view moving.
    let id: String
    let files: [CodeFile]
    let truncated: Bool
    /// Every file is under a heading with its name, as in a diff.
    let headed: Bool

    init(id: String, files: [CodeFile], truncated: Bool, headed: Bool) {
        self.id = id
        self.files = files
        self.truncated = truncated
        self.headed = headed
    }

    convenience init(diff json: JSON, id: String) {
        self.init(id: id, files: json.objects("files").map { CodeFile(diff: $0) }, truncated: json.bool("truncated"), headed: true)
    }

    var added: Int { files.reduce(0) { $0 + $1.added } }
    var removed: Int { files.reduce(0) { $0 + $1.removed } }
}

/// Puts the code view in SwiftUI.
struct CodeViewRepresentable: NSViewRepresentable {
    let document: CodeDocument
    var collapsed: Set<String> = []
    var reveal: (path: String, count: Int)?
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }

    func makeNSView(context: Context) -> CodeView { CodeView() }

    func updateNSView(_ view: CodeView, context: Context) {
        view.onToggle = onToggle
        view.onOpenFile = onOpenFile
        view.show(document, collapsed: collapsed)
        if let reveal { view.reveal(reveal.path, count: reveal.count) }
    }
}

/// Symbols drawn in a colour that follows the appearance.
enum TintedSymbol {
    private static var cache: [String: NSImage] = [:]

    static func image(_ name: String, size: CGFloat, weight: NSFont.Weight = .regular, color: NSColor) -> NSImage? {
        let key = "\(name)/\(size)/\(weight.rawValue)/\(ObjectIdentifier(color).hashValue)"
        if let cached = cache[key] { return cached }
        let configuration = NSImage.SymbolConfiguration(pointSize: size, weight: weight)
        guard let symbol = NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(configuration) else {
            return nil
        }
        let tinted = NSImage(size: symbol.size, flipped: false) { rect in
            symbol.draw(in: rect)
            color.set()
            rect.fill(using: .sourceAtop)
            return true
        }
        cache[key] = tinted
        return tinted
    }

    /// Draws the symbol in the middle of `rect`.
    static func draw(_ name: String, size: CGFloat, weight: NSFont.Weight = .regular, color: NSColor, in rect: NSRect) {
        guard let image = image(name, size: size, weight: weight, color: color) else { return }
        let origin = NSPoint(x: (rect.midX - image.size.width / 2).rounded(), y: (rect.midY - image.size.height / 2).rounded())
        image.draw(in: NSRect(origin: origin, size: image.size), from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }
}

/// The lines added and removed, as they are written everywhere: `+12 −3`.
enum LineCountText {
    static let font = NSFont.monospacedDigitSystemFont(ofSize: 11.5, weight: .medium)

    static func text(added: Int, removed: Int) -> NSAttributedString {
        let text = NSMutableAttributedString()
        if added > 0 || removed == 0 {
            text.append(NSAttributedString(string: "+\(added)", attributes: [.font: font, .foregroundColor: Theme.success]))
        }
        if removed > 0 || added == 0 {
            let space = text.length > 0 ? " " : ""
            text.append(NSAttributedString(string: "\(space)−\(removed)", attributes: [.font: font, .foregroundColor: Theme.danger]))
        }
        return text
    }
}

/// Shows the files of a diff one under the other, or one whole file. Only the lines on screen
/// are drawn, so a document of any length costs what is seen of it.
final class CodeView: NSView {
    var onToggle: (String) -> Void = { _ in }
    var onOpenFile: (String) -> Void = { _ in }

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

    override func layout() {
        super.layout()
        scrollView.frame = bounds
        canvas.fit(to: scrollView.contentSize)
        place()
    }

    func show(_ document: CodeDocument, collapsed: Set<String>) {
        guard canvas.document !== document || canvas.collapsed != collapsed else { return }
        let same = canvas.document?.id == document.id
        let anchor = same ? canvas.anchor(at: scrollView.contentView.bounds.minY) : nil
        canvas.set(document, collapsed: collapsed, size: scrollView.contentSize)
        let y = anchor.map { canvas.offset(of: $0) } ?? 0
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
        guard let path = wanted, let top = canvas.top(ofFile: path) else { return }
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
        guard notification.object as AnyObject? === canvas.document, let file = notification.userInfo?["file"] as? Int else { return }
        canvas.recolour(file: file)
    }

    /// Keeps the heading of the file whose lines are at the top in view, until the next file's
    /// heading pushes it out.
    private func place() {
        let visible = scrollView.contentView.bounds
        guard let heading = canvas.pinnedHeading(at: visible.minY) else {
            pinned.isHidden = true
            return
        }
        pinned.isHidden = false
        pinned.file = heading.file
        pinned.frame = NSRect(x: 0, y: heading.offset, width: scrollView.contentSize.width, height: CodeCanvas.headingHeight)
        pinned.needsDisplay = true
    }

    fileprivate func toggle(_ file: Int) {
        guard let document = canvas.document else { return }
        // A file that is closed from under its pinned heading leaves the view at its heading.
        if let top = canvas.top(ofFile: document.files[file].path), top < scrollView.contentView.bounds.minY {
            scroll(to: NSPoint(x: scrollView.contentView.bounds.minX, y: top))
        }
        onToggle(document.files[file].path)
    }

    fileprivate func open(_ file: Int) {
        guard let document = canvas.document else { return }
        onOpenFile(document.files[file].path)
    }
}

/// The heading of the file whose lines are at the top of the view, over them.
private final class PinnedHeading: NSView {
    weak var canvas: CodeCanvas?
    var file = 0

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        canvas?.drawHeading(of: file, in: bounds)
    }

    override func mouseDown(with event: NSEvent) {
        canvas?.clickedHeading(of: file, at: convert(event.locationInWindow, from: nil).x, width: bounds.width)
    }
}

/// Draws the document: every file's heading and under it its lines, each with its numbers.
final class CodeCanvas: NSView, NSMenuItemValidation {
    static let headingHeight: CGFloat = 34
    private static let lineHeight = Theme.codeLineHeight
    private static let fileGap: CGFloat = 12
    private static let textInset: CGFloat = 12
    private static let numberFont = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
    private static let advance = Theme.codeFont.maximumAdvancement.width
    private static let addedFill = Theme.dynamic(Theme.hex(0x1a7f37, alpha: 0.11), Theme.hex(0x3fb950, alpha: 0.15))
    private static let removedFill = Theme.dynamic(Theme.hex(0xcf222e, alpha: 0.09), Theme.hex(0xf85149, alpha: 0.15))
    private static let selectionFill = Theme.dynamic(Theme.hex(0x2a5bd7, alpha: 0.22), Theme.hex(0x4f7cff, alpha: 0.35))
    private static let lineStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byClipping
        style.defaultTabInterval = 4 * advance
        style.tabStops = []
        return style
    }()

    /// Where a file is in the canvas.
    private struct Block {
        let file: Int
        let top: CGFloat
        let rows: Int

        var linesTop: CGFloat { top + headingHeight }
        var bottom: CGFloat { linesTop + CGFloat(rows) * lineHeight }
        let headingHeight: CGFloat
    }

    /// A place in the text: a line of a file and a position in it, in UTF-16 units.
    private struct Place: Comparable {
        var file: Int
        var line: Int
        var offset: Int

        static func < (a: Place, b: Place) -> Bool { (a.line, a.offset) < (b.line, b.offset) }
    }

    /// A row that stays where it is on screen while the document is replaced.
    struct Anchor {
        let path: String
        let line: Int
        let delta: CGFloat
    }

    fileprivate weak var owner: CodeView?
    private(set) var document: CodeDocument?
    private(set) var collapsed: Set<String> = []
    private var blocks: [Block] = []
    private var gutter: CGFloat = 0
    private var numberWidth: CGFloat = 0
    private var contentSize = NSSize.zero
    private var typeset: [Int: CTLine] = [:]
    private var selection: (from: Place, to: Place)?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // The numbers stay at the left edge while the lines move sideways, so every scroll draws anew.
    override class var isCompatibleWithResponsiveScrolling: Bool { false }

    override func viewDidChangeEffectiveAppearance() {
        typeset.removeAll()
        needsDisplay = true
    }

    // MARK: Layout

    func set(_ document: CodeDocument, collapsed: Set<String>, size: NSSize) {
        if self.document !== document {
            typeset.removeAll()
            selection = nil
        }
        self.document = document
        self.collapsed = collapsed
        var blocks: [Block] = []
        var y: CGFloat = 0
        var lastNumber: Int32 = 1
        var columns = 0
        for (index, file) in document.files.enumerated() {
            let closed = document.headed && collapsed.contains(file.path)
            let heading = document.headed ? Self.headingHeight : 0
            let block = Block(file: index, top: y, rows: closed ? 0 : file.lines.count, headingHeight: heading)
            blocks.append(block)
            y = block.bottom + (document.headed ? Self.fileGap : 0)
            lastNumber = max(lastNumber, file.old.max() ?? 0, file.new.max() ?? 0)
            columns = max(columns, file.columns)
        }
        self.blocks = blocks
        let digits = max(2, String(lastNumber).count)
        numberWidth = CGFloat(digits) * 6.8 + 12
        gutter = (document.headed ? 2 : 1) * numberWidth + 4
        contentSize = NSSize(width: gutter + Self.textInset + CGFloat(columns) * Self.advance + 24, height: y + 8)
        fit(to: size)
        needsDisplay = true
    }

    func fit(to size: NSSize) {
        let wanted = NSSize(width: max(size.width, contentSize.width), height: max(size.height, contentSize.height))
        guard frame.size != wanted else { return }
        setFrameSize(wanted)
        needsDisplay = true
    }

    func recolour(file: Int) {
        typeset = typeset.filter { $0.key >> 32 != file }
        needsDisplay = true
    }

    func top(ofFile path: String) -> CGFloat? {
        guard let document, let index = document.files.firstIndex(where: { $0.path == path }), index < blocks.count else { return nil }
        return blocks[index].top
    }

    /// The row at the top of the viewport, to find again once the document is replaced.
    func anchor(at y: CGFloat) -> Anchor? {
        guard let document, let block = blocks.last(where: { $0.top <= y }) else { return nil }
        let line = max(-1, min(block.rows - 1, Int(floor((y - block.linesTop) / Self.lineHeight))))
        let rowTop = line < 0 ? block.top : block.linesTop + CGFloat(line) * Self.lineHeight
        return Anchor(path: document.files[block.file].path, line: line, delta: y - rowTop)
    }

    func offset(of anchor: Anchor) -> CGFloat {
        guard let document, let index = document.files.firstIndex(where: { $0.path == anchor.path }), index < blocks.count else { return 0 }
        let block = blocks[index]
        let line = min(anchor.line, block.rows - 1)
        let rowTop = line < 0 ? block.top : block.linesTop + CGFloat(line) * Self.lineHeight
        return rowTop + anchor.delta
    }

    /// The file whose heading has scrolled out while its lines are at the top, and where its
    /// pinned heading goes: at the top, or above it when the file ends there.
    func pinnedHeading(at y: CGFloat) -> (file: Int, offset: CGFloat)? {
        guard document?.headed == true, let block = blocks.last(where: { $0.top < y }), block.rows > 0, y < block.bottom else {
            return nil
        }
        return (block.file, min(0, block.bottom - Self.headingHeight - y))
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let document, let context = NSGraphicsContext.current?.cgContext else { return }
        let visible = visibleRect
        for block in blocks where block.bottom + Self.fileGap >= dirtyRect.minY && block.top <= dirtyRect.maxY {
            let file = document.files[block.file]
            if document.headed {
                drawHeading(of: block.file, in: NSRect(x: visible.minX, y: block.top, width: visible.width, height: Self.headingHeight))
            }
            guard block.rows > 0 else { continue }
            let first = max(0, Int(floor((dirtyRect.minY - block.linesTop) / Self.lineHeight)))
            let last = min(block.rows - 1, Int(floor((dirtyRect.maxY - block.linesTop) / Self.lineHeight)))
            guard first <= last else { continue }

            for line in first...last {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                let row = NSRect(x: visible.minX, y: y, width: visible.width, height: Self.lineHeight)
                switch file.kind(line) {
                case .added: Self.addedFill.setFill()
                case .removed: Self.removedFill.setFill()
                case .note: Theme.hover.setFill()
                case .unchanged: continue
                }
                row.fill()
            }

            context.saveGState()
            context.clip(to: NSRect(x: visible.minX + gutter, y: dirtyRect.minY, width: visible.width - gutter, height: dirtyRect.height))
            for line in first...last where file.kind(line) != .note {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                let typeset = self.line(line, of: block.file, in: file)
                drawSelection(file: block.file, line: line, typeset: typeset, y: y)
                context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
                context.textPosition = CGPoint(x: gutter + Self.textInset, y: y + 13)
                CTLineDraw(typeset, context)
            }
            context.restoreGState()

            for line in first...last {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                // A note stays where it is while the lines move sideways.
                guard file.kind(line) != .note else {
                    context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
                    context.textPosition = CGPoint(x: visible.minX + gutter + Self.textInset, y: y + 13)
                    CTLineDraw(self.line(line, of: block.file, in: file), context)
                    continue
                }
                if document.headed { drawNumber(file.old[line], right: visible.minX + numberWidth, y: y) }
                drawNumber(file.new[line], right: visible.minX + gutter - 4, y: y)
            }
        }
    }

    private func drawNumber(_ number: Int32, right: CGFloat, y: CGFloat) {
        guard number > 0 else { return }
        let text = NSAttributedString(string: String(number), attributes: [.font: Self.numberFont, .foregroundColor: Theme.tertiary])
        let size = text.size()
        text.draw(at: NSPoint(x: right - 6 - size.width, y: y + ((Self.lineHeight - size.height) / 2).rounded()))
    }

    private func line(_ line: Int, of index: Int, in file: CodeFile) -> CTLine {
        let key = index << 32 | line
        if let cached = typeset[key] { return cached }
        let note = file.kind(line) == .note
        let text = NSMutableAttributedString(
            string: file.lines[line],
            attributes: [
                .font: Theme.codeFont, .foregroundColor: note ? Theme.secondary : Theme.text, .paragraphStyle: Self.lineStyle,
            ])
        if line < file.spans.count {
            let spans = file.spans[line]
            var at = 0
            while at + 2 < spans.count {
                let range = NSRange(location: Int(spans[at]), length: Int(spans[at + 1]))
                let colour = Int(spans[at + 2])
                at += 3
                guard NSMaxRange(range) <= text.length, colour > 0, colour < Theme.syntax.count else { continue }
                text.addAttribute(.foregroundColor, value: Theme.syntax[colour], range: range)
            }
        }
        let made = CTLineCreateWithAttributedString(text)
        if typeset.count > 4000 { typeset.removeAll() }
        typeset[key] = made
        return made
    }

    /// The name of the file with what happened to it and how many lines changed, and the
    /// buttons that close its lines and open the file.
    func drawHeading(of index: Int, in rect: NSRect) {
        guard let document, index < document.files.count else { return }
        let file = document.files[index]
        Theme.codeBackground.setFill()
        rect.fill()
        Theme.border.setFill()
        NSRect(x: rect.minX, y: rect.minY, width: rect.width, height: 1).fill()
        NSRect(x: rect.minX, y: rect.maxY - 1, width: rect.width, height: 1).fill()

        let closed = collapsed.contains(file.path)
        let chevron = NSRect(x: rect.minX + 8, y: rect.minY, width: 16, height: rect.height)
        TintedSymbol.draw(closed ? "chevron.right" : "chevron.down", size: 9, weight: .semibold, color: Theme.tertiary, in: chevron)
        let icon = NSRect(x: rect.minX + 28, y: rect.minY, width: 18, height: rect.height)
        TintedSymbol.draw(FileSymbol.name(for: file.path), size: 11, color: Theme.secondary, in: icon)

        let open = NSRect(x: rect.maxX - 34, y: rect.minY, width: 28, height: rect.height)
        TintedSymbol.draw("arrow.up.forward.square", size: 12, color: Theme.secondary, in: open)
        let counts = LineCountText.text(added: file.added, removed: file.removed)
        let countsSize = counts.size()
        let countsX = open.minX - 4 - countsSize.width
        if file.added + file.removed > 0 {
            counts.draw(at: NSPoint(x: countsX, y: rect.minY + ((rect.height - countsSize.height) / 2).rounded()))
        }

        let name = Self.title(of: file)
        let nameX = rect.minX + 50
        let room = (file.added + file.removed > 0 ? countsX : open.minX) - 10 - nameX
        let height = ceil(name.size().height)
        name.draw(
            with: NSRect(x: nameX, y: rect.minY + ((rect.height - height) / 2).rounded(), width: max(0, room), height: height),
            options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    private static func title(of file: CodeFile) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingMiddle
        let font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        let folder = (file.path as NSString).deletingLastPathComponent
        let title = NSMutableAttributedString()
        if !folder.isEmpty {
            title.append(NSAttributedString(string: folder + "/", attributes: [.font: NSFont.systemFont(ofSize: 12.5), .foregroundColor: Theme.secondary]))
        }
        title.append(NSAttributedString(string: (file.path as NSString).lastPathComponent, attributes: [.font: font, .foregroundColor: Theme.text]))
        let note: String? =
            switch file.change {
            case "added": "new"
            case "deleted": "deleted"
            case "renamed": file.from.map { "was \($0)" }
            default: nil
            }
        if let note {
            title.append(NSAttributedString(string: "   \(note)", attributes: [.font: NSFont.systemFont(ofSize: 11.5), .foregroundColor: Theme.tertiary]))
        }
        title.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: title.length))
        return title
    }

    // MARK: Clicks and selection

    fileprivate func clickedHeading(of file: Int, at x: CGFloat, width: CGFloat) {
        if x > width - 38 { owner?.open(file) } else { owner?.toggle(file) }
    }

    private func block(at y: CGFloat) -> Block? {
        blocks.last { $0.top <= y }
    }

    /// The place in the text nearest to the point, in the file `within` when one is given.
    private func place(at point: NSPoint, within file: Int? = nil) -> Place? {
        guard let document else { return nil }
        guard let block = file.map({ blocks[$0] }) ?? self.block(at: point.y), block.rows > 0 else { return nil }
        let line = min(block.rows - 1, max(0, Int(floor((point.y - block.linesTop) / Self.lineHeight))))
        let typeset = self.line(line, of: block.file, in: document.files[block.file])
        let offset = CTLineGetStringIndexForPosition(typeset, CGPoint(x: point.x - gutter - Self.textInset, y: 0))
        return Place(file: block.file, line: line, offset: max(0, offset))
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let point = convert(event.locationInWindow, from: nil)
        guard let block = block(at: point.y) else { return }
        if document?.headed == true, point.y < block.linesTop {
            clickedHeading(of: block.file, at: point.x - visibleRect.minX, width: visibleRect.width)
            return
        }
        guard let start = place(at: point) else { return }
        selection = event.clickCount == 2 ? word(at: start) : nil
        needsDisplay = true
        guard event.clickCount == 1 else { return }
        while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]), next.type == .leftMouseDragged {
            autoscroll(with: next)
            guard let end = place(at: convert(next.locationInWindow, from: nil), within: start.file) else { continue }
            selection = (min(start, end), max(start, end))
            needsDisplay = true
        }
    }

    private func word(at place: Place) -> (from: Place, to: Place)? {
        guard let text = document?.files[place.file].lines[place.line] as NSString?, text.length > 0 else { return nil }
        let isWord = { (unit: unichar) in
            guard let scalar = Unicode.Scalar(unit) else { return true }
            return CharacterSet.alphanumerics.contains(scalar) || scalar == "_"
        }
        var start = min(place.offset, text.length - 1)
        guard isWord(text.character(at: start)) else { return nil }
        var end = start
        while start > 0, isWord(text.character(at: start - 1)) { start -= 1 }
        while end < text.length, isWord(text.character(at: end)) { end += 1 }
        return (Place(file: place.file, line: place.line, offset: start), Place(file: place.file, line: place.line, offset: end))
    }

    private func drawSelection(file: Int, line: Int, typeset: CTLine, y: CGFloat) {
        guard let selection, selection.from.file == file, line >= selection.from.line, line <= selection.to.line else { return }
        let x = gutter + Self.textInset
        let start = line == selection.from.line ? CTLineGetOffsetForStringIndex(typeset, selection.from.offset, nil) : 0
        // A line that is selected to its end is lit a little past it, as its line break.
        let whole = CGFloat(CTLineGetTypographicBounds(typeset, nil, nil, nil)) + Self.advance
        let end = line == selection.to.line ? CTLineGetOffsetForStringIndex(typeset, selection.to.offset, nil) : whole
        guard end > start else { return }
        Self.selectionFill.setFill()
        NSRect(x: x + start, y: y, width: end - start, height: Self.lineHeight).fill()
    }

    private var selectedText: String? {
        guard let selection, let file = document?.files[selection.from.file] else { return nil }
        var parts: [String] = []
        for line in selection.from.line...selection.to.line where file.kind(line) != .note || selection.from.line == selection.to.line {
            let text = file.lines[line] as NSString
            let start = line == selection.from.line ? min(selection.from.offset, text.length) : 0
            let end = line == selection.to.line ? min(selection.to.offset, text.length) : text.length
            parts.append(text.substring(with: NSRange(location: start, length: max(0, end - start))))
        }
        let text = parts.joined(separator: "\n")
        return text.isEmpty ? nil : text
    }

    @objc func copy(_ sender: Any?) {
        guard let text = selectedText else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    override func selectAll(_ sender: Any?) {
        guard let document, document.files.count == 1, let file = document.files.first, let last = file.lines.last else { return }
        selection = (Place(file: 0, line: 0, offset: 0), Place(file: 0, line: file.lines.count - 1, offset: (last as NSString).length))
        needsDisplay = true
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        item.action == #selector(copy(_:)) ? selectedText != nil : true
    }

    override func keyDown(with event: NSEvent) {
        guard event.keyCode == 53, selection != nil else { return super.keyDown(with: event) }
        selection = nil
        needsDisplay = true
    }
}
