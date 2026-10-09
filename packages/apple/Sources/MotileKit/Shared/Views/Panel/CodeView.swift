import CoreText
import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// An image or a video a diff shows in place of a file's lines, by the blob git keeps it as.
final class CodeMedia {
    let blob: String
    let video: Bool
    /// It couldn't be fetched or drawn.
    var failed = false
    var asked = false

    var id: String { "blob:\(blob)" }

    init(blob: String, video: Bool) {
        self.blob = blob
        self.video = video
    }
}

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
    let media: CodeMedia?
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
        let blob = json.string("blob")
        let kind = json.string("media")
        media = blob.isEmpty || kind.isEmpty ? nil : CodeMedia(blob: blob, video: kind == "video")
        let lines = json.strings("lines")
        guard media == nil else {
            (self.lines, kinds, old, new, columns) = ([], [], [], [], 0)
            return
        }
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
        media = nil
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

/// What a pull request's diff shows besides its lines: which files the user has viewed, and the
/// lines that can have comments and those that have some.
struct CodeMarks: Equatable {
    /// Each file's heading has a box to mark it viewed.
    var viewable = false
    var viewed: Set<String> = []
    /// A line's number can be clicked to comment on it.
    var commentable = false
    /// The lines with conversations or comments waiting, by file, by their index in it.
    var marked: [String: Set<Int>] = [:]
}

/// A document as the code view shows it: where every file's heading and lines are, how they
/// are drawn, and what is selected. The view around it scrolls it and takes the clicks.
final class CodeSheet {
    static let headingHeight: CGFloat = scaled(34)
    /// Lines are as small on iOS as on the Mac, so that as much of one fits.
    static let lineHeight: CGFloat = 18
    private static let fileGap: CGFloat = 12
    private static let mediaInset: CGFloat = 12
    private static let mediaMaxHeight: CGFloat = 320
    /// The height of an image or a video until its picture is there.
    private static let mediaWaitingHeight: CGFloat = 96
    /// The pictures of images and videos are decoded no larger than this on a side.
    static let mediaPixels: CGFloat = 1600
    private static let textInset: CGFloat = 8
    private static let font = PlatformFont.monospacedSystemFont(ofSize: 12.5, weight: .regular)
    private static let numberFont = PlatformFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
    private static let advance = font.letterWidth
    /// How far under a line's top its text stands.
    private static let baseline: CGFloat = 13
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

    /// Where a file is in the sheet.
    struct Block {
        let file: Int
        let top: CGFloat
        let rows: Int
        let headingHeight: CGFloat
        /// The room an image or a video takes in place of the lines.
        var mediaHeight: CGFloat = 0

        var linesTop: CGFloat { top + headingHeight }
        var bottom: CGFloat { linesTop + CGFloat(rows) * CodeSheet.lineHeight + mediaHeight }
    }

    /// A place in the text: a line of a file and a position in it, in UTF-16 units.
    struct Place: Comparable {
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

    /// What a click on a file's heading does.
    enum HeadingPress {
        case toggle, open, viewed
    }

    private(set) var document: CodeDocument?
    private(set) var collapsed: Set<String> = []
    /// How wide the view is, which images and videos fit in.
    private(set) var width: CGFloat = 0
    var marks = CodeMarks()
    /// Asked for the picture of a file's image or video once it is to be drawn.
    var wantsMedia: (CodeFile) -> Void = { _ in }
    /// The line under the pointer, which shows that a comment can be written on it.
    var hovered: (file: Int, line: Int)?
    private(set) var blocks: [Block] = []
    private(set) var gutter: CGFloat = 0
    private var numberWidth: CGFloat = 0
    /// How large all of it is.
    private(set) var contentSize = CGSize.zero
    private var typeset: [Int: CTLine] = [:]
    var selection: (from: Place, to: Place)?

    // MARK: Layout

    func set(_ document: CodeDocument, collapsed: Set<String>, width: CGFloat) {
        if self.document !== document {
            typeset.removeAll()
            selection = nil
        }
        self.document = document
        self.collapsed = collapsed
        self.width = width
        var blocks: [Block] = []
        var y: CGFloat = 0
        var lastNumber: Int32 = 1
        var columns = 0
        for (index, file) in document.files.enumerated() {
            let closed = document.headed && collapsed.contains(file.path)
            let heading = document.headed ? Self.headingHeight : 0
            var block = Block(file: index, top: y, rows: closed ? 0 : file.lines.count, headingHeight: heading)
            if let media = file.media, !closed { block.mediaHeight = Self.height(of: media, width: width) }
            blocks.append(block)
            y = block.bottom + (document.headed ? Self.fileGap : 0)
            lastNumber = max(lastNumber, file.old.max() ?? 0, file.new.max() ?? 0)
            columns = max(columns, file.columns)
        }
        self.blocks = blocks
        let digits = max(2, String(lastNumber).count)
        numberWidth = CGFloat(digits) * 6.8 + 12
        gutter = (document.headed ? 2 : 1) * numberWidth + 4
        contentSize = CGSize(width: gutter + Self.textInset + CGFloat(columns) * Self.advance + 24, height: y + 8)
    }

    /// Lays the document out again for a view `width` wide, when its images and videos take
    /// other room in it. Answers whether it did.
    func fit(width: CGFloat) -> Bool {
        guard width != self.width, let document else { return false }
        self.width = width
        guard document.files.contains(where: { $0.media != nil }) else { return false }
        set(document, collapsed: collapsed, width: width)
        return true
    }

    private static func height(of media: CodeMedia, width: CGFloat) -> CGFloat {
        guard let picture = Pictures.cached(media.id) else { return mediaWaitingHeight + 2 * mediaInset }
        let room = CGSize(width: width - 2 * mediaInset, height: mediaMaxHeight)
        return size(of: picture, in: room).height + 2 * mediaInset
    }

    /// The size a picture is drawn at: its own, or smaller to fit the room.
    private static func size(of picture: CGImage, in room: CGSize) -> CGSize {
        let points = CGSize(width: CGFloat(picture.width) / Platform.pixelsPerPoint, height: CGFloat(picture.height) / Platform.pixelsPerPoint)
        let scale = max(0, min(1, room.width / points.width, room.height / points.height))
        return CGSize(width: (points.width * scale).rounded(), height: (points.height * scale).rounded())
    }

    /// Whether the point is on the image or the video a file shows.
    func onMedia(_ point: CGPoint, of block: Block) -> Bool {
        block.mediaHeight > 0 && point.y >= block.linesTop && point.y < block.bottom
    }

    /// The colours are other ones now, so every line is typeset again.
    func forgetLines() {
        typeset.removeAll()
    }

    func recolour(file: Int) {
        typeset = typeset.filter { $0.key >> 32 != file }
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

    /// Draws what of the sheet is in `dirty`. `visible` is the part the view shows: the numbers
    /// and the headings stay at its left edge while the lines move sideways.
    func draw(visible: CGRect, dirty: CGRect, in context: CGContext) {
        guard let document else { return }
        for block in blocks where block.bottom + Self.fileGap >= dirty.minY && block.top <= dirty.maxY {
            let file = document.files[block.file]
            if document.headed {
                let heading = CGRect(x: visible.minX, y: block.top, width: visible.width, height: Self.headingHeight)
                drawHeading(of: block.file, in: heading, lineAbove: block.file > 0)
            }
            if let media = file.media, block.mediaHeight > 0 {
                let room = CGRect(x: visible.minX, y: block.linesTop, width: visible.width, height: block.mediaHeight)
                drawMedia(media, of: file, in: room.insetBy(dx: Self.mediaInset, dy: Self.mediaInset), context: context)
            }
            guard block.rows > 0 else { continue }
            let first = max(0, Int(floor((dirty.minY - block.linesTop) / Self.lineHeight)))
            let last = min(block.rows - 1, Int(floor((dirty.maxY - block.linesTop) / Self.lineHeight)))
            guard first <= last else { continue }

            for line in first...last {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                let row = CGRect(x: visible.minX, y: y, width: visible.width, height: Self.lineHeight)
                switch file.kind(line) {
                case .added: Self.addedFill.setFill()
                case .removed: Self.removedFill.setFill()
                case .note: Theme.backgroundSecondary.setFill()
                case .unchanged: continue
                }
                row.fillCurrent()
            }

            context.saveGState()
            context.clip(to: CGRect(x: visible.minX + gutter, y: dirty.minY, width: visible.width - gutter, height: dirty.height))
            for line in first...last where file.kind(line) != .note {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                let typeset = self.line(line, of: block.file, in: file)
                drawSelection(file: block.file, line: line, typeset: typeset, y: y)
                context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
                context.textPosition = CGPoint(x: gutter + Self.textInset, y: y + Self.baseline)
                CTLineDraw(typeset, context)
            }
            context.restoreGState()

            for line in first...last {
                let y = block.linesTop + CGFloat(line) * Self.lineHeight
                // A note stays where it is while the lines move sideways.
                guard file.kind(line) != .note else {
                    context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
                    context.textPosition = CGPoint(x: visible.minX + gutter + Self.textInset, y: y + Self.baseline)
                    CTLineDraw(self.line(line, of: block.file, in: file), context)
                    continue
                }
                if document.headed { drawNumber(file.old[line], right: visible.minX + numberWidth, y: y) }
                drawNumber(file.new[line], right: visible.minX + gutter - 4, y: y)
                if marks.marked[file.path]?.contains(line) == true {
                    Theme.link.setFill()
                    RoundedBox.fill(CGRect(x: visible.minX + 3, y: y + 5, width: 3, height: Self.lineHeight - 10), radius: 1.5)
                }
                if marks.commentable, let hovered, hovered.file == block.file, hovered.line == line {
                    drawCommentBadge(at: CGPoint(x: visible.minX + gutter - 2, y: y))
                }
            }
        }
    }

    /// The picture of an image or a video, as large as it is or smaller to fit, or the room it
    /// waits in. A video's picture is its first frame, under the sign that it plays.
    private func drawMedia(_ media: CodeMedia, of file: CodeFile, in room: CGRect, context: CGContext) {
        guard let picture = Pictures.cached(media.id) else {
            Theme.backgroundSecondary.setFill()
            RoundedBox.fill(room, radius: Radius.large)
            guard media.failed else {
                if !media.asked { wantsMedia(file) }
                return
            }
            let note = NSAttributedString(
                string: media.video ? "This video couldn't be shown." : "This image couldn't be shown.",
                attributes: [.font: PlatformFont.ui(12.5), .foregroundColor: Theme.secondary])
            let size = note.size()
            note.draw(at: CGPoint(x: (room.midX - size.width / 2).rounded(), y: (room.midY - size.height / 2).rounded()))
            return
        }
        let frame = CGRect(origin: room.origin, size: Self.size(of: picture, in: room.size))
        context.saveGState()
        context.addPath(RoundedBox.path(frame, radius: Radius.small))
        context.clip()
        // The sheet is drawn from the top down and the picture from the bottom up.
        context.translateBy(x: 0, y: frame.minY + frame.maxY)
        context.scaleBy(x: 1, y: -1)
        context.draw(picture, in: frame)
        context.restoreGState()
        if media.video {
            TintedSymbol.draw(.circlePlay, size: 40, color: .white, in: frame)
        }
    }

    /// The sign that a comment can be written on the line, over the end of its number.
    private func drawCommentBadge(at topRight: CGPoint) {
        let size: CGFloat = 16
        let badge = CGRect(x: topRight.x - size, y: topRight.y + (Self.lineHeight - size) / 2, width: size, height: size)
        Theme.primary.setFill()
        RoundedBox.fill(badge, radius: 4)
        TintedSymbol.draw(.plus, size: 10, color: .white, in: badge)
    }

    /// Whether a click at `x` from the view's left edge is on the numbers of the lines.
    func inGutter(_ x: CGFloat) -> Bool {
        x < gutter
    }

    /// The line of the file at the place, as GitHub counts it, and on which side, when a comment
    /// can be written on it.
    func commentable(_ place: Place) -> (path: String, line: Int, side: String, code: String)? {
        guard marks.commentable, let document, place.file < document.files.count else { return nil }
        let file = document.files[place.file]
        guard place.line < file.lines.count else { return nil }
        switch file.kind(place.line) {
        case .note: return nil
        case .removed:
            let line = Int(file.old[place.line])
            return line > 0 ? (file.path, line, "left", file.lines[place.line]) : nil
        case .added, .unchanged:
            let line = Int(file.new[place.line])
            return line > 0 ? (file.path, line, "right", file.lines[place.line]) : nil
        }
    }

    private func drawNumber(_ number: Int32, right: CGFloat, y: CGFloat) {
        guard number > 0 else { return }
        let text = NSAttributedString(string: String(number), attributes: [.font: Self.numberFont, .foregroundColor: Theme.tertiary])
        let size = text.size()
        text.draw(at: CGPoint(x: right - 6 - size.width, y: y + ((Self.lineHeight - size.height) / 2).rounded()))
    }

    private func line(_ line: Int, of index: Int, in file: CodeFile) -> CTLine {
        let key = index << 32 | line
        if let cached = typeset[key] { return cached }
        let note = file.kind(line) == .note
        let text = NSMutableAttributedString(
            string: file.lines[line],
            attributes: [
                .font: Self.font, .foregroundColor: (note ? Theme.secondary : Theme.text).drawn, .paragraphStyle: Self.lineStyle,
            ])
        if line < file.spans.count {
            let spans = file.spans[line]
            var at = 0
            while at + 2 < spans.count {
                let range = NSRange(location: Int(spans[at]), length: Int(spans[at + 1]))
                let colour = Int(spans[at + 2])
                at += 3
                guard NSMaxRange(range) <= text.length, colour > 0, colour < Theme.syntax.count else { continue }
                text.addAttribute(.foregroundColor, value: Theme.syntax[colour].drawn, range: range)
            }
        }
        let made = CTLineCreateWithAttributedString(text)
        if typeset.count > 4000 { typeset.removeAll() }
        typeset[key] = made
        return made
    }

    /// The name of the file with what happened to it and how many lines changed, and the
    /// buttons that close its lines and open the file. A heading under the panel's bar has
    /// the bar's line above it.
    func drawHeading(of index: Int, in rect: CGRect, lineAbove: Bool) {
        guard let document, index < document.files.count else { return }
        let file = document.files[index]
        Theme.backgroundSecondary.setFill()
        rect.fillCurrent()
        Theme.border.setFill()
        if lineAbove { CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: 1).fillCurrent() }
        CGRect(x: rect.minX, y: rect.maxY - 1, width: rect.width, height: 1).fillCurrent()

        let closed = collapsed.contains(file.path)
        let chevron = CGRect(x: rect.minX + 8, y: rect.minY, width: 16, height: rect.height)
        TintedSymbol.draw(closed ? .chevronRight : .chevronDown, size: 9, color: Theme.tertiary, in: chevron)
        let icon = CGRect(x: rect.minX + 28, y: rect.minY, width: 18, height: rect.height)
        TintedSymbol.draw(FileSymbol.symbol(for: file.path), size: 11, color: Theme.secondary, in: icon)

        let open = CGRect(x: rect.maxX - 34, y: rect.minY, width: 28, height: rect.height)
        TintedSymbol.draw(.squareArrowOutUpRight, size: 12, color: Theme.secondary, in: open)
        let counts = LineCountText.text(added: file.added, removed: file.removed)
        let countsSize = counts.size()
        let countsX = open.minX - 4 - countsSize.width
        if file.added + file.removed > 0 {
            counts.draw(at: CGPoint(x: countsX, y: rect.minY + ((rect.height - countsSize.height) / 2).rounded()))
        }
        let viewedBox = viewedBox(of: file, in: rect)
        if let viewedBox {
            drawViewed(marks.viewed.contains(file.path), in: viewedBox)
        }

        let name = Self.title(of: file)
        let nameX = rect.minX + 50
        let room = (viewedBox?.minX ?? (file.added + file.removed > 0 ? countsX : open.minX)) - 10 - nameX
        let height = ceil(name.size().height)
        name.drawTruncated(in: CGRect(x: nameX, y: rect.minY + ((rect.height - height) / 2).rounded(), width: max(0, room), height: height))
    }

    private static func title(of file: CodeFile) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingMiddle
        let font = PlatformFont.ui(12.5, weight: .medium)
        let folder = (file.path as NSString).deletingLastPathComponent
        let title = NSMutableAttributedString()
        if !folder.isEmpty {
            title.append(NSAttributedString(string: folder + "/", attributes: [.font: PlatformFont.ui(12.5), .foregroundColor: Theme.secondary]))
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
            title.append(NSAttributedString(string: "   \(note)", attributes: [.font: PlatformFont.ui(11.5), .foregroundColor: Theme.tertiary]))
        }
        title.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: title.length))
        return title
    }

    // MARK: Clicks and selection

    /// Where a file's box to mark it viewed is in its heading, when it has one.
    private func viewedBox(of file: CodeFile, in rect: CGRect) -> CGRect? {
        guard marks.viewable else { return nil }
        let countsWidth = file.added + file.removed > 0 ? LineCountText.text(added: file.added, removed: file.removed).size().width + 10 : 0
        let right = rect.maxX - 38 - countsWidth
        return CGRect(x: right - 70, y: rect.minY, width: 70, height: rect.height)
    }

    private func drawViewed(_ viewed: Bool, in rect: CGRect) {
        let box = CGRect(x: rect.minX + 6, y: rect.midY - 7, width: 14, height: 14)
        if viewed {
            Theme.primary.setFill()
            RoundedBox.fill(box, radius: 3.5)
            TintedSymbol.draw(.check, size: 9, color: .white, in: box)
        } else {
            Theme.borderSecondary.setStroke()
            RoundedBox.stroke(box.insetBy(dx: 0.5, dy: 0.5), radius: 3.5)
        }
        let label = NSAttributedString(string: "Viewed", attributes: [
            .font: PlatformFont.ui(11.5), .foregroundColor: viewed ? Theme.text : Theme.secondary,
        ])
        let size = label.size()
        label.draw(at: CGPoint(x: box.maxX + 6, y: rect.minY + ((rect.height - size.height) / 2).rounded()))
    }

    /// What a click in the heading of file `index`, as wide as `width`, `x` from its left, does.
    func headingPress(at x: CGFloat, width: CGFloat, file index: Int) -> HeadingPress {
        if x > width - 38 { return .open }
        if let document, index < document.files.count,
            let box = viewedBox(of: document.files[index], in: CGRect(x: 0, y: 0, width: width, height: Self.headingHeight)),
            box.minX <= x, x <= box.maxX
        {
            return .viewed
        }
        return .toggle
    }

    func block(at y: CGFloat) -> Block? {
        blocks.last { $0.top <= y }
    }

    /// The place in the text nearest to the point, in the file `within` when one is given.
    func place(at point: CGPoint, within file: Int? = nil) -> Place? {
        guard let document else { return nil }
        guard let block = file.map({ blocks[$0] }) ?? self.block(at: point.y), block.rows > 0 else { return nil }
        let line = min(block.rows - 1, max(0, Int(floor((point.y - block.linesTop) / Self.lineHeight))))
        let typeset = self.line(line, of: block.file, in: document.files[block.file])
        let offset = CTLineGetStringIndexForPosition(typeset, CGPoint(x: point.x - gutter - Self.textInset, y: 0))
        return Place(file: block.file, line: line, offset: max(0, offset))
    }

    func word(at place: Place) -> (from: Place, to: Place)? {
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

    /// The whole of the lines from one to another of a file.
    func lines(_ first: Int, through last: Int, of file: Int) -> (from: Place, to: Place)? {
        guard let lines = document?.files[file].lines, !lines.isEmpty else { return nil }
        let (from, to) = (max(0, min(first, last)), min(lines.count - 1, max(first, last)))
        return (Place(file: file, line: from, offset: 0), Place(file: file, line: to, offset: (lines[to] as NSString).length))
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
        CGRect(x: x + start, y: y, width: end - start, height: Self.lineHeight).fillCurrent()
    }

    /// Where the selection is in the sheet, for a menu to point at.
    var selectionFrame: CGRect? {
        guard let selection, selection.from.file < blocks.count else { return nil }
        let block = blocks[selection.from.file]
        let top = block.linesTop + CGFloat(selection.from.line) * Self.lineHeight
        let bottom = block.linesTop + CGFloat(selection.to.line + 1) * Self.lineHeight
        return CGRect(x: gutter, y: top, width: max(1, contentSize.width - gutter), height: bottom - top)
    }

    var selectedText: String? {
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

    /// Selects all of a document that is one file.
    func selectAll() {
        guard let document, document.files.count == 1, let file = document.files.first, let last = file.lines.last else { return }
        selection = (Place(file: 0, line: 0, offset: 0), Place(file: 0, line: file.lines.count - 1, offset: (last as NSString).length))
    }
}

extension PlatformColor {
    /// The colour as Core Text draws it. On iOS it has to be resolved for the appearance first.
    var drawn: PlatformColor {
        #if os(macOS)
        self
        #else
        resolvedColor(with: UITraitCollection.current)
        #endif
    }
}
