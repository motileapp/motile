import Foundation
import os

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// How wide the transcript's column is, for measuring rows on the thread that decodes them.
enum TranscriptColumn {
    private static let shared = OSAllocatedUnfairLock<CGFloat>(initialState: 0)

    static var width: CGFloat {
        get { shared.withLock { $0 } }
        set { shared.withLock { $0 = newValue } }
    }
}

/// One row of the transcript, decoded from the core's JSON. Its text is laid out for display
/// here, off the main thread, so showing a row costs the main thread nothing but drawing.
final class RowModel {
    enum Kind {
        case user(text: NSAttributedString, attachments: [AttachedFile], at: Double)
        /// `above` is the space it keeps from the row above it. `uncoloured` when code inside it
        /// still waits for highlighting.
        case prose(NSAttributedString, above: CGFloat, uncoloured: Bool)
        case code(CodeContent)
        case tool(ToolContent)
        case thinking(NSAttributedString)
        case media(MediaContent)
        case group(GroupContent)
        case fold(FoldContent)
        case error(NSAttributedString)
        case changes(ChangesContent)
        case turnEnd(TurnEnd)
        case queued(QueuedContent)
    }

    let id: String
    let itemID: String
    /// The row belongs to the group above it, which is open.
    let nested: Bool
    let kind: Kind
    /// How tall the row is in a column of that width. It is set while the row is decoded, and
    /// after that only on the main thread.
    var measured: (width: CGFloat, height: CGFloat)?

    init(id: String, itemID: String, kind: Kind) {
        self.id = id
        self.itemID = itemID
        nested = false
        self.kind = kind
    }

    init?(json: JSON) {
        id = json.string("id")
        itemID = json.string("item")
        nested = json.bool("nested")
        switch json.string("kind") {
        case "user":
            kind = .user(text: Typesetter.plain(json.string("text"), color: Theme.text), attachments: json.objects("attachments").map(AttachedFile.init(json:)), at: json.double("at"))
        case "prose":
            let uncoloured = json.objects("paras").contains { $0.string("kind") == "pre" && $0["spans"] as? [NSNumber] == nil }
            kind = .prose(Typesetter.prose(json), above: Typesetter.spaceAbove(json), uncoloured: uncoloured)
        case "code":
            kind = .code(CodeContent(language: json.string("language"), code: json.string("code"), spans: json["spans"] as? [NSNumber]))
        case "tool":
            kind = .tool(ToolContent(json: json))
        case "thinking":
            kind = .thinking(Typesetter.plain(json.string("text"), color: Theme.secondary, size: 13 * Platform.scale))
        case "media":
            kind = .media(MediaContent(json: json))
        case "group":
            kind = .group(GroupContent(json: json))
        case "fold":
            kind = .fold(FoldContent(json: json))
        case "error":
            kind = .error(Typesetter.plain(json.string("message"), color: Theme.danger, size: 13 * Platform.scale))
        case "changes":
            kind = .changes(ChangesContent(json: json))
        case "turn_end":
            kind = .turnEnd(TurnEnd(json: json))
        case "queued":
            kind = .queued(QueuedContent(json: json))
        default:
            return nil
        }
        let width = TranscriptColumn.width
        guard width > 0 else { return }
        measured = (width, RowView.height(self, width: width))
    }

    /// A user message shown the moment it is sent, before the server has it.
    static func pending(text: String, attachments: [AttachedFile]) -> RowModel {
        RowModel(id: "pending", itemID: "pending", kind: .user(text: Typesetter.plain(text, color: Theme.text), attachments: attachments, at: Date().timeIntervalSince1970))
    }

    /// The plain text of the row, for copying a whole reply.
    var plainText: String? {
        switch kind {
        case .prose(let text, _, _): TextSystem.withLineBreaks(Typesetter.words(of: text))
        case .code(let content): "```\(content.language)\n\(content.code)\n```"
        default: nil
        }
    }

    /// The row has code that came without highlighting.
    var needsHighlight: Bool {
        switch kind {
        case .code(let content): !content.highlighted
        case .prose(_, _, let uncoloured): uncoloured
        default: false
        }
    }

    var isUser: Bool {
        if case .user = kind { return true }
        return false
    }

    var isQueued: Bool {
        if case .queued = kind { return true }
        return false
    }

    /// The row is a message the server has: one in the transcript, or one that waits for the agent.
    var isSentMessage: Bool { isUser || isQueued }
}

final class CodeContent {
    let language: String
    let code: String
    let lineCount: Int
    private(set) var highlighted: Bool
    private(set) var attributed: NSAttributedString

    init(language: String, code: String, spans: [NSNumber]?) {
        self.language = language
        self.code = code
        lineCount = code.isEmpty ? 1 : code.reduce(1) { $1 == "\n" ? $0 + 1 : $0 }
        highlighted = spans != nil
        attributed = Typesetter.code(code, spans: spans ?? [])
    }

    func apply(spans: [NSNumber]) {
        highlighted = true
        attributed = Typesetter.code(code, spans: spans)
    }
}

struct ToolContent {
    enum Status: String {
        case running, succeeded, failed
    }

    let name: String
    let icon: String
    let verb: String
    let target: String
    let status: Status
    let input: String
    let inputLanguage: String
    let output: String?
    /// When it started, while it runs.
    let startedAt: Double?
    /// It started an agent, whose transcript the row's item names.
    let agent: Bool
    /// What that agent is doing now.
    let progress: String?

    init(json: JSON) {
        name = json.string("name")
        icon = json.string("icon")
        verb = json.string("verb")
        target = json.string("target")
        status = Status(rawValue: json.string("status")) ?? .succeeded
        input = json.string("input")
        inputLanguage = json.string("input_language")
        output = json.optionalString("output")
        startedAt = json.optionalDouble("started_at")
        agent = json.bool("agent")
        progress = json.optionalString("progress")
    }

    var symbol: Symbol { Self.symbol(for: icon) }

    static func symbol(for icon: String) -> Symbol {
        switch icon {
        case "terminal": .terminal
        case "file": .fileText
        case "edit": .pencil
        case "search": .search
        case "web": .globe
        case "agent": .users
        case "watch": .eye
        case "question": .messageCircleQuestionMark
        case "todo": .listChecks
        default: .wrench
        }
    }

    var hasDetail: Bool { !input.isEmpty || output != nil }

    /// What opening the row shows: the input, then what came back.
    func detail() -> NSAttributedString {
        let result = NSMutableAttributedString()
        if !input.isEmpty {
            result.append(inputLanguage == "diff" ? Typesetter.diff(input) : Typesetter.mono(input, color: Theme.text))
        }
        if let output, !output.isEmpty {
            if result.length > 0 { result.append(Typesetter.mono("\n\n", color: Theme.secondary)) }
            result.append(Typesetter.mono(Self.clipped(output), color: status == .failed ? Theme.danger : Theme.secondary))
        }
        return result
    }

    /// The first lines of long output; the rest is in the transcript on the server.
    private static func clipped(_ output: String) -> String {
        let lines = output.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count > 60 else { return output }
        return lines.prefix(60).joined(separator: "\n") + "\n… \(lines.count - 60) more lines"
    }
}

/// Tool calls that followed one another, as one row that opens into them.
struct GroupContent {
    let title: String
    let target: String
    let icon: String
    let running: Bool
    let failed: Bool
    let open: Bool
    /// When the latest of them that still runs started.
    let startedAt: Double?

    init(json: JSON) {
        startedAt = json.optionalDouble("started_at")
        title = json.string("title")
        target = json.string("target")
        icon = json.string("icon")
        running = json.bool("running")
        failed = json.bool("failed")
        open = json.bool("open")
    }
}

/// Stands for what a finished turn did before its last message, and opens into it.
struct FoldContent {
    let label: String
    let open: Bool

    init(json: JSON) {
        label = TurnEnd.label(stopped: json.bool("stopped"), durationMs: (json["duration_ms"] as? NSNumber)?.intValue)
        open = json.bool("open")
    }
}

/// What a finished turn changed in the thread's folder: its files under their folders. The row's
/// item is the one that ends the turn.
struct ChangesContent {
    struct Entry {
        /// What opens and closes a folder.
        let id: String
        let path: String
        let depth: Int
        let folder: Bool
        let open: Bool
        let name: NSAttributedString
        let counts: NSAttributedString
    }

    let files: Int
    let at: Double
    let title: NSAttributedString
    let entries: [Entry]

    private static let nameStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingMiddle
        return style
    }()

    init(json: JSON) {
        files = json.int("files")
        at = json.double("at")
        let title = NSMutableAttributedString(
            string: files == 1 ? "1 changed file" : "\(files) changed files",
            attributes: [.font: PlatformFont.ui(13, weight: .medium), .foregroundColor: Theme.text])
        title.append(NSAttributedString(string: "   "))
        title.append(LineCountText.text(added: json.int("added"), removed: json.int("removed")))
        self.title = title
        entries = json.objects("entries").map { entry in
            let folder = entry.bool("folder")
            let name = NSAttributedString(
                string: entry.string("name"),
                attributes: [
                    .font: Theme.smallMono, .foregroundColor: folder ? Theme.secondary : Theme.text, .paragraphStyle: Self.nameStyle,
                ])
            return Entry(
                id: entry.string("id"), path: entry.string("path"), depth: entry.int("depth"), folder: folder, open: entry.bool("open"),
                name: name, counts: LineCountText.text(added: entry.int("added"), removed: entry.int("removed")))
        }
    }
}

/// A message that waits for the agent to take it. The row's item is the message.
struct QueuedContent {
    let text: NSAttributedString
    let attachments: [AttachedFile]
    /// How it waits: queued, held, or being given to the agent.
    let status: String
    /// The agent is being given it, so it can no longer be sent now or taken back.
    let sending: Bool

    init(json: JSON) {
        text = Typesetter.plain(json.string("text"), color: Theme.prose)
        attachments = json.objects("attachments").map(AttachedFile.init(json:))
        status = json.string("status")
        sending = json.bool("sending")
    }
}

struct TurnEnd {
    let durationMs: Int?
    let costUSD: Double?
    let isError: Bool
    let stopped: Bool
    /// The turn's fold says how long it took, so the end of the turn doesn't.
    let folded: Bool
    let at: Double

    init(json: JSON) {
        durationMs = (json["duration_ms"] as? NSNumber)?.intValue
        costUSD = json.optionalDouble("cost_usd")
        isError = json.bool("is_error")
        stopped = json.bool("stopped")
        folded = json.bool("folded")
        at = json.double("at")
    }

    /// When the turn ended and, unless its fold says so, how long it took.
    var stamp: String {
        guard !folded, stopped || durationMs != nil else { return Time.stamp(at) }
        return "\(Time.stamp(at)) · \(label)"
    }

    var label: String { Self.label(stopped: stopped, durationMs: durationMs) }

    static func label(stopped: Bool, durationMs: Int?) -> String {
        let duration = durationMs.map { Time.duration(milliseconds: $0) }
        switch (stopped, duration) {
        case (true, let duration?): return "You stopped after \(duration)"
        case (true, nil): return "You stopped this response"
        case (false, let duration?): return "Worked for \(duration)"
        case (false, nil): return "Done"
        }
    }
}

extension NSAttributedString.Key {
    /// How many quote bars to draw beside the paragraph.
    static let motileQuote = NSAttributedString.Key("motile.quote")
    /// The paragraph is a horizontal rule.
    static let motileRule = NSAttributedString.Key("motile.rule")
    /// The paragraph is a code block inside a list or a quote; the value is its `CodeBox`.
    static let motileCode = NSAttributedString.Key("motile.code")
}

/// Turns the core's rows into attributed strings. Safe to call from any thread.
enum Typesetter {
    static let inlineCodeBackground = Theme.dynamic(Theme.hex(0x000000, alpha: 0.06), Theme.hex(0xffffff, alpha: 0.1))

    private static let italicFont = Theme.proseFont.italicised
    private static let boldItalicFont = Theme.proseBold.italicised
    private static let inlineCodeBold = PlatformFont.uiMono(12.5, weight: .semibold)

    /// What a paragraph, a list, a quote and a table keep from what follows them.
    private static let blockGap: CGFloat = 12
    private static let headingGap: CGFloat = 14

    private static let bodyStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 7
        style.paragraphSpacing = blockGap
        return style
    }()

    /// A reply is cut into rows, and its rows are as far apart as its paragraphs.
    static func spaceAbove(_ json: JSON) -> CGFloat {
        let after = json.string("after")
        guard !after.isEmpty else { return 0 }
        let heading = json.objects("paras").first?.string("kind") == "heading" ? headingGap : 0
        switch after {
        case "prose": return bodyStyle.lineSpacing + blockGap - ProseRowView.gap + heading
        case "table": return blockGap - ProseRowView.gap + heading
        default: return heading
        }
    }

    /// The list, quote or table a paragraph is part of.
    private static func block(_ para: JSON) -> String {
        switch para.string("kind") {
        case "list_item": para.int("quote") > 0 ? "quote" : "list"
        case "quote": "quote"
        case "cell": "table \(para.int("table"))"
        default: ""
        }
    }

    private static let monoStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = Theme.codeLineHeight
        style.maximumLineHeight = Theme.codeLineHeight
        style.lineBreakMode = .byCharWrapping
        return style
    }()

    private static let codeStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = Theme.codeLineHeight
        style.maximumLineHeight = Theme.codeLineHeight
        style.lineBreakMode = .byClipping
        // Wide enough that code indented with tabs keeps its columns.
        style.defaultTabInterval = 4 * 7.5
        style.tabStops = []
        return style
    }()

    static func plain(_ text: String, color: PlatformColor, size: CGFloat = Theme.proseSize) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 5
        style.paragraphSpacing = 6
        return NSAttributedString(
            string: text,
            attributes: [.font: PlatformFont.systemFont(ofSize: size), .foregroundColor: color, .paragraphStyle: style]
        )
    }

    static func mono(_ text: String, color: PlatformColor) -> NSAttributedString {
        NSAttributedString(string: text, attributes: [.font: Theme.smallMono, .foregroundColor: color, .paragraphStyle: monoStyle])
    }

    /// Removed lines in red and added lines in green.
    static func diff(_ text: String) -> NSAttributedString {
        let result = NSMutableAttributedString()
        for (index, line) in text.split(separator: "\n", omittingEmptySubsequences: false).enumerated() {
            let color = line.hasPrefix("+") ? Theme.syntax[10] : line.hasPrefix("-") ? Theme.syntax[11] : Theme.text
            result.append(mono((index == 0 ? "" : "\n") + line, color: color))
        }
        return result
    }

    static func code(_ code: String, spans: [NSNumber]) -> NSAttributedString {
        let result = NSMutableAttributedString(
            string: code,
            attributes: [.font: Theme.codeFont, .foregroundColor: Theme.text, .paragraphStyle: codeStyle]
        )
        colour(result, spans: spans, in: NSRange(location: 0, length: result.length))
        return result
    }

    /// Colours the code in `range` with its spans, which count from the start of the range.
    private static func colour(_ text: NSMutableAttributedString, spans: [NSNumber], in range: NSRange) {
        var index = 0
        while index + 2 < spans.count {
            let span = NSRange(location: range.location + spans[index].intValue, length: spans[index + 1].intValue)
            let color = spans[index + 2].intValue
            index += 3
            guard NSMaxRange(span) <= NSMaxRange(range), color > 0, color < Theme.syntax.count else { continue }
            text.addAttribute(.foregroundColor, value: Theme.syntax[color], range: span)
        }
    }

    static func prose(_ json: JSON) -> NSAttributedString {
        let result = NSMutableAttributedString(
            string: json.string("text"),
            attributes: [.font: Theme.proseFont, .foregroundColor: Theme.prose, .paragraphStyle: bodyStyle]
        )
        let length = result.length
        func clamp(_ start: Int, _ len: Int) -> NSRange? {
            guard start >= 0, len > 0, start + len <= length else { return nil }
            return NSRange(location: start, length: len)
        }

        var tables = TableSetter()
        var codeBlocks = 0
        var headingRanges: [(NSRange, PlatformFont)] = []
        for para in json.objects("paras") {
            // The last paragraph has no line break; its style still has to reach the end.
            guard let range = clamp(para.int("start"), para.int("len")) else { continue }
            let kind = para.string("kind")
            switch kind {
            case "heading":
                let font = Theme.heading(para.int("level"))
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 3
                style.paragraphSpacingBefore = range.location == 0 ? 0 : headingGap
                style.paragraphSpacing = 8
                result.addAttributes([.font: font, .foregroundColor: Theme.text, .paragraphStyle: style], range: range)
                headingRanges.append((range, font))
            case "list_item":
                let depth = CGFloat(max(1, para.int("depth")))
                let quote = CGFloat(para.int("quote")) * 14
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 7
                style.paragraphSpacing = 6
                style.headIndent = quote + depth * 22
                style.firstLineHeadIndent = para.bool("marker") ? quote + (depth - 1) * 22 + 4 : style.headIndent
                style.tabStops = [NSTextTab(textAlignment: .left, location: style.headIndent)]
                style.defaultTabInterval = 22
                result.addAttribute(.paragraphStyle, value: style, range: range)
                if para.int("quote") > 0 {
                    result.addAttribute(.motileQuote, value: NSNumber(value: para.int("quote")), range: range)
                }
            case "quote":
                let depth = para.int("depth")
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 7
                style.paragraphSpacing = 8
                style.headIndent = CGFloat(depth) * 14
                style.firstLineHeadIndent = style.headIndent
                result.addAttributes(
                    [.paragraphStyle: style, .foregroundColor: Theme.secondary, .motileQuote: NSNumber(value: depth)],
                    range: range
                )
            case "pre":
                let indent = CGFloat(para.int("depth")) * 22
                let style = NSMutableParagraphStyle()
                style.minimumLineHeight = Theme.codeLineHeight
                style.maximumLineHeight = Theme.codeLineHeight
                style.lineBreakMode = .byCharWrapping
                style.defaultTabInterval = 4 * 7.5
                style.tabStops = []
                let block = CodeBoxes.make(language: para.string("language"), index: codeBlocks, indent: indent, style: style)
                codeBlocks += 1
                result.addAttributes(
                    [.paragraphStyle: style, .font: Theme.codeFont, .foregroundColor: Theme.text, .motileCode: block],
                    range: range
                )
                colour(result, spans: para["spans"] as? [NSNumber] ?? [], in: range)
            case "rule":
                let style = NSMutableParagraphStyle()
                style.paragraphSpacing = 10
                style.paragraphSpacingBefore = 4
                result.addAttributes([.paragraphStyle: style, .motileRule: NSNumber(value: true)], range: range)
            case "cell":
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 3
                style.alignment = [NSTextAlignment.left, .center, .right][min(2, max(0, para.int("align")))]
                tables.set(para, style: style)
                result.addAttributes([.paragraphStyle: style, .font: PlatformFont.ui(13)], range: range)
                if para.bool("header") {
                    let bold = PlatformFont.ui(13, weight: .semibold)
                    result.addAttributes([.font: bold, .foregroundColor: Theme.text], range: range)
                    headingRanges.append((range, bold))
                }
            default:
                break
            }
        }

        // A list, a quote and a table only keep their own paragraphs apart; what follows one
        // brings the rest of the gap.
        let paras = json.objects("paras")
        for (index, para) in paras.enumerated().dropFirst() where para.string("kind") != "pre" {
            let ended = block(paras[index - 1])
            guard !ended.isEmpty, ended != block(para),
                let range = clamp(para.int("start"), para.int("len")),
                let last = clamp(paras[index - 1].int("start"), paras[index - 1].int("len")),
                let above = result.attribute(.paragraphStyle, at: last.location, effectiveRange: nil) as? NSParagraphStyle,
                let current = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle,
                let style = current.mutableCopy() as? NSMutableParagraphStyle
            else { continue }
            style.paragraphSpacingBefore += blockGap - above.paragraphSpacing
            result.addAttribute(.paragraphStyle, value: style, range: range)
        }

        for run in json["runs"] as? [[NSNumber]] ?? [] {
            guard run.count == 3, let range = clamp(run[0].intValue, run[1].intValue) else { continue }
            let style = run[2].intValue
            let (bold, italic, code) = (style & 1 != 0, style & 2 != 0, style & 4 != 0)
            let heading = headingRanges.first { NSLocationInRange(range.location, $0.0) }?.1
            if code {
                let font = bold || heading != nil ? inlineCodeBold : Theme.inlineCodeFont
                result.addAttributes([.font: font, .foregroundColor: Theme.text, .backgroundColor: inlineCodeBackground], range: range)
            } else if let heading {
                if italic { result.addAttribute(.font, value: heading.italicised, range: range) }
            } else if bold || italic {
                let font = bold && italic ? boldItalicFont : bold ? Theme.proseBold : italicFont
                result.addAttribute(.font, value: font, range: range)
                if bold { result.addAttribute(.foregroundColor, value: Theme.text, range: range) }
            }
            if style & 8 != 0 {
                result.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
        }

        for link in json.objects("links") {
            guard let range = clamp(link.int("start"), link.int("len")), let url = URL(string: link.string("url")) else { continue }
            result.addAttributes([.link: url, .foregroundColor: Theme.link], range: range)
        }
        return tables.finished(result, paras: paras)
    }

    /// The text as it is copied: what a table stands for instead of what holds its place.
    static func words(of text: NSAttributedString) -> String {
        #if os(macOS)
        return text.string
        #else
        return TableSetter.words(of: text)
        #endif
    }
}

#if os(macOS)
/// Sets the cells of a table as the Mac's text system lays them out, in the text itself.
struct TableSetter {
    private var tables: [Int: NSTextTable] = [:]

    mutating func set(_ para: JSON, style: NSMutableParagraphStyle) {
        let columns = max(1, para.int("columns"))
        let table = tables[para.int("table")] ?? {
            let table = NSTextTable()
            table.numberOfColumns = columns
            table.collapsesBorders = true
            table.hidesEmptyCells = false
            table.layoutAlgorithm = .automaticLayoutAlgorithm
            tables[para.int("table")] = table
            return table
        }()
        let block = NSTextTableBlock(
            table: table,
            startingRow: para.int("row"),
            rowSpan: 1,
            startingColumn: para.int("column"),
            columnSpan: 1
        )
        block.setWidth(6, type: .absoluteValueType, for: .padding)
        block.setWidth(10, type: .absoluteValueType, for: .padding, edge: .minX)
        block.setWidth(10, type: .absoluteValueType, for: .padding, edge: .maxX)
        block.setWidth(1, type: .absoluteValueType, for: .border, edge: .maxY)
        block.setBorderColor(Theme.border)
        style.textBlocks = [block]
    }

    func finished(_ text: NSMutableAttributedString, paras: [JSON]) -> NSAttributedString { text }
}
#endif
