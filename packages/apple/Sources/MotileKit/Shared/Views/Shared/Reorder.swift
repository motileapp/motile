import SwiftUI
import UniformTypeIdentifiers

/// Where a dragged item lands among others along a line: in the slot whose middle is nearest its
/// own, so a neighbour makes way once the item is halfway across it, whatever their lengths.
/// `spans` are where the items lay when it lifted and `from` is its place among them; a slot is
/// counted among the others.
enum ReorderSlot {
    /// How much nearer another slot must be before the item leaves the one it would land in, so a
    /// pointer that trembles on the edge doesn't move the others to and fro.
    static let hysteresis: CGFloat = 4

    /// Where the item starts when it lies in `slot`.
    static func start(of slot: Int, from: Int, spans: [ClosedRange<CGFloat>]) -> CGFloat {
        slot <= from ? spans[slot].lowerBound : spans[slot].upperBound - length(spans[from])
    }

    /// The slot for an item whose middle is at `middle`, among `slots`, staying in `current` unless
    /// another is nearer by more than the hysteresis.
    static func nearest(to middle: CGFloat, from: Int, spans: [ClosedRange<CGFloat>], current: Int, among slots: ClosedRange<Int>) -> Int {
        let half = length(spans[from]) / 2
        let distance = { (slot: Int) in abs(start(of: slot, from: from, spans: spans) + half - middle) }
        guard let best = slots.min(by: { distance($0) < distance($1) }) else { return current }
        guard slots.contains(current), distance(current) - distance(best) <= hysteresis else { return best }
        return current
    }

    private static func length(_ span: ClosedRange<CGFloat>) -> CGFloat { span.upperBound - span.lowerBound }
}

/// A row or a column of views that can be dragged into other places among them, as the sidebar's
/// rows can. A lifted view is hidden where it lies and a copy of it, on a light of its own, goes
/// with the pointer, while the others slide into the order it would leave. On the Mac a pull lifts
/// it and the copy floats over the stack; on iOS a hold lifts it as the system lifts what is
/// dragged, out of its menu when it has one. Where the views lie is worked out from their frames
/// when one lifted, so nothing that moves is measured while it moves.
@Observable
final class Reorder {
    fileprivate let axis: Axis
    fileprivate let space = UUID()
    /// The order the views lie in while one is dragged.
    private(set) var order: [String]?
    /// The view that is hidden while its copy is carried.
    private(set) var dragged: String?
    /// How far the copy is from where the dragged view lifted.
    fileprivate var travel: CGFloat = 0
    @ObservationIgnored private var settling = false
    /// Where each view is laid out.
    @ObservationIgnored private var frames: [String: CGRect] = [:]
    /// The frames when the view lifted, and where each lay along the axis, in their order then.
    @ObservationIgnored private var lifted: [String: CGRect] = [:]
    @ObservationIgnored private var spans: [ClosedRange<CGFloat>] = []
    @ObservationIgnored private var lift: (id: String, ids: [String], from: Int)?
    /// The slot the view would land in, among the others.
    @ObservationIgnored private var landing = 0

    init(_ axis: Axis) {
        self.axis = axis
    }

    /// A view is carried or slides into its place, so a press on it is no click.
    var isMoving: Bool { dragged != nil }

    /// The items in the order they lie in while one is dragged.
    func arranged<Item>(_ items: [Item], by id: (Item) -> String) -> [Item] {
        guard let order else { return items }
        let byID = Dictionary(items.map { (id($0), $0) }, uniquingKeysWith: { first, _ in first })
        guard byID.count == order.count, order.allSatisfy({ byID[$0] != nil }) else { return items }
        return order.compactMap { byID[$0] }
    }

    /// Where the copy of the dragged view floats.
    fileprivate var floating: CGRect? {
        guard let dragged, let frame = lifted[dragged] else { return nil }
        return axis == .horizontal ? frame.offsetBy(dx: travel, dy: 0) : frame.offsetBy(dx: 0, dy: travel)
    }

    fileprivate func measured(_ id: String, _ frame: CGRect) {
        frames[id] = frame
    }

    /// Lifts the view: hides it and keeps where all of them lie.
    fileprivate func pickUp(_ id: String, among ids: [String]) {
        guard dragged == nil, !settling, ids.contains(id), ids.allSatisfy({ frames[$0] != nil }) else { return }
        lifted = frames.filter { ids.contains($0.key) }
        spans = ids.compactMap { lifted[$0] }.map { start($0)...(start($0) + length($0)) }
        let from = ids.firstIndex(of: id) ?? 0
        lift = (id, ids, from)
        landing = from
        travel = 0
        order = ids
        dragged = id
    }

    /// Carries the copy `translation` from where the view lifted, and lets the others make way.
    fileprivate func carry(by translation: CGFloat) {
        guard let dragged, !settling, let lift, let frame = lifted[dragged] else { return }
        travel = translation
        let middle = start(frame) + translation + length(frame) / 2
        let landing = ReorderSlot.nearest(to: middle, from: lift.from, spans: spans, current: landing, among: 0...(spans.count - 1))
        guard landing != self.landing || order == nil else { return }
        self.landing = landing
        var order = lift.ids
        order.remove(at: lift.from)
        order.insert(dragged, at: landing)
        withAnimation(.easeOut(duration: 0.2)) { self.order = order }
    }

    /// Carries the copy with its middle at `point`.
    fileprivate func carry(to point: CGPoint) {
        guard let dragged, let frame = lifted[dragged] else { return }
        carry(by: axis == .horizontal ? point.x - frame.midX : point.y - frame.midY)
    }

    /// Lets the copy slide into the gap, then keeps the order.
    fileprivate func drop(_ move: @escaping (String, Int) -> Void) {
        guard let dragged, !settling, let lift, let frame = lifted[dragged] else { return }
        settling = true
        withAnimation(.easeOut(duration: 0.2)) {
            travel = ReorderSlot.start(of: landing, from: lift.from, spans: spans) - start(frame)
        } completion: { [weak self] in
            self?.keep(move)
        }
    }

    /// Keeps the order the views lie in, without moving them.
    fileprivate func keep(_ move: (String, Int) -> Void) {
        var still = Transaction()
        still.disablesAnimations = true
        withTransaction(still) {
            if let lift, order != nil, landing != lift.from {
                move(lift.id, landing)
            }
            reset()
        }
    }

    /// Puts the views back where they were, as when the copy is carried away.
    fileprivate func putBack() {
        guard lift != nil else { return }
        withAnimation(.easeOut(duration: 0.2)) {
            order = nil
            dragged = nil
        }
    }

    /// The copy came back: hides the view again.
    fileprivate func resume() {
        guard let lift, dragged == nil else { return }
        landing = lift.from
        order = lift.ids
        dragged = lift.id
    }

    /// A copy is carried, over the stack or away from it.
    fileprivate var carrying: Bool { lift != nil }

    /// How big the view is laid out.
    fileprivate func size(of id: String) -> CGSize? {
        frames[id]?.size
    }

    private func reset() {
        dragged = nil
        order = nil
        travel = 0
        settling = false
        lift = nil
    }

    private func start(_ frame: CGRect) -> CGFloat { axis == .horizontal ? frame.minX : frame.minY }
    private func length(_ frame: CGRect) -> CGFloat { axis == .horizontal ? frame.width : frame.height }
}

/// The light a lifted view lies on: its shape, inset from its edges.
struct ReorderLight {
    var radius: CGFloat = Radius.md
    var inset = EdgeInsets()
}

/// What the views of a stack share: the ids in their order, where one landed, and its copy.
private struct ReorderStack {
    var ids: [String] = []
    var move: (String, Int) -> Void = { _, _ in }
    var copy: (String) -> AnyView = { _ in AnyView(EmptyView()) }
    /// The views have menus, so on iOS they lift as the system lifts what is dragged, out of them.
    var menus = false
}

extension EnvironmentValues {
    @Entry fileprivate var reorderStack = ReorderStack()
}

extension View {
    /// The views `ids` of `reorder` are laid out in this one, and can be dragged into other places
    /// among them. `move` is told where one landed, in the views without it. `copy` draws the one
    /// that is carried, which lies on `light`. Views with `menus` lift out of them on iOS.
    func reorderSpace<Copy: View>(
        _ reorder: Reorder, ids: [String], light: ReorderLight = ReorderLight(), menus: Bool = false,
        move: @escaping (String, Int) -> Void, @ViewBuilder copy: @escaping (String) -> Copy
    ) -> some View {
        modifier(ReorderSpace(reorder: reorder, ids: ids, light: light, menus: menus, move: move, copy: copy))
    }

    /// The view `id` of the stack `reorder` lays out, which can be dragged.
    func reorderable(_ id: String, in reorder: Reorder) -> some View {
        modifier(Reorderable(id: id, reorder: reorder))
    }
}

private struct ReorderSpace<Copy: View>: ViewModifier {
    let reorder: Reorder
    let ids: [String]
    let light: ReorderLight
    let menus: Bool
    let move: (String, Int) -> Void
    let copy: (String) -> Copy
    @Environment(\.surface) private var surface
    /// What the views lie in, which the system's picture of a carried one is drawn without.
    @Environment(\.self) private var environment

    @ViewBuilder
    func body(content: Content) -> some View {
        #if os(macOS)
        let system = false
        #else
        let system = menus
        #endif
        let stack = ReorderStack(
            ids: ids, move: move,
            copy: { id in
                let size = reorder.size(of: id)
                return AnyView(lifted(id).frame(width: size?.width, height: size?.height).environment(\.self, environment))
            }, menus: system)
        let stacked = content
            .coordinateSpace(.named(reorder.space))
            .environment(\.reorderStack, stack)
        if system {
            #if os(iOS)
            stacked
                .contentShape(Rectangle())
                .onDrop(of: [.text], delegate: ReorderDrop(reorder: reorder, move: move))
            #endif
        } else {
            stacked.overlay(alignment: .topLeading) {
                if let id = reorder.dragged, let frame = reorder.floating {
                    lifted(id)
                        .frame(width: frame.width, height: frame.height)
                        .offset(x: frame.minX, y: frame.minY)
                        .allowsHitTesting(false)
                }
            }
        }
    }

    private func lifted(_ id: String) -> some View {
        let shape = RoundedRectangle(cornerRadius: light.radius, style: .continuous)
        return copy(id)
            .environment(\.row, .controlLit)
            .background {
                shape.fill(surface.color)
                    .overlay(shape.fill(surface.color(.controlLit)))
                    .shadow(.md)
                    .padding(light.inset)
            }
    }
}

private struct Reorderable: ViewModifier {
    let id: String
    let reorder: Reorder
    @Environment(\.reorderStack) private var stack

    func body(content: Content) -> some View {
        content
            .opacity(reorder.dragged == id ? 0 : 1)
            #if os(macOS)
            // The others don't light up under the pointer while it carries one.
            .allowsHitTesting(reorder.dragged == nil || reorder.dragged == id)
            #endif
            .contentShape(Rectangle())
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(reorder.space)) } action: { reorder.measured(id, $0) }
            .modifier(ReorderGesture(id: id, reorder: reorder, stack: stack))
    }
}

#if os(macOS)
/// A pull lifts the view, as it lifts a row of the sidebar.
private struct ReorderGesture: ViewModifier {
    let id: String
    let reorder: Reorder
    let stack: ReorderStack
    /// Ends with the gesture however it ends, also when something takes it.
    @GestureState private var pulling = false

    func body(content: Content) -> some View {
        content
            .highPriorityGesture(
                DragGesture(minimumDistance: 3, coordinateSpace: .named(reorder.space))
                    .updating($pulling) { _, pulling, _ in pulling = true }
                    .onChanged { value in
                        if reorder.dragged == nil { reorder.pickUp(id, among: stack.ids) }
                        reorder.carry(by: reorder.axis == .horizontal ? value.translation.width : value.translation.height)
                    }
                    .onEnded { _ in reorder.drop(stack.move) }
            )
            .onChange(of: pulling) { _, pulling in
                if !pulling { reorder.drop(stack.move) }
            }
    }
}
#else
/// A hold lifts the view and a move carries it. A view with a menu lifts as the system lifts what is
/// dragged, so that a hold that doesn't move brings up the menu and a move drags it out of it.
private struct ReorderGesture: ViewModifier {
    let id: String
    let reorder: Reorder
    let stack: ReorderStack
    /// Ends with the gesture however it ends, also when something takes it.
    @GestureState private var holding = false

    @ViewBuilder
    func body(content: Content) -> some View {
        if stack.menus {
            content
                .onDrag {
                    reorder.pickUp(id, among: stack.ids)
                    return NSItemProvider(object: id as NSString)
                } preview: {
                    stack.copy(id)
                }
                .sensoryFeedback(.selection, trigger: reorder.order ?? []) { _, _ in reorder.dragged == id }
        } else {
            content
                .simultaneousGesture(
                    LongPressGesture(minimumDuration: 0.3)
                        .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .named(reorder.space)))
                        .updating($holding) { value, holding, _ in
                            if case .second(true, _) = value { holding = true }
                        }
                        .onChanged { value in
                            guard case .second(true, let drag) = value else { return }
                            if reorder.dragged == nil { reorder.pickUp(id, among: stack.ids) }
                            guard let drag else { return }
                            reorder.carry(by: reorder.axis == .horizontal ? drag.translation.width : drag.translation.height)
                        }
                        .onEnded { _ in reorder.drop(stack.move) }
                )
                .onChange(of: holding) { _, holding in
                    if !holding { reorder.drop(stack.move) }
                }
                .sensoryFeedback(.impact(weight: .medium), trigger: reorder.dragged == id) { _, lifted in lifted }
                .sensoryFeedback(.selection, trigger: reorder.order ?? []) { _, _ in reorder.dragged == id }
        }
    }
}

/// Where the copy is carried over the stack, the others make way; let go there, the order is kept.
/// Carried away, everything goes back to where it was.
private struct ReorderDrop: DropDelegate {
    let reorder: Reorder
    let move: (String, Int) -> Void

    func validateDrop(info: DropInfo) -> Bool { reorder.carrying }

    func dropEntered(info: DropInfo) {
        reorder.resume()
        reorder.carry(to: info.location)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        reorder.carry(to: info.location)
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        reorder.putBack()
    }

    func performDrop(info: DropInfo) -> Bool {
        reorder.keep(move)
        return true
    }
}
#endif
