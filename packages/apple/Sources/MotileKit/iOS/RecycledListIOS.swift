#if os(iOS)
import SwiftUI
import UIKit

/// A list that only has views for the rows on screen and reuses them as it scrolls, so that it
/// goes through thousands of rows as fast as through ten. Each row is as tall as it needs.
struct RecycledList<Item: Identifiable, Row: View>: UIViewRepresentable {
    let items: [Item]
    var bottomInset: CGFloat = 0
    /// When it changes, the list scrolls to its row.
    var scrollTarget: Item.ID?
    /// A finger has started to scroll the list, or to drag a row.
    var scrolled: () -> Void = {}
    /// The shape a row lifts in when it is held: its light, inset from the row's edges.
    var rowInset = UIEdgeInsets.zero
    var rowRadius: CGFloat = 0
    /// Whether the row can be held and dragged into another place among the rows that can.
    var movable: (Item) -> Bool = { _ in false }
    /// The row was dropped where `index` is in `items` without it.
    var moved: (Item, Int) -> Void = { _, _ in }
    /// What a hold on the row offers, in groups a line divides.
    var menu: (Item) -> [[RowAction]] = { _ in [] }
    @ViewBuilder let row: (Item) -> Row
    @Environment(AppStore.self) private var store

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> UICollectionView {
        var layout = UICollectionLayoutListConfiguration(appearance: .plain)
        layout.showsSeparators = false
        layout.backgroundColor = .clear
        let view = UICollectionView(frame: .zero, collectionViewLayout: UICollectionViewCompositionalLayout.list(using: layout))
        view.backgroundColor = .clear
        view.allowsSelection = false
        view.keyboardDismissMode = .onDrag
        view.delegate = context.coordinator
        view.dragDelegate = context.coordinator
        view.dropDelegate = context.coordinator
        view.dragInteractionEnabled = true
        context.coordinator.attach(view)
        context.coordinator.scrollTarget = scrollTarget
        return view
    }

    func updateUIView(_ view: UICollectionView, context: Context) {
        let coordinator = context.coordinator
        coordinator.list = self
        coordinator.store = store
        coordinator.surface = context.environment.surface
        view.contentInset.bottom = bottomInset
        coordinator.show(in: view)
        coordinator.scroll(view, to: scrollTarget)
    }

    /// Takes the room it is offered, so that SwiftUI doesn't measure the rows through Auto Layout.
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UICollectionView, context: Context) -> CGSize? {
        proposal.replacingUnspecifiedDimensions()
    }

    final class Coordinator: NSObject, UICollectionViewDelegate, UICollectionViewDragDelegate, UICollectionViewDropDelegate {
        var list: RecycledList?
        var store: AppStore?
        var surface = Surface.background
        var scrollTarget: Item.ID?
        private var source: UICollectionViewDiffableDataSource<Int, Item.ID>?
        private var ids: [Item.ID] = []
        /// Counts the list's updates, so that a cell made ahead of time is redrawn if one came since.
        private var generation = 0
        /// The row being dragged, and the rows it can land among.
        private var dragged: Item.ID?
        private var landing: ClosedRange<Int>?

        func attach(_ view: UICollectionView) {
            let registration = UICollectionView.CellRegistration<RecycledCell, Item.ID> { [weak self] cell, indexPath, _ in
                self?.configure(cell, at: indexPath.item)
            }
            let source = UICollectionViewDiffableDataSource<Int, Item.ID>(collectionView: view) { view, indexPath, id in
                view.dequeueConfiguredReusableCell(using: registration, for: indexPath, item: id)
            }
            source.reorderingHandlers.canReorderItem = { [weak self] id in self?.movable(id) == true }
            source.reorderingHandlers.didReorder = { [weak self] transaction in self?.reordered(to: transaction.finalSnapshot.itemIdentifiers) }
            self.source = source
        }

        /// Applies the rows when they are others, and redraws the rows on screen.
        func show(in view: UICollectionView) {
            generation += 1
            let ids = list?.items.map(\.id) ?? []
            if ids != self.ids {
                self.ids = ids
                var snapshot = NSDiffableDataSourceSnapshot<Int, Item.ID>()
                snapshot.appendSections([0])
                snapshot.appendItems(ids)
                source?.apply(snapshot, animatingDifferences: false)
            }
            for indexPath in view.indexPathsForVisibleItems {
                guard let cell = view.cellForItem(at: indexPath) as? RecycledCell else { continue }
                configure(cell, at: indexPath.item)
            }
        }

        func collectionView(_ view: UICollectionView, willDisplay cell: UICollectionViewCell, forItemAt indexPath: IndexPath) {
            guard let cell = cell as? RecycledCell, cell.generation != generation else { return }
            configure(cell, at: indexPath.item)
        }

        func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
            list?.scrolled()
        }

        /// Brings the target's row into sight once it is in the list.
        func scroll(_ view: UICollectionView, to target: Item.ID?) {
            guard target != scrollTarget else { return }
            guard let target else {
                scrollTarget = nil
                return
            }
            guard let index = list?.items.firstIndex(where: { $0.id == target }) else { return }
            scrollTarget = target
            view.layoutIfNeeded()
            guard let frame = view.layoutAttributesForItem(at: IndexPath(item: index, section: 0))?.frame else { return }
            view.scrollRectToVisible(frame, animated: true)
        }

        private func configure(_ cell: RecycledCell, at index: Int) {
            guard let list, let store, let item = list.items[safe: index] else { return }
            cell.generation = generation
            let row = RecycledRow(row: list.row(item), store: store, surface: surface)
            cell.contentConfiguration = UIHostingConfiguration { row }
                .margins(.all, 0)
                .minSize(width: 0, height: 0)
        }

        private func item(_ id: Item.ID) -> Item? {
            list?.items.first { $0.id == id }
        }

        private func movable(_ id: Item.ID) -> Bool {
            guard let list, let item = item(id) else { return false }
            return list.movable(item)
        }

        // MARK: Moving a row

        func collectionView(_ view: UICollectionView, itemsForBeginning session: UIDragSession, at indexPath: IndexPath) -> [UIDragItem] {
            guard let list, let item = list.items[safe: indexPath.item], list.movable(item) else { return [] }
            let movable = list.items.indices.filter { list.movable(list.items[$0]) }
            guard let first = movable.first, let last = movable.last else { return [] }
            dragged = item.id
            landing = first...last
            return [UIDragItem(itemProvider: NSItemProvider())]
        }

        func collectionView(_ view: UICollectionView, dragSessionIsRestrictedToDraggingApplication session: UIDragSession) -> Bool { true }

        func collectionView(_ view: UICollectionView, dragSessionWillBegin session: UIDragSession) {
            list?.scrolled()
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        }

        func collectionView(_ view: UICollectionView, dragSessionDidEnd session: UIDragSession) {
            dragged = nil
            landing = nil
        }

        func collectionView(_ view: UICollectionView, dragPreviewParametersForItemAt indexPath: IndexPath) -> UIDragPreviewParameters? {
            lifted(view, at: indexPath, UIDragPreviewParameters())
        }

        func collectionView(_ view: UICollectionView, dropPreviewParametersForItemAt indexPath: IndexPath) -> UIDragPreviewParameters? {
            lifted(view, at: indexPath, UIDragPreviewParameters())
        }

        func collectionView(_ view: UICollectionView, canHandle session: UIDropSession) -> Bool {
            session.localDragSession != nil && dragged != nil
        }

        /// The other rows part where the dragged one can land: among the rows that move. Past
        /// them, it lands at the nearest end.
        func collectionView(
            _ view: UICollectionView, dropSessionDidUpdate session: UIDropSession, withDestinationIndexPath destination: IndexPath?
        ) -> UICollectionViewDropProposal {
            guard dragged != nil, let landing else { return UICollectionViewDropProposal(operation: .cancel) }
            guard let destination, landing.contains(destination.item) else {
                return UICollectionViewDropProposal(operation: .move, intent: .unspecified)
            }
            return UICollectionViewDropProposal(operation: .move, intent: .insertAtDestinationIndexPath)
        }

        func collectionView(_ view: UICollectionView, performDropWith coordinator: UICollectionViewDropCoordinator) {
            guard let drop = coordinator.items.first,
                  let index = lands(view, at: coordinator.destinationIndexPath, point: coordinator.session.location(in: view))
            else { return }
            coordinator.drop(drop.dragItem, toItemAt: IndexPath(item: index, section: 0))
        }

        /// Where a drop lands: where the finger is among the rows that move, or at the end it is past.
        private func lands(_ view: UICollectionView, at destination: IndexPath?, point: CGPoint) -> Int? {
            guard let landing else { return nil }
            if let destination { return min(max(destination.item, landing.lowerBound), landing.upperBound) }
            guard let first = view.layoutAttributesForItem(at: IndexPath(item: landing.lowerBound, section: 0)) else { return nil }
            return point.y < first.frame.midY ? landing.lowerBound : landing.upperBound
        }

        private func reordered(to ids: [Item.ID]) {
            self.ids = ids
            guard let list, let dragged, let item = item(dragged), let index = ids.firstIndex(of: dragged) else { return }
            list.moved(item, index)
        }

        /// The row lifts in the shape of its light, on the colour its list lies on.
        private func lifted<Parameters: UIPreviewParameters>(_ view: UICollectionView, at indexPath: IndexPath, _ parameters: Parameters) -> Parameters? {
            guard let list, let cell = view.cellForItem(at: indexPath) else { return nil }
            parameters.visiblePath = UIBezierPath(roundedRect: cell.bounds.inset(by: list.rowInset), cornerRadius: list.rowRadius)
            parameters.backgroundColor = surface.rowAccent
            return parameters
        }

        // MARK: The row's menu

        func collectionView(
            _ view: UICollectionView, contextMenuConfigurationForItemsAt indexPaths: [IndexPath], point: CGPoint
        ) -> UIContextMenuConfiguration? {
            guard let list, let indexPath = indexPaths.first, let item = list.items[safe: indexPath.item] else { return nil }
            let groups = list.menu(item)
            guard !groups.isEmpty else { return nil }
            return UIContextMenuConfiguration { _ in
                UIMenu(children: groups.map { group in
                    UIMenu(options: .displayInline, children: group.map { action in
                        var attributes = UIMenuElement.Attributes()
                        if action.destructive { attributes.insert(.destructive) }
                        if action.disabled { attributes.insert(.disabled) }
                        return UIAction(title: action.title, attributes: attributes) { _ in action.perform() }
                    })
                })
            }
        }

        func collectionView(
            _ view: UICollectionView, contextMenuConfiguration configuration: UIContextMenuConfiguration, highlightPreviewForItemAt indexPath: IndexPath
        ) -> UITargetedPreview? {
            held(view, at: indexPath)
        }

        func collectionView(
            _ view: UICollectionView, contextMenuConfiguration configuration: UIContextMenuConfiguration, dismissalPreviewForItemAt indexPath: IndexPath
        ) -> UITargetedPreview? {
            held(view, at: indexPath)
        }

        private func held(_ view: UICollectionView, at indexPath: IndexPath) -> UITargetedPreview? {
            guard let cell = view.cellForItem(at: indexPath), let parameters = lifted(view, at: indexPath, UIPreviewParameters()) else { return nil }
            return UITargetedPreview(view: cell, parameters: parameters)
        }
    }
}

final class RecycledCell: UICollectionViewCell {
    var generation = 0
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
#endif
