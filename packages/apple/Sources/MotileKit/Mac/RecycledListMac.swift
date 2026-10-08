#if os(macOS)
import AppKit
import SwiftUI

/// A list that only has views for the rows on screen and reuses them as it scrolls, so that it
/// goes through thousands of rows as fast as through ten. Each row is as tall as `height` says.
struct RecycledList<Item: Identifiable, Row: View>: NSViewRepresentable {
    let items: [Item]
    let height: (Item) -> CGFloat
    var topInset: CGFloat = 0
    var bottomInset: CGFloat = 0
    /// When it changes, the list scrolls to its row.
    var scrollTarget: Item.ID?
    /// A click on a row that doesn't take it itself.
    var clicked: (Item) -> Void = { _ in }
    /// Whether the row can be dragged into another place among the rows that can.
    var movable: (Item) -> Bool = { _ in false }
    /// The row was dropped where `index` is in `items` without it.
    var moved: (Item, Int) -> Void = { _, _ in }
    @ViewBuilder let row: (Item) -> Row
    @Environment(AppStore.self) private var store

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSScrollView {
        let table = RecycledTable()
        table.headerView = nil
        table.style = .plain
        table.backgroundColor = .clear
        table.intercellSpacing = .zero
        table.selectionHighlightStyle = .none
        table.allowsTypeSelect = false
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("row"))
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.columnAutoresizingStyle = .firstColumnOnlyAutoresizingStyle
        table.dataSource = context.coordinator
        table.delegate = context.coordinator
        table.target = context.coordinator
        table.action = #selector(Coordinator.clicked(_:))
        table.canDrag = { [weak coordinator = context.coordinator] in coordinator?.movable($0) ?? false }
        table.clicked = { [weak coordinator = context.coordinator] in coordinator?.clicked(row: $0) }
        table.dropped = { [weak coordinator = context.coordinator, weak table] from, index in
            guard let table else { return }
            coordinator?.dropped(from, at: index, in: table)
        }

        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.automaticallyAdjustsContentInsets = false
        context.coordinator.scrollTarget = scrollTarget
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let table = scroll.documentView as? NSTableView else { return }
        let coordinator = context.coordinator
        coordinator.list = self
        coordinator.store = store
        coordinator.surface = context.environment.surface
        scroll.contentInsets.top = topInset
        scroll.contentInsets.bottom = bottomInset
        if let first = items.first { table.rowHeight = height(first) }
        coordinator.show(in: table)
        coordinator.scroll(scroll, table: table, to: scrollTarget)
    }

    /// Takes the room it is offered, so that SwiftUI doesn't measure the rows through Auto Layout.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSScrollView, context: Context) -> CGSize? {
        proposal.replacingUnspecifiedDimensions()
    }

    final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
        typealias Cell = RecycledCell<RecycledRow<Row>>

        var list: RecycledList?
        var store: AppStore?
        var surface = Surface.background
        var scrollTarget: Item.ID?
        private var ids: [Item.ID] = []
        private var heights: [CGFloat] = []
        private let cellID = NSUserInterfaceItemIdentifier("row")

        func numberOfRows(in table: NSTableView) -> Int { ids.count }

        func tableView(_ table: NSTableView, heightOfRow row: Int) -> CGFloat {
            guard let list, let item = list.items[safe: row] else { return table.rowHeight }
            return list.height(item)
        }

        func tableView(_ table: NSTableView, viewFor column: NSTableColumn?, row: Int) -> NSView? {
            guard let content = content(row) else { return nil }
            if let cell = table.makeView(withIdentifier: cellID, owner: nil) as? Cell {
                cell.host.rootView = content
                return cell
            }
            let cell = Cell(content)
            cell.identifier = cellID
            return cell
        }

        func tableView(_ table: NSTableView, shouldSelectRow row: Int) -> Bool { false }

        @objc func clicked(_ table: NSTableView) { clicked(row: table.clickedRow) }

        func clicked(row: Int) {
            guard let list, let item = list.items[safe: row] else { return }
            list.clicked(item)
        }

        /// Reloads the table when its rows are others, or else only redraws the rows it has,
        /// resized where their heights changed.
        func show(in table: NSTableView) {
            let ids = list?.items.map(\.id) ?? []
            let heights = list?.items.map { list?.height($0) ?? 0 } ?? []
            guard ids == self.ids else {
                self.ids = ids
                self.heights = heights
                (table as? RecycledTable)?.cancelLift()
                table.reloadData()
                return
            }
            if heights != self.heights {
                let changed = IndexSet(heights.indices.filter { heights[$0] != self.heights[safe: $0] })
                self.heights = heights
                table.noteHeightOfRows(withIndexesChanged: changed)
            }
            table.enumerateAvailableRowViews { rowView, row in
                guard let cell = rowView.view(atColumn: 0) as? Cell, let content = content(row) else { return }
                cell.host.rootView = content
            }
        }

        /// Brings the target's row into sight once it is in the list.
        func scroll(_ scroll: NSScrollView, table: NSTableView, to target: Item.ID?) {
            guard target != scrollTarget else { return }
            guard let target else {
                scrollTarget = nil
                return
            }
            guard let row = list?.items.firstIndex(where: { $0.id == target }) else { return }
            scrollTarget = target
            let clip = scroll.contentView
            let rect = table.rect(ofRow: row)
            var origin = clip.bounds.origin
            if rect.minY < clip.bounds.minY {
                origin.y = rect.minY
            } else if rect.maxY > clip.bounds.maxY {
                origin.y = rect.maxY - clip.bounds.height
            } else {
                return
            }
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.15
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                clip.animator().setBoundsOrigin(origin)
            }
        }

        // MARK: Moving a row

        func movable(_ row: Int) -> Bool {
            guard let list, let item = list.items[safe: row] else { return false }
            return list.movable(item)
        }

        /// The row at `from` landed where `index` is in the list without it. The rows are already
        /// drawn there, so the table's move must not animate.
        func dropped(_ from: Int, at index: Int, in table: NSTableView) {
            guard let list, let item = list.items[safe: from], index != from else { return }
            ids.insert(ids.remove(at: from), at: index)
            heights.insert(heights.remove(at: from), at: index)
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0
                table.beginUpdates()
                table.moveRow(at: from, to: index)
                table.endUpdates()
            }
            list.moved(item, index)
        }

        private func content(_ row: Int) -> RecycledRow<Row>? {
            guard let list, let store, let item = list.items[safe: row] else { return nil }
            return RecycledRow(row: list.row(item), store: store, surface: surface)
        }
    }
}

/// Clicking a row never takes the keyboard from the composer. A row that moves is picked up by
/// the table itself, not by a dragging session: it shrinks a little and follows the pointer, the
/// rows it passes slide out of its way, and let go it slides into its place and grows back.
/// Nothing is hidden or faded on the way.
private final class RecycledTable: NSTableView {
    var canDrag: (Int) -> Bool = { _ in false }
    var clicked: (Int) -> Void = { _ in }
    /// The row at `from` was let go where `index` is in the list without it.
    var dropped: (_ from: Int, _ index: Int) -> Void = { _, _ in }
    private var press: (row: Int, point: NSPoint)?
    private var lift: RowLift?

    override var acceptsFirstResponder: Bool { false }

    override func validateProposedFirstResponder(_ responder: NSResponder, for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        guard lift == nil else { return }
        let point = convert(event.locationInWindow, from: nil)
        let row = row(at: point)
        guard row >= 0, canDrag(row) else {
            super.mouseDown(with: event)
            return
        }
        press = (row, point)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let press else {
            super.mouseDragged(with: event)
            return
        }
        let point = convert(event.locationInWindow, from: nil)
        if lift == nil {
            guard hypot(point.x - press.point.x, point.y - press.point.y) >= 3 else { return }
            lift = RowLift(table: self, row: press.row, grip: press.point.y, movable: canDrag)
        }
        lift?.follow(point.y)
    }

    override func mouseUp(with event: NSEvent) {
        guard let press else {
            super.mouseUp(with: event)
            return
        }
        self.press = nil
        guard let lift else {
            clicked(press.row)
            return
        }
        lift.drop { [weak self] from, index in
            self?.lift = nil
            self?.dropped(from, index)
        }
    }

    /// Puts the rows back where they are laid out, for when the list changes under a lift.
    func cancelLift() {
        press = nil
        lift?.cancel()
        lift = nil
    }
}

/// A row on its way: where it was picked up, where it hangs, and where it would land. It is
/// moved with transforms only, so the table's layout stays as it was until it lands.
private final class RowLift {
    private static let scale: CGFloat = 0.96
    private let table: NSTableView
    private let from: Int
    /// How far below the row's top it was picked up.
    private let grip: CGFloat
    /// Every row as it is laid out.
    private let rects: [CGRect]
    /// Where its top can hang: over the rows that move.
    private let tops: ClosedRange<CGFloat>
    /// Where it can land, in the list without it.
    private let landings: ClosedRange<Int>
    private var index: Int

    init(table: NSTableView, row: Int, grip: CGFloat, movable: (Int) -> Bool) {
        self.table = table
        from = row
        index = row
        rects = (0..<table.numberOfRows).map(table.rect(ofRow:))
        self.grip = grip - rects[row].minY
        let movers = rects.indices.filter(movable)
        tops = rects[movers.first ?? row].minY...rects[movers.last ?? row].maxY - rects[row].height
        let others = movers.filter { $0 != row }.map { $0 > row ? $0 - 1 : $0 }
        landings = (others.first ?? row)...(others.last.map { $0 + 1 } ?? row)
        rowView?.layer?.zPosition = 1
        guard let cell, let layer = cell.layer else { return }
        animate(layer, to: Self.scaled(Self.scale, in: cell.bounds.size), duration: 0.15)
    }

    private var rowView: NSTableRowView? { table.rowView(atRow: from, makeIfNecessary: false) }
    private var cell: NSView? { rowView?.view(atColumn: 0) as? NSView }

    func follow(_ y: CGFloat) {
        let top = min(max(y - grip, tops.lowerBound), tops.upperBound)
        rowView?.layer?.transform = CATransform3DMakeTranslation(0, top - rects[from].minY, 0)
        let center = top + rects[from].height / 2
        let landing = index
        while index < landings.upperBound, center >= middle(ofRowBelow: index) { index += 1 }
        while index > landings.lowerBound, center <= middle(ofRowAbove: index) { index -= 1 }
        guard landing != index else { return }
        table.enumerateAvailableRowViews { [self] rowView, row in
            guard row != from, let layer = rowView.layer else { return }
            animate(layer, to: CATransform3DMakeTranslation(0, shift(row), 0), duration: 0.2)
        }
    }

    /// How far the row is slid out of the way, with the gap where it is.
    private func shift(_ row: Int) -> CGFloat {
        let height = rects[from].height
        return row > from && row <= index ? -height : row < from && row >= index ? height : 0
    }

    /// The middle of the row right below the gap at `landing`, as it is drawn.
    private func middle(ofRowBelow landing: Int) -> CGFloat {
        let row = landing < from ? landing : landing + 1
        return rects[row].midY + shift(row)
    }

    /// The middle of the row right above the gap at `landing`, as it is drawn.
    private func middle(ofRowAbove landing: Int) -> CGFloat {
        let row = landing <= from ? landing - 1 : landing
        return rects[row].midY + shift(row)
    }

    /// Slides the row into its place and grows it back, then says where it landed.
    func drop(_ landed: @escaping (_ from: Int, _ index: Int) -> Void) {
        let top = index <= from ? rects[index].minY : rects[index].maxY - rects[from].height
        CATransaction.begin()
        CATransaction.setCompletionBlock { [self] in
            cancel()
            landed(from, index)
        }
        if let layer = rowView?.layer {
            animate(layer, to: CATransform3DMakeTranslation(0, top - rects[from].minY, 0), duration: 0.25)
        }
        if let layer = cell?.layer {
            animate(layer, to: CATransform3DIdentity, duration: 0.25)
        }
        CATransaction.commit()
    }

    /// Puts every row back where it is laid out.
    func cancel() {
        table.enumerateAvailableRowViews { rowView, _ in
            for layer in [rowView.layer, (rowView.view(atColumn: 0) as? NSView)?.layer] {
                guard let layer else { continue }
                layer.removeAllAnimations()
                layer.transform = CATransform3DIdentity
                layer.zPosition = 0
            }
        }
    }

    private func animate(_ layer: CALayer, to transform: CATransform3D, duration: CFTimeInterval) {
        let animation = CABasicAnimation(keyPath: "transform")
        animation.fromValue = layer.presentation()?.transform ?? layer.transform
        animation.toValue = transform
        animation.duration = duration
        animation.timingFunction = CAMediaTimingFunction(name: .easeOut)
        layer.transform = transform
        layer.add(animation, forKey: "transform")
    }

    private static func scaled(_ scale: CGFloat, in size: CGSize) -> CATransform3D {
        var transform = CATransform3DMakeTranslation(size.width / 2, size.height / 2, 0)
        transform = CATransform3DScale(transform, scale, scale, 1)
        return CATransform3DTranslate(transform, -size.width / 2, -size.height / 2, 0)
    }
}

final class RecycledCell<Content: View>: NSView {
    let host: NSHostingView<Content>

    init(_ content: Content) {
        host = NSHostingView(rootView: content)
        host.sizingOptions = []
        host.safeAreaRegions = []
        super.init(frame: .zero)
        addSubview(host)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layout() {
        super.layout()
        host.frame = bounds
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
#endif
