import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What a node is laid out in: the transcript's column, the window, the surface it lies on.
struct ProgramEnv {
    let column: CGFloat
    let window: CGSize
    /// The program's row, by which its pages keep the heights they reported.
    var row = ""
    var surface = Surface.background
    /// How the lines of text line up, set by a column that centres or ends its children.
    var align = NSTextAlignment.natural
    /// A row gave the node its width, which its own `width` already decided.
    var sized = false
    /// The height of the box around it, when it has one, for a share of it.
    var parentHeight: CGFloat?
    var cache: ProgramCache?
    var generation = 0

    var narrow: Bool { column < ProgramStyle.narrowBelow }

    func on(_ surface: Surface) -> ProgramEnv {
        var env = self
        env.surface = surface
        return env
    }

    var inner: ProgramEnv {
        var env = self
        env.sized = false
        return env
    }
}

/// Text a node draws, where it draws it.
struct ProgramTextPart {
    let text: NSAttributedString
    let frame: CGRect
    /// The room the text system leaves over a line that is taller than its font.
    var lift: CGFloat = 0
    var maxLines: Int?
    var oneLine = false
}

/// Where a node is and what it draws, laid out for one width. Its children's boxes are placed
/// inside it; what it draws itself are its texts and rectangles, by name.
final class ProgramBox {
    let node: UiNode
    let size: CGSize
    let children: [ProgramBox]
    let origins: [CGPoint]
    /// What the node lies on, and what its children lie on.
    let surface: Surface
    let inner: Surface
    let texts: [String: ProgramTextPart]
    let rects: [String: CGRect]
    /// The views of a canvas, a carousel's track or a table's columns: what one kind needs more.
    let extra: Any?

    init(
        _ node: UiNode, size: CGSize, children: [ProgramBox] = [], origins: [CGPoint] = [], surface: Surface, inner: Surface? = nil,
        texts: [String: ProgramTextPart] = [:], rects: [String: CGRect] = [:], extra: Any? = nil
    ) {
        self.node = node
        self.size = size
        self.children = children
        self.origins = origins
        self.surface = surface
        self.inner = inner ?? surface
        self.texts = texts
        self.rects = rects
        self.extra = extra
    }

    var id: String { node.id }

    /// The box at least `height` tall, as a grid's cell is stretched to its row.
    func stretched(to height: CGFloat) -> ProgramBox {
        guard size.height < height else { return self }
        return ProgramBox(
            node, size: CGSize(width: size.width, height: height), children: children, origins: origins, surface: surface, inner: inner,
            texts: texts, rects: rects, extra: extra)
    }
}

/// Lays a program's tree out as `crates/openui/LAYOUT.md` says, without views: the transcript
/// measures a row with it off the main thread, the views are placed by it, and the frame dump
/// reads it.
enum ProgramLayout {
    static func layout(_ node: UiNode, width: CGFloat, fill: Bool = true, height: CGFloat? = nil, env: ProgramEnv) -> ProgramBox {
        let width = max(0, width)
        let key = ProgramCacheKey(
            id: node.id, width: width, fill: fill, height: height ?? -1, narrow: env.narrow, surface: env.surface, align: env.align.rawValue,
            sized: env.sized, window: env.window)
        if let cached = env.cache?.box(key, generation: env.generation) { return cached }
        let box = make(node, width: width, fill: fill, height: height, env: env)
        env.cache?.keep(box, for: key, generation: env.generation)
        return box
    }

    /// How wide the node is with nothing wrapped; `infinity` for what takes every width it is given.
    static func natural(_ node: UiNode, env: ProgramEnv) -> CGFloat {
        let key = ProgramCacheKey(
            id: node.id, width: -1, fill: false, height: -1, narrow: env.narrow, surface: env.surface, align: 0, sized: env.sized,
            window: env.window)
        if let cached = env.cache?.width(key, generation: env.generation) { return cached }
        let width = measureNatural(node, env: env)
        env.cache?.keep(width: width, for: key, generation: env.generation)
        return width
    }

    /// The node keeps its own width in a column that stretches its children.
    static func keepsWidth(_ node: UiNode) -> Bool {
        switch node.kind {
        case .button(let button): !button.block
        case .iconButton, .badge, .icon, .popover: true
        case .segmentedControl(let control): !control.block
        default: false
        }
    }

    // MARK: Each kind

    private static func make(_ node: UiNode, width: CGFloat, fill: Bool, height: CGFloat?, env: ProgramEnv) -> ProgramBox {
        let surface = env.surface
        switch node.kind {
        case .col(let col):
            return column(node, col.children, spacing: col.spacing, align: col.align, width: width, fill: fill, height: height, env: env)
        case .row(let row):
            return self.row(
                node, row.children, sizing: row.sizing, gap: row.gap, align: row.align, justify: row.justify, wrap: row.wrap, width: width,
                fill: fill, height: height, env: env)
        case .box(let box):
            return self.box(node, box, width: width, fill: fill, height: height, env: env)
        case .grid(let grid):
            return self.grid(node, grid, width: width, height: height, env: env)
        case .card(let card):
            return self.card(node, card, width: width, height: height, env: env)
        case .carousel(let carousel):
            return self.carousel(node, carousel, width: width, env: env)
        case .popover(let popover):
            return slot(node, popover.trigger, gap: ProgramStyle.popoverGap, width: width, env: env)
        case .divider:
            return ProgramBox(node, size: CGSize(width: width, height: 1), surface: surface)
        case .spacer, .mapMarker:
            return ProgramBox(node, size: .zero, surface: surface)
        case .celebration(let celebration):
            return ProgramBox(node, size: .zero, surface: surface, extra: scene(celebration.canvas, width: env.column)?.motion)
        case .title(let title):
            let look = ProgramStyle.look(title.style, narrow: env.narrow)
            let color = ProgramStyle.color(title.color, on: surface)
            let text = ProgramText.set(title.text, look: look, color: color, options: .init(alignment: env.align))
            return textBox(node, text, look: look, width: width, fill: fill, env: env)
        case .text(let text):
            return self.text(node, text, width: width, fill: fill, env: env)
        case .caption(let caption):
            let look = ProgramStyle.look(caption.style, narrow: env.narrow)
            let color = ProgramStyle.color(caption.color, on: surface, current: Theme.mutedForeground)
            let text = ProgramText.set(caption.text, look: look, color: color, options: .init(alignment: env.align))
            return textBox(node, text, look: look, width: width, fill: fill, env: env)
        case .label(let label):
            let look = ProgramStyle.look(.label, narrow: env.narrow)
            let text = ProgramText.plain(label.text, look: look, color: ProgramStyle.color(label.color, on: surface), options: .init(alignment: env.align))
            return textBox(node, text, look: look, width: width, fill: fill, env: env)
        case .badge(let badge):
            let size = ProgramBadge.size(badge)
            return ProgramBox(node, size: CGSize(width: min(size.width, width), height: size.height), surface: surface)
        case .link(let link):
            return self.link(node, link, width: width, fill: fill, env: env)
        case .list(let list):
            return self.list(node, list, width: width, env: env)
        case .listItem(let item):
            return listItem(node, item, lane: ProgramList.Lane(list: .bullet, wide: false, timeline: false, narrow: env.narrow), last: true, width: width, env: env)
        case .markdown(let markdown):
            return self.markdown(node, markdown, width: width, env: env)
        case .quote(let quote):
            return self.quote(node, quote, width: width, env: env)
        case .code(let code):
            return self.code(node, code, width: width, env: env)
        case .button(let button):
            let size = ProgramStyle.controlSize(button.size)
            let own = ControlView.width(title: button.label, symbol: button.icon != nil, size: size)
            return ProgramBox(node, size: CGSize(width: button.block ? width : min(own, width), height: size.height), surface: surface)
        case .iconButton:
            let side = ControlSize.regular.height
            return ProgramBox(node, size: CGSize(width: side, height: side), surface: surface)
        case .pressable(let pressable):
            return self.pressable(node, pressable, width: width, fill: fill, height: height, env: env)
        case .input(let input):
            return ProgramFields.field(node, label: input.label, error: input.error, control: ControlSize.regular.height, width: width, env: env)
        case .textArea(let area):
            let control = TextAreaView.height(rows: max(2, min(20, area.rows)))
            return ProgramFields.field(node, label: area.label, error: area.error, control: control, width: width, env: env)
        case .select(let select):
            return ProgramFields.field(node, label: select.label, error: select.error, control: ControlSize.regular.height, width: width, env: env)
        case .datePicker(let picker):
            return ProgramFields.field(node, label: picker.label, error: picker.error, control: ControlSize.regular.height, width: width, env: env)
        case .segmentedControl(let control):
            let own = SegmentedView.width(control.options.map(\.label), size: .regular)
            return ProgramBox(
                node, size: CGSize(width: control.block ? width : min(own, width), height: SegmentedView.height(.regular)), surface: surface,
                extra: own)
        case .slider(let slider):
            return ProgramFields.slider(node, slider, width: width, env: env)
        case .checkbox(let checkbox):
            return ProgramFields.checkbox(node, checkbox, width: width, env: env)
        case .radioGroup(let group):
            return ProgramFields.radioGroup(node, group, width: width, env: env)
        case .timer(let timer):
            return ProgramFields.timer(node, timer, width: width, env: env)
        case .form(let form):
            return column(node, form.children, spacing: form.spacing, align: .stretch, width: width, fill: true, height: height, env: env)
        case .searchImage(let image):
            return picture(node, aspect: image.aspectRatio, width: width, env: env)
        case .image(let image):
            return picture(node, aspect: image.aspectRatio, width: width, env: env)
        case .icon(let icon):
            let side = CGFloat(icon.size)
            return ProgramBox(node, size: CGSize(width: side, height: side), surface: surface)
        case .table(let table):
            return ProgramTable.layout(node, table, width: width, env: env)
        case .chart, .svg, .math:
            return canvas(node, width: width, fill: fill, env: env)
        case .flashcard(let card):
            return flashcard(node, card, width: width, env: env)
        case .map(let map):
            return ProgramBox(node, size: CGSize(width: width, height: MapView.height(node: map, windowWidth: env.window.width)), surface: surface)
        case .youTube(let video):
            return ProgramWeb.youTube(node, video, width: width, env: env)
        case .htmlView(let view):
            return ProgramWeb.htmlView(node, view, width: width, env: env)
        case .g, .circle, .ellipse, .rect, .line, .path, .polyline, .polygon, .svgText:
            return ProgramBox(node, size: .zero, surface: surface)
        }
    }

    private static func measureNatural(_ node: UiNode, env: ProgramEnv) -> CGFloat {
        let wide = CGFloat.infinity
        switch node.kind {
        case .col(let col):
            return col.children.map { natural($0, env: env.inner) }.max() ?? 0
        case .form(let form):
            return form.children.map { natural($0, env: env.inner) }.max() ?? 0
        case .row(let row):
            return rowNatural(row.children, sizing: row.sizing, gap: row.gap, env: env)
        case .box(let box):
            if let width = box.width, box.width?.unit == .points { return width.value }
            let inner = env.on(box.innerSurface ? env.surface.inBox : env.surface).inner
            let content = box.direction == .row
                ? rowNatural(box.children, sizing: box.sizing, gap: box.gap, env: inner)
                : box.children.map { natural($0, env: inner) }.max() ?? 0
            return content + 2 * box.padding
        case .pressable(let pressable):
            return (pressable.children.map { natural($0, env: env.inner) }.max() ?? 0) + 2 * pressable.padding
        case .card, .grid, .carousel, .divider, .list, .listItem, .markdown, .quote, .code, .input, .textArea, .select, .slider,
            .radioGroup, .datePicker, .timer, .searchImage, .image, .table, .chart, .map, .youTube, .flashcard, .svg, .math, .htmlView:
            return wide
        case .spacer, .mapMarker, .celebration, .g, .circle, .ellipse, .rect, .line, .path, .polyline, .polygon, .svgText:
            return 0
        case .popover(let popover):
            return slotNatural(popover.trigger, gap: ProgramStyle.popoverGap, env: env)
        case .title, .text, .caption, .label, .link:
            return make(node, width: .greatestFiniteMagnitude, fill: false, height: nil, env: env).size.width
        case .badge(let badge):
            return ProgramBadge.size(badge).width
        case .button(let button):
            return ControlView.width(title: button.label, symbol: button.icon != nil, size: ProgramStyle.controlSize(button.size))
        case .iconButton:
            return ControlSize.regular.height
        case .segmentedControl(let control):
            return SegmentedView.width(control.options.map(\.label), size: .regular)
        case .checkbox(let checkbox):
            return PickMarkView.side + PickText.gap + ProgramText.width(PickRowView.title(checkbox.label, struck: checkbox.strike))
        case .icon(let icon):
            return CGFloat(icon.size)
        }
    }

    // MARK: Containers

    static func column(
        _ node: UiNode, _ children: [UiNode], spacing: [Double], align: UiAlign, width: CGFloat, fill: Bool, height: CGFloat?,
        padding: CGFloat = 0, env: ProgramEnv, inner: Surface? = nil, rects: [String: CGRect] = [:], justify: UiJustify = .start
    ) -> ProgramBox {
        var childEnv = env.on(inner ?? env.surface).inner
        childEnv.align = align == .center ? .center : align == .end ? .right : align == .start ? .natural : env.align == .center ? .natural : env.align
        if align == .stretch { childEnv.align = .natural }
        let room = max(0, width - 2 * padding)
        let boxes = children.map { child in
            let stretches = align == .stretch && !keepsWidth(child)
            return layout(child, width: room, fill: stretches, env: childEnv)
        }
        let content = fill ? room : min(room, boxes.map(\.size.width).max() ?? 0)
        var y = padding
        var origins: [CGPoint] = []
        for (index, box) in boxes.enumerated() {
            y += index < spacing.count ? CGFloat(spacing[index]) : 0
            let x: CGFloat
            switch align {
            case .center: x = ((content - box.size.width) / 2).rounded()
            case .end: x = content - box.size.width
            case .start, .stretch:
                guard case .svg = box.node.kind else { x = 0; break }
                x = ((content - box.size.width) / 2).rounded()
            }
            origins.append(CGPoint(x: padding + x, y: y))
            y += box.size.height
        }
        var total = y + padding
        if let height, height > total {
            let free = height - total
            origins = justified(origins, boxes: boxes, free: free, justify: justify)
            total = height
        }
        return ProgramBox(
            node, size: CGSize(width: content + 2 * padding, height: total), children: boxes, origins: origins, surface: env.surface,
            inner: inner ?? env.surface, rects: rects)
    }

    private static func justified(_ origins: [CGPoint], boxes: [ProgramBox], free: CGFloat, justify: UiJustify) -> [CGPoint] {
        switch justify {
        case .start: return origins
        case .center: return origins.map { CGPoint(x: $0.x, y: $0.y + (free / 2).rounded()) }
        case .end: return origins.map { CGPoint(x: $0.x, y: $0.y + free) }
        case .between:
            guard origins.count > 1 else { return origins }
            let step = free / CGFloat(origins.count - 1)
            return origins.enumerated().map { CGPoint(x: $1.x, y: $1.y + step * CGFloat($0)) }
        }
    }

    /// The width a child of a row starts at.
    private static func basis(_ child: UiNode, _ sizing: UiSizing?, row: CGFloat, env: ProgramEnv) -> CGFloat {
        switch sizing?.basis ?? .content {
        case .content: return min(natural(child, env: env), max(row, 0) * 4 + 4000)
        case .zero: return 0
        case .length: return sizing?.length.map { resolve($0, of: row, env: env) } ?? 0
        case .row: return row
        case .media: return min(200, max(120, row / 3))
        }
    }

    static func resolve(_ length: UiLength, of parent: CGFloat, env: ProgramEnv) -> CGFloat {
        switch length.unit {
        case .points: CGFloat(length.value)
        case .fraction: CGFloat(length.value) * parent
        case .column: CGFloat(length.value) * env.column
        case .window: CGFloat(length.value) * env.window.height
        }
    }

    private static func rowNatural(_ children: [UiNode], sizing: [UiSizing], gap: Double, env: ProgramEnv) -> CGFloat {
        guard !children.isEmpty else { return 0 }
        var total = CGFloat(gap) * CGFloat(children.count - 1)
        for (index, child) in children.enumerated() {
            let size = index < sizing.count ? sizing[index] : nil
            switch size?.basis ?? .content {
            case .content, .row: total += natural(child, env: env.inner)
            case .zero: break
            case .length: total += size?.length?.unit == .points ? CGFloat(size?.length?.value ?? 0) : natural(child, env: env.inner)
            case .media: total += 200
            }
        }
        return total
    }

    static func row(
        _ node: UiNode, _ children: [UiNode], sizing: [UiSizing], gap: Double, align: UiAlign, justify: UiJustify, wrap: Bool,
        width: CGFloat, fill: Bool, height: CGFloat?, padding: CGFloat = 0, env: ProgramEnv, inner: Surface? = nil,
        rects: [String: CGRect] = [:]
    ) -> ProgramBox {
        var childEnv = env.on(inner ?? env.surface).inner
        childEnv.sized = true
        let gap = CGFloat(gap)
        let available = max(0, width - 2 * padding)
        let room = fill ? available : min(available, rowNatural(children, sizing: sizing, gap: Double(gap), env: env.on(inner ?? env.surface)))
        let sizes = children.indices.map { $0 < sizing.count ? sizing[$0] : nil }
        let bases = children.indices.map { basis(children[$0], sizes[$0], row: room, env: childEnv) }

        var lines: [[Int]] = []
        var line: [Int] = []
        var used: CGFloat = 0
        for index in children.indices {
            let needed = (line.isEmpty ? 0 : gap) + bases[index]
            if wrap, !line.isEmpty, used + needed > room + 0.5 {
                lines.append(line)
                line = []
                used = 0
            }
            used += line.isEmpty ? bases[index] : needed
            line.append(index)
        }
        if !line.isEmpty { lines.append(line) }

        var boxes = [ProgramBox?](repeating: nil, count: children.count)
        var origins = [CGPoint](repeating: .zero, count: children.count)
        var y = padding
        for (number, members) in lines.enumerated() {
            if number > 0 { y += gap }
            var widths = members.map { bases[$0] }
            let free = room - widths.reduce(0, +) - gap * CGFloat(max(0, members.count - 1))
            let weights = members.map { CGFloat(sizes[$0]?.grow ?? 0) }
            let totalWeight = weights.reduce(0, +)
            var leading: CGFloat = 0
            var between = gap
            if free >= 0 {
                if totalWeight > 0 {
                    for (slot, weight) in weights.enumerated() where weight > 0 { widths[slot] += free * weight / totalWeight }
                } else {
                    switch justify {
                    case .start: break
                    case .center: leading = (free / 2).rounded()
                    case .end: leading = free
                    case .between: if members.count > 1 { between += free / CGFloat(members.count - 1) }
                    }
                }
            } else {
                let shrinking = members.indices.filter { sizes[members[$0]]?.shrink ?? true }
                let total = shrinking.reduce(0) { $0 + widths[$1] }
                if total > 0 {
                    for slot in shrinking { widths[slot] = max(0, widths[slot] + free * widths[slot] / total) }
                }
            }
            var lineBoxes = members.indices.map { slot in layout(children[members[slot]], width: widths[slot], fill: true, env: childEnv) }
            var lineHeight = lineBoxes.map(\.size.height).max() ?? 0
            if align == .stretch {
                lineBoxes = members.indices.map { slot in
                    layout(children[members[slot]], width: widths[slot], fill: true, height: lineHeight, env: childEnv)
                }
                lineHeight = lineBoxes.map(\.size.height).max() ?? lineHeight
            }
            var x = padding + leading
            for (slot, member) in members.enumerated() {
                let box = lineBoxes[slot]
                let top: CGFloat
                switch align {
                case .start, .stretch: top = 0
                case .center: top = ((lineHeight - box.size.height) / 2).rounded()
                case .end: top = lineHeight - box.size.height
                }
                boxes[member] = box
                origins[member] = CGPoint(x: x, y: y + top)
                x += widths[slot] + between
            }
            y += lineHeight
        }
        let total = max(y + padding, height ?? 0)
        return ProgramBox(
            node, size: CGSize(width: room + 2 * padding, height: total), children: boxes.compactMap { $0 }, origins: origins, surface: env.surface,
            inner: inner ?? env.surface, rects: rects)
    }

    private static func box(_ node: UiNode, _ box: UiKind.Box, width: CGFloat, fill: Bool, height: CGFloat?, env: ProgramEnv) -> ProgramBox {
        let own = env.sized ? nil : box.width.map { resolve($0, of: width, env: env) }
        var outer = own.map { min($0, width) } ?? width
        if own == nil, !fill { outer = min(width, natural(node, env: env)) }
        var fixed = box.height.flatMap { length -> CGFloat? in
            guard length.unit != .fraction else { return env.parentHeight.map { CGFloat(length.value) * $0 } }
            return resolve(length, of: 0, env: env)
        }
        if let height { fixed = max(fixed ?? 0, height) }
        let inner = box.innerSurface ? env.surface.inBox : env.surface
        var childEnv = env
        childEnv.parentHeight = box.height == nil ? nil : fixed.map { $0 - 2 * box.padding }
        let padding = CGFloat(box.padding)
        let laid = box.direction == .row
            ? row(
                node, box.children, sizing: box.sizing, gap: box.gap, align: box.align, justify: box.justify, wrap: box.wrap, width: outer, fill: true,
                height: fixed, padding: padding, env: childEnv, inner: inner)
            : column(
                node, box.children, spacing: box.spacing, align: box.align, width: outer, fill: true, height: fixed, padding: padding, env: childEnv,
                inner: inner, justify: box.justify)
        let size = CGSize(width: outer, height: box.height == nil ? laid.size.height : max(fixed ?? laid.size.height, height ?? 0))
        return ProgramBox(node, size: size, children: laid.children, origins: laid.origins, surface: env.surface, inner: inner)
    }

    private static func grid(_ node: UiNode, _ grid: UiKind.Grid, width: CGFloat, height: CGFloat?, env: ProgramEnv) -> ProgramBox {
        let gap = CGFloat(grid.gap)
        let count = columns(grid, width: width)
        let cell = max(0, (width - CGFloat(count - 1) * gap) / CGFloat(count))
        let childEnv = env.inner
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        var y: CGFloat = 0
        for start in stride(from: 0, to: grid.children.count, by: count) {
            if start > 0 { y += gap }
            let cells = Array(grid.children[start..<min(start + count, grid.children.count)])
            let tallest = cells.map { layout($0, width: cell, env: childEnv).size.height }.max() ?? 0
            for (column, child) in cells.enumerated() {
                boxes.append(layout(child, width: cell, height: tallest, env: childEnv).stretched(to: tallest))
                origins.append(CGPoint(x: CGFloat(column) * (cell + gap), y: y))
            }
            y += tallest
        }
        return ProgramBox(node, size: CGSize(width: width, height: max(y, height ?? 0)), children: boxes, origins: origins, surface: env.surface)
    }

    /// How many columns of a grid fit the width.
    static func columns(_ grid: UiKind.Grid, width: CGFloat) -> Int {
        let most = max(1, min(12, grid.columns))
        let gap = CGFloat(grid.gap)
        guard let least = grid.minChildWidth.map({ CGFloat($0) }) else { return most }
        let share = (width - CGFloat(most - 1) * gap) / CGFloat(most)
        guard share < least else { return most }
        let column = min(width, least)
        guard column + gap > 0 else { return 1 }
        return max(1, min(most, Int(((width + gap) / (column + gap)).rounded(.down))))
    }

    private static func card(_ node: UiNode, _ card: UiKind.Card, width: CGFloat, height: CGFloat?, env: ProgramEnv) -> ProgramBox {
        let padding = card.padding.map { CGFloat($0) } ?? (card.nested ? ProgramStyle.nestedCardPadding : ProgramStyle.cardPadding(narrow: env.narrow))
        let inner: Surface = card.soft ? env.surface.inBox : .card
        return column(
            node, card.children, spacing: card.spacing, align: .stretch, width: width, fill: true, height: height, padding: padding, env: env,
            inner: inner)
    }

    private static func carousel(_ node: UiNode, _ carousel: UiKind.Carousel, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let item = min(min(640, max(80, CGFloat(carousel.itemWidth))), (width * 0.85).rounded(.down))
        let gap = CGFloat(carousel.gap)
        let childEnv = env.inner
        let tallest = carousel.children.map { layout($0, width: item, env: childEnv).size.height }.max() ?? 0
        let boxes = carousel.children.enumerated().map { index, child in
            let fills = index < carousel.fill.count && carousel.fill[index]
            return layout(child, width: item, height: fills ? tallest : nil, env: childEnv)
        }
        let origins = boxes.indices.map { CGPoint(x: CGFloat($0) * (item + gap), y: 0) }
        let track = boxes.isEmpty ? 0 : CGFloat(boxes.count) * item + CGFloat(boxes.count - 1) * gap
        return ProgramBox(
            node, size: CGSize(width: width, height: tallest), children: boxes, origins: origins, surface: env.surface,
            extra: ProgramCarousel(item: item, gap: gap, track: track))
    }

    private static func pressable(
        _ node: UiNode, _ pressable: UiKind.Pressable, width: CGFloat, fill: Bool, height: CGFloat?, env: ProgramEnv
    ) -> ProgramBox {
        let inner = pressable.innerSurface ? env.surface.inBox : env.surface
        let spacing = [Double](repeating: 0, count: pressable.children.count)
        return column(
            node, pressable.children, spacing: spacing, align: .stretch, width: width, fill: fill, height: height, padding: CGFloat(pressable.padding),
            env: env, inner: inner)
    }

    /// A slot's text and its components side by side, as a popover's trigger lies inline.
    private static func slot(_ node: UiNode, _ slot: UiSlot, gap: CGFloat, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let childEnv = env.inner
        var x: CGFloat = 0
        var texts: [String: ProgramTextPart] = [:]
        var height: CGFloat = 0
        if let rich = slot.text, !rich.text.isEmpty {
            let look = ProgramStyle.look(slot.style, narrow: env.narrow)
            let text = ProgramText.set(rich, look: look, color: Theme.foreground)
            let size = CGSize(width: min(width, ProgramText.width(text)), height: ProgramText.height(text, width: width, lineHeight: look.lineHeight))
            texts["text"] = ProgramTextPart(text: text, frame: CGRect(origin: .zero, size: size), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
            x = size.width
            height = size.height
        }
        var boxes: [ProgramBox] = []
        for child in slot.children {
            if x > 0 { x += gap }
            let box = layout(child, width: max(0, width - x), fill: false, env: childEnv)
            boxes.append(box)
            x += box.size.width
            height = max(height, box.size.height)
        }
        var origins: [CGPoint] = []
        var left = texts["text"].map { $0.frame.width + gap } ?? 0
        for box in boxes {
            origins.append(CGPoint(x: left, y: ((height - box.size.height) / 2).rounded()))
            left += box.size.width + gap
        }
        if let part = texts["text"] {
            let frame = CGRect(x: 0, y: ((height - part.frame.height) / 2).rounded(), width: part.frame.width, height: part.frame.height)
            texts["text"] = ProgramTextPart(text: part.text, frame: frame, lift: part.lift)
        }
        return ProgramBox(node, size: CGSize(width: min(width, x), height: height), children: boxes, origins: origins, surface: env.surface, texts: texts)
    }

    private static func slotNatural(_ slot: UiSlot, gap: CGFloat, env: ProgramEnv) -> CGFloat {
        var parts: [CGFloat] = []
        if let rich = slot.text, !rich.text.isEmpty {
            parts.append(ProgramText.width(ProgramText.set(rich, look: ProgramStyle.look(slot.style, narrow: env.narrow), color: Theme.foreground)))
        }
        parts += slot.children.map { natural($0, env: env.inner) }
        return parts.reduce(0, +) + gap * CGFloat(max(0, parts.count - 1))
    }

    // MARK: Text

    static func textBox(
        _ node: UiNode, _ text: NSAttributedString, look: ProgramStyle.Look, maxLines: Int? = nil, width: CGFloat, fill: Bool, env: ProgramEnv,
        inlines: [ProgramBox] = []
    ) -> ProgramBox {
        let natural = ProgramText.width(text)
        let textWidth = fill ? width : min(width, natural)
        let height = ProgramText.height(text, width: textWidth, lineHeight: look.lineHeight, maxLines: maxLines)
        let lift = ControlText.lift(look.font, lineHeight: look.lineHeight)
        let part = ProgramTextPart(text: text, frame: CGRect(x: 0, y: 0, width: textWidth, height: height), lift: lift, maxLines: maxLines)
        let origins = inlines.isEmpty ? [] : ProgramInline.places(in: text, width: textWidth, count: inlines.count).map { CGPoint(x: $0.x, y: $0.y - lift) }
        return ProgramBox(
            node, size: CGSize(width: textWidth, height: height), children: Array(inlines.prefix(origins.count)), origins: origins, surface: env.surface,
            texts: ["text": part])
    }

    private static func text(_ node: UiNode, _ text: UiKind.Text, width: CGFloat, fill: Bool, env: ProgramEnv) -> ProgramBox {
        let look = ProgramStyle.look(text.style, narrow: env.narrow)
        let color = ProgramStyle.color(text.color, on: env.surface)
        let options = ProgramText.Options(
            weight: text.weight == .regular ? nil : text.weight, italic: text.italic, strike: text.strike, digits: text.digits, alignment: env.align)
        var set = ProgramText.set(text.text, look: look, color: color, options: options)
        var inlines: [ProgramBox] = []
        if !text.inlines.isEmpty {
            inlines = text.inlines.map { layout($0, width: width, fill: false, env: env.inner) }
            set = ProgramInline.attach(inlines, to: set, size: look.font.pointSize, lineHeight: look.lineHeight)
        }
        return textBox(node, set, look: look, maxLines: text.maxLines, width: width, fill: fill, env: env, inlines: inlines)
    }

    private static func link(_ node: UiNode, _ link: UiKind.Link, width: CGFloat, fill: Bool, env: ProgramEnv) -> ProgramBox {
        let look = ProgramStyle.look(.body, narrow: env.narrow)
        let color = link.url == nil ? ProgramStyle.color(.token("foreground"), on: env.surface) : Theme.primary
        let text = ProgramText.plain(link.text, look: look, color: color, options: .init(alignment: env.align))
        let arrow = link.url == nil ? 0 : ProgramLink.arrowSide(look) + ProgramLink.arrowGap
        let natural = ProgramText.width(text) + arrow
        let textWidth = max(0, (fill ? width : min(width, natural)) - arrow)
        let height = ProgramText.height(text, width: textWidth, lineHeight: look.lineHeight)
        let part = ProgramTextPart(
            text: text, frame: CGRect(x: 0, y: 0, width: textWidth, height: height), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
        var rects: [String: CGRect] = [:]
        if link.url != nil {
            let side = ProgramLink.arrowSide(look)
            let lastLine = max(0, height - look.lineHeight)
            let x = min(textWidth, ProgramInline.lastLineEnd(of: text, width: textWidth)) + ProgramLink.arrowGap
            rects["arrow"] = CGRect(x: x, y: lastLine + ((look.lineHeight - side) / 2).rounded(), width: side, height: side)
        }
        return ProgramBox(node, size: CGSize(width: textWidth + arrow, height: height), surface: env.surface, texts: ["text": part], rects: rects)
    }

    // MARK: Lists, prose and code

    private static func list(_ node: UiNode, _ list: UiKind.List, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        var y: CGFloat = 0
        let lane = ProgramList.Lane(list: list.marker, wide: list.wide, timeline: list.timeline, narrow: env.narrow)
        for (index, item) in list.items.enumerated() {
            if index > 0, !list.timeline { y += ProgramList.itemGap }
            let box: ProgramBox
            if case .listItem(let content) = item.kind {
                box = listItem(item, content, lane: lane, last: index == list.items.count - 1, width: width, env: env)
            } else {
                box = layout(item, width: width, env: env.inner)
            }
            boxes.append(box)
            origins.append(CGPoint(x: 0, y: y))
            y += box.size.height
        }
        return ProgramBox(node, size: CGSize(width: width, height: y), children: boxes, origins: origins, surface: env.surface)
    }

    static func listItem(_ node: UiNode, _ item: UiKind.ListItem, lane: ProgramList.Lane, last: Bool, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let look = ProgramStyle.look(item.style, narrow: env.narrow)
        let (laneWidth, gap) = lane.room(for: item.marker)
        let x = laneWidth + gap
        let bodyWidth = max(0, width - x)
        let title = ProgramText.set(item.title, look: look, color: Theme.foreground, options: .init(weight: item.medium ? .medium : nil))
        let lift = ControlText.lift(look.font, lineHeight: look.lineHeight)
        var texts: [String: ProgramTextPart] = [:]
        var y: CGFloat = 0
        let titleHeight = ProgramText.height(title, width: bodyWidth, lineHeight: look.lineHeight)
        texts["title"] = ProgramTextPart(text: title, frame: CGRect(x: x, y: 0, width: bodyWidth, height: titleHeight), lift: lift)
        y = titleHeight
        if let description = item.description {
            let small = ProgramStyle.look(.small, narrow: env.narrow)
            let text = ProgramText.set(description, look: small, color: Theme.mutedForeground)
            let height = ProgramText.height(text, width: bodyWidth, lineHeight: small.lineHeight)
            y += ProgramList.descriptionGap
            texts["description"] = ProgramTextPart(
                text: text, frame: CGRect(x: x, y: y, width: bodyWidth, height: height), lift: ControlText.lift(small.font, lineHeight: small.lineHeight))
            y += height
        }
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        if !item.children.isEmpty { y += ProgramList.childrenGap }
        for child in item.children {
            let box = layout(child, width: bodyWidth, env: env.inner)
            boxes.append(box)
            origins.append(CGPoint(x: x, y: y))
            y += box.size.height
        }
        if item.description != nil || !item.children.isEmpty { y += ProgramList.detailRoom }
        if lane.timeline, !last { y += ProgramList.timelineGap }
        var rects: [String: CGRect] = [:]
        var marker = ProgramList.marker(item.marker, lane: lane, laneWidth: laneWidth, firstLine: look.lineHeight, env: env)
        if let part = marker.text { texts["marker"] = part }
        rects["marker"] = marker.frame
        if lane.timeline, !last, lane.list != .plain || lane.wide {
            let size = look.font.pointSize / Platform.scale
            let center = lane.wide ? laneWidth + (env.narrow ? 9.5 : 7.5) : 12
            let top = (1.7 * size).rounded()
            rects["line"] = CGRect(x: center - 0.5, y: top, width: 1, height: max(0, y - top - (0.15 * size).rounded()))
        }
        marker.text = nil
        return ProgramBox(
            node, size: CGSize(width: width, height: y), children: boxes, origins: origins, surface: env.surface, texts: texts, rects: rects,
            extra: marker.kind)
    }

    private static func markdown(_ node: UiNode, _ markdown: UiKind.Markdown, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        var y: CGFloat = 0
        var blocks: [ProgramProse.Block] = []
        for (index, block) in markdown.blocks.enumerated() {
            if index > 0 { y += ProgramStyle.rowSpace }
            switch block {
            case .prose(let prose):
                let text = Typesetter.prose(prose.prose)
                let height = TextMeasure.height(of: text, width: width)
                blocks.append(ProgramProse.Block(frame: CGRect(x: 0, y: y, width: width, height: height), prose: text, code: nil))
                y += height
            case .code(let code):
                let content = CodeContent(language: code.language.isEmpty ? "code" : code.language, code: code.code, spans: code.spans?.map { NSNumber(value: $0) })
                let height = ProgramCode.height(content, width: width)
                blocks.append(ProgramProse.Block(frame: CGRect(x: 0, y: y, width: width, height: height), prose: nil, code: content))
                y += height
            }
        }
        return ProgramBox(node, size: CGSize(width: width, height: y), surface: env.surface, extra: blocks)
    }

    private static func quote(_ node: UiNode, _ quote: UiKind.Quote, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let body = ProgramStyle.look(.body, narrow: env.narrow)
        let textWidth = max(0, width - ProgramQuote.inset)
        let text = ProgramText.set(quote.text, look: body, color: Theme.mutedForeground)
        let height = ProgramText.height(text, width: textWidth, lineHeight: body.lineHeight)
        var texts = [
            "text": ProgramTextPart(
                text: text, frame: CGRect(x: ProgramQuote.inset, y: 0, width: textWidth, height: height),
                lift: ControlText.lift(body.font, lineHeight: body.lineHeight))
        ]
        var y = height
        if let cite = quote.cite {
            let small = ProgramStyle.look(.small, narrow: env.narrow)
            let citeText = ProgramText.set(cite, look: small, color: Theme.mutedStrongerForeground)
            let citeHeight = ProgramText.height(citeText, width: textWidth, lineHeight: small.lineHeight)
            y += ProgramQuote.citeGap
            texts["cite"] = ProgramTextPart(
                text: citeText, frame: CGRect(x: ProgramQuote.inset, y: y, width: textWidth, height: citeHeight),
                lift: ControlText.lift(small.font, lineHeight: small.lineHeight))
            y += citeHeight
        }
        let bar = CGRect(x: ProgramQuote.barX, y: 0, width: ProgramQuote.barWidth, height: y)
        return ProgramBox(node, size: CGSize(width: width, height: y), surface: env.surface, texts: texts, rects: ["bar": bar])
    }

    private static func code(_ node: UiNode, _ code: UiKind.Code, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let content = CodeContent(language: code.language, code: code.code, spans: code.spans?.map { NSNumber(value: $0) })
        return ProgramBox(node, size: CGSize(width: width, height: ProgramCode.height(content, width: width)), surface: env.surface, extra: content)
    }

    // MARK: Media and play

    private static func picture(_ node: UiNode, aspect: Double, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let ratio = CGFloat(min(5, max(0.2, aspect)))
        return ProgramBox(node, size: CGSize(width: width, height: (width / ratio).rounded()), surface: env.surface)
    }

    private static func canvas(_ node: UiNode, width: CGFloat, fill: Bool, env: ProgramEnv) -> ProgramBox {
        let canvas: UiCanvas
        switch node.kind {
        case .chart(let chart): canvas = chart.canvas
        case .svg(let svg): canvas = svg.canvas
        case .math(let math): canvas = math.canvas
        default: return ProgramBox(node, size: .zero, surface: env.surface)
        }
        guard let scene = scene(canvas, width: width) else { return ProgramBox(node, size: CGSize(width: width, height: 0), surface: env.surface) }
        guard case .chart = node.kind else {
            let size = scene.size
            let svg = if case .svg = node.kind { true } else { false }
            let width = fill && !svg ? width : min(width, size.width)
            let place = CGRect(x: max(0, ((width - size.width) / 2).rounded()), y: ProgramCanvasPart.margin(node), width: size.width, height: size.height)
            let height = size.height + 2 * ProgramCanvasPart.margin(node)
            return ProgramBox(node, size: CGSize(width: width, height: height), surface: env.surface, rects: ["scene": place], extra: scene)
        }
        let content = CanvasContent(scene: scene, width: width)
        return ProgramBox(node, size: CGSize(width: width, height: content.size.height), surface: env.surface, extra: content)
    }

    static func scene(_ canvas: UiCanvas, width: CGFloat) -> CanvasScene? {
        CanvasLayout.scene(kind: canvas.kind, props: canvas.props as? JSON ?? [:], width: width, textSize: Theme.proseSize)
    }

    private static func flashcard(_ node: UiNode, _ card: UiKind.Flashcard, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let inner = width - 2 * ProgramFlashcard.sides
        let front = face(node, card.front, back: false, width: inner, env: env.inner)
        let back = face(node, card.back, back: true, width: inner, env: env.on(env.surface.inBox).inner)
        let content = max(front.height, back.height)
        let height = max(ProgramFlashcard.least, content + ProgramFlashcard.top + ProgramFlashcard.bottom)
        let hintLook = ProgramStyle.look(.tick, narrow: env.narrow)
        let hint = ProgramText.plain(card.hint, look: hintLook, color: Theme.mutedStrongerForeground, options: .init(alignment: .center))
        let hintFrame = CGRect(x: 0, y: height - ProgramFlashcard.hintBottom - hintLook.lineHeight, width: width, height: hintLook.lineHeight)
        var texts = ["hint": ProgramTextPart(text: hint, frame: hintFrame, lift: ControlText.lift(hintLook.font, lineHeight: hintLook.lineHeight))]
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        for (side, laid) in [("front", front), ("back", back)] {
            let top = ProgramFlashcard.top + ((height - ProgramFlashcard.top - ProgramFlashcard.bottom - laid.height) / 2).rounded()
            if let text = laid.text {
                texts[side] = ProgramTextPart(
                    text: text.text, frame: text.frame.offsetBy(dx: ProgramFlashcard.sides, dy: top), lift: text.lift)
            }
            for (box, origin) in zip(laid.boxes, laid.origins) {
                boxes.append(box)
                origins.append(CGPoint(x: ProgramFlashcard.sides + origin.x, y: top + origin.y))
            }
        }
        var flip: CanvasFlip?
        if case .flip(let motion)? = scene(card.canvas, width: width)?.motion { flip = motion }
        return ProgramBox(
            node, size: CGSize(width: width, height: height), children: boxes, origins: origins, surface: env.surface, inner: env.surface.inBox,
            texts: texts, extra: ProgramFlashcardParts(front: front.boxes.count, flip: flip))
    }

    /// One face of a flashcard: its word, or its components in a centred column 8 apart.
    private static func face(
        _ node: UiNode, _ slot: UiSlot, back: Bool, width: CGFloat, env: ProgramEnv
    ) -> (text: ProgramTextPart?, boxes: [ProgramBox], origins: [CGPoint], height: CGFloat) {
        var y: CGFloat = 0
        var part: ProgramTextPart?
        if let rich = slot.text, !rich.text.isEmpty {
            let look = ProgramStyle.look(slot.style, narrow: env.narrow)
            let text = ProgramText.set(rich, look: look, color: Theme.foreground, options: .init(alignment: .center))
            let height = ProgramText.height(text, width: width, lineHeight: look.lineHeight)
            part = ProgramTextPart(text: text, frame: CGRect(x: 0, y: 0, width: width, height: height), lift: ControlText.lift(look.font, lineHeight: look.lineHeight))
            y = height
        }
        var childEnv = env
        childEnv.align = .center
        var boxes: [ProgramBox] = []
        var origins: [CGPoint] = []
        for child in slot.children {
            if y > 0 { y += ProgramFlashcard.gap }
            let box = layout(child, width: width, fill: false, env: childEnv)
            boxes.append(box)
            origins.append(CGPoint(x: ((width - box.size.width) / 2).rounded(), y: y))
            y += box.size.height
        }
        return (part, boxes, origins, y)
    }
}

/// A carousel's track: how wide its items are, how far apart, and how long it is.
struct ProgramCarousel {
    let item: CGFloat
    let gap: CGFloat
    let track: CGFloat
}

enum ProgramLink {
    static let arrowGap: CGFloat = 2

    static func arrowSide(_ look: ProgramStyle.Look) -> CGFloat {
        (look.font.pointSize * 0.85).rounded()
    }
}

enum ProgramQuote {
    static let barX: CGFloat = 1
    static let barWidth: CGFloat = 2
    static let inset: CGFloat = 14
    static let citeGap: CGFloat = 4
}

enum ProgramFlashcard {
    static let least: CGFloat = 168
    static let top: CGFloat = 20
    static let sides: CGFloat = 18
    static let bottom: CGFloat = 30
    static let gap: CGFloat = 8
    static let hintBottom: CGFloat = 10
}

/// Chips: a Badge is Motile's `Chip`.
enum ProgramBadge {
    static let padding: CGFloat = 7
    static let iconSize = IconSize.sm
    static let gap: CGFloat = 5
    static let font = PlatformFont.ui(11.5, weight: .medium)

    static func size(_ badge: UiKind.Badge) -> CGSize {
        let words = ProgramText.width(ControlText.line(badge.text, font: font, color: Theme.foreground))
        let icon = badge.icon == nil ? 0 : PlatformImage.symbolSide(iconSize) + gap
        return CGSize(width: ceil(padding * 2 + icon + words), height: Chip.height)
    }
}

/// Code as prose draws a code box: a header with its language and a copy button, the code under
/// it wrapped at any character.
enum ProgramCode {
    static let inset: CGFloat = 14
    static let bottom: CGFloat = 12

    static func text(_ content: CodeContent) -> NSAttributedString {
        let text = NSMutableAttributedString(attributedString: content.attributed)
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = Theme.codeLineHeight
        style.maximumLineHeight = Theme.codeLineHeight
        style.lineBreakMode = .byCharWrapping
        text.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: text.length))
        return text
    }

    static func height(_ content: CodeContent, width: CGFloat) -> CGFloat {
        let body = max(Theme.codeLineHeight, ControlText.height(text(content), width: max(1, width - 2 * inset)))
        return CodeHeader.height + body + bottom
    }
}

/// The paragraphs of a Markdown component, each set as a prose row's, or a code box.
enum ProgramProse {
    struct Block {
        let frame: CGRect
        let prose: NSAttributedString?
        let code: CodeContent?
    }
}
