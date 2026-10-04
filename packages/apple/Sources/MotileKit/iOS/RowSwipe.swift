#if os(iOS)
import SwiftUI
import UIKit

extension View {
    /// Lets a swipe to the right slide a sidebar row aside and show a round button behind it. A
    /// short swipe leaves the button there to tap, a long one does what the button does. The row
    /// leaves with a long swipe unless the button won't take it away.
    func rowSwipe(
        _ symbol: Symbol, _ label: String, tint: Color, size: CGFloat, leaves: Bool = true, isOpen: Binding<Bool>,
        action: @escaping () -> Void
    ) -> some View {
        modifier(RowSwipe(symbol: symbol, label: label, tint: tint, size: size, leaves: leaves, isOpen: isOpen, action: action))
    }
}

private struct RowSwipe: ViewModifier {
    let symbol: Symbol
    let label: String
    let tint: Color
    let size: CGFloat
    let leaves: Bool
    @Binding var isOpen: Bool
    let action: () -> Void

    @State private var offset = 0.0
    @State private var offsetAtStart: Double?
    @State private var width = 0.0

    private static let leading = sidebarRowInset + 8
    private static let settle = Animation.spring(response: 0.29, dampingFraction: 0.86)

    /// How far the row rests aside while its button shows.
    private var openWidth: Double { size + 20 }
    /// From here on, letting go does what the button does.
    private var fullSwipe: Double { max(openWidth + 44, width * 0.58) }
    private var armed: Bool { offset >= fullSwipe }

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
            .background(alignment: .leading) { button }
            .clipped()
            .onGeometryChange(for: Double.self) { $0.size.width } action: { width = $0 }
            .gesture(RowPan(isOpen: offset > 0, changed: dragged, ended: released))
            .onChange(of: isOpen) { _, open in
                guard !open, offsetAtStart == nil, offset > 0 else { return }
                withAnimation(Self.settle) { offset = 0 }
            }
            .sensoryFeedback(.impact(weight: .medium), trigger: armed) { _, armed in armed }
            .accessibilityAction(named: label, action)
    }

    /// The button comes in as the row leaves, and past its place stretches after the row.
    private var button: some View {
        let stretch = max(0, offset - openWidth)
        let entry = min(1, max(0, (offset - 8) / (openWidth * 0.72 - 8)))
        return Button(action: commit) {
            Image(symbol, size: size * 0.37)
                .foregroundStyle(Color.themeBackground)
                .offset(x: armed ? stretch / 2 : 0)
                .animation(.easeOut(duration: 0.15), value: armed)
                .frame(width: size + stretch, height: size)
                .background(tint, in: Capsule())
                .padding(.leading, Self.leading)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .scaleEffect(0.78 + 0.22 * entry)
        .opacity(entry)
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
        guard end < fullSwipe else { return commit() }
        settle(open: abs(speed) > 300 ? speed > 0 : end > openWidth / 2)
    }

    private func settle(open: Bool) {
        isOpen = open
        withAnimation(Self.settle) { offset = open ? openWidth : 0 }
    }

    /// The row leaves to the right, and then what the button does happens.
    private func commit() {
        guard leaves else {
            settle(open: false)
            return action()
        }
        withAnimation(.easeOut(duration: 0.18)) {
            offset = max(width, offset)
        } completion: {
            action()
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
