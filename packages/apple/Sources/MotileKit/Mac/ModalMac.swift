#if os(macOS)
import AppKit
import SwiftUI

/// The modals open over the window, the last on top. The views that show them put them here and
/// `ModalLayer` draws them.
@Observable final class Modals {
    static let shared = Modals()

    struct Shown: Identifiable {
        let id: UUID
        let content: AnyView
        /// Tells the view that showed it that it closed.
        let closed: () -> Void
    }

    private(set) var stack: [Shown] = []

    var isEmpty: Bool { stack.isEmpty }

    fileprivate func show(_ shown: Shown) {
        withAnimation(ModalLayer.animation) {
            guard let index = stack.firstIndex(where: { $0.id == shown.id }) else { return stack.append(shown) }
            stack[index] = shown
        }
    }

    fileprivate func remove(_ id: UUID) {
        guard stack.contains(where: { $0.id == id }) else { return }
        withAnimation(ModalLayer.animation) { stack.removeAll { $0.id == id } }
    }

    func close(_ id: UUID) {
        stack.first { $0.id == id }?.closed()
        remove(id)
    }

    func closeTop() {
        guard let top = stack.last else { return }
        close(top.id)
    }
}

struct ModalPresenter<Item: Identifiable, ModalContent: View>: ViewModifier {
    @Binding var item: Item?
    let content: (Item) -> ModalContent
    @State private var id = UUID()

    func body(content view: Content) -> some View {
        view
            .onChange(of: item?.id, initial: true) { present() }
            .onDisappear { Modals.shared.close(id) }
    }

    private func present() {
        guard let item else { return Modals.shared.remove(id) }
        let binding = $item
        Modals.shared.show(Modals.Shown(id: id, content: AnyView(content(item)), closed: { binding.wrappedValue = nil }))
    }
}

/// The open modals over the window, the top one on the scrim. Esc or a click on the scrim closes
/// it. While one is open the window's own views lose the keys, and get them back after.
struct ModalLayer: View {
    static let animation = Animation.easeOut(duration: 0.18)

    private let modals = Modals.shared
    @State private var keys: Any?
    @State private var focusedBefore: NSResponder?

    var body: some View {
        let stack = modals.stack
        ZStack {
            if !stack.isEmpty {
                Color.themeScrim
                    .ignoresSafeArea()
                    .onTapGesture { modals.closeTop() }
                    .appearing()
                    .zIndex(Double(stack.count * 2 - 2))
            }
            ForEach(Array(stack.enumerated()), id: \.element.id) { index, shown in
                ModalCard(shown: shown, top: index == stack.count - 1)
                    .zIndex(Double(index * 2 + 1))
            }
        }
        .onChange(of: stack.isEmpty) { _, empty in empty ? stopTakingKeys() : takeKeys() }
    }

    private func takeKeys() {
        ThreadPeek.shared.hide()
        let window = NSApp.keyWindow
        focusedBefore = window?.firstResponder
        window?.makeFirstResponder(nil)
        keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard event.keyCode == 53 else { return event }
            Modals.shared.closeTop()
            return nil
        }
    }

    private func stopTakingKeys() {
        if let keys { NSEvent.removeMonitor(keys) }
        keys = nil
        guard let view = focusedBefore as? NSView, let window = view.window else { return }
        window.makeFirstResponder(view)
        focusedBefore = nil
    }
}

private struct ModalCard: View {
    let shown: Modals.Shown
    let top: Bool

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: Radius.xl, style: .continuous) }

    var body: some View {
        shown.content
            .environment(\.closeModal, CloseModal { Modals.shared.close(shown.id) })
            .environment(\.surface, .background)
            .background {
                shape
                    .fill(Color.themeBackground)
                    .shadow(.xl, .shadowStrongest)
            }
            .overlay { shape.strokeBorder(Color.themeBorder, lineWidth: 1) }
            .disabled(!top)
            .padding(32)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .appearing(.opacity.combined(with: .offset(y: 8)))
            .onAppear { FirstField.focus() }
    }
}

/// Puts the keys in the top modal's first field, as the system does in a sheet: the highest field
/// a click can reach, which only the top modal's are. Its fields are made a moment after it
/// appears, so it looks again a few times.
private enum FirstField {
    static func focus(tries: Int = 10) {
        guard tries > 0, let window = NSApp.keyWindow, let root = window.contentView else { return }
        let fields = editable(in: root).filter { reached($0, in: root) }
        guard let first = fields.min(by: { topLeft(of: $0, in: root) < topLeft(of: $1, in: root) }) else {
            return DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focus(tries: tries - 1) }
        }
        window.makeFirstResponder(first)
    }

    private static func editable(in view: NSView) -> [NSView] {
        guard !view.isHidden else { return [] }
        if let field = view as? NSTextField { return field.isEditable ? [field] : [] }
        if let text = view as? NSTextView { return text.isEditable && !text.isFieldEditor ? [text] : [] }
        return view.subviews.flatMap(editable)
    }

    private static func reached(_ view: NSView, in root: NSView) -> Bool {
        let middle = view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        let point = root.superview?.convert(middle, from: nil) ?? middle
        return root.hitTest(point)?.isDescendant(of: view) == true
    }

    private static func topLeft(of view: NSView, in root: NSView) -> Place {
        let rect = view.convert(view.bounds, to: root)
        return Place(y: root.isFlipped ? rect.minY : root.bounds.height - rect.maxY, x: rect.minX)
    }

    private struct Place: Comparable {
        let y: CGFloat
        let x: CGFloat

        static func < (a: Place, b: Place) -> Bool { a.y == b.y ? a.x < b.x : a.y < b.y }
    }
}

/// A confirmation as a modal: its title and message, the button that lets it go, then its answers.
struct ConfirmationView: View {
    /// How wide its text is at least. Its buttons widen it when they need more.
    private static let textWidth: CGFloat = 360

    @Environment(\.closeModal) private var close
    let confirmation: Confirmation
    @State private var buttonsWidth: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(confirmation.title)
                .font(.ui(size: 15, weight: .semibold))
                .fixedSize(horizontal: false, vertical: true)
            if !confirmation.message.isEmpty {
                Text(confirmation.message)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 6)
            }
            buttons
                .fixedSize()
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { buttonsWidth = $0 }
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.top, 22)
        }
        .frame(width: max(Self.textWidth, buttonsWidth), alignment: .leading)
        .padding(20)
    }

    private var buttons: some View {
        HStack(spacing: 8) {
            ActionButton(confirmation.dismissal, variant: confirmation.answers.isEmpty ? .primary : .secondary) { close() }
                .keyboardShortcut(confirmation.answers.isEmpty ? .defaultAction : .cancelAction)
            ForEach(Array(confirmation.answers.enumerated()), id: \.offset) { index, answer in
                let button = ActionButton(answer.title, variant: answer.variant) {
                    close()
                    answer.action()
                }
                if index == confirmation.answers.count - 1 {
                    button.keyboardShortcut(.defaultAction)
                } else {
                    button
                }
            }
        }
    }
}
#endif
