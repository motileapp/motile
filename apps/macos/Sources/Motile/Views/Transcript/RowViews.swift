import AppKit
import QuartzCore

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
    /// Gives the agent a queued message now, or takes it back into the composer.
    func sendQueued(messageID: String)
    func cancelQueued(messageID: String)
}

/// A filled, rounded rectangle whose colours follow the appearance.
final class SurfaceView: FlippedView {
    var fill: NSColor = .clear { didSet { needsDisplay = true } }
    var stroke: NSColor? { didSet { needsDisplay = true } }
    var radius: CGFloat = 0 { didSet { needsDisplay = true } }
    var onClick: (() -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.backgroundColor = fill.cgColor
        layer?.cornerRadius = radius
        layer?.cornerCurve = .continuous
        layer?.borderColor = stroke?.cgColor
        layer?.borderWidth = stroke == nil ? 0 : 1
    }

    /// A clickable surface takes the clicks on the labels and icons inside it.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard onClick != nil else { return super.hitTest(point) }
        return frame.contains(point) ? self : nil
    }

    override func mouseDown(with event: NSEvent) {
        guard let onClick else { return super.mouseDown(with: event) }
        onClick()
    }

    /// A clickable surface keeps the arrow, also when it floats over text.
    override func resetCursorRects() {
        guard onClick != nil else { return }
        addCursorRect(bounds, cursor: .arrow)
    }
}

private func label(_ font: NSFont, _ color: NSColor) -> NSTextField {
    let field = NSTextField(labelWithString: "")
    field.font = font
    field.textColor = color
    field.lineBreakMode = .byTruncatingTail
    field.maximumNumberOfLines = 1
    return field
}

private func symbol(_ name: String, size: CGFloat = 12, weight: NSFont.Weight = .regular) -> NSImage? {
    let configuration = NSImage.SymbolConfiguration(pointSize: size, weight: weight)
    return NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(configuration)
}

/// A bright copy of a label, laid over it and seen only through a soft band that sweeps across.
final class ShimmerLabel: NSTextField {
    private static let bandWidth: CGFloat = 72
    private static let period: CFTimeInterval = 2.2

    private let band = CAGradientLayer()

    var sweeps = false {
        didSet { restart() }
    }

    override var frame: NSRect {
        didSet {
            guard frame.size != oldValue.size else { return }
            restart()
        }
    }

    override var stringValue: String {
        didSet {
            guard stringValue != oldValue else { return }
            restart()
        }
    }

    static func make(_ font: NSFont) -> ShimmerLabel {
        let field = ShimmerLabel(labelWithString: "")
        field.font = font
        field.textColor = Theme.text
        field.lineBreakMode = .byTruncatingTail
        field.maximumNumberOfLines = 1
        field.setAccessibilityElement(false)

        let alphas: [CGFloat] = [0, 0.12, 0.55, 1, 0.55, 0.12, 0]
        field.band.colors = alphas.map { NSColor.black.withAlphaComponent($0).cgColor }
        field.band.locations = [0, 0.15, 0.35, 0.5, 0.65, 0.85, 1]
        field.band.startPoint = CGPoint(x: 0, y: 0.5)
        field.band.endPoint = CGPoint(x: 1, y: 0.5)
        field.wantsLayer = true
        field.layer?.mask = field.band
        return field
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        restart()
    }

    override func viewDidHide() {
        super.viewDidHide()
        restart()
    }

    override func viewDidUnhide() {
        super.viewDidUnhide()
        restart()
    }

    private func restart() {
        band.removeAllAnimations()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        band.frame = NSRect(x: -Self.bandWidth, y: 0, width: Self.bandWidth, height: bounds.height)
        CATransaction.commit()
        guard sweeps, window != nil, !isHiddenOrHasHiddenAncestor else { return }
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else { return }

        let sweep = CABasicAnimation(keyPath: "position.x")
        sweep.fromValue = -Self.bandWidth / 2
        // Across the words, however much room the label has.
        sweep.toValue = min(bounds.width, intrinsicContentSize.width) + Self.bandWidth / 2
        sweep.duration = Self.period
        sweep.repeatCount = .infinity
        // Every sweep starts on the same beat, so labels move together and a restart doesn't show.
        let now = band.convertTime(CACurrentMediaTime(), from: nil)
        sweep.beginTime = now - now.truncatingRemainder(dividingBy: Self.period)
        band.add(sweep, forKey: "sweep")
    }
}

/// A borderless button with an SF Symbol and, optionally, a title. It lights up under the
/// pointer.
final class IconButton: NSButton {
    static let side: CGFloat = 28
    private static let symbolSize: CGFloat = 14

    private var action_: (() -> Void)?
    private var tracking: NSTrackingArea?

    convenience init(symbolName: String, title: String = "", tooltip: String, action: @escaping () -> Void) {
        self.init(frame: .zero)
        isBordered = false
        bezelStyle = .inline
        wantsLayer = true
        layer?.cornerRadius = 6
        image = symbol(symbolName, size: Self.symbolSize, weight: .medium)
        imagePosition = title.isEmpty ? .imageOnly : .imageLeading
        self.title = title
        font = Theme.smallFont
        contentTintColor = Theme.secondary
        toolTip = tooltip
        target = self
        self.action = #selector(pressed)
        action_ = action
        setButtonType(.momentaryChange)
    }

    func set(symbolName: String, title: String = "") {
        image = symbol(symbolName, size: Self.symbolSize, weight: .medium)
        self.title = title
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow], owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Theme.hover.cgColor
        }
    }

    override func mouseExited(with event: NSEvent) {
        layer?.backgroundColor = nil
    }

    @objc private func pressed() {
        action_?()
    }
}

/// A button of words inside a row. All of its frame takes the click, and what lights up under
/// the pointer is inset from it, so buttons that touch each other and the row's edge look apart.
final class RowButton: FlippedView {
    private let highlight = SurfaceView()
    private let title = label(Theme.smallFont, Theme.secondary)
    private let insets: NSEdgeInsets
    private let action: () -> Void
    private var tracking: NSTrackingArea?

    init(title: String, tooltip: String, insets: NSEdgeInsets, action: @escaping () -> Void) {
        self.insets = insets
        self.action = action
        super.init(frame: .zero)
        highlight.radius = 6
        addSubview(highlight)
        self.title.stringValue = title
        highlight.addSubview(self.title)
        toolTip = tooltip
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(title)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// The label's own measure comes up a little short, so the words are measured directly.
    var width: CGFloat {
        let words = ceil(title.stringValue.size(withAttributes: [.font: Theme.smallFont]).width) + 4
        return words + 20 + insets.left + insets.right
    }

    override var frame: NSRect {
        didSet {
            let lit = NSSize(width: bounds.width - insets.left - insets.right, height: bounds.height - insets.top - insets.bottom)
            highlight.frame = NSRect(x: insets.left, y: insets.top, width: max(0, lit.width), height: max(0, lit.height))
            title.frame = NSRect(x: 8, y: ((lit.height - 16) / 2).rounded(), width: max(0, lit.width - 16), height: 16)
        }
    }

    /// A button that is shown again starts unlit, wherever the pointer left it.
    func dim() {
        highlight.fill = .clear
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    override func mouseDown(with event: NSEvent) {
        action()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow], owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        highlight.fill = Theme.hover
    }

    override func mouseExited(with event: NSEvent) {
        dim()
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }
}

/// The base of every row. A row is as wide as the transcript's column; `layout(width:)` places
/// its parts for that width and says how tall it is.
class RowView: FlippedView {
    weak var owner: RowOwner?
    private(set) var rowID = ""
    private(set) var nested = false
    /// Set when the row grew while its reply streams: the next layout fades the new part in.
    var fadesGrowth = false

    func configure(_ row: RowModel) {
        rowID = row.id
        nested = row.nested
    }

    func layout(width: CGFloat) -> CGFloat { 0 }

    func clearSelection() {}

    /// What the row would be without measuring it, to place rows nobody has scrolled to yet.
    class func estimatedHeight(_ row: RowModel, width: CGFloat) -> CGFloat {
        switch row.kind {
        case .user(let text, let attachments):
            return estimatedTextHeight(text.length, width: width * 0.75) + 48 + (attachments.isEmpty ? 0 : 22)
        case .prose(let text, let above, _):
            return estimatedTextHeight(text.length, width: width) + ProseRowView.gap + above
        case .code(let content):
            return CodeRowView.height(lines: content.lineCount)
        case .tool, .thinking, .group, .fold:
            return ToolRowView.rowHeight
        case .media(let content):
            return MediaRowView.height(content, width: width)
        case .error(let text):
            return estimatedTextHeight(text.length, width: width - 40) + 34
        case .turnEnd:
            return TurnEndRowView.height
        case .queued(let content):
            let attachments: CGFloat = content.attachments.isEmpty ? 0 : 22
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
        case .turnEnd: return "turnEnd"
        case .queued: return "queued"
        }
    }
}

final class UserRowView: RowView {
    private let bubble = SurfaceView()
    private let text = RowTextView.make()
    private let attachments = label(Theme.smallFont, Theme.secondary)
    private var hasAttachments = false
    private var pending = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        bubble.fill = Theme.bubble
        bubble.radius = 18
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.owner?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .user(let content, let files) = row.kind else { return }
        text.content = content
        hasAttachments = !files.isEmpty
        attachments.isHidden = !hasAttachments
        attachments.stringValue = files.map { "📎 \($0)" }.joined(separator: "   ")
        pending = row.id == "pending"
        bubble.alphaValue = pending ? 0.6 : 1
    }

    override func layout(width: CGFloat) -> CGFloat {
        let padding: CGFloat = 14
        let widest = max(120, width * 0.8) - padding * 2
        // The bubble is as wide as its text, up to the widest it may be.
        let natural = text.content.boundingRect(
            with: NSSize(width: widest, height: CGFloat.greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        )
        let attachmentsWidth = hasAttachments ? min(widest, attachments.intrinsicContentSize.width) : 0
        let textWidth = min(widest, max(ceil(natural.width) + 2, attachmentsWidth, 12))
        let textHeight = text.height(forWidth: textWidth)
        let attachmentsHeight: CGFloat = hasAttachments ? 22 : 0
        let bubbleSize = NSSize(width: textWidth + padding * 2, height: textHeight + 20 + attachmentsHeight)
        bubble.frame = NSRect(x: width - bubbleSize.width, y: 14, width: bubbleSize.width, height: bubbleSize.height)
        text.frame = NSRect(x: padding, y: 10, width: textWidth, height: textHeight)
        attachments.frame = NSRect(x: padding, y: 10 + textHeight + 6, width: textWidth, height: 16)
        return bubbleSize.height + 14 + 14
    }

    override func clearSelection() { text.clearSelection() }
}

/// A message that waits for the agent: what it says, when the agent gets it, and the buttons that
/// send it now or take it back. It stands where the user's messages do, outlined instead of filled.
final class QueuedRowView: RowView {
    /// The strip under the message, down to the bubble's edge, that holds the status and the buttons.
    static let footHeight: CGFloat = 34

    private let bubble = SurfaceView()
    private let text = RowTextView.make()
    private let attachments = label(Theme.smallFont, Theme.secondary)
    private let clock = NSImageView()
    private let status = label(Theme.smallFont, Theme.secondary)
    private var sendButton: RowButton!
    private var cancelButton: RowButton!
    private var messageID = ""
    private var hasText = false
    private var hasAttachments = false
    private var sending = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        bubble.stroke = Theme.strongBorder
        bubble.radius = 18
        addSubview(bubble)
        bubble.addSubview(text)
        bubble.addSubview(attachments)
        clock.image = symbol("clock", size: 11)
        clock.contentTintColor = Theme.secondary
        clock.imageScaling = .scaleNone
        bubble.addSubview(clock)
        bubble.addSubview(status)
        sendButton = RowButton(
            title: "Send now",
            tooltip: "Give it to the agent without waiting",
            insets: NSEdgeInsets(top: 3, left: 2, bottom: 7, right: 2)
        ) { [weak self] in
            guard let self else { return }
            self.owner?.sendQueued(messageID: self.messageID)
        }
        cancelButton = RowButton(
            title: "Cancel",
            tooltip: "Take it back into the composer",
            insets: NSEdgeInsets(top: 3, left: 2, bottom: 7, right: 8)
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
        hasAttachments = !content.attachments.isEmpty
        attachments.isHidden = !hasAttachments
        attachments.stringValue = content.attachments.map { "📎 \($0)" }.joined(separator: "   ")
        status.stringValue = content.status
        sending = content.sending
        sendButton.isHidden = sending
        cancelButton.isHidden = sending
        sendButton.dim()
        cancelButton.dim()
    }

    override func layout(width: CGFloat) -> CGFloat {
        let padding: CGFloat = 14
        let widest = max(120, width * 0.8) - padding * 2
        let natural = text.content.boundingRect(
            with: NSSize(width: widest, height: CGFloat.greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        )
        // The buttons reach the bubble's edge, so they take the padding on that side too.
        let buttonsWidth = sending ? 0 : sendButton.width + cancelButton.width
        let statusWidth = 18 + ceil(status.stringValue.size(withAttributes: [.font: Theme.smallFont]).width) + 8
        let footWidth = sending ? statusWidth : statusWidth + 10 + buttonsWidth - padding
        let attachmentsWidth = hasAttachments ? min(widest, attachments.intrinsicContentSize.width) : 0
        let innerWidth = min(widest, max(hasText ? ceil(natural.width) + 2 : 0, attachmentsWidth, footWidth, 12))
        let textHeight = hasText ? text.height(forWidth: innerWidth) : 0
        let attachmentsHeight: CGFloat = hasAttachments ? 22 : 0
        let footY = 10 + textHeight + attachmentsHeight + 2
        let bubbleSize = NSSize(width: innerWidth + padding * 2, height: footY + Self.footHeight)
        bubble.frame = NSRect(x: width - bubbleSize.width, y: 14, width: bubbleSize.width, height: bubbleSize.height)
        text.frame = NSRect(x: padding, y: 10, width: innerWidth, height: textHeight)
        attachments.frame = NSRect(x: padding, y: 10 + textHeight + (hasText ? 6 : 0), width: innerWidth, height: 16)

        cancelButton.frame = NSRect(x: bubbleSize.width - cancelButton.width, y: footY, width: cancelButton.width, height: Self.footHeight)
        sendButton.frame = NSRect(x: cancelButton.frame.minX - sendButton.width, y: footY, width: sendButton.width, height: Self.footHeight)
        let statusEnd = sending ? bubbleSize.width - padding : sendButton.frame.minX - 6
        clock.frame = NSRect(x: padding, y: footY + 7, width: 14, height: 16)
        status.frame = NSRect(x: padding + 18, y: footY + 7, width: max(0, statusEnd - padding - 18), height: 16)
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

    override init(frame: NSRect) {
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
        text.frame = NSRect(x: 0, y: 3 + above, width: width, height: height)
        placeHeaders()
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
            header.place(NSRect(x: box.frame.minX, y: box.frame.minY, width: box.frame.width, height: CodeHeader.height))
        }
    }

    override func clearSelection() { text.clearSelection() }
}

/// The top of a code box: the language, and a button that copies the code.
final class CodeHeader: FlippedView {
    private static let buttonInset: CGFloat = 4
    static let height = IconButton.side + buttonInset * 2

    private let language = label(Theme.smallMono, Theme.secondary)
    private var copyButton: IconButton!
    private var code = ""

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(language)
        copyButton = IconButton(symbolName: "doc.on.doc", tooltip: "Copy code") { [weak self] in self?.copy() }
        addSubview(copyButton)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(language: String, code: String) {
        self.language.stringValue = language.isEmpty ? "text" : language
        self.code = code
    }

    func place(_ frame: NSRect) {
        self.frame = frame
        language.frame = NSRect(x: 14, y: 11, width: max(0, frame.width - 60), height: 15)
        copyButton.frame = NSRect(x: frame.width - IconButton.side - Self.buttonInset, y: Self.buttonInset, width: IconButton.side, height: IconButton.side)
    }

    private func copy() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(code, forType: .string)
        copyButton.set(symbolName: "checkmark")
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            self?.copyButton.set(symbolName: "doc.on.doc")
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

    override init(frame: NSRect) {
        super.init(frame: frame)
        surface.fill = Theme.codeBackground
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
        surface.frame = NSRect(x: 0, y: 4, width: width, height: height)
        header.place(NSRect(x: 0, y: 0, width: width, height: CodeHeader.height))
        scroll.frame = NSRect(x: 0, y: CodeHeader.height, width: width, height: bodyHeight + Self.bottomPadding)
        let advance = Theme.codeFont.maximumAdvancement.width
        let textWidth = max(width - 28, CGFloat(widestLine) * advance + 8)
        text.textContainerInset = NSSize(width: 14, height: 0)
        scroll.setContent(text, size: NSSize(width: textWidth + 28, height: bodyHeight))
        return height + 14
    }

    override func clearSelection() { text.clearSelection() }
}

/// A tool call or the agent's thinking: one line, which opens to show the detail.
final class ToolRowView: RowView {
    static let rowHeight: CGFloat = 28

    private let header = SurfaceView()
    private let icon = NSImageView()
    private let title = label(NSFont.systemFont(ofSize: 13), Theme.secondary)
    private let shine = ShimmerLabel.make(NSFont.systemFont(ofSize: 13))
    private let chevron = NSImageView()
    private let detailSurface = SurfaceView()
    private let detail = RowTextView.make()
    private var detailText: (() -> NSAttributedString)?
    private var hasDetail = false
    private var loadedDetail = false
    private var running = false
    /// For a group or a fold: whether it is open. Their rows are the core's, not a detail here.
    private var open: Bool?

    override init(frame: NSRect) {
        super.init(frame: frame)
        header.radius = 6
        addSubview(header)
        icon.contentTintColor = Theme.secondary
        icon.imageScaling = .scaleNone
        header.addSubview(icon)
        header.addSubview(title)
        header.addSubview(shine)
        chevron.contentTintColor = Theme.tertiary
        chevron.imageScaling = .scaleNone
        header.addSubview(chevron)

        detailSurface.fill = Theme.codeBackground
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
            guard self.open == nil else {
                self.owner?.toggleRow(id: self.rowID)
                return
            }
            self.owner?.rowToggledExpansion(id: self.rowID)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        loadedDetail = false
        open = nil
        switch row.kind {
        case .tool(let tool):
            icon.image = symbol(tool.symbol)
            running = tool.status == .running
            setTitle(tool.verb, target: tool.target, failed: tool.status == .failed)
            hasDetail = tool.hasDetail
            detailText = { tool.detail() }
        case .thinking(let thought):
            icon.image = symbol("brain")
            running = false
            setTitle("Thought")
            hasDetail = thought.length > 0
            detailText = { thought }
        case .group(let group):
            icon.image = symbol(ToolContent.symbol(for: group.icon))
            running = group.running
            setTitle(group.title, target: group.target, failed: group.failed)
            hasDetail = true
            detailText = nil
            open = group.open
        case .fold(let fold):
            icon.image = symbol("clock")
            running = false
            setTitle(fold.label)
            hasDetail = true
            detailText = nil
            open = fold.open
        default:
            break
        }
        shine.sweeps = running
    }

    /// A running call's title is all muted, so the band shows on every part of it.
    private func setTitle(_ words: String, target: String = "", failed: Bool = false) {
        let text = NSMutableAttributedString(
            string: target.isEmpty ? words : words + " ",
            attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: Theme.secondary]
        )
        let targetColor = failed ? Theme.danger : running ? Theme.secondary : Theme.prose
        text.append(NSAttributedString(string: target, attributes: [.font: Theme.inlineCodeFont, .foregroundColor: targetColor]))
        title.attributedStringValue = text
        title.lineBreakMode = .byTruncatingTail
        guard running else { return }
        text.addAttribute(.foregroundColor, value: Theme.text, range: NSRange(location: 0, length: text.length))
        shine.attributedStringValue = text
        shine.lineBreakMode = .byTruncatingTail
    }

    override func layout(width: CGFloat) -> CGFloat {
        let expanded = open == nil && hasDetail && (owner?.isExpanded(id: rowID) ?? false)
        // The rows of an open group stand in from the group's own.
        let inset: CGFloat = nested ? 24 : 0
        let width = width - inset
        header.frame = NSRect(x: inset - 6, y: 1, width: width + 12, height: Self.rowHeight - 2)
        icon.frame = NSRect(x: 6, y: 5, width: 16, height: 16)
        let titleWidth = min(title.intrinsicContentSize.width + 4, width - 60)
        title.frame = NSRect(x: 30, y: 4, width: titleWidth, height: 18)
        shine.frame = title.frame
        chevron.isHidden = !hasDetail
        chevron.image = symbol(open ?? expanded ? "chevron.down" : "chevron.right", size: 9, weight: .semibold)
        chevron.frame = NSRect(x: 30 + titleWidth + 2, y: 5, width: 14, height: 16)

        detailSurface.isHidden = !expanded
        guard expanded else { return Self.rowHeight }
        if !loadedDetail {
            detail.content = detailText?() ?? NSAttributedString()
            loadedDetail = true
        }
        let inner = width - 30 - 24
        let detailHeight = detail.height(forWidth: inner)
        detailSurface.frame = NSRect(x: inset + 30, y: Self.rowHeight + 2, width: width - 30, height: detailHeight + 20)
        detail.frame = NSRect(x: 12, y: 10, width: inner, height: detailHeight)
        return Self.rowHeight + detailHeight + 20 + 8
    }

    override func clearSelection() { detail.clearSelection() }
}

final class ErrorRowView: RowView {
    private let surface = SurfaceView()
    private let icon = NSImageView()
    private let text = RowTextView.make()

    override init(frame: NSRect) {
        super.init(frame: frame)
        surface.fill = Theme.dangerBackground
        surface.radius = 10
        addSubview(surface)
        icon.image = symbol("exclamationmark.circle", size: 13)
        icon.contentTintColor = Theme.danger
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
        let inner = width - 36 - 14
        let height = text.height(forWidth: inner)
        surface.frame = NSRect(x: 0, y: 4, width: width, height: height + 20)
        icon.frame = NSRect(x: 12, y: 10, width: 16, height: 18)
        text.frame = NSRect(x: 36, y: 10, width: inner, height: height)
        return height + 20 + 14
    }

    override func clearSelection() { text.clearSelection() }
}

/// The line that closes a turn: how long it took, a way to copy the reply and, when the agent
/// was refused a tool, the choice to allow it.
final class TurnEndRowView: RowView {
    static let height: CGFloat = 47

    private let summary = label(Theme.smallFont, Theme.tertiary)
    private var copyButton: IconButton!
    private let rule = SurfaceView()
    private var folded = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(summary)
        copyButton = IconButton(symbolName: "doc.on.doc", tooltip: "Copy reply") { [weak self] in
            guard let self else { return }
            self.owner?.copyReply(endingAt: self.rowID)
            self.copyButton.set(symbolName: "checkmark")
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
                self?.copyButton.set(symbolName: "doc.on.doc")
            }
        }
        addSubview(copyButton)
        rule.fill = Theme.border
        addSubview(rule)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .turnEnd(let end) = row.kind else { return }
        summary.stringValue = end.label
        folded = end.folded
        summary.isHidden = folded
    }

    override func layout(width: CGFloat) -> CGFloat {
        let y: CGFloat = 2
        let summaryWidth = folded ? 0 : min(summary.intrinsicContentSize.width + 4, width - 40)
        summary.frame = NSRect(x: 0, y: y + 3, width: summaryWidth, height: 16)
        copyButton.frame = NSRect(x: folded ? -6 : summaryWidth + 4, y: y - 3, width: IconButton.side, height: IconButton.side)
        rule.frame = NSRect(x: 0, y: y + 30, width: width, height: 1)
        return Self.height
    }
}

/// Shown under the transcript while the agent is at work or waits for an approval.
final class WorkingView: FlippedView {
    /// Digits of one width, so the line doesn't change size with every second.
    private static let font = NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .regular)

    private let text = label(WorkingView.font, Theme.secondary)
    private let shine = ShimmerLabel.make(WorkingView.font)
    private var timer: Timer?
    private var activity = Activity()

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(text)
        addSubview(shine)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var frame: NSRect {
        didSet {
            text.frame = NSRect(x: 0, y: 3, width: bounds.width, height: 18)
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
        let verb = activity.thinking ? "Thinking" : "Working"
        guard let started = activity.startedAt else {
            show("\(verb)…")
            return
        }
        show("\(verb) for \(Time.elapsed(since: started))")
    }

    private func show(_ words: String) {
        text.stringValue = words
        shine.stringValue = words
    }
}
