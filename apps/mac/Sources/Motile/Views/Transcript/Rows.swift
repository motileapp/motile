import AppKit

/// One row of the transcript, decoded from the core's JSON. Its text is laid out for display
/// here, off the main thread, so showing a row costs the main thread nothing but drawing.
final class RowModel {
    enum Kind {
        case user(text: NSAttributedString, attachments: [String])
        /// `uncoloured` when code inside it still waits for highlighting.
        case prose(NSAttributedString, uncoloured: Bool)
        case code(CodeContent)
        case tool(ToolContent)
        case thinking(NSAttributedString)
        case group(GroupContent)
        case fold(FoldContent)
        case error(NSAttributedString)
        case turnEnd(TurnEnd)
    }

    let id: String
    let itemID: String
    /// The row belongs to the group above it, which is open.
    let nested: Bool
    let kind: Kind

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
            kind = .user(text: Typesetter.plain(json.string("text"), color: Theme.text), attachments: json.strings("attachments"))
        case "prose":
            let uncoloured = json.objects("paras").contains { $0.string("kind") == "pre" && $0["spans"] as? [NSNumber] == nil }
            kind = .prose(Typesetter.prose(json), uncoloured: uncoloured)
        case "code":
            kind = .code(CodeContent(language: json.string("language"), code: json.string("code"), spans: json["spans"] as? [NSNumber]))
        case "tool":
            kind = .tool(ToolContent(json: json))
        case "thinking":
            kind = .thinking(Typesetter.plain(json.string("text"), color: Theme.secondary, size: 13))
        case "group":
            kind = .group(GroupContent(json: json))
        case "fold":
            kind = .fold(FoldContent(json: json))
        case "error":
            kind = .error(Typesetter.plain(json.string("message"), color: Theme.danger, size: 13))
        case "turn_end":
            kind = .turnEnd(TurnEnd(json: json))
        default:
            return nil
        }
    }

    /// A user message shown the moment it is sent, before the host has it.
    static func pending(text: String) -> RowModel {
        RowModel(id: "pending", itemID: "pending", kind: .user(text: Typesetter.plain(text, color: Theme.text), attachments: []))
    }

    /// The plain text of the row, for copying a whole reply.
    var plainText: String? {
        switch kind {
        case .prose(let text, _): RowTextView.withLineBreaks(text.string)
        case .code(let content): "```\(content.language)\n\(content.code)\n```"
        default: nil
        }
    }

    /// The row has code that came without highlighting.
    var needsHighlight: Bool {
        switch kind {
        case .code(let content): !content.highlighted
        case .prose(_, let uncoloured): uncoloured
        default: false
        }
    }

    var isUser: Bool {
        if case .user = kind { return true }
        return false
    }
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

    init(json: JSON) {
        name = json.string("name")
        icon = json.string("icon")
        verb = json.string("verb")
        target = json.string("target")
        status = Status(rawValue: json.string("status")) ?? .succeeded
        input = json.string("input")
        inputLanguage = json.string("input_language")
        output = json.optionalString("output")
    }

    var symbol: String { Self.symbol(for: icon) }

    static func symbol(for icon: String) -> String {
        switch icon {
        case "terminal": "terminal"
        case "file": "doc.text"
        case "edit": "pencil"
        case "search": "magnifyingglass"
        case "web": "globe"
        case "agent": "person.2"
        case "todo": "checklist"
        default: "wrench.and.screwdriver"
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

    /// The first lines of long output; the rest is in the transcript on the host.
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

    init(json: JSON) {
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

struct TurnEnd {
    let durationMs: Int?
    let costUSD: Double?
    let isError: Bool
    let stopped: Bool
    let denials: [Denial]
    /// The turn's fold says how long it took, so the end of the turn doesn't.
    let folded: Bool

    init(json: JSON) {
        durationMs = (json["duration_ms"] as? NSNumber)?.intValue
        costUSD = json.optionalDouble("cost_usd")
        isError = json.bool("is_error")
        stopped = json.bool("stopped")
        denials = json.objects("denials").map { Denial(json: $0) }
        folded = json.bool("folded")
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
    /// The paragraph is a code block inside a list or a quote; the value is its `CodeTextBlock`.
    static let motileCode = NSAttributedString.Key("motile.code")
}

/// Turns the core's rows into attributed strings. Safe to call from any thread.
enum Typesetter {
    static let inlineCodeBackground = Theme.dynamic(Theme.hex(0x000000, alpha: 0.06), Theme.hex(0xffffff, alpha: 0.1))

    private static let italicFont = italic(Theme.proseFont)
    private static let boldItalicFont = italic(Theme.proseBold)
    private static let inlineCodeBold = NSFont.monospacedSystemFont(ofSize: 12.5, weight: .semibold)

    private static func italic(_ font: NSFont) -> NSFont {
        let descriptor = font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(.italic))
        return NSFont(descriptor: descriptor, size: font.pointSize) ?? font
    }

    private static let bodyStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 7
        style.paragraphSpacing = 12
        return style
    }()

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

    static let plainLineSpacing: CGFloat = 5

    static func plain(_ text: String, color: NSColor, size: CGFloat = Theme.proseSize) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = plainLineSpacing
        style.paragraphSpacing = 6
        return NSAttributedString(
            string: text,
            attributes: [.font: NSFont.systemFont(ofSize: size), .foregroundColor: color, .paragraphStyle: style]
        )
    }

    static func mono(_ text: String, color: NSColor) -> NSAttributedString {
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

        var tables: [Int: NSTextTable] = [:]
        var codeBlocks = 0
        var headingRanges: [(NSRange, NSFont)] = []
        for para in json.objects("paras") {
            // The last paragraph has no line break; its style still has to reach the end.
            guard let range = clamp(para.int("start"), para.int("len")) else { continue }
            let kind = para.string("kind")
            switch kind {
            case "heading":
                let font = Theme.heading(para.int("level"))
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 3
                style.paragraphSpacingBefore = range.location == 0 ? 0 : 14
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
                let block = CodeTextBlock(language: para.string("language"), index: codeBlocks, indent: CGFloat(para.int("depth")) * 22)
                codeBlocks += 1
                let style = NSMutableParagraphStyle()
                style.minimumLineHeight = Theme.codeLineHeight
                style.maximumLineHeight = Theme.codeLineHeight
                style.lineBreakMode = .byCharWrapping
                style.defaultTabInterval = 4 * 7.5
                style.tabStops = []
                style.textBlocks = [block]
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
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 3
                style.textBlocks = [block]
                style.alignment = [NSTextAlignment.left, .center, .right][min(2, max(0, para.int("align")))]
                result.addAttributes([.paragraphStyle: style, .font: NSFont.systemFont(ofSize: 13)], range: range)
                if para.bool("header") {
                    let bold = NSFont.systemFont(ofSize: 13, weight: .semibold)
                    result.addAttributes([.font: bold, .foregroundColor: Theme.text], range: range)
                    headingRanges.append((range, bold))
                }
            default:
                break
            }
        }

        // A table's last row has no spacing of its own; what follows it brings the gap.
        let paras = json.objects("paras")
        for (index, para) in paras.enumerated().dropFirst() where para.string("kind") != "cell" && paras[index - 1].string("kind") == "cell" {
            guard let range = clamp(para.int("start"), para.int("len")),
                let current = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle,
                let style = current.mutableCopy() as? NSMutableParagraphStyle
            else { continue }
            style.paragraphSpacingBefore = 12
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
                if italic { result.addAttribute(.font, value: self.italic(heading), range: range) }
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
        return result
    }
}
