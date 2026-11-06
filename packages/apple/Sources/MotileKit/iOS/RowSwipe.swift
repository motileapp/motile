#if os(iOS)
import SwiftUI
import UIKit

/// A round button a swipe shows behind a sidebar row: it does something, or opens a menu.
struct SwipeAction {
    let symbol: Symbol
    let label: String
    let tint: Color
    /// The row leaves when it is done, unless the button won't take it away.
    var leaves = true
    var perform: (() -> Void)?
    var menu: AnyView?

    static func button(_ symbol: Symbol, _ label: String, tint: Color, leaves: Bool = true, perform: @escaping () -> Void) -> SwipeAction {
        SwipeAction(symbol: symbol, label: label, tint: tint, leaves: leaves, perform: perform)
    }

    static func menu(_ symbol: Symbol, _ label: String, tint: Color, @ViewBuilder content: () -> some View) -> SwipeAction {
        SwipeAction(symbol: symbol, label: label, tint: tint, menu: AnyView(content()))
    }
}

extension View {
    /// Lets a swipe to the right slide a sidebar row aside and show a round button behind it. A
    /// short swipe leaves the button there to tap, a long one does what the button does. The row
    /// leaves with a long swipe unless the button won't take it away.
    func rowSwipe(
        _ symbol: Symbol, _ label: String, tint: Color, size: ControlSize, leaves: Bool = true, isOpen: Binding<Bool>,
        action: @escaping () -> Void
    ) -> some View {
        rowSwipe([.button(symbol, label, tint: tint, leaves: leaves, perform: action)], size: size, isOpen: isOpen)
    }

    /// The same with a button for each action, the first against the edge. A long swipe does what
    /// the first does, when it is a button.
    func rowSwipe(_ actions: [SwipeAction], size: ControlSize, isOpen: Binding<Bool>) -> some View {
        modifier(RowSwipe(actions: actions, size: size, isOpen: isOpen))
    }
}

private struct RowSwipe: ViewModifier {
    let actions: [SwipeAction]
    let size: ControlSize
    @Binding var isOpen: Bool

    @State private var offset = 0.0
    @State private var offsetAtStart: Double?
    @State private var width = 0.0

    private static let leading = sidebarEdge
    private static let gap = 10.0
    private static let settle = Animation.spring(response: 0.29, dampingFraction: 0.86)

    /// How far the row rests aside while its buttons show.
    private var openWidth: Double { Double(actions.count) * (size.height + Self.gap) + 10 }
    /// The first action, which a long swipe does.
    private var swiped: SwipeAction? { actions.first.flatMap { $0.perform == nil ? nil : $0 } }
    /// From here on, letting go does what the first button does.
    private var fullSwipe: Double { max(openWidth + 44, width * 0.3) }
    private var armed: Bool { swiped != nil && offset >= fullSwipe }

    func body(content: Content) -> some View {
        content
            .overlay {
                if offset > 0 {
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { settle(open: false) }
                }
            }
            .offset(x: offset)
            .background(alignment: .leading) { buttons }
            .clipped()
            .onGeometryChange(for: Double.self) { $0.size.width } action: { width = $0 }
            .gesture(RowPan(isOpen: offset > 0, changed: dragged, ended: released))
            .onChange(of: isOpen) { _, open in
                guard offsetAtStart == nil, open != (offset > 0) else { return }
                withAnimation(Self.settle) { offset = open ? openWidth : 0 }
            }
            .sensoryFeedback(.impact(weight: .medium), trigger: armed) { _, armed in armed }
            .accessibilityActions {
                ForEach(actions.indices.filter { actions[$0].perform != nil }, id: \.self) { index in
                    Button(actions[index].label) { commit(actions[index]) }
                }
            }
    }

    /// The buttons come in as the row leaves, and past their place the first stretches after the row.
    private var buttons: some View {
        let stretch = swiped == nil ? 0 : max(0, offset - openWidth)
        let entry = min(1, max(0, (offset - 8) / (openWidth * 0.72 - 8)))
        return HStack(spacing: Self.gap) {
            ForEach(actions.indices, id: \.self) { index in
                button(actions[index], stretch: index == 0 ? stretch : 0)
            }
        }
        .padding(.leading, Self.leading)
        .frame(maxHeight: .infinity)
        .scaleEffect(0.78 + 0.22 * entry, anchor: .leading)
        .opacity(entry)
    }

    @ViewBuilder
    private func button(_ action: SwipeAction, stretch: Double) -> some View {
        let face = Image(action.symbol, size: size.filledSymbol)
            .foregroundStyle(Color.themeBackground)
            .offset(x: armed && stretch > 0 ? stretch / 2 : 0)
            .animation(.easeOut(duration: 0.15), value: armed)
            .frame(width: size.height + stretch, height: size.height)
            .background(action.tint, in: Capsule())
            .contentShape(Capsule())
        if let menu = action.menu {
            Menu { menu } label: { face }
                .accessibilityLabel(action.label)
        } else {
            Button { commit(action) } label: { face }
                .buttonStyle(.plain)
                .accessibilityLabel(action.label)
        }
    }

    private func dragged(_ moved: Double) {
        if offsetAtStart == nil {
            offsetAtStart = offset
            isOpen = true
        }
        offset = max(0, (offsetAtStart ?? 0) + moved)
    }

    private func released(_ moved: Double, speed: Double) {
        let end = max(0, (offsetAtStart ?? offset) + moved)
        offsetAtStart = nil
        if let swiped, end >= fullSwipe { return commit(swiped) }
        settle(open: abs(speed) > 300 ? speed > 0 : end > openWidth / 2)
    }

    private func settle(open: Bool) {
        isOpen = open
        withAnimation(Self.settle) { offset = open ? openWidth : 0 }
    }

    /// The row leaves to the right, and then what the button does happens.
    private func commit(_ action: SwipeAction) {
        guard let perform = action.perform else { return }
        guard action.leaves else {
            settle(open: false)
            return perform()
        }
        withAnimation(.easeOut(duration: 0.18)) {
            offset = max(width, offset)
        } completion: {
            perform()
            isOpen = false
            offset = 0
        }
    }
}

/// The finger that moves a row: only a swipe to the side, and from rest only one to the right.
private struct RowPan: UIGestureRecognizerRepresentable {
    let isOpen: Bool
    let changed: (Double) -> Void
    let ended: (Double, Double) -> Void

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Coordinator { Coordinator() }

    func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
        let pan = UIPanGestureRecognizer()
        pan.name = DrawerController.rowSwipe
        pan.delegate = context.coordinator
        return pan
    }

    func updateUIGestureRecognizer(_ pan: UIPanGestureRecognizer, context: Context) {
        context.coordinator.isOpen = isOpen
    }

    func handleUIGestureRecognizerAction(_ pan: UIPanGestureRecognizer, context: Context) {
        let moved = pan.translation(in: pan.view).x
        switch pan.state {
        case .changed: changed(moved)
        case .ended: ended(moved, pan.velocity(in: pan.view).x)
        case .cancelled, .failed: ended(moved, 0)
        default: break
        }
    }

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var isOpen = false

        func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
            guard let pan = recognizer as? UIPanGestureRecognizer else { return false }
            let speed = pan.velocity(in: pan.view)
            guard abs(speed.x) > abs(speed.y) * 1.3 else { return false }
            return isOpen || speed.x > 0
        }
    }
}
#endif
