import SwiftUI
import UniformTypeIdentifiers

/// A row or a column of views that can be dragged into other places among them, as the
/// sidebar's rows can. On the Mac a view that is pulled lifts, the others it passes slide out of
/// its way, and let go it slides into its place. On iOS a hold lifts it, and the others make way
/// as it passes them.
@Observable
final class Reorder {
    fileprivate let axis: Axis
    fileprivate let space = UUID()
    fileprivate(set) var lifted: String?
    fileprivate var translation: CGFloat = 0
    /// Where the lifted view would land, in the views without it.
    fileprivate var landing = 0
    @ObservationIgnored fileprivate var frames: [String: CGRect] = [:]
    @ObservationIgnored fileprivate var order: [String] = []

    init(_ axis: Axis) {
        self.axis = axis
    }

    fileprivate func middle(_ frame: CGRect) -> CGFloat { axis == .horizontal ? frame.midX : frame.midY }
    fileprivate func length(_ frame: CGRect) -> CGFloat { axis == .horizontal ? frame.width : frame.height }

    fileprivate func drag(_ id: String, among order: [String], by translation: CGSize) {
        guard let frame = frames[id] else { return }
        if lifted != id {
            self.order = order
            landing = order.firstIndex(of: id) ?? 0
            withAnimation(.easeOut(duration: 0.15)) { lifted = id }
        }
        self.translation = axis == .horizontal ? translation.width : translation.height
        let center = middle(frame) + self.translation
        let landing = order.filter { $0 != id }.count { other in frames[other].map { middle($0) < center } ?? false }
        if landing != self.landing { self.landing = landing }
    }

    fileprivate func drop(_ move: (String, Int) -> Void) {
        guard let lifted else { return }
        let from = order.firstIndex(of: lifted)
        withAnimation(.easeOut(duration: 0.25)) {
            if landing != from { move(lifted, landing) }
            self.lifted = nil
            translation = 0
        }
    }

    /// How far the view slides out of the way of the lifted one.
    fileprivate func shift(_ id: String) -> CGFloat {
        guard let lifted, lifted != id, let from = order.firstIndex(of: lifted), let index = order.firstIndex(of: id),
              let size = frames[lifted].map(length)
        else { return 0 }
        if index > from && index <= landing { return -size }
        if index < from && index >= landing { return size }
        return 0
    }
}

extension View {
    /// The views of `reorder` are laid out in this one.
    func reorderSpace(_ reorder: Reorder) -> some View {
        coordinateSpace(.named(reorder.space))
    }

    /// Lets the view, one of `order`, be dragged into another place among them. `move` is told
    /// where it landed, in the views without it.
    func reorderable(_ id: String, among order: [String], in reorder: Reorder, move: @escaping (String, Int) -> Void) -> some View {
        modifier(Reorderable(id: id, order: order, reorder: reorder, move: move))
    }
}

private struct Reorderable: ViewModifier {
    static let liftedScale: CGFloat = 0.96
    let id: String
    let order: [String]
    let reorder: Reorder
    let move: (String, Int) -> Void

    #if os(macOS)
    func body(content: Content) -> some View {
        let lifted = reorder.lifted == id
        let travel = lifted ? reorder.translation : 0
        let shift = reorder.shift(id)
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(reorder.space)) } action: { frame in
                // What is measured includes the offsets of a drag, so only where the views lie before it counts.
                guard reorder.lifted == nil else { return }
                reorder.frames[id] = frame
            }
            .animation(.easeOut(duration: 0.2)) {
                $0.offset(x: reorder.axis == .horizontal ? shift : 0, y: reorder.axis == .vertical ? shift : 0)
            }
            .offset(x: reorder.axis == .horizontal ? travel : 0, y: reorder.axis == .vertical ? travel : 0)
            .scaleEffect(lifted ? Self.liftedScale : 1)
            .zIndex(lifted ? 1 : 0)
            .highPriorityGesture(
                DragGesture(minimumDistance: 3, coordinateSpace: .global)
                    .onChanged { reorder.drag(id, among: order, by: $0.translation) }
                    .onEnded { _ in reorder.drop(move) }
            )
    }
    #else
    func body(content: Content) -> some View {
        content
            .onDrag {
                reorder.lifted = id
                return NSItemProvider(object: id as NSString)
            }
            .onDrop(of: [.text], delegate: ReorderDrop(id: id, order: order, reorder: reorder, move: move))
    }
    #endif
}

#if os(iOS)
/// Moves the lifted view into the place of the one it passes over.
private struct ReorderDrop: DropDelegate {
    let id: String
    let order: [String]
    let reorder: Reorder
    let move: (String, Int) -> Void

    func dropEntered(info: DropInfo) {
        guard let lifted = reorder.lifted, lifted != id, let index = order.firstIndex(of: id) else { return }
        withAnimation(.easeOut(duration: 0.2)) { move(lifted, index) }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        reorder.lifted = nil
        return true
    }
}
#endif
