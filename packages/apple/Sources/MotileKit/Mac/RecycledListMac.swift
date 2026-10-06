#if os(macOS)
import AppKit
import SwiftUI

/// A list that only has views for the rows on screen and reuses them as it scrolls, so that it
/// goes through thousands of rows as fast as through ten. Its rows are all as tall.
struct RecycledList<Item: Identifiable, Row: View>: NSViewRepresentable {
    let items: [Item]
    let rowHeight: CGFloat
    var bottomInset: CGFloat = 0
    /// When it changes, the list scrolls to its row.
    var scrollTarget: Item.ID?
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
        scroll.contentInsets.bottom = bottomInset
        table.rowHeight = rowHeight
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
        private let cellID = NSUserInterfaceItemIdentifier("row")

        func numberOfRows(in table: NSTableView) -> Int { ids.count }

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

        private func content(_ row: Int) -> RecycledRow<Row>? {
            guard let list, let store, list.items.indices.contains(row) else { return nil }
            return RecycledRow(row: list.row(list.items[row]), store: store, surface: surface)
        }
    }
}

/// The rows take their own clicks, and clicking them never takes the keyboard from the composer.
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
#endif
