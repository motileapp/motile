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
    /// Hands over a file an agent sent, or why it can't be had.
    func fetchFile(id: String, done: @escaping (Result<URL, Error>) -> Void)
    /// Stops fetching the file, whose `fetchFile` then fails.
    func cancelFetch(id: String)
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
    /// Opens the thread an agent the thread's agent started works in.
    func openThread(id: String)
    /// Whether a file that couldn't be fetched can be asked for again: not from a removed server.
    var fetchesAgain: Bool { get }
    /// Opens a new thread on another server that goes on from this one, whose server was unlinked.
    func continueElsewhere()
    /// Opens the settings' servers, where an unlinked server's threads are deleted with it.
    func openServerSettings()
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
        case .prose(let text, _):
            return TextMeasure.height(of: text, width: width) + ProseRowView.gap + row.above
        case .error(let text):
            return TextMeasure.height(of: text, width: width - ErrorRowView.textInset) + ErrorRowView.padding
        case .removed(let content):
            return RemovedRowView.height(textHeight: TextMeasure.height(of: content.message, width: RemovedRowView.inner(width)))
        case .code, .tool, .thinking, .group, .fold, .agent, .agents, .media, .file, .changes, .turnEnd, .handoff, .queued:
            return estimatedHeight(row, width: width)
        }
    }

    /// What the row would be without measuring its text, until it has been measured.
    class func estimatedHeight(_ row: RowModel, width: CGFloat) -> CGFloat {
        switch row.kind {
        case .user(let text, let attachments, _):
            return estimatedTextHeight(text.length, width: width * 0.75) + 34 + UserRowView.footHeight + AttachedFilesView.height(attachments, width: width)
        case .prose(let text, _):
            return estimatedTextHeight(text.length, width: width) + ProseRowView.gap + row.above
        case .code(let content):
            return CodeRowView.height(lines: content.lineCount)
        case .tool, .thinking, .group, .fold:
            return ToolRowView.rowHeight + row.above
        case .agent(let content):
            return AgentRowView.height(content)
        case .agents:
            return AgentRowView.rowHeight
        case .media(let content):
            return MediaRowView.height(content, width: width)
        case .file:
            return FileRowView.height
        case .error(let text):
            return estimatedTextHeight(text.length, width: width - ErrorRowView.textInset) + ErrorRowView.padding
        case .changes(let content):
            return ChangesRowView.height(entries: content.entries.count)
        case .turnEnd:
            return TurnEndRowView.height
        case .handoff:
            return HandoffRowView.height
        case .removed(let content):
            return RemovedRowView.height(textHeight: estimatedTextHeight(content.message.length, width: RemovedRowView.inner(width)))
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
        case .agent: return AgentRowView()
        case .agents: return AgentsRowView()
        case .media: return MediaRowView()
        case .file: return FileRowView()
        case .error: return ErrorRowView()
        case .changes: return ChangesRowView()
        case .turnEnd: return TurnEndRowView()
        case .handoff: return HandoffRowView()
        case .queued: return QueuedRowView()
        case .removed: return RemovedRowView()
        }
    }

    static func reuseKey(for row: RowModel) -> String {
        switch row.kind {
        case .user: return "user"
        case .prose: return "prose"
        case .code: return "code"
        case .tool, .thinking, .group, .fold: return "tool"
        case .agent: return "agent"
        case .agents: return "agents"
        case .media: return "media"
        case .file: return "file"
        case .error: return "error"
        case .changes: return "changes"
        case .turnEnd: return "turnEnd"
        case .handoff: return "handoff"
        case .queued: return "queued"
        case .removed: return "removed"
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
        bubble.fill = Theme.backgroundSecondary
        bubble.radius = 18
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        addSubview(meta)
        meta.onCopy = { [weak self] in
            guard let self else { return }
            Platform.copy(self.text.content.withSkillNames)
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
        pending = row.id == RowModel.pendingID
        bubble.opacity = pending ? 0.6 : 1
    }

    private static let margin: CGFloat = 14
    /// The room under the bubble for when the message was sent and the button that copies it.
    static let footHeight = metaGap + MessageMeta.height + 25
    private static let metaGap: CGFloat = 4

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
        meta.frame = CGRect(x: 0, y: bubble.frame.maxY + Self.metaGap, width: width, height: MessageMeta.height)
        return Self.height(fit, textHeight: textHeight)
    }

    override func clearSelection() { text.clearSelection() }
}

/// A message that waits for the agent: what it says, how it waits, and the buttons that steer
/// the turn with it or take it back into the composer. It stands where the user's messages do,
/// dotted around instead of filled.
final class QueuedRowView: RowView {
    /// The strip under the message, down to the bubble's edge, that holds the status and the buttons.
    static let footHeight: CGFloat = buttonGap + RowButton.metrics.height + buttonMargin
    private static let radius = Radius.xl
    /// The buttons are this far from the bubble's right and bottom.
    private static let buttonMargin: CGFloat = 10
    /// The room between the message and the buttons.
    private static let buttonGap: CGFloat = 18
    /// How far above the buttons a click still presses them.
    private static let buttonReach: CGFloat = 4

    private let bubble = SurfaceView()
    private let text = RowTextView.make()
    private let attachments = AttachedFilesView()
    private let clock = SymbolView(.clock, size: 11)
    private let status = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
    private var sendButton: RowButton!
    private var cancelButton: RowButton!
    private var messageID = ""
    private var attached: [AttachedFile] = []
    private var hasText = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        bubble.stroke = Theme.border
        bubble.dotted = true
        bubble.radius = Self.radius
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        bubble.addSubview(clock)
        bubble.addSubview(status)
        sendButton = RowButton(
            title: "Steer",
            tooltip: "Send as a steer instead",
            radius: Self.radius - Self.buttonMargin,
            bordered: true,
            insets: PlatformEdgeInsets(top: Self.buttonReach, left: 3, bottom: Self.buttonMargin, right: 3)
        ) { [weak self] in
            guard let self, self.messageID != RowModel.pendingID else { return }
            self.owner?.sendQueued(messageID: self.messageID)
        }
        cancelButton = RowButton(
            title: "Edit",
            tooltip: "Edit in the composer",
            radius: Self.radius - Self.buttonMargin,
            bordered: true,
            insets: PlatformEdgeInsets(top: Self.buttonReach, left: 3, bottom: Self.buttonMargin, right: Self.buttonMargin)
        ) { [weak self] in
            guard let self, self.messageID != RowModel.pendingID else { return }
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
        sendButton.dim()
        cancelButton.dim()
    }

    override func layout(width: CGFloat) -> CGFloat {
        let padding = BubbleFit.padding
        // The buttons reach the bubble's edge, so they take the padding on that side too.
        let buttonsWidth = sendButton.width + cancelButton.width
        let statusWidth = 18 + ceil(status.string.size(withAttributes: [.font: Theme.smallFont]).width) + 8
        let footWidth = statusWidth + 10 + buttonsWidth - padding
        let fit = BubbleFit(text: text.content, attachments: attached, width: width, least: max(footWidth, 12))
        let (innerWidth, top, between, files) = (fit.innerWidth, fit.top, fit.between, fit.files)
        let textHeight = hasText ? text.height(forWidth: innerWidth) : 0
        let footY = top + files.height + between + textHeight
        let bubbleSize = CGSize(width: innerWidth + padding * 2, height: footY + Self.footHeight)
        bubble.frame = CGRect(x: width - bubbleSize.width, y: 14, width: bubbleSize.width, height: bubbleSize.height)
        attachments.frame = CGRect(x: padding, y: top, width: innerWidth, height: files.height)
        attachments.layout(width: innerWidth)
        text.frame = CGRect(x: padding, y: top + files.height + between, width: innerWidth, height: textHeight)

        let buttonsY = footY + Self.buttonGap - Self.buttonReach
        let buttonsHeight = Self.footHeight - Self.buttonGap + Self.buttonReach
        cancelButton.frame = CGRect(x: bubbleSize.width - cancelButton.width, y: buttonsY, width: cancelButton.width, height: buttonsHeight)
        sendButton.frame = CGRect(x: cancelButton.frame.minX - sendButton.width, y: buttonsY, width: sendButton.width, height: buttonsHeight)
        let statusEnd = sendButton.frame.minX - 6
        // In the middle of the buttons.
        let lineHeight = scaled(16)
        let lineY = footY + Self.buttonGap + ((RowButton.metrics.height - lineHeight) / 2).rounded()
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
        guard case .prose(let content, _) = row.kind else { return }
        text.content = content
        above = row.above
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

    private let language = TextLabel(font: Theme.smallMono, color: Theme.mutedForeground)
    private var copyButton: IconButton!
    private var code = ""

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(language)
        copyButton = IconButton(symbol: .copy, symbolSize: 12, tooltip: "Copy code") { [weak self] in self?.copy() }
        copyButton.surface = .secondary
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
        surface.fill = Theme.backgroundSecondary
        surface.stroke = Theme.border
        surface.radius = Radius.lg
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
    /// What the row keeps from a paragraph above it, so that the two are a paragraph apart.
    static let afterProse: CGFloat = 8
    private var above: CGFloat = 0

    /// The header reaches past the row's sides by as much room as its icon has above it, so
    /// that what it lights up stands out while the icon lines up with the text.
    private static let line = rowHeight - 2
    private static let bleed = (line - 16) / 2
    private let header = FlippedView()
    private let highlight = SurfaceView()
    private static let titleFont = PlatformFont.ui(13)

    private let icon = SymbolView(tint: Theme.mutedStrongerForeground)
    private let title = TextLabel(font: ToolRowView.titleFont, color: Theme.mutedStrongerForeground)
    private let shine = ShimmerLabel.make(ToolRowView.titleFont)
    private let chevron = SymbolView(tint: Theme.mutedStrongerForeground)
    /// How long a call that still runs has been running.
    private let elapsed = TextLabel(font: .uiDigits(12), color: Theme.mutedStrongerForeground)
    private var startedAt: Double?
    private var timer: Timer?
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
        highlight.radius = 6
        addSubview(header)
        header.addSubview(highlight)
        header.addSubview(icon)
        header.addSubview(title)
        header.addSubview(shine)
        header.addSubview(chevron)
        header.addSubview(elapsed)

        detailSurface.fill = Theme.backgroundSecondary
        detailSurface.radius = 8
        detailSurface.isHidden = true
        addSubview(detailSurface)
        detailSurface.addSubview(detail)
        detail.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
        header.onHover = { [weak self] point in
            guard let self else { return }
            self.highlight.fill = point != nil && self.hasDetail ? Theme.backgroundAccentLarger : .clear
        }
        header.onPress = { [weak self] _ in
            guard let self, self.hasDetail else { return }
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
        highlight.fill = .clear
        open = nil
        startedAt = nil
        switch row.kind {
        case .tool(let tool):
            icon.show(tool.symbol)
            running = tool.status == .running
            startedAt = tool.startedAt
            setTitle(tool.verb, target: tool.target, failed: tool.status == .failed)
            hasDetail = tool.hasDetail
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
        above = row.above
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
            self.fitHeader()
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    /// A running call's title is all muted, so the band shows on every part of it.
    private func setTitle(_ words: String, target: String = "", failed: Bool = false) {
        let text = NSMutableAttributedString(
            string: target.isEmpty ? words : words + " ",
            attributes: [.font: Self.titleFont, .foregroundColor: Theme.mutedStrongerForeground]
        )
        let targetColor = failed ? Theme.destructive : running ? Theme.mutedStrongerForeground : Theme.mutedForeground
        text.append(NSAttributedString(string: target, attributes: [.font: Theme.inlineCodeFont, .foregroundColor: targetColor]))
        // Without this a title too long for the row wraps, and loses its last words unseen.
        let cut = NSMutableParagraphStyle()
        cut.lineBreakMode = .byTruncatingTail
        text.addAttribute(.paragraphStyle, value: cut, range: NSRange(location: 0, length: text.length))
        title.attributed = text
        title.breaks = .byTruncatingTail
        guard running else { return }
        text.addAttribute(.foregroundColor, value: Theme.foreground, range: NSRange(location: 0, length: text.length))
        shine.attributed = text
        shine.breaks = .byTruncatingTail
    }

    override func layout(width: CGFloat) -> CGFloat {
        let expanded = open == nil && hasDetail && (owner?.isExpanded(id: rowID) ?? false)
        // The rows of an open group stand in from the group's own.
        let inset: CGFloat = nested ? 24 : 0
        let width = width - inset
        let bleed = Self.bleed
        let middle = { (height: CGFloat) in (1 + (Self.line - height) / 2).rounded() }
        header.frame = CGRect(x: inset - bleed, y: above, width: header.frame.width, height: Self.rowHeight)
        icon.frame = CGRect(x: bleed, y: middle(16), width: 16, height: 16)
        let timeWidth: CGFloat = startedAt == nil ? 0 : scaled(58)
        let titleWidth = min(title.naturalWidth + 4, width - 60 - timeWidth)
        title.frame = CGRect(x: bleed + 24, y: middle(scaled(18)), width: titleWidth, height: scaled(18))
        shine.frame = title.frame
        chevron.isHidden = !hasDetail
        chevron.show(open ?? expanded ? .chevronDown : .chevronRight, size: 9)
        chevron.frame = CGRect(x: title.frame.maxX + 2, y: middle(16), width: 14, height: 16)
        elapsed.frame = CGRect(x: chevron.frame.maxX + 6, y: middle(scaled(16)), width: 0, height: scaled(16))
        fitHeader()

        detailSurface.isHidden = !expanded
        guard expanded else { return Self.rowHeight + above }
        if !loadedDetail {
            detail.content = detailText?() ?? NSAttributedString()
            loadedDetail = true
        }
        let inner = width - 30 - 24
        let detailHeight = detail.height(forWidth: inner)
        detailSurface.frame = CGRect(x: inset + 30, y: Self.rowHeight + above + 3, width: width - 30, height: detailHeight + 20)
        detail.frame = CGRect(x: 12, y: 10, width: inner, height: detailHeight)
        return Self.rowHeight + above + detailHeight + 20 + 9
    }

    override func clearSelection() { detail.clearSelection() }

    /// The header ends as far past the last thing on it as it starts before the icon, and a point
    /// further past the chevron, whose 9 points are drawn in the middle of its frame.
    private func fitHeader() {
        elapsed.frame.size.width = elapsed.isHidden ? 0 : elapsed.naturalWidth
        let end = !elapsed.isHidden ? elapsed.frame.maxX : hasDetail ? chevron.frame.midX + 5.5 : title.frame.maxX
        header.frame.size.width = (end + Self.bleed).rounded()
        highlight.frame = CGRect(x: 0, y: 1, width: header.frame.width, height: Self.line)
    }
}

final class ErrorRowView: RowView {
    /// The room beside the text, and the room above and under it.
    static let textInset: CGFloat = 36 + 14
    static let padding: CGFloat = 20 + 14

    private let surface = SurfaceView()
    private let icon = SymbolView(.circleAlert, size: 13, tint: Theme.destructive)
    private let text = RowTextView.make()

    override init(frame: CGRect) {
        super.init(frame: frame)
        surface.fill = Theme.destructiveTint
        surface.radius = Radius.lg
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
    #if os(macOS)
    fileprivate static let entryHeight: CGFloat = 28
    #else
    fileprivate static let entryHeight: CGFloat = 36
    #endif
    private static let topMargin: CGFloat = 12
    private static let bottomPadding: CGFloat = 6
    private static let radius = Radius.lg
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
        surface.fill = Theme.backgroundSecondary
        surface.stroke = Theme.border
        surface.radius = Self.radius
        addSubview(surface)
        surface.addSubview(list)
        openButton = RowButton(
            title: "Open diff",
            tooltip: "Show what this turn changed",
            radius: Self.radius - Self.buttonMargin,
            hover: Theme.backgroundSecondaryAccent,
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
        private let highlightHeight = ControlSize.small.height

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
                var x = 12 + CGFloat(entry.depth) * 16
                if hovered == index {
                    let start = (entry.folder ? x : x + 16) - 6
                    let highlight = CGRect(x: start, y: row.midY - highlightHeight / 2, width: row.maxX - 6 - start, height: highlightHeight)
                    Theme.backgroundSecondaryAccent.setFill()
                    RoundedBox.fill(highlight, radius: Radius.sm)
                }
                if entry.folder {
                    let chevron = CGRect(x: x, y: row.minY, width: 12, height: row.height)
                    TintedSymbol.draw(entry.open ? .chevronDown : .chevronRight, size: 8, color: Theme.mutedStrongerForeground, in: chevron)
                }
                x += 16
                let symbol: Symbol = entry.folder ? .folder : FileSymbol.symbol(for: entry.path)
                TintedSymbol.draw(symbol, size: 11, color: Theme.mutedForeground, in: CGRect(x: x, y: row.minY, width: 16, height: row.height))
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

/// An agent the thread's agent started, or what it was told of one: its avatar, its title and word,
/// then what it runs on and does or said, and how long it worked. A click opens it: its thread, or
/// what it did. In the card of an open group it lies on the card.
final class AgentRowView: RowView {
    static let rowHeight = scaled(50)
    private static let avatarGap = scaled(10)
    /// How far a row in a group's card stands in from the card's sides.
    private static let cardPadding = scaled(8)
    /// How far the light reaches past the row's sides, so that the avatar lines up with the text.
    private static let bleed = scaled(6)
    /// How far the light stands in from the row's top and bottom, half the room between two lights.
    private static let lightGap: CGFloat = 2
    /// How far in from the card's border a row in it lights, on every side.
    private static let cardLight = scaled(4)
    /// The room above a group's card and under it, which its border stands in from by what the
    /// light leaves at the card's ends.
    private static let cardAbove = scaled(4) + cardLight - lightGap
    private static let cardBelow = scaled(10) + cardLight - lightGap

    static func height(_ content: AgentContent) -> CGFloat {
        rowHeight + (content.card == .first ? cardAbove : 0) + (content.card == .last ? cardBelow : 0)
    }

    /// Holds the card, of which it shows only the row's own part.
    private let cardClip = FlippedView()
    private let card = SurfaceView()
    private let highlight = SurfaceView()
    private let avatar = AgentAvatarView()
    private let title = TextLabel(font: .ui(13, weight: .medium), color: Theme.foreground)
    private let role = TextLabel(font: .ui(11), color: Theme.mutedStrongerForeground)
    private let label = TextLabel(font: .ui(11, weight: .medium), color: Theme.mutedForeground)
    private let line = TextLabel(font: .ui(12), color: Theme.mutedForeground)
    private let time = TextLabel(font: .uiDigits(12), color: Theme.mutedStrongerForeground)
    private let chevron = SymbolView(tint: Theme.mutedStrongerForeground)
    private var content: AgentContent?
    private var timer: Timer?

    override init(frame: CGRect) {
        super.init(frame: frame)
        card.fill = Theme.card
        card.stroke = Theme.borderCard
        card.radius = Radius.lg
        highlight.radius = Radius.md
        cardClip.addSubview(card)
        [cardClip, highlight, avatar, title, role, label, line, time, chevron].forEach { addSubview($0) }
        #if os(macOS)
        cardClip.wantsLayer = true
        cardClip.layer?.masksToBounds = true
        #else
        cardClip.clipsToBounds = true
        #endif
        onHover = { [weak self] point in
            guard let self else { return }
            // A row of a card lights in the card's own accent, inside the card's border.
            let light = self.content?.card == nil ? Theme.backgroundAccentLarger : Theme.cardAccent
            self.highlight.fill = point != nil ? light : .clear
        }
        onPress = { [weak self] _ in
            guard let self, let content = self.content else { return }
            if let thread = content.thread { return self.owner?.openThread(id: thread) ?? () }
            self.owner?.openAgent(itemID: self.itemID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit { timer?.invalidate() }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .agent(let content) = row.kind else { return }
        self.content = content
        highlight.fill = .clear
        avatar.agent = content.agent
        avatar.state = content.state
        avatar.ground = content.card == nil ? Theme.background : Theme.card
        cardClip.isHidden = content.card == nil
        title.string = content.title
        role.string = content.role ?? ""
        role.isHidden = content.role == nil
        label.string = content.label
        label.color = content.state.flagged ? content.state.platformColor : Theme.mutedForeground
        label.isHidden = !content.flagged
        line.string = content.line
        line.color = content.state == .failed ? Theme.destructive : Theme.mutedForeground
        chevron.show(.chevronRight, size: 9)
        keepTime()
    }

    /// Counts the seconds of an agent that works, for as long as the row is on screen.
    private func keepTime() {
        timer?.invalidate()
        timer = nil
        guard let content else { return }
        if let at = content.at {
            time.string = Time.stamp(at)
            return
        }
        guard [.working, .waiting, .limited].contains(content.state) else {
            time.string = content.durationMs.map { Time.duration(milliseconds: $0) } ?? ""
            return
        }
        time.string = Time.elapsed(since: content.startedAt)
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] timer in
            guard let self, self.superview != nil else { return timer.invalidate() }
            self.time.string = Time.elapsed(since: content.startedAt)
            self.layoutNow()
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    override func layoutNow() {
        _ = layout(width: bounds.width)
    }

    override func layout(width: CGFloat) -> CGFloat {
        guard let content else { return Self.rowHeight }
        let height = Self.rowHeight
        let place = content.card
        let inset = place == nil ? 0 : Self.cardPadding
        let y = place == .first ? Self.cardAbove : 0
        // A card's rows are one card: each draws the part of it that is its own.
        let overhang = Radius.lg + 1
        let ends = Self.cardLight - Self.lightGap
        let top: CGFloat = place == .first ? y - ends : -overhang
        let bottom: CGFloat = place == .last ? y + height + ends : y + height + overhang
        cardClip.frame = CGRect(x: 0, y: 0, width: width, height: Self.height(content))
        card.frame = CGRect(x: 0, y: top, width: width, height: bottom - top)
        // In a card the light keeps as far from the card's border as from the next row's light.
        let lit = place == nil ? -Self.bleed : Self.cardLight
        highlight.frame = CGRect(x: lit, y: y + Self.lightGap, width: width - lit * 2, height: height - Self.lightGap * 2)
        let side = AgentAvatarDrawing.side
        avatar.frame = CGRect(x: inset, y: y + ((height - side) / 2).rounded(), width: side, height: side)
        let textX = inset + side + Self.avatarGap
        let right = width - inset
        chevron.frame = CGRect(x: right - 14, y: y + ((height - 16) / 2).rounded(), width: 14, height: 16)
        let timeWidth = time.string.isEmpty ? 0 : time.naturalWidth + 4
        let timeY = y + ((height - scaled(16)) / 2).rounded()
        time.frame = CGRect(x: chevron.frame.minX - 6 - timeWidth, y: timeY, width: timeWidth, height: scaled(16))
        let room = time.frame.minX - 12 - textX
        let lineHeight = scaled(18)
        let titleY = y + (height / 2 - lineHeight + 1).rounded()
        var x = textX
        for (view, gap) in [(title, CGFloat(6)), (role, 6), (label, 0)] where !view.isHidden {
            let width = min(view.naturalWidth + 4, max(0, textX + room - x))
            view.frame = CGRect(x: x, y: titleY, width: width, height: lineHeight)
            x += width + gap
        }
        line.frame = CGRect(x: textX, y: y + (height / 2 + 1).rounded(), width: max(0, room), height: scaled(16))
        return Self.height(content)
    }
}

/// Agents started one after the other: their avatars side by side, how many and how they are
/// doing, and how long they have worked. A click opens it into a card of their rows.
final class AgentsRowView: RowView {
    private static let bleed = scaled(6)
    private static let step = AgentAvatarDrawing.side * 3 / 4

    private let highlight = SurfaceView()
    private var avatars: [AgentAvatarView] = []
    private let more = TextLabel(font: .ui(10, weight: .medium), color: Theme.mutedForeground)
    private let moreSurface = SurfaceView()
    private let title = TextLabel(font: .ui(13, weight: .semibold), color: Theme.foreground)
    private let summary = TextLabel(font: .ui(12), color: Theme.mutedForeground)
    private let time = TextLabel(font: .uiDigits(12), color: Theme.mutedStrongerForeground)
    private let chevron = SymbolView(tint: Theme.mutedStrongerForeground)
    private var content: AgentsContent?
    private var timer: Timer?

    override init(frame: CGRect) {
        super.init(frame: frame)
        highlight.radius = Radius.md
        moreSurface.fill = Theme.borderAccent
        moreSurface.radius = AgentAvatarDrawing.side / 2
        [highlight, moreSurface, more, title, summary, time, chevron].forEach { addSubview($0) }
        onHover = { [weak self] point in
            guard let self else { return }
            self.highlight.fill = point != nil ? Theme.backgroundAccentLarger : .clear
            // The ring that sets each avatar apart is of what it lies on.
            self.avatars.forEach { $0.ground = point != nil ? Theme.backgroundAccentLarger : Theme.background }
        }
        onPress = { [weak self] _ in
            guard let self else { return }
            self.owner?.toggleRow(id: self.rowID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit { timer?.invalidate() }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .agents(let content) = row.kind else { return }
        self.content = content
        highlight.fill = .clear
        avatars.forEach { $0.removeFromSuperview() }
        // The first lies over the second, so it is added last.
        avatars = content.avatars.map { avatar in
            let view = AgentAvatarView()
            view.agent = avatar.agent
            // Side by side they leave how they are doing to the summary.
            view.state = content.avatars.count > 1 ? nil : avatar.state
            view.ringed = content.avatars.count > 1
            return view
        }
        avatars.reversed().forEach { addSubview($0) }
        more.string = "+\(content.more)"
        more.isHidden = content.more == 0
        moreSurface.isHidden = content.more == 0
        title.string = content.title
        summary.attributed = Self.summary(content.summary)
        chevron.show(content.open ? .chevronDown : .chevronRight, size: 9)
        keepTime()
    }

    /// Each count in its state's colour, between muted dots.
    private static func summary(_ counts: [(text: String, state: AgentState)]) -> NSAttributedString {
        let text = NSMutableAttributedString()
        for (index, count) in counts.enumerated() {
            if index > 0 {
                text.append(NSAttributedString(string: " · ", attributes: [.font: PlatformFont.ui(12), .foregroundColor: Theme.mutedStrongerForeground]))
            }
            text.append(NSAttributedString(string: count.text, attributes: [.font: PlatformFont.ui(12), .foregroundColor: count.state.platformColor]))
        }
        return text
    }

    private func keepTime() {
        timer?.invalidate()
        timer = nil
        guard let content else { return }
        guard let startedAt = content.startedAt else {
            time.string = content.durationMs.map { Time.duration(milliseconds: $0) } ?? ""
            return
        }
        time.string = Time.elapsed(since: startedAt)
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] timer in
            guard let self, self.superview != nil else { return timer.invalidate() }
            self.time.string = Time.elapsed(since: startedAt)
            self.layoutNow()
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    override func layoutNow() {
        _ = layout(width: bounds.width)
    }

    override func layout(width: CGFloat) -> CGFloat {
        let height = AgentRowView.rowHeight
        let side = AgentAvatarDrawing.side
        highlight.frame = CGRect(x: -Self.bleed, y: 2, width: width + Self.bleed * 2, height: height - 4)
        let y = ((height - side) / 2).rounded()
        for (index, avatar) in avatars.enumerated() {
            avatar.frame = CGRect(x: CGFloat(index) * Self.step, y: y, width: side, height: side)
        }
        var end = avatars.last.map { $0.frame.maxX } ?? 0
        if !moreSurface.isHidden {
            let size = more.natural
            let pill = max(side, (size.width + AgentAvatarDrawing.morePadding * 2).rounded(.up))
            moreSurface.frame = CGRect(x: CGFloat(avatars.count) * Self.step, y: y, width: pill, height: side)
            more.frame = CGRect(x: moreSurface.frame.midX - size.width / 2, y: moreSurface.frame.midY - size.height / 2, width: size.width, height: size.height)
            end = moreSurface.frame.maxX
        }
        let textX = end + scaled(10)
        chevron.frame = CGRect(x: width - 14, y: ((height - 16) / 2).rounded(), width: 14, height: 16)
        let timeWidth = time.string.isEmpty ? 0 : time.naturalWidth + 4
        time.frame = CGRect(x: chevron.frame.minX - 6 - timeWidth, y: ((height - scaled(16)) / 2).rounded(), width: timeWidth, height: scaled(16))
        let room = max(0, time.frame.minX - 12 - textX)
        title.frame = CGRect(x: textX, y: (height / 2 - scaled(18) + 1).rounded(), width: room, height: scaled(18))
        summary.frame = CGRect(x: textX, y: (height / 2 + 1).rounded(), width: room, height: scaled(16))
        return height
    }
}

/// The way back to the thread whose agent started this one: a line across, broken by a round
/// button that names it. The transcript shows it above its first row and under its last.
final class LineageView: FlippedView {
    static let height = ControlSize.large.height

    private static let size = ControlSize.small
    private static let lineGap = ControlSize.regular.padding
    /// The arrow's square leaves room around its ink, so it needs little more from the words.
    private static let arrowGap: CGFloat = 1

    private let leading = SurfaceView()
    private let trailing = SurfaceView()
    private let pill = SurfaceView()
    private let symbol = SymbolView(.cornerLeftUp, size: size.smallSymbol, tint: Theme.mutedForeground)
    private let words = TextLabel(font: .ui(size.textSize, weight: .medium), color: Theme.foreground)

    var title = "" {
        didSet {
            guard title != oldValue else { return }
            let line = NSMutableParagraphStyle()
            line.lineBreakMode = .byTruncatingTail
            let font = PlatformFont.ui(Self.size.textSize, weight: .medium)
            let text = NSMutableAttributedString(string: "Started by ", attributes: [.font: font, .foregroundColor: Theme.mutedForeground, .paragraphStyle: line])
            text.append(NSAttributedString(string: title, attributes: [.font: font, .foregroundColor: Theme.foreground, .paragraphStyle: line]))
            words.attributed = text
            describe("Started by \(title)", button: true)
            placeParts()
        }
    }

    var action: (() -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        leading.fill = Theme.border
        trailing.fill = Theme.border
        pill.radius = Self.size.height / 2
        pill.stroke = Theme.borderAccent
        [leading, trailing, pill, symbol, words].forEach { addSubview($0) }
        light(false)
        onHover = { [weak self] point in
            guard let self else { return }
            self.light(point.map { self.pill.frame.contains($0) } ?? false)
        }
        onPress = { [weak self] point in
            guard let self, self.pill.frame.contains(point) else { return }
            self.action?()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var frame: CGRect {
        didSet { placeParts() }
    }

    private func light(_ lit: Bool) {
        pill.fill = lit ? Theme.borderAccent : Theme.backgroundAccent
    }

    private func placeParts() {
        let size = Self.size
        let side = PlatformImage.symbolSide(size.smallSymbol)
        let ends = size.padding + side + Self.arrowGap + size.padding
        let room = max(0, bounds.width - Self.lineGap * 2 - ends)
        let wordsWidth = min(ceil(words.attributed.size().width) + 2, room)
        let width = ends + wordsWidth
        let x = ((bounds.width - width) / 2).rounded()
        let y = ((bounds.height - size.height) / 2).rounded()
        pill.frame = CGRect(x: x, y: y, width: width, height: size.height)
        let symbolX = x + size.padding
        // The arrow stands on the baseline, lower than the caps' middle it is raised to.
        symbol.frame = CGRect(x: symbolX, y: y + ((size.height - side) / 2).rounded() - scaled(1), width: side, height: side)
        let wordsHeight = scaled(16)
        words.frame = CGRect(x: symbolX + side + Self.arrowGap, y: y + ((size.height - wordsHeight) / 2).rounded(), width: wordsWidth, height: wordsHeight)
        let lineY = (bounds.height / 2).rounded()
        leading.frame = CGRect(x: 0, y: lineY, width: max(0, x - Self.lineGap), height: 1)
        let after = x + width + Self.lineGap
        trailing.frame = CGRect(x: after, y: lineY, width: max(0, bounds.width - after), height: 1)
    }
}

/// Where the thread went on with another agent or account: a line across, broken by the model it
/// left and the one it went on with.
final class HandoffRowView: RowView {
    static let height = ControlSize.large.height

    private static let logoSide = ControlSize.regular.symbol
    private static let arrowSize = ControlSize.regular.smallSymbol
    private static let labelGap = ControlSize.regular.gap
    private static let arrowGap = ControlSize.small.padding
    private static let lineGap = ControlSize.regular.padding
    private static let logos: [Agent: PlatformImage] = Dictionary(uniqueKeysWithValues: Agent.allCases.compactMap { agent in
        agent.logo.map { (agent, ImageFiles.tinted(ImageFiles.sized($0, logoSide), agent.platformColor)) }
    })

    private var content: HandoffContent?

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .handoff(let content) = row.kind else { return }
        self.content = content
        redraw()
    }

    override func layout(width: CGFloat) -> CGFloat { Self.height }

    /// The model, and the account after a muted dot, as the model picker shows them.
    private static func label(_ end: HandoffContent.End) -> NSAttributedString {
        let text = { (string: String, color: PlatformColor) in
            NSAttributedString(string: string, attributes: [.font: Theme.smallFont, .foregroundColor: color])
        }
        let label = NSMutableAttributedString(attributedString: text(end.label, Theme.foreground))
        guard let account = end.account else { return label }
        label.append(text(" · ", Theme.mutedStrongerForeground))
        label.append(text(account, Theme.foreground))
        return label
    }

    /// The widths of the two labels: as wide as they are where they fit, else the shorter one whole
    /// and the other what is left, or each half.
    private static func fitted(_ from: CGFloat, _ to: CGFloat, in room: CGFloat) -> (CGFloat, CGFloat) {
        let room = max(0, room)
        guard from + to > room else { return (from, to) }
        let half = (room / 2).rounded(.down)
        if from <= half { return (from, room - from) }
        if to <= half { return (room - to, to) }
        return (half, half)
    }

    override func draw(_ dirtyRect: CGRect) {
        guard let content else { return }
        let from = Self.label(content.from)
        let to = Self.label(content.to)
        let arrowSide = PlatformImage.symbolSide(Self.arrowSize)
        let fixed = (Self.logoSide + Self.labelGap) * 2 + Self.arrowGap * 2 + arrowSide
        let room = bounds.width - Self.lineGap * 4 - fixed
        let (fromWidth, toWidth) = Self.fitted(ceil(from.size().width), ceil(to.size().width), in: room)
        let middle = fixed + fromWidth + toWidth
        let midY = bounds.height / 2
        var x = ((bounds.width - middle) / 2).rounded()

        Theme.border.setFill()
        let lineY = midY.rounded()
        CGRect(x: 0, y: lineY, width: max(0, x - Self.lineGap), height: 1).fillCurrent()
        let rightLine = x + middle + Self.lineGap
        CGRect(x: rightLine, y: lineY, width: max(0, bounds.width - rightLine), height: 1).fillCurrent()

        let drawEnd = { (end: HandoffContent.End, label: NSAttributedString, width: CGFloat) in
            if let logo = Self.logos[end.agent] {
                let side = Self.logoSide
                logo.drawUpright(in: CGRect(x: x, y: (midY - side / 2).rounded(), width: side, height: side))
            }
            x += Self.logoSide + Self.labelGap
            let height = ceil(label.size().height)
            label.drawTruncated(in: CGRect(x: x, y: (midY - height / 2).rounded(), width: width, height: height))
            x += width
        }
        drawEnd(content.from, from, fromWidth)
        x += Self.arrowGap
        let arrow = CGRect(x: x, y: 0, width: arrowSide, height: bounds.height)
        TintedSymbol.draw(.arrowRight, size: Self.arrowSize, color: Theme.mutedStrongerForeground, in: arrow)
        x += arrowSide + Self.arrowGap
        drawEnd(content.to, to, toWidth)
    }
}

/// The end of a thread whose server was unlinked from the account, in place of the composer: why
/// the thread can only be read, and the ways on from it.
final class RemovedRowView: RowView {
    private static let margin = scaled(12)
    private static let padding = scaled(16)
    private static let titleHeight = scaled(18)
    private static let iconGap = scaled(8)
    private static let textGap = scaled(6)
    private static let buttonsGap = scaled(14)
    /// Under this width the two buttons share the row in halves.
    private static let halvesBelow = scaled(420)

    private let surface = SurfaceView()
    private let icon = SymbolView(.unlink, size: 13, tint: Theme.mutedForeground)
    private let title = TextLabel(font: .ui(scaled(13), weight: .semibold), color: Theme.foreground)
    private let text = RowTextView.make()
    private var continueButton: RowButton!
    private var settingsButton: RowButton!

    static func inner(_ width: CGFloat) -> CGFloat { max(0, width - padding * 2) }

    static func height(textHeight: CGFloat) -> CGFloat {
        margin + padding + titleHeight + textGap + textHeight + buttonsGap + RowButton.metrics.height + padding + margin
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        surface.fill = Theme.backgroundSecondary
        surface.stroke = Theme.border
        surface.radius = Radius.lg
        addSubview(surface)
        [icon, title, text].forEach { surface.addSubview($0) }
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
        let button = { (title: String, tooltip: String, primary: Bool, action: @escaping () -> Void) in
            RowButton(
                title: title, tooltip: tooltip, radius: RowButton.metrics.radius, bordered: true, primary: primary,
                hover: Theme.border, rest: Theme.background, insets: PlatformEdgeInsets(top: 0, left: 0, bottom: 0, right: 0),
                action: action
            )
        }
        continueButton = button("Continue Thread", "Start a thread on another server with everything that happened here", true) {
            [weak self] in self?.owner?.continueElsewhere()
        }
        settingsButton = button("Open Settings", "Unlinked servers are deleted with their threads in settings", false) {
            [weak self] in self?.owner?.openServerSettings()
        }
        surface.addSubview(continueButton)
        surface.addSubview(settingsButton)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .removed(let content) = row.kind else { return }
        title.string = content.title
        text.content = content.message
        continueButton.dim()
        settingsButton.dim()
    }

    override func layout(width: CGFloat) -> CGFloat {
        let inner = Self.inner(width)
        let textHeight = text.height(forWidth: inner)
        let height = Self.height(textHeight: textHeight)
        surface.frame = CGRect(x: 0, y: Self.margin, width: width, height: height - Self.margin * 2)
        let side = icon.intrinsicContentSize.width
        icon.frame = CGRect(x: Self.padding, y: Self.padding, width: side, height: Self.titleHeight)
        let titleX = icon.frame.maxX + Self.iconGap
        title.frame = CGRect(x: titleX, y: Self.padding, width: max(0, width - titleX - Self.padding), height: Self.titleHeight)
        text.frame = CGRect(x: Self.padding, y: title.frame.maxY + Self.textGap, width: inner, height: textHeight)
        let buttonsY = text.frame.maxY + Self.buttonsGap
        let gap = ControlSize.regular.gap
        let buttonHeight = RowButton.metrics.height
        // Narrow, each takes half the row; wide, they keep their size at its end.
        let (settingsWidth, continueWidth) = inner < Self.halvesBelow
            ? (((inner - gap) / 2).rounded(.down), ((inner - gap) / 2).rounded(.down))
            : (settingsButton.width, continueButton.width)
        let end = Self.padding + inner
        continueButton.frame = CGRect(x: end - continueWidth, y: buttonsY, width: continueWidth, height: buttonHeight)
        settingsButton.frame = CGRect(x: continueButton.frame.minX - gap - settingsWidth, y: buttonsY, width: settingsWidth, height: buttonHeight)
        return height
    }

    override func clearSelection() { text.clearSelection() }
}

/// Under a message or a reply: when it was sent and a button that copies it, at the side the
/// message is on. Where there is a pointer it is only seen while the pointer is over its message.
final class MessageMeta: FlippedView {
    static let height = IconButton.side

    var onCopy: (() -> Void)?
    var shown = !Platform.hoverReveals {
        didSet {
            guard shown != oldValue else { return }
            reveal()
        }
    }

    private let trailing: Bool
    private let time = TextLabel(font: Theme.smallFont, color: Theme.mutedStrongerForeground)
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
            button.frame = CGRect(x: 0, y: 0, width: side, height: side)
            time.frame = CGRect(x: button.frame.maxX, y: lineY, width: timeWidth, height: lineHeight)
            return
        }
        button.frame = CGRect(x: bounds.width - side, y: 0, width: side, height: side)
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

    private let text = TextLabel(font: WorkingView.font, color: Theme.mutedStrongerForeground)
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
        let asking = activity.approvals.first?.ask
        isHidden = !activity.running
        shine.sweeps = activity.running && asking == nil
        timer?.invalidate()
        timer = nil
        guard activity.running, asking == nil else {
            if activity.running, let asking { show(asking.waiting) }
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
