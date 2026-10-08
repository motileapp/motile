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
        table.registerForDraggedTypes([Coordinator.rowType])
        table.setDraggingSourceOperationMask(.move, forLocal: true)
        table.draggingDestinationFeedbackStyle = .gap

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

        static var rowType: NSPasteboard.PasteboardType { NSPasteboard.PasteboardType("app.motile.row") }

        var list: RecycledList?
        var store: AppStore?
        var surface = Surface.background
        var scrollTarget: Item.ID?
        private var ids: [Item.ID] = []
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

        @objc func clicked(_ table: NSTableView) {
            guard let list, let item = list.items[safe: table.clickedRow] else { return }
            list.clicked(item)
        }

        /// Reloads the table when its rows are others, or else only redraws the rows it has.
        func show(in table: NSTableView) {
            let ids = list?.items.map(\.id) ?? []
            guard ids == self.ids else {
                self.ids = ids
                table.reloadData()
                return
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

        func tableView(_ table: NSTableView, pasteboardWriterForRow row: Int) -> NSPasteboardWriting? {
            guard let list, let item = list.items[safe: row], list.movable(item) else { return nil }
            let written = NSPasteboardItem()
            written.setString(String(row), forType: Self.rowType)
            return written
        }

        /// The other rows part where the dragged one can land: among the rows that move, or
        /// right after the last of them.
        func tableView(
            _ table: NSTableView, validateDrop info: NSDraggingInfo, proposedRow row: Int, proposedDropOperation: NSTableView.DropOperation
        ) -> NSDragOperation {
            guard info.draggingSource as? NSTableView === table, let from = draggedRow(info), let to = place(of: from, above: row) else {
                return []
            }
            table.setDropRow(to.row, dropOperation: .above)
            return .move
        }

        func tableView(_ table: NSTableView, acceptDrop info: NSDraggingInfo, row: Int, dropOperation: NSTableView.DropOperation) -> Bool {
            guard let list, let from = draggedRow(info), let item = list.items[safe: from], let to = place(of: from, above: row),
                  to.index != from
            else { return false }
            ids.remove(at: from)
            ids.insert(item.id, at: to.index)
            table.moveRow(at: from, to: to.index)
            list.moved(item, to.index)
            return true
        }

        private func draggedRow(_ info: NSDraggingInfo) -> Int? {
            guard let written = info.draggingPasteboard.string(forType: Self.rowType), let row = Int(written) else { return nil }
            return list?.items.indices.contains(row) == true ? row : nil
        }

        /// Where the dragged row would land when dropped above `row`: in the list as it is, and in
        /// the list without it. Nowhere outside the rows that move.
        private func place(of from: Int, above row: Int) -> (row: Int, index: Int)? {
            guard let list else { return nil }
            let index = row > from ? row - 1 : row
            var others = list.items
            others.remove(at: from)
            guard let first = others.firstIndex(where: list.movable), let last = others.lastIndex(where: list.movable),
                  index >= first, index <= last + 1
            else { return nil }
            return (index >= from ? index + 1 : index, index)
        }

        private func content(_ row: Int) -> RecycledRow<Row>? {
            guard let list, let store, let item = list.items[safe: row] else { return nil }
            return RecycledRow(row: list.row(item), store: store, surface: surface)
        }
    }
}

/// Clicking a row never takes the keyboard from the composer.
private final class RecycledTable: NSTableView {
    override var acceptsFirstResponder: Bool { false }

    override func validateProposedFirstResponder(_ responder: NSResponder, for event: NSEvent?) -> Bool { true }
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
