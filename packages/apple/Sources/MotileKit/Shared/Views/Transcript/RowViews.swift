import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What a row needs from the transcript around it.
protocol RowOwner: AnyObject {
    func rowWillSelect(_ view: RowView)
    func rowToggledExpansion(id: String)
    /// Opens or closes a group or a fold, whose rows the core adds and removes.
    func toggleRow(id: String)
    func isExpanded(id: String) -> Bool
    func copyReply(endingAt rowID: String)
    /// Hands over the file of an image or a video, or nothing when it can't be had.
    func media(id: String, done: @escaping (URL?) -> Void)
    /// Opens the images and videos in the viewer, on the one at `index`.
    func view(_ media: [ViewedMedia], at index: Int)
    /// Gives the agent a queued message now, or takes it back into the composer.
    func sendQueued(messageID: String)
    func cancelQueued(messageID: String)
    /// Opens or closes a folder among a turn's changed files, in the row `rowID`.
    func toggleFolder(id: String, rowID: String)
    /// Shows what the turn that ended with the item changed, with the file at `path` in view.
    func openDiff(turn itemID: String, path: String?)
    /// Shows what the agent did that the item's tool call started.
    func openAgent(itemID: String)
}

/// The base of every row. A row is as wide as the transcript's column; `layout(width:)` places
/// its parts for that width and says how tall it is.
class RowView: FlippedView {
    weak var owner: RowOwner?
    private(set) var rowID = ""
    private(set) var itemID = ""
    private(set) var nested = false
    /// Set when the row grew while its reply streams: the next layout fades the new part in.
    var fadesGrowth = false

    func configure(_ row: RowModel) {
        rowID = row.id
        itemID = row.itemID
        nested = row.nested
    }

    func layout(width: CGFloat) -> CGFloat { 0 }

    func clearSelection() {}

    /// What the row says about its message, like when it was sent, is shown or faded out.
    var showsMeta = false

    /// How tall the row is in a column `width` wide, as its view lays it out closed. It runs off
    /// the main thread. A queued message is only estimated: it is always among the last rows.
    class func height(_ row: RowModel, width: CGFloat) -> CGFloat {
        switch row.kind {
        case .user(let text, let attachments, _):
            let fit = BubbleFit(text: text, attachments: attachments, width: width)
            return UserRowView.height(fit, textHeight: fit.hasText ? TextMeasure.height(of: text, width: fit.innerWidth) : 0)
        case .prose(let text, let above, _):
            return TextMeasure.height(of: text, width: width) + ProseRowView.gap + above
        case .error(let text):
            return TextMeasure.height(of: text, width: width - ErrorRowView.textInset) + ErrorRowView.padding
        case .code, .tool, .thinking, .group, .fold, .media, .changes, .turnEnd, .queued:
            return estimatedHeight(row, width: width)
        }
    }

    /// What the row would be without measuring its text, until it has been measured.
    class func estimatedHeight(_ row: RowModel, width: CGFloat) -> CGFloat {
        switch row.kind {
        case .user(let text, let attachments, _):
            return estimatedTextHeight(text.length, width: width * 0.75) + 34 + UserRowView.footHeight + AttachedFilesView.height(attachments, width: width)
        case .prose(let text, let above, _):
            return estimatedTextHeight(text.length, width: width) + ProseRowView.gap + above
        case .code(let content):
            return CodeRowView.height(lines: content.lineCount)
        case .tool, .thinking, .group, .fold:
            return ToolRowView.rowHeight
        case .media(let content):
            return MediaRowView.height(content, width: width)
        case .error(let text):
            return estimatedTextHeight(text.length, width: width - ErrorRowView.textInset) + ErrorRowView.padding
        case .changes(let content):
            return ChangesRowView.height(entries: content.entries.count)
        case .turnEnd:
            return TurnEndRowView.height
        case .queued(let content):
            let attachments = AttachedFilesView.height(content.attachments, width: width)
            return estimatedTextHeight(content.text.length, width: width * 0.75) + 48 + QueuedRowView.footHeight + attachments
        }
    }

    private static func estimatedTextHeight(_ length: Int, width: CGFloat) -> CGFloat {
        let perLine = max(20, width / 7.2)
        return ceil(CGFloat(length) / perLine + 0.5) * Theme.proseLineHeight
    }

    static func make(for row: RowModel) -> RowView {
        switch row.kind {
        case .user: return UserRowView()
        case .prose: return ProseRowView()
        case .code: return CodeRowView()
        case .tool, .thinking, .group, .fold: return ToolRowView()
        case .media: return MediaRowView()
        case .error: return ErrorRowView()
        case .changes: return ChangesRowView()
        case .turnEnd: return TurnEndRowView()
        case .queued: return QueuedRowView()
        }
    }

    static func reuseKey(for row: RowModel) -> String {
        switch row.kind {
        case .user: return "user"
        case .prose: return "prose"
        case .code: return "code"
        case .tool, .thinking, .group, .fold: return "tool"
        case .media: return "media"
        case .error: return "error"
        case .changes: return "changes"
        case .turnEnd: return "turnEnd"
        case .queued: return "queued"
        }
    }
}

/// How a message fits its bubble in a column `width` wide. The bubble is as wide as its text, up
/// to the widest it may be, and the files stand above the text with their own room around them.
struct BubbleFit {
    static let padding: CGFloat = 14

    let hasText: Bool
    let files: CGSize
    let innerWidth: CGFloat
    let top: CGFloat
    let between: CGFloat

    init(text: NSAttributedString, attachments: [AttachedFile], width: CGFloat, least: CGFloat = 12) {
        let widest = max(120, width * 0.8) - Self.padding * 2
        let natural = text.bounds(width: widest)
        hasText = text.length > 0
        files = AttachedFilesView.size(attachments, width: widest)
        innerWidth = min(widest, max(hasText ? ceil(natural.width) + 2 : 0, files.width, least))
        top = files.height > 0 ? Self.padding : 10
        between = files.height > 0 && hasText ? 8 : 0
    }
}

final class UserRowView: RowView {
    private let bubble = SurfaceView()
    private let text = RowTextView.make()
    private let attachments = AttachedFilesView()
    private let meta = MessageMeta(trailing: true, tooltip: "Copy message")
    private var files: [AttachedFile] = []
    private var pending = false

    override var showsMeta: Bool {
        didSet { meta.shown = showsMeta }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        bubble.fill = Theme.bubble
        bubble.radius = 18
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        addSubview(meta)
        meta.onCopy = { [weak self] in
            guard let self else { return }
            Platform.copy(self.text.content.string)
        }
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .user(let content, let files, let at) = row.kind else { return }
        meta.show(Time.stamp(at))
        text.content = content
        self.files = files
        text.isHidden = content.length == 0
        attachments.show(files, owner: owner)
        pending = row.id == "pending"
        bubble.opacity = pending ? 0.6 : 1
    }

    private static let margin: CGFloat = 14
    /// The room under the bubble for when the message was sent and the button that copies it.
    static let footHeight = metaGap + MessageMeta.height + 4
    private static let metaGap: CGFloat = 4
    /// Keeps the time and the button clear of the bubble's round corner.
    private static let metaInset: CGFloat = 6

    static func height(_ fit: BubbleFit, textHeight: CGFloat) -> CGFloat {
        margin + bubbleHeight(fit, textHeight: textHeight) + footHeight
    }

    private static func bubbleHeight(_ fit: BubbleFit, textHeight: CGFloat) -> CGFloat {
        let bottom: CGFloat = fit.hasText ? 10 : BubbleFit.padding
        return fit.top + fit.files.height + fit.between + textHeight + bottom
    }

    override func layout(width: CGFloat) -> CGFloat {
        let padding = BubbleFit.padding
        let fit = BubbleFit(text: text.content, attachments: files, width: width)
        let textHeight = fit.hasText ? text.height(forWidth: fit.innerWidth) : 0
        let bubbleSize = CGSize(width: fit.innerWidth + padding * 2, height: Self.bubbleHeight(fit, textHeight: textHeight))
        bubble.frame = CGRect(x: width - bubbleSize.width, y: Self.margin, width: bubbleSize.width, height: bubbleSize.height)
        attachments.frame = CGRect(x: padding, y: fit.top, width: fit.innerWidth, height: fit.files.height)
        attachments.layout(width: fit.innerWidth)
        text.frame = CGRect(x: padding, y: fit.top + fit.files.height + fit.between, width: fit.innerWidth, height: textHeight)
        meta.frame = CGRect(x: 0, y: bubble.frame.maxY + Self.metaGap, width: width - Self.metaInset, height: MessageMeta.height)
        return Self.height(fit, textHeight: textHeight)
    }

    override func clearSelection() { text.clearSelection() }
}

/// A message that waits for the agent: what it says, how it waits, and the buttons that send it
/// now or take it back. It stands where the user's messages do, outlined instead of filled.
final class QueuedRowView: RowView {
    /// The strip under the message, down to the bubble's edge, that holds the status and the buttons.
    static let footHeight: CGFloat = scaled(34)
    private static let radius: CGFloat = 18
    /// The buttons' highlight is this far from the bubble's right and bottom, so that their words
    /// end where the message's do.
    private static let buttonMargin = BubbleFit.padding - RowButton.padding

    private let bubble = SurfaceView()
    private let text = RowTextView.make()
    private let attachments = AttachedFilesView()
    private let clock = SymbolView(.clock, size: 11)
    private let status = TextLabel(font: Theme.smallFont, color: Theme.secondary)
    private var sendButton: RowButton!
    private var cancelButton: RowButton!
    private var messageID = ""
    private var attached: [AttachedFile] = []
    private var hasText = false
    private var sending = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        bubble.stroke = Theme.strongBorder
        bubble.radius = Self.radius
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        bubble.addSubview(clock)
        bubble.addSubview(status)
        sendButton = RowButton(
            title: "Send now",
            tooltip: "Have the agent take it at once, in the turn that runs",
            radius: Self.radius - Self.buttonMargin,
            insets: PlatformEdgeInsets(top: 4, left: 2, bottom: Self.buttonMargin, right: 2)
        ) { [weak self] in
            guard let self else { return }
            self.owner?.sendQueued(messageID: self.messageID)
        }
        cancelButton = RowButton(
            title: "Cancel",
            tooltip: "Take it back into the composer",
            radius: Self.radius - Self.buttonMargin,
            insets: PlatformEdgeInsets(top: 4, left: 2, bottom: Self.buttonMargin, right: Self.buttonMargin)
        ) { [weak self] in
            guard let self else { return }
            self.owner?.cancelQueued(messageID: self.messageID)
        }
        bubble.addSubview(sendButton)
        bubble.addSubview(cancelButton)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .queued(let content) = row.kind else { return }
        messageID = row.itemID
        text.content = content.text
        hasText = content.text.length > 0
        text.isHidden = !hasText
        attached = content.attachments
        attachments.show(content.attachments, owner: owner)
        status.string = content.status
        sending = content.sending
        sendButton.isHidden = sending
        cancelButton.isHidden = sending
        sendButton.dim()
        cancelButton.dim()
    }

    override func layout(width: CGFloat) -> CGFloat {
        let padding = BubbleFit.padding
        // The buttons reach the bubble's edge, so they take the padding on that side too.
        let buttonsWidth = sending ? 0 : sendButton.width + cancelButton.width
        let statusWidth = 18 + ceil(status.string.size(withAttributes: [.font: Theme.smallFont]).width) + 8
        let footWidth = sending ? statusWidth : statusWidth + 10 + buttonsWidth - padding
        let fit = BubbleFit(text: text.content, attachments: attached, width: width, least: max(footWidth, 12))
        let (innerWidth, top, between, files) = (fit.innerWidth, fit.top, fit.between, fit.files)
        let textHeight = hasText ? text.height(forWidth: innerWidth) : 0
        let footY = top + files.height + between + textHeight + 2
        let bubbleSize = CGSize(width: innerWidth + padding * 2, height: footY + Self.footHeight)
        bubble.frame = CGRect(x: width - bubbleSize.width, y: 14, width: bubbleSize.width, height: bubbleSize.height)
        attachments.frame = CGRect(x: padding, y: top, width: innerWidth, height: files.height)
        attachments.layout(width: innerWidth)
        text.frame = CGRect(x: padding, y: top + files.height + between, width: innerWidth, height: textHeight)

        cancelButton.frame = CGRect(x: bubbleSize.width - cancelButton.width, y: footY, width: cancelButton.width, height: Self.footHeight)
        sendButton.frame = CGRect(x: cancelButton.frame.minX - sendButton.width, y: footY, width: sendButton.width, height: Self.footHeight)
        let statusEnd = sending ? bubbleSize.width - padding : sendButton.frame.minX - 6
        // In the middle of what the buttons light up.
        let lineHeight = scaled(16)
        let lineY = footY + ((4 + Self.footHeight - Self.buttonMargin - lineHeight) / 2).rounded()
        clock.frame = CGRect(x: padding, y: lineY, width: 14, height: lineHeight)
        status.frame = CGRect(x: padding + 18, y: lineY, width: max(0, statusEnd - padding - 18), height: lineHeight)
        return bubbleSize.height + 14 + 14
    }

    override func clearSelection() { text.clearSelection() }
}

final class ProseRowView: RowView {
    /// The room a row has around its text, which is what keeps two rows apart.
    static let gap: CGFloat = 9

    private let text = RowTextView.make()
    private var above: CGFloat = 0
    private var headers: [CodeHeader] = []
    #if os(iOS)
    private var tables: [TableView] = []
    #endif

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(text)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .prose(let content, let above, _) = row.kind else { return }
        text.content = content
        self.above = above
    }

    override func layout(width: CGFloat) -> CGFloat {
        let before = text.frame.size
        let height = text.height(forWidth: width)
        text.frame = CGRect(x: 0, y: 3 + above, width: width, height: height)
        placeHeaders()
        #if os(iOS)
        tables = TableView.place(text.tableBoxes(), in: text, reusing: tables)
        #endif
        if fadesGrowth, before.width == width, before.height > 0, height > before.height + 1 {
            text.fadeIn(below: before.height)
        }
        fadesGrowth = false
        return height + Self.gap + above
    }

    /// Puts the language and a copy button in the top of every code box.
    private func placeHeaders() {
        let boxes = text.codeBoxes()
        while headers.count < boxes.count {
            let header = CodeHeader()
            text.addSubview(header)
            headers.append(header)
        }
        for (index, header) in headers.enumerated() {
            header.isHidden = index >= boxes.count
            guard index < boxes.count else { continue }
            let box = boxes[index]
            header.show(language: box.language, code: box.code)
            header.place(CGRect(x: box.frame.minX, y: box.frame.minY, width: box.frame.width, height: CodeHeader.height))
        }
    }

    override func clearSelection() { text.clearSelection() }
}

/// The top of a code box: the language, and a button that copies the code.
final class CodeHeader: FlippedView {
    private static let buttonInset: CGFloat = 4
    static let height = IconButton.side + buttonInset * 2

    private let language = TextLabel(font: Theme.smallMono, color: Theme.secondary)
    private var copyButton: IconButton!
    private var code = ""

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(language)
        copyButton = IconButton(symbol: .copy, tooltip: "Copy code") { [weak self] in self?.copy() }
        addSubview(copyButton)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(language: String, code: String) {
        self.language.string = language.isEmpty ? "text" : language
        self.code = code
    }

    func place(_ frame: CGRect) {
        self.frame = frame
        let lineHeight = scaled(15)
        language.frame = CGRect(x: 14, y: ((Self.height - lineHeight) / 2).rounded(), width: max(0, frame.width - 60), height: lineHeight)
        copyButton.frame = CGRect(x: frame.width - IconButton.side - Self.buttonInset, y: Self.buttonInset, width: IconButton.side, height: IconButton.side)
    }

    private func copy() {
        Platform.copy(code)
        copyButton.set(symbol: .check)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            self?.copyButton.set(symbol: .copy)
        }
    }
}

final class CodeRowView: RowView {
    private static let bottomPadding: CGFloat = 12

    private let surface = SurfaceView()
    private let header = CodeHeader()
    private let scroll = SidewaysClipView()
    private let text = RowTextView.make(wraps: false)
    private var lines = 1
    private var widestLine = 0

    static func height(lines: Int) -> CGFloat {
        CodeHeader.height + CGFloat(lines) * Theme.codeLineHeight + bottomPadding + 14
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        surface.fill = Theme.bubble
        surface.stroke = Theme.border
        surface.radius = 10
        addSubview(surface)
        surface.addSubview(header)

        scroll.addSubview(text)
        surface.addSubview(scroll)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .code(let content) = row.kind else { return }
        lines = content.lineCount
        header.show(language: content.language, code: content.code)
        text.content = content.attributed
        widestLine = Self.columns(of: content.code)
    }

    /// Only the colours changed; the layout stays.
    func recolor(_ content: CodeContent) {
        text.content = content.attributed
    }

    /// The length of the longest line in columns, counting what is wider than a letter as two.
    private static func columns(of code: String) -> Int {
        var widest = 0
        var current = 0
        for unit in code.utf16 {
            switch unit {
            case 10:
                widest = max(widest, current)
                current = 0
            case 9: current += 4
            case 0..<0x2e80: current += 1
            default: current += 2
            }
        }
        return max(widest, current)
    }

    override func layout(width: CGFloat) -> CGFloat {
        let bodyHeight = CGFloat(lines) * Theme.codeLineHeight
        let height = CodeHeader.height + bodyHeight + Self.bottomPadding
        surface.frame = CGRect(x: 0, y: 4, width: width, height: height)
        header.place(CGRect(x: 0, y: 0, width: width, height: CodeHeader.height))
        scroll.frame = CGRect(x: 0, y: CodeHeader.height, width: width, height: bodyHeight + Self.bottomPadding)
        let advance = Theme.codeFont.letterWidth
        let textWidth = max(width - 28, CGFloat(widestLine) * advance + 8)
        text.sideInset = 14
        scroll.setContent(text, size: CGSize(width: textWidth + 28, height: bodyHeight))
        return height + 14
    }

    override func clearSelection() { text.clearSelection() }
}

/// A tool call or the agent's thinking: one line, which opens to show the detail.
final class ToolRowView: RowView {
    static let rowHeight: CGFloat = Platform.scale > 1 ? 36 : 28

    private let header = SurfaceView()
    private static let titleFont = PlatformFont.ui(13)

    private let icon = SymbolView(tint: Theme.secondary)
    private let title = TextLabel(font: ToolRowView.titleFont, color: Theme.secondary)
    private let shine = ShimmerLabel.make(ToolRowView.titleFont)
    private let chevron = SymbolView(tint: Theme.tertiary)
    /// How long a call that still runs has been running.
    private let elapsed = TextLabel(font: .uiDigits(12), color: Theme.tertiary)
    private var startedAt: Double?
    private var timer: Timer?
    /// The call started an agent: the row opens what that agent did.
    private var opensAgent = false
    private let detailSurface = SurfaceView()
    private let detail = RowTextView.make()
    private var detailText: (() -> NSAttributedString)?
    private var hasDetail = false
    private var loadedDetail = false
    private var running = false
    /// For a group or a fold: whether it is open. Their rows are the core's, not a detail here.
    private var open: Bool?

    override init(frame: CGRect) {
        super.init(frame: frame)
        header.radius = 6
        addSubview(header)
        header.addSubview(icon)
        header.addSubview(title)
        header.addSubview(shine)
        header.addSubview(chevron)
        header.addSubview(elapsed)

        detailSurface.fill = Theme.bubble
        detailSurface.radius = 8
        detailSurface.isHidden = true
        addSubview(detailSurface)
        detailSurface.addSubview(detail)
        detail.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
        header.onClick = { [weak self] in
            guard let self, self.hasDetail else { return }
            guard !self.opensAgent else {
                self.owner?.openAgent(itemID: self.itemID)
                return
            }
            guard self.open == nil else {
                self.owner?.toggleRow(id: self.rowID)
                return
            }
            self.owner?.rowToggledExpansion(id: self.rowID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit { timer?.invalidate() }

    override func configure(_ row: RowModel) {
        super.configure(row)
        loadedDetail = false
        open = nil
        startedAt = nil
        opensAgent = false
        switch row.kind {
        case .tool(let tool):
            icon.show(tool.symbol)
            running = tool.status == .running
            startedAt = tool.startedAt
            opensAgent = tool.agent
            setTitle(tool.verb, target: tool.target, note: tool.progress, failed: tool.status == .failed)
            hasDetail = tool.hasDetail || tool.agent
            detailText = { tool.detail() }
        case .thinking(let thought):
            icon.show(.brain)
            running = false
            setTitle("Thought")
            hasDetail = thought.length > 0
            detailText = { thought }
        case .group(let group):
            icon.show(ToolContent.symbol(for: group.icon))
            running = group.running
            startedAt = group.startedAt
            setTitle(group.title, target: group.target, failed: group.failed)
            hasDetail = true
            detailText = nil
            open = group.open
        case .fold(let fold):
            icon.show(.clock)
            running = false
            setTitle(fold.label)
            hasDetail = true
            detailText = nil
            open = fold.open
        default:
            break
        }
        shine.sweeps = running
        keepTime()
    }

    /// Counts the seconds of a call that still runs, for as long as the row is on screen.
    private func keepTime() {
        timer?.invalidate()
        timer = nil
        elapsed.isHidden = startedAt == nil
        guard let startedAt else { return }
        elapsed.string = Time.elapsed(since: startedAt)
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] timer in
            guard let self, self.superview != nil else { return timer.invalidate() }
            self.elapsed.string = Time.elapsed(since: startedAt)
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    /// A running call's title is all muted, so the band shows on every part of it. `note` is
    /// what an agent the call started is doing.
    private func setTitle(_ words: String, target: String = "", note: String? = nil, failed: Bool = false) {
        let text = NSMutableAttributedString(
            string: target.isEmpty ? words : words + " ",
            attributes: [.font: Self.titleFont, .foregroundColor: Theme.secondary]
        )
        let targetColor = failed ? Theme.danger : running ? Theme.secondary : Theme.prose
        text.append(NSAttributedString(string: target, attributes: [.font: Theme.inlineCodeFont, .foregroundColor: targetColor]))
        if let note {
            let attributes: [NSAttributedString.Key: Any] = [.font: Self.titleFont, .foregroundColor: Theme.tertiary]
            text.append(NSAttributedString(string: "  \(note)", attributes: attributes))
        }
        // Without this a title too long for the row wraps, and loses its last words unseen.
        let cut = NSMutableParagraphStyle()
        cut.lineBreakMode = .byTruncatingTail
        text.addAttribute(.paragraphStyle, value: cut, range: NSRange(location: 0, length: text.length))
        title.attributed = text
        title.breaks = .byTruncatingTail
        guard running else { return }
        text.addAttribute(.foregroundColor, value: Theme.text, range: NSRange(location: 0, length: text.length))
        shine.attributed = text
        shine.breaks = .byTruncatingTail
    }

    override func layout(width: CGFloat) -> CGFloat {
        let expanded = open == nil && hasDetail && (owner?.isExpanded(id: rowID) ?? false)
        // The rows of an open group stand in from the group's own.
        let inset: CGFloat = nested ? 24 : 0
        let width = width - inset
        let line = Self.rowHeight - 2
        let middle = { (height: CGFloat) in ((line - height) / 2).rounded() }
        header.frame = CGRect(x: inset - 6, y: 1, width: width + 12, height: line)
        icon.frame = CGRect(x: 6, y: middle(16), width: 16, height: 16)
        let timeWidth: CGFloat = startedAt == nil ? 0 : scaled(58)
        let titleWidth = min(title.naturalWidth + 4, width - 60 - timeWidth)
        title.frame = CGRect(x: 30, y: middle(scaled(18)), width: titleWidth, height: scaled(18))
        shine.frame = title.frame
        chevron.isHidden = !hasDetail
        chevron.show(open ?? expanded ? .chevronDown : .chevronRight, size: 9)
        chevron.frame = CGRect(x: 30 + titleWidth + 2, y: middle(16), width: 14, height: 16)
        elapsed.frame = CGRect(x: chevron.frame.maxX + 6, y: middle(scaled(16)), width: timeWidth, height: scaled(16))

        detailSurface.isHidden = !expanded
        guard expanded else { return Self.rowHeight }
        if !loadedDetail {
            detail.content = detailText?() ?? NSAttributedString()
            loadedDetail = true
        }
        let inner = width - 30 - 24
        let detailHeight = detail.height(forWidth: inner)
        detailSurface.frame = CGRect(x: inset + 30, y: Self.rowHeight + 2, width: width - 30, height: detailHeight + 20)
        detail.frame = CGRect(x: 12, y: 10, width: inner, height: detailHeight)
        return Self.rowHeight + detailHeight + 20 + 8
    }

    override func clearSelection() { detail.clearSelection() }
}

final class ErrorRowView: RowView {
    /// The room beside the text, and the room above and under it.
    static let textInset: CGFloat = 36 + 14
    static let padding: CGFloat = 20 + 14

    private let surface = SurfaceView()
    private let icon = SymbolView(.circleAlert, size: 13, tint: Theme.danger)
    private let text = RowTextView.make()

    override init(frame: CGRect) {
        super.init(frame: frame)
        surface.fill = Theme.dangerBackground
        surface.radius = 10
        addSubview(surface)
        surface.addSubview(icon)
        surface.addSubview(text)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .error(let message) = row.kind else { return }
        text.content = message
    }

    override func layout(width: CGFloat) -> CGFloat {
        let inner = width - Self.textInset
        let height = text.height(forWidth: inner)
        surface.frame = CGRect(x: 0, y: 4, width: width, height: height + 20)
        icon.frame = CGRect(x: 12, y: 10, width: 16, height: 18)
        text.frame = CGRect(x: 36, y: 10, width: inner, height: height)
        return height + Self.padding
    }

    override func clearSelection() { text.clearSelection() }
}

/// What a turn changed: how many files and lines, and the files under their folders. A click on
/// a file opens what the turn changed in it in a tab of the panel.
final class ChangesRowView: RowView {
    fileprivate static let headHeight: CGFloat = scaled(40)
    fileprivate static let entryHeight: CGFloat = scaled(26)
    private static let topMargin: CGFloat = 12
    private static let bottomPadding: CGFloat = 6
    private static let radius: CGFloat = 10
    /// The button's highlight is this far from the top and the right, so that its words end as far
    /// from the edge as the title starts.
    private static let buttonMargin = 14 - RowButton.padding

    private let surface = SurfaceView()
    private let list = ChangesListView()
    private var openButton: RowButton!

    static func height(entries: Int) -> CGFloat {
        topMargin + headHeight + CGFloat(entries) * entryHeight + bottomPadding + 14
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        surface.fill = Theme.bubble
        surface.stroke = Theme.border
        surface.radius = Self.radius
        addSubview(surface)
        surface.addSubview(list)
        openButton = RowButton(
            title: "Open diff",
            tooltip: "Show what this turn changed",
            radius: Self.radius - Self.buttonMargin,
            insets: PlatformEdgeInsets(top: Self.buttonMargin, left: 2, bottom: Self.buttonMargin, right: Self.buttonMargin)
        ) { [weak self] in
            guard let self else { return }
            self.owner?.openDiff(turn: self.itemID, path: nil)
        }
        surface.addSubview(openButton)
        list.onClick = { [weak self] entry in
            guard let self else { return }
            guard entry.folder else {
                self.owner?.openDiff(turn: self.itemID, path: entry.path)
                return
            }
            self.owner?.toggleFolder(id: entry.id, rowID: self.rowID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .changes(let content) = row.kind else { return }
        list.content = content
        openButton.dim()
    }

    override func layout(width: CGFloat) -> CGFloat {
        let entries = list.content?.entries.count ?? 0
        let height = Self.headHeight + CGFloat(entries) * Self.entryHeight + Self.bottomPadding
        surface.frame = CGRect(x: 0, y: Self.topMargin, width: width, height: height)
        list.frame = CGRect(x: 0, y: 0, width: width, height: height)
        openButton.frame = CGRect(x: width - openButton.width, y: 0, width: openButton.width, height: Self.headHeight)
        return height + Self.topMargin + 14
    }

    /// Draws the head and the entries, and lights the entry under the pointer.
    private final class ChangesListView: FlippedView {
        var content: ChangesContent? {
            didSet {
                hovered = nil
                redraw()
            }
        }
        var onClick: ((ChangesContent.Entry) -> Void)?
        private var hovered: Int?
        private let headHeight = ChangesRowView.headHeight
        private let entryHeight = ChangesRowView.entryHeight

        override init(frame: CGRect) {
            super.init(frame: frame)
            onPress = { [weak self] point in
                guard let self, let content = self.content, let index = self.entry(at: point) else { return }
                self.onClick?(content.entries[index])
            }
            onHover = { [weak self] point in
                guard let self else { return }
                self.light(point.flatMap { self.entry(at: $0) })
            }
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        override func draw(_ dirtyRect: CGRect) {
            guard let content else { return }
            let titleSize = content.title.size()
            content.title.draw(at: CGPoint(x: 14, y: ((headHeight - titleSize.height) / 2).rounded()))
            for (index, entry) in content.entries.enumerated() {
                let row = CGRect(x: 0, y: headHeight + CGFloat(index) * entryHeight, width: bounds.width, height: entryHeight)
                guard row.intersects(dirtyRect) else { continue }
                if hovered == index {
                    Theme.hover.setFill()
                    RoundedBox.fill(row.insetBy(dx: 6, dy: 1), radius: 6)
                }
                var x = 12 + CGFloat(entry.depth) * 16
                if entry.folder {
                    let chevron = CGRect(x: x, y: row.minY, width: 12, height: row.height)
                    TintedSymbol.draw(entry.open ? .chevronDown : .chevronRight, size: 8, color: Theme.tertiary, in: chevron)
                }
                x += 16
                let symbol: Symbol = entry.folder ? .folder : FileSymbol.symbol(for: entry.path)
                TintedSymbol.draw(symbol, size: 11, color: Theme.secondary, in: CGRect(x: x, y: row.minY, width: 16, height: row.height))
                x += 24
                let counts = entry.counts.size()
                let countsX = bounds.width - 14 - counts.width
                entry.counts.draw(at: CGPoint(x: countsX, y: row.minY + ((row.height - counts.height) / 2).rounded()))
                let height = ceil(entry.name.size().height)
                entry.name.drawTruncated(
                    in: CGRect(x: x, y: row.minY + ((row.height - height) / 2).rounded(), width: max(0, countsX - 12 - x), height: height))
            }
        }

        private func entry(at point: CGPoint) -> Int? {
            guard let content, point.y >= headHeight else { return nil }
            let index = Int((point.y - headHeight) / entryHeight)
            return index < content.entries.count ? index : nil
        }

        override func takesPress(at point: CGPoint) -> Bool {
            entry(at: point) != nil
        }

        private func light(_ index: Int?) {
            guard index != hovered else { return }
            hovered = index
            redraw()
        }
    }
}

/// Closes a turn: a way to copy the reply and when it ended.
final class TurnEndRowView: RowView {
    static let height = MessageMeta.height + 10

    private let meta = MessageMeta(trailing: false, tooltip: "Copy reply")

    override var showsMeta: Bool {
        didSet { meta.shown = showsMeta }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(meta)
        meta.onCopy = { [weak self] in
            guard let self else { return }
            self.owner?.copyReply(endingAt: self.rowID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .turnEnd(let end) = row.kind else { return }
        meta.show(end.stamp)
    }

    override func layout(width: CGFloat) -> CGFloat {
        meta.frame = CGRect(x: 0, y: 0, width: width, height: MessageMeta.height)
        return Self.height
    }
}

/// Under a message or a reply: when it was sent and a button that copies it, at the side the
/// message is on. Where there is a pointer it is only seen while the pointer is over its message.
final class MessageMeta: FlippedView {
    static let height = IconButton.side
    /// How far the button's symbol is from the button's edge, which goes past the column's.
    private static let symbolInset: CGFloat = Platform.scale > 1 ? 10 : 7

    var onCopy: (() -> Void)?
    var shown = !Platform.hoverReveals {
        didSet {
            guard shown != oldValue else { return }
            reveal()
        }
    }

    private let trailing: Bool
    private let time = TextLabel(font: Theme.smallFont, color: Theme.tertiary)
    private var button: IconButton?
    private var copied = false

    init(trailing: Bool, tooltip: String) {
        self.trailing = trailing
        super.init(frame: .zero)
        addSubview(time)
        let button = IconButton(symbol: .copy, symbolSize: 12, tooltip: tooltip) { [weak self] in self?.copy() }
        self.button = button
        addSubview(button)
        opacity = shown ? 1 : 0
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ stamp: String) {
        time.string = stamp
        layoutNow()
    }

    override var frame: CGRect {
        didSet { layoutNow() }
    }

    override func layoutNow() {
        guard let button else { return }
        let side = IconButton.side
        let lineHeight = scaled(16)
        let lineY = ((side - lineHeight) / 2).rounded()
        let words = ceil(time.string.size(withAttributes: [.font: Theme.smallFont]).width)
        let timeWidth = min(words + 4, max(0, bounds.width - side))
        guard trailing else {
            button.frame = CGRect(x: -Self.symbolInset, y: 0, width: side, height: side)
            time.frame = CGRect(x: button.frame.maxX, y: lineY, width: timeWidth, height: lineHeight)
            return
        }
        button.frame = CGRect(x: bounds.width - side + Self.symbolInset, y: 0, width: side, height: side)
        time.frame = CGRect(x: button.frame.minX - timeWidth, y: lineY, width: timeWidth, height: lineHeight)
    }

    /// The check mark that says it was copied stays to be seen, also when the pointer has left.
    private func copy() {
        onCopy?()
        copied = true
        button?.set(symbol: .check)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            guard let self else { return }
            self.copied = false
            self.button?.set(symbol: .copy)
            self.reveal()
        }
    }

    private func reveal() {
        fade(to: shown || copied ? 1 : 0, duration: 0.12)
    }
}

/// Shown under the transcript while the agent is at work or waits for an approval.
final class WorkingView: FlippedView {
    /// Digits of one width, so the line doesn't change size with every second.
    private static let font = PlatformFont.uiDigits(13)

    private let text = TextLabel(font: WorkingView.font, color: Theme.secondary)
    private let shine = ShimmerLabel.make(WorkingView.font)
    private var timer: Timer?
    private var activity = Activity()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(text)
        addSubview(shine)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var frame: CGRect {
        didSet {
            text.frame = CGRect(x: 0, y: 3, width: bounds.width, height: scaled(18))
            shine.frame = text.frame
        }
    }

    func update(_ activity: Activity) {
        self.activity = activity
        let waiting = !activity.approvals.isEmpty
        isHidden = !activity.running
        shine.sweeps = activity.running && !waiting
        timer?.invalidate()
        timer = nil
        guard activity.running, !waiting else {
            if activity.running { show("Waiting for your approval") }
            return
        }
        refresh()
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] _ in self?.refresh() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func refresh() {
        let verb = activity.compacting ? "Compacting" : activity.thinking ? "Thinking" : "Working"
        guard let started = activity.startedAt else {
            show("\(verb)…")
            return
        }
        show("\(verb) for \(Time.elapsed(since: started))")
    }

    private func show(_ words: String) {
        text.string = words
        shine.string = words
    }
}
