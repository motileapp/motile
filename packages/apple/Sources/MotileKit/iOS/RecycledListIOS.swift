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
    /// A finger has started to scroll the list.
    var scrolled: () -> Void = {}
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

    final class Coordinator: NSObject, UICollectionViewDelegate {
        var list: RecycledList?
        var store: AppStore?
        var surface = Surface.background
        var scrollTarget: Item.ID?
        private var source: UICollectionViewDiffableDataSource<Int, Item.ID>?
        private var ids: [Item.ID] = []
        /// Counts the list's updates, so that a cell made ahead of time is redrawn if one came since.
        private var generation = 0

        func attach(_ view: UICollectionView) {
            let registration = UICollectionView.CellRegistration<RecycledCell, Item.ID> { [weak self] cell, indexPath, _ in
                self?.configure(cell, at: indexPath.item)
            }
            source = UICollectionViewDiffableDataSource(collectionView: view) { view, indexPath, id in
                view.dequeueConfiguredReusableCell(using: registration, for: indexPath, item: id)
            }
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
            guard let list, let store, list.items.indices.contains(index) else { return }
            cell.generation = generation
            let row = RecycledRow(row: list.row(list.items[index]), store: store, surface: surface)
            cell.contentConfiguration = UIHostingConfiguration { row }
                .margins(.all, 0)
                .minSize(width: 0, height: 0)
        }
    }
}

final class RecycledCell: UICollectionViewCell {
    var generation = 0
}
#endif
