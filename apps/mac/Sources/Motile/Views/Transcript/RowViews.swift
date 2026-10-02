import AppKit
import QuartzCore

/// What a row needs from the transcript around it.
protocol RowHost: AnyObject {
    func rowWillSelect(_ view: RowView)
    func rowToggledExpansion(id: String)
    /// Opens or closes a group or a fold, whose rows the core adds and removes.
    func toggleRow(id: String)
    func isExpanded(id: String) -> Bool
    func allow(_ denials: [Denial])
    func copyReply(endingAt rowID: String)
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

/// The base of every row. A row is as wide as the transcript's column; `layout(width:)` places
/// its parts for that width and says how tall it is.
class RowView: FlippedView {
    weak var host: RowHost?
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
            return estimatedTextHeight(text.length, width: width * 0.75) + 43 + (attachments.isEmpty ? 0 : 24)
        case .prose(let text, _):
            return estimatedTextHeight(text.length, width: width) + 9
        case .code(let content):
            return CodeRowView.height(lines: content.lineCount)
        case .tool, .thinking, .group, .fold:
            return ToolRowView.rowHeight
        case .error(let text):
            return estimatedTextHeight(text.length, width: width - 40) + 34
        case .turnEnd(let end):
            return end.denials.isEmpty ? 46 : 110 + CGFloat(end.denials.count) * 20
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
        case .error: return ErrorRowView()
        case .turnEnd: return TurnEndRowView()
        }
    }

    static func reuseKey(for row: RowModel) -> String {
        switch row.kind {
        case .user: return "user"
        case .prose: return "prose"
        case .code: return "code"
        case .tool, .thinking, .group, .fold: return "tool"
        case .error: return "error"
        case .turnEnd: return "turnEnd"
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
            self.host?.rowWillSelect(self)
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
        // The space under the last line isn't part of the text the bubble wraps.
        let textHeight = text.height(forWidth: textWidth) - Typesetter.plainLineSpacing
        let attachmentsHeight: CGFloat = hasAttachments ? 22 : 0
        let bubbleSize = NSSize(width: textWidth + padding * 2, height: textHeight + 20 + attachmentsHeight)
        bubble.frame = NSRect(x: width - bubbleSize.width, y: 14, width: bubbleSize.width, height: bubbleSize.height)
        text.frame = NSRect(x: padding, y: 10, width: textWidth, height: textHeight + Typesetter.plainLineSpacing)
        attachments.frame = NSRect(x: padding, y: 10 + textHeight + 4, width: textWidth, height: 16)
        return bubbleSize.height + 14 + 14
    }

    override func clearSelection() { text.clearSelection() }
}

final class ProseRowView: RowView {
    private let text = RowTextView.make()
    private var headers: [CodeHeader] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(text)
        text.onSelect = { [weak self] in
            guard let self else { return }
            self.host?.rowWillSelect(self)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .prose(let content, _) = row.kind else { return }
        text.content = content
    }

    override func layout(width: CGFloat) -> CGFloat {
        let before = text.frame.size
        let height = text.height(forWidth: width)
        text.frame = NSRect(x: 0, y: 3, width: width, height: height)
        placeHeaders()
        if fadesGrowth, before.width == width, before.height > 0, height > before.height + 1 {
            text.fadeIn(below: before.height)
        }
        fadesGrowth = false
        return height + 9
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
    static let height: CGFloat = 32

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
        language.frame = NSRect(x: 14, y: 9, width: max(0, frame.width - 60), height: 15)
        copyButton.frame = NSRect(x: frame.width - IconButton.side - 4, y: 2, width: IconButton.side, height: IconButton.side)
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
            self.host?.rowWillSelect(self)
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
    private let chevron = NSImageView()
    private let spinner = NSProgressIndicator()
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
        chevron.contentTintColor = Theme.tertiary
        chevron.imageScaling = .scaleNone
        header.addSubview(chevron)
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        header.addSubview(spinner)

        detailSurface.fill = Theme.codeBackground
        detailSurface.radius = 8
        detailSurface.isHidden = true
        addSubview(detailSurface)
        detailSurface.addSubview(detail)
        detail.onSelect = { [weak self] in
            guard let self else { return }
            self.host?.rowWillSelect(self)
        }
        header.onClick = { [weak self] in
            guard let self, self.hasDetail else { return }
            guard self.open == nil else {
                self.host?.toggleRow(id: self.rowID)
                return
            }
            self.host?.rowToggledExpansion(id: self.rowID)
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
            setTitle(tool.verb, target: tool.target, failed: tool.status == .failed)
            running = tool.status == .running
            hasDetail = tool.hasDetail
            detailText = { tool.detail() }
        case .thinking(let thought):
            icon.image = symbol("brain")
            setTitle("Thought")
            running = false
            hasDetail = thought.length > 0
            detailText = { thought }
        case .group(let group):
            icon.image = symbol(ToolContent.symbol(for: group.icon))
            setTitle(group.title, target: group.target, failed: group.failed)
            running = group.running
            hasDetail = true
            detailText = nil
            open = group.open
        case .fold(let fold):
            icon.image = symbol("clock")
            setTitle(fold.label)
            running = false
            hasDetail = true
            detailText = nil
            open = fold.open
        default:
            break
        }
        if running { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
    }

    private func setTitle(_ words: String, target: String = "", failed: Bool = false) {
        let text = NSMutableAttributedString(
            string: target.isEmpty ? words : words + " ",
            attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: Theme.secondary]
        )
        let targetColor = failed ? Theme.danger : Theme.prose
        text.append(NSAttributedString(string: target, attributes: [.font: Theme.inlineCodeFont, .foregroundColor: targetColor]))
        title.attributedStringValue = text
        title.lineBreakMode = .byTruncatingTail
    }

    override func layout(width: CGFloat) -> CGFloat {
        let expanded = open == nil && hasDetail && (host?.isExpanded(id: rowID) ?? false)
        // The rows of an open group stand in from the group's own.
        let inset: CGFloat = nested ? 24 : 0
        let width = width - inset
        header.frame = NSRect(x: inset - 6, y: 1, width: width + 12, height: Self.rowHeight - 2)
        icon.frame = NSRect(x: 6, y: 5, width: 16, height: 16)
        let titleWidth = min(title.intrinsicContentSize.width + 4, width - 60)
        title.frame = NSRect(x: 30, y: 4, width: titleWidth, height: 18)
        spinner.frame = NSRect(x: 30 + titleWidth + 6, y: 5, width: 16, height: 16)
        chevron.isHidden = !hasDetail
        chevron.image = symbol(open ?? expanded ? "chevron.down" : "chevron.right", size: 9, weight: .semibold)
        chevron.frame = NSRect(x: 30 + titleWidth + (running ? 26 : 2), y: 5, width: 14, height: 16)

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
            self.host?.rowWillSelect(self)
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
    private let summary = label(Theme.smallFont, Theme.tertiary)
    private var copyButton: IconButton!
    private let rule = SurfaceView()
    private let approval = SurfaceView()
    private let approvalTitle = label(NSFont.systemFont(ofSize: 12, weight: .semibold), Theme.warning)
    private let approvalList = RowTextView.make()
    private let allowButton = NSButton(title: "Allow and continue", target: nil, action: nil)
    private var denials: [Denial] = []
    private var folded = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(summary)
        copyButton = IconButton(symbolName: "doc.on.doc", tooltip: "Copy reply") { [weak self] in
            guard let self else { return }
            self.host?.copyReply(endingAt: self.rowID)
            self.copyButton.set(symbolName: "checkmark")
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
                self?.copyButton.set(symbolName: "doc.on.doc")
            }
        }
        addSubview(copyButton)
        rule.fill = Theme.border
        addSubview(rule)

        approval.fill = Theme.warningBackground
        approval.radius = 10
        addSubview(approval)
        approvalTitle.stringValue = "Waiting for your approval"
        approval.addSubview(approvalTitle)
        approval.addSubview(approvalList)
        allowButton.bezelStyle = .rounded
        allowButton.controlSize = .regular
        allowButton.target = self
        allowButton.action = #selector(allow)
        approval.addSubview(allowButton)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .turnEnd(let end) = row.kind else { return }
        summary.stringValue = end.label
        folded = end.folded
        summary.isHidden = folded
        denials = end.denials
        approval.isHidden = denials.isEmpty
        let lines = denials.map { "\($0.toolName)  \($0.summary)" }.joined(separator: "\n")
        approvalList.content = Typesetter.mono(lines, color: Theme.text)
    }

    override func layout(width: CGFloat) -> CGFloat {
        var y: CGFloat = 2
        if !denials.isEmpty {
            let inner = width - 28
            let listHeight = approvalList.height(forWidth: inner)
            approvalTitle.frame = NSRect(x: 14, y: 12, width: inner, height: 16)
            approvalList.frame = NSRect(x: 14, y: 34, width: inner, height: listHeight)
            allowButton.sizeToFit()
            allowButton.frame.origin = NSPoint(x: 12, y: 34 + listHeight + 10)
            let height = 34 + listHeight + 10 + allowButton.frame.height + 12
            approval.frame = NSRect(x: 0, y: y, width: width, height: height)
            y += height + 10
        }
        let summaryWidth = folded ? 0 : min(summary.intrinsicContentSize.width + 4, width - 40)
        summary.frame = NSRect(x: 0, y: y + 3, width: summaryWidth, height: 16)
        copyButton.frame = NSRect(x: folded ? -6 : summaryWidth + 4, y: y - 3, width: IconButton.side, height: IconButton.side)
        rule.frame = NSRect(x: 0, y: y + 30, width: width, height: 1)
        return y + 30 + 1 + 14
    }

    @objc private func allow() {
        host?.allow(denials)
    }

    override func clearSelection() { approvalList.clearSelection() }
}

/// Shown under the transcript while the agent is at work.
final class WorkingView: FlippedView {
    private let dot = SurfaceView()
    private let text = label(NSFont.systemFont(ofSize: 13), Theme.secondary)
    private var timer: Timer?
    private var activity = Activity()

    override init(frame: NSRect) {
        super.init(frame: frame)
        dot.fill = Theme.working
        dot.radius = 3.5
        dot.frame = NSRect(x: 2, y: 9, width: 7, height: 7)
        addSubview(dot)
        text.frame = NSRect(x: 18, y: 3, width: 300, height: 18)
        addSubview(text)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func update(_ activity: Activity) {
        self.activity = activity
        isHidden = !activity.running
        timer?.invalidate()
        timer = nil
        dot.layer?.removeAllAnimations()
        guard activity.running else { return }
        refresh()
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] _ in self?.refresh() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer

        let pulse = CABasicAnimation(keyPath: "opacity")
        pulse.fromValue = 1
        pulse.toValue = 0.25
        pulse.duration = 0.8
        pulse.autoreverses = true
        pulse.repeatCount = .infinity
        dot.layer?.add(pulse, forKey: "pulse")
    }

    private func refresh() {
        let verb = activity.thinking ? "Thinking" : "Working"
        guard let started = activity.startedAt else {
            text.stringValue = "\(verb)…"
            return
        }
        text.stringValue = "\(verb) for \(Time.elapsed(since: started))"
    }
}
