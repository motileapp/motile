import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A list's marker lane and its markers.
enum ProgramList {
    static let itemGap: CGFloat = 10
    static let descriptionGap: CGFloat = 2
    static let childrenGap: CGFloat = 8
    static let detailRoom: CGFloat = 4
    static let timelineGap: CGFloat = 18
    static let dot: CGFloat = 5
    static let ring: CGFloat = 9
    static let number: CGFloat = 24
    static let chip: CGFloat = 20

    struct Lane {
        let list: UiListMarker
        let wide: Bool
        let timeline: Bool
        let narrow: Bool

        /// How wide the lane is and how far the text keeps from it, for an item's marker.
        func room(for marker: UiItemMarker) -> (lane: CGFloat, gap: CGFloat) {
            if wide { return (narrow ? 64 : 76, timeline ? (narrow ? 20 : 24) : (narrow ? 20 : 16)) }
            if case .chip = marker { return (24, 12) }
            switch list {
            case .bullet: return timeline ? (24, 12) : (14, 8)
            case .number: return (24, 12)
            case .plain: return (0, 0)
            }
        }
    }

    enum Kind {
        case none
        case dot
        /// A timeline's bullet: a circle in the surface's colour ringed in the text's.
        case ring
        case number
        case chip
        case label
    }

    struct Marker {
        var frame: CGRect
        var text: ProgramTextPart?
        let kind: Kind
    }

    static func marker(_ marker: UiItemMarker, lane: Lane, laneWidth: CGFloat, firstLine: CGFloat, env: ProgramEnv) -> Marker {
        let centered = { (side: CGFloat, x: CGFloat) in CGRect(x: x, y: ((firstLine - side) / 2).rounded(), width: side, height: side) }
        let onSurface = env.surface.platform
        switch marker {
        case .blank:
            guard lane.wide, lane.timeline else { return Marker(frame: .zero, kind: .none) }
            return Marker(frame: wideRing(lane: laneWidth, narrow: lane.narrow, firstLine: firstLine), kind: .ring)
        case .dot:
            guard lane.timeline else { return Marker(frame: centered(dot, ((laneWidth - dot) / 2).rounded()), kind: .dot) }
            return Marker(frame: centered(ring, 12 - ring / 2), kind: .ring)
        case .number(let number):
            let look = ProgramStyle.Look(font: .uiDigits(13), lineHeight: numberLine)
            let text = ProgramText.plain("\(number.number)", look: look, color: onSurface, options: .init(alignment: .center))
            let frame = centered(Self.number, 0)
            let part = ProgramTextPart(
                text: text, frame: CGRect(x: frame.minX, y: frame.midY - look.lineHeight / 2, width: frame.width, height: look.lineHeight),
                lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
            return Marker(frame: frame, text: part, kind: .number)
        case .chip(let chip):
            let look = ProgramStyle.Look(font: .uiDigits(12, weight: .medium), lineHeight: scaled(16))
            let text = ProgramText.plain(chip.text, look: look, color: onSurface, options: .init(alignment: .center))
            let width = max(Self.chip, ceil(ProgramText.width(text)) + 10)
            let frame = CGRect(x: max(0, ((laneWidth - width) / 2).rounded()), y: ((firstLine - Self.chip) / 2).rounded(), width: width, height: Self.chip)
            let part = ProgramTextPart(
                text: text, frame: CGRect(x: frame.minX, y: frame.midY - look.lineHeight / 2, width: width, height: look.lineHeight),
                lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
            return Marker(frame: frame, text: part, kind: .chip)
        case .label(let label):
            let look = ProgramStyle.Look(font: .uiDigits(13), lineHeight: scaled(20))
            let text = ProgramText.plain(label.text, look: look, color: Theme.mutedForeground, options: .init(oneLine: true))
            let frame = CGRect(x: 0, y: ((firstLine - look.lineHeight) / 2).rounded(), width: laneWidth, height: look.lineHeight)
            let ring = lane.timeline ? wideRing(lane: laneWidth, narrow: lane.narrow, firstLine: firstLine) : .zero
            return Marker(
                frame: ring, text: ProgramTextPart(text: text, frame: frame, lift: ControlText.lift(look.font, lineHeight: look.lineHeight), oneLine: true),
                kind: lane.timeline ? .ring : .label)
        }
    }

    private static let numberLine = scaled(18)

    private static func wideRing(lane: CGFloat, narrow: Bool, firstLine: CGFloat) -> CGRect {
        let size = Theme.proseSize / Platform.scale
        return CGRect(x: lane + (narrow ? 6 : 4), y: (0.55 * size).rounded(), width: ring, height: ring)
    }
}

/// Components inside a run of text: each an attachment as large as its box, which the text's view
/// lays the component's view over.
enum ProgramInline {
    private static let blank: PlatformImage = {
        #if os(macOS)
        NSImage(size: NSSize(width: 1, height: 1))
        #else
        UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1)).image { _ in }
        #endif
    }()

    static func attach(_ boxes: [ProgramBox], to text: NSAttributedString, size: CGFloat, lineHeight: CGFloat) -> NSAttributedString {
        let result = NSMutableAttributedString(attributedString: text)
        let whole = NSRange(location: 0, length: result.length)
        if let style = result.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle,
            let grown = style.mutableCopy() as? NSMutableParagraphStyle
        {
            grown.maximumLineHeight = 0
            result.addAttribute(.paragraphStyle, value: grown, range: whole)
        }
        let points = size / Platform.scale
        var index = 0
        for (offset, unit) in (result.string as NSString).enumeratedUnits() where unit == 0xFFFC && index < boxes.count {
            let box = boxes[index]
            index += 1
            let attachment = NSTextAttachment()
            attachment.image = blank
            var lowered = -0.15 * points
            if case .badge = box.node.kind { lowered += 0.1 * points }
            attachment.bounds = CGRect(x: 0, y: lowered, width: box.size.width + 2, height: box.size.height)
            result.addAttribute(.attachment, value: attachment, range: NSRange(location: offset, length: 1))
        }
        return result
    }

    /// Where each attachment's component goes in the text wrapped at `width`.
    static func places(in text: NSAttributedString, width: CGFloat, count: Int) -> [CGPoint] {
        let (storage, layout, _) = stack(text, width: width)
        var places: [CGPoint] = []
        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let attachment = value as? NSTextAttachment else { return }
            let glyph = layout.glyphIndexForCharacter(at: range.location)
            let line = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            let location = layout.location(forGlyphAt: glyph)
            let top = line.minY + location.y - attachment.bounds.maxY
            places.append(CGPoint(x: line.minX + location.x + 1, y: top.rounded()))
        }
        return Array(places.prefix(count))
    }

    /// Where the last line of the text ends, wrapped at `width`.
    static func lastLineEnd(of text: NSAttributedString, width: CGFloat) -> CGFloat {
        guard text.length > 0 else { return 0 }
        let single = ProgramText.width(text)
        guard single > width else { return single }
        let (_, layout, _) = stack(text, width: width)
        let last = layout.glyphIndexForCharacter(at: text.length - 1)
        return ceil(layout.lineFragmentUsedRect(forGlyphAt: last, effectiveRange: nil).maxX)
    }

    private static func stack(_ text: NSAttributedString, width: CGFloat) -> (NSTextStorage, NSLayoutManager, NSTextContainer) {
        let storage = NSTextStorage(attributedString: text)
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: CGSize(width: max(1, width), height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        layout.ensureLayout(for: container)
        return (storage, layout, container)
    }
}

extension NSString {
    /// Each UTF-16 unit with its offset.
    func enumeratedUnits() -> [(Int, unichar)] {
        (0..<length).map { ($0, character(at: $0)) }
    }
}

/// The fields of inputs: a label over the control, an error under it.
enum ProgramFields {
    static func labelPart(_ label: String, width: CGFloat, env: ProgramEnv) -> ProgramTextPart {
        let look = ProgramStyle.look(.label, narrow: env.narrow)
        let text = ProgramText.plain(label, look: look, color: Theme.foreground)
        let height = ProgramText.height(text, width: width, lineHeight: look.lineHeight)
        return ProgramTextPart(text: text, frame: CGRect(x: 0, y: 0, width: width, height: height), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
    }

    static func errorPart(_ error: String, at y: CGFloat, width: CGFloat, env: ProgramEnv) -> ProgramTextPart {
        let look = ProgramStyle.look(.caption, narrow: env.narrow)
        let text = ProgramText.plain(error, look: look, color: Theme.destructive)
        let height = ProgramText.height(text, width: width, lineHeight: look.lineHeight)
        return ProgramTextPart(text: text, frame: CGRect(x: 0, y: y, width: width, height: height), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
    }

    /// A control `control` tall, with its label 8 above it and its error 4 under it.
    static func field(_ node: UiNode, label: String?, error: String?, control: CGFloat, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        var texts: [String: ProgramTextPart] = [:]
        var y: CGFloat = 0
        if let label, !label.isEmpty {
            let part = labelPart(label, width: width, env: env)
            texts["label"] = part
            y = part.frame.height + ProgramStyle.labelGap
        }
        let rect = CGRect(x: 0, y: y, width: width, height: control)
        y += control
        if let error, !error.isEmpty {
            let part = errorPart(error, at: y + ProgramStyle.errorGap, width: width, env: env)
            texts["error"] = part
            y = part.frame.maxY
        }
        return ProgramBox(node, size: CGSize(width: width, height: y), surface: env.surface, texts: texts, rects: ["control": rect])
    }

    static func slider(_ node: UiNode, _ slider: UiKind.Slider, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let label = slider.head ? slider.label.map(SliderView.label) : nil
        let readout = slider.head ? SliderView.readout(slider.readout) : nil
        let height = SliderView.height(label: label, readout: readout, marks: !slider.marks.isEmpty, width: width)
        return ProgramBox(node, size: CGSize(width: width, height: height), surface: env.surface, rects: ["control": CGRect(x: 0, y: 0, width: width, height: height)])
    }

    static func checkbox(_ node: UiNode, _ checkbox: UiKind.Checkbox, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let height = PickRowView.height(title: PickRowView.title(checkbox.label, struck: checkbox.strike && checkbox.checked), detail: nil, width: width)
        var texts: [String: ProgramTextPart] = [:]
        var total = height
        if let error = checkbox.error, !error.isEmpty {
            let part = errorPart(error, at: height + ProgramStyle.errorGap, width: width, env: env)
            texts["error"] = part
            total = part.frame.maxY
        }
        return ProgramBox(
            node, size: CGSize(width: width, height: total), surface: env.surface, texts: texts,
            rects: ["control": CGRect(x: 0, y: 0, width: width, height: height)])
    }

    static let radioLabelGap: CGFloat = 10
    static let radioRowGap: CGFloat = 2
    static let choiceGap: CGFloat = 8
    static let cardLeast: CGFloat = 200

    static func radioGroup(_ node: UiNode, _ group: UiKind.RadioGroup, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        var texts: [String: ProgramTextPart] = [:]
        var y: CGFloat = 0
        if let label = group.label, !label.isEmpty {
            let part = labelPart(label, width: width, env: env)
            texts["label"] = part
            y = part.frame.height + radioLabelGap
        }
        var frames: [CGRect] = []
        switch group.variant {
        case .list:
            for (index, option) in group.options.enumerated() {
                if index > 0 { y += radioRowGap }
                let height = PickRowView.height(title: PickRowView.title(option.label), detail: option.description.map(PickRowView.detail), width: width)
                frames.append(CGRect(x: 0, y: y, width: width, height: height))
                y += height
            }
        case .pills:
            var x: CGFloat = 0
            let height = ControlSize.regular.height
            for option in group.options {
                let pill = min(width, ControlView.width(title: option.label, symbol: false, size: .regular))
                if x > 0, x + pill > width + 0.5 {
                    x = 0
                    y += height + choiceGap
                }
                frames.append(CGRect(x: x, y: y, width: pill, height: height))
                x += pill + choiceGap
            }
            if !group.options.isEmpty { y += height }
        case .cards:
            let count = max(1, Int(((width + choiceGap) / (min(width, cardLeast) + choiceGap)).rounded(.down)))
            let column = (width - CGFloat(count - 1) * choiceGap) / CGFloat(count)
            for start in stride(from: 0, to: group.options.count, by: count) {
                if start > 0 { y += choiceGap }
                let options = group.options[start..<min(start + count, group.options.count)]
                let tallest = options.map { OptionCardView.height(title: $0.label, detail: $0.description, width: column) }.max() ?? 0
                for slot in options.indices {
                    frames.append(CGRect(x: (CGFloat(slot - start) * (column + choiceGap)).rounded(), y: y, width: column, height: tallest))
                }
                y += tallest
            }
        }
        if let error = group.error, !error.isEmpty {
            let part = errorPart(error, at: y + ProgramStyle.errorGap, width: width, env: env)
            texts["error"] = part
            y = part.frame.maxY
        }
        return ProgramBox(node, size: CGSize(width: width, height: y), surface: env.surface, texts: texts, extra: frames)
    }

    static let timerPadding = (ends: CGFloat(12), sides: CGFloat(16))
    static let timerGap: CGFloat = 12
    static let timerLabelGap: CGFloat = 2
    static let timerButtonGap: CGFloat = 8

    static func timer(_ node: UiNode, _ timer: UiKind.Timer, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let room = max(0, width - 2 * timerPadding.sides)
        var texts: [String: ProgramTextPart] = [:]
        var read: CGFloat = 0
        var readWidth: CGFloat = 0
        if let label = timer.label, !label.isEmpty {
            let part = labelPart(label, width: room, env: env)
            texts["label"] = part
            read = part.frame.height + timerLabelGap
            readWidth = ProgramText.width(part.text)
        }
        let look = ProgramStyle.look(.timer, narrow: env.narrow)
        let time = ProgramText.plain(timer.time, look: look, color: timer.done ? Theme.success : Theme.foreground)
        let timeWidth = ProgramText.width(time)
        texts["time"] = ProgramTextPart(
            text: time, frame: CGRect(x: 0, y: read, width: min(room, timeWidth), height: look.lineHeight), lift: ControlText.lift(look.font, lineHeight: look.lineHeight),
            oneLine: true)
        read += look.lineHeight
        readWidth = min(room, max(readWidth, timeWidth))
        let toggle = ControlView.width(title: timer.toggle.label, symbol: timer.toggle.icon != nil, size: .small)
        let reset = ControlView.width(title: timer.reset.label, symbol: timer.reset.icon != nil, size: .small)
        let buttons = toggle + timerButtonGap + reset
        let buttonHeight = ControlSize.small.height
        let side = readWidth + timerGap + buttons <= room
        let content = side ? max(read, buttonHeight) : read + timerGap + buttonHeight
        let top = timerPadding.ends
        let readTop = top + (side ? ((content - read) / 2).rounded() : 0)
        let buttonsTop = top + (side ? ((content - buttonHeight) / 2).rounded() : read + timerGap)
        let buttonsX = side ? width - timerPadding.sides - buttons : timerPadding.sides
        for (key, part) in texts {
            texts[key] = ProgramTextPart(text: part.text, frame: part.frame.offsetBy(dx: timerPadding.sides, dy: readTop), lift: part.lift, oneLine: part.oneLine)
        }
        let rects = [
            "toggle": CGRect(x: buttonsX, y: buttonsTop, width: toggle, height: buttonHeight),
            "reset": CGRect(x: buttonsX + toggle + timerButtonGap, y: buttonsTop, width: reset, height: buttonHeight),
        ]
        return ProgramBox(node, size: CGSize(width: width, height: content + 2 * top), surface: env.surface, texts: texts, rects: rects)
    }
}

/// A table: a card of a header row and rows of cells, its columns as wide as the automatic table
/// layout makes them.
enum ProgramTable {
    struct Layout {
        /// Where each column starts and how wide it is.
        let columns: [(x: CGFloat, width: CGFloat)]
        /// Where each row starts and how tall it is, the header first.
        let rows: [(y: CGFloat, height: CGFloat)]
        let contentWidth: CGFloat
    }

    static func padding(narrow: Bool) -> (ends: CGFloat, sides: CGFloat) { narrow ? (8, 10) : (10, 12) }
    static let headerEnds: CGFloat = 8
    static let unknownLeast: CGFloat = 120
    static let unknownMost: CGFloat = 240

    static func layout(_ node: UiNode, _ table: UiKind.Table, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let pad = padding(narrow: env.narrow)
        let header = ProgramStyle.look(.label, narrow: env.narrow)
        let body = ProgramStyle.look(.small, narrow: env.narrow)
        let cellEnv = env.on(.card).inner
        let count = max(table.columns.count, table.rows.map(\.count).max() ?? 0)
        guard count > 0 else { return ProgramBox(node, size: CGSize(width: width, height: 0), surface: env.surface, inner: .card) }
        let numeric = (0..<count).map { $0 < table.columns.count && table.columns[$0].numeric }
        let headers = (0..<count).map { column -> NSAttributedString in
            let title = column < table.columns.count ? table.columns[column].title : ""
            return ProgramText.plain(title, look: header, color: Theme.foreground, options: .init(alignment: numeric[column] ? .right : .natural, oneLine: true))
        }
        let cells: [[NSAttributedString?]] = table.rows.map { row in
            (0..<count).map { column in
                guard column < row.count, case .text(let text) = row[column] else { return nil }
                let options = ProgramText.Options(digits: numeric[column], alignment: numeric[column] ? .right : .natural, oneLine: numeric[column])
                return ProgramText.set(text.text, look: body, color: Theme.foreground, options: options)
            }
        }
        var least = [CGFloat](repeating: 0, count: count)
        var most = [CGFloat](repeating: 0, count: count)
        for column in 0..<count {
            let headerWidth = ProgramText.width(headers[column])
            least[column] = headerWidth
            most[column] = headerWidth
            for (index, row) in table.rows.enumerated() {
                if let text = cells[index][column] {
                    let whole = ProgramText.width(text)
                    most[column] = max(most[column], whole)
                    least[column] = max(least[column], numeric[column] ? whole : widestWord(text))
                } else if column < row.count, case .node(let cell) = row[column] {
                    let natural = ProgramLayout.natural(cell.node, env: cellEnv)
                    least[column] = max(least[column], natural.isFinite ? natural : unknownLeast)
                    most[column] = max(most[column], natural.isFinite ? natural : unknownMost)
                }
            }
            least[column] += 2 * pad.sides
            most[column] += 2 * pad.sides
        }
        let sumMost = most.reduce(0, +)
        let sumLeast = least.reduce(0, +)
        let widths: [CGFloat]
        if sumMost <= width {
            widths = most.map { $0 + (width - sumMost) * $0 / max(1, sumMost) }
        } else if sumLeast <= width {
            let spread = zip(most, least).map { $0 - $1 }.reduce(0, +)
            widths = (0..<count).map { least[$0] + (width - sumLeast) * (spread > 0 ? (most[$0] - least[$0]) / spread : 1 / CGFloat(count)) }
        } else {
            widths = least
        }
        var columns: [(x: CGFloat, width: CGFloat)] = []
        var x: CGFloat = 0
        for width in widths {
            columns.append((x.rounded(), width))
            x += width
        }
        let contentWidth = max(width, x)

        var texts: [String: ProgramTextPart] = [:]
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        var rows: [(y: CGFloat, height: CGFloat)] = []
        let headerHeight = header.lineHeight + 2 * headerEnds
        for column in 0..<count {
            let frame = CGRect(x: columns[column].x + pad.sides, y: headerEnds, width: max(0, columns[column].width - 2 * pad.sides), height: header.lineHeight)
            texts["h\(column)"] = ProgramTextPart(text: headers[column], frame: frame, lift: ControlText.lift(header.font, lineHeight: header.lineHeight), oneLine: true)
        }
        rows.append((0, headerHeight))
        var y = headerHeight
        for (index, row) in table.rows.enumerated() {
            var heights: [CGFloat] = []
            var laid: [(Int, ProgramBox)] = []
            for column in 0..<count {
                let room = max(0, columns[column].width - 2 * pad.sides)
                if let text = cells[index][column] {
                    heights.append(ProgramText.height(text, width: room, lineHeight: body.lineHeight))
                } else if column < row.count, case .node(let cell) = row[column] {
                    let box = ProgramLayout.layout(cell.node, width: room, fill: !ProgramLayout.keepsWidth(cell.node), env: cellEnv)
                    laid.append((column, box))
                    heights.append(box.size.height)
                }
            }
            let rowHeight = (heights.max() ?? body.lineHeight) + 2 * pad.ends
            for column in 0..<count {
                guard let text = cells[index][column] else { continue }
                let room = max(0, columns[column].width - 2 * pad.sides)
                let height = ProgramText.height(text, width: room, lineHeight: body.lineHeight)
                let frame = CGRect(x: columns[column].x + pad.sides, y: y + ((rowHeight - height) / 2).rounded(), width: room, height: height)
                texts["c\(index)_\(column)"] = ProgramTextPart(
                    text: text, frame: frame, lift: ControlText.lift(body.font, lineHeight: body.lineHeight), oneLine: numeric[column])
            }
            for (column, box) in laid {
                let room = max(0, columns[column].width - 2 * pad.sides)
                let x = numeric[column] ? columns[column].x + pad.sides + room - box.size.width : columns[column].x + pad.sides
                boxes.append(box)
                origins.append(CGPoint(x: x, y: y + ((rowHeight - box.size.height) / 2).rounded()))
            }
            rows.append((y, rowHeight))
            y += rowHeight
        }
        return ProgramBox(
            node, size: CGSize(width: width, height: y), children: boxes, origins: origins, surface: env.surface, inner: .card, texts: texts,
            extra: Layout(columns: columns, rows: rows, contentWidth: contentWidth))
    }

    private static func widestWord(_ text: NSAttributedString) -> CGFloat {
        var widest: CGFloat = 0
        let string = text.string as NSString
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: .byWords) { _, range, _, _ in
            widest = max(widest, ProgramText.width(text.attributedSubstring(from: range)))
        }
        return widest
    }
}

/// Where a canvas lies in its node.
enum ProgramCanvasPart {
    /// A block formula keeps 4 above and below.
    static func margin(_ node: UiNode) -> CGFloat {
        guard case .math(let math) = node.kind, (math.canvas.props as? JSON)?["inline"] as? Bool != true else { return 0 }
        return 4
    }
}
