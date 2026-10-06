import SwiftUI

/// The button that copies. It shows a check mark for a moment once it has, also when `copies`
/// goes up because something else, like a shortcut, copied.
struct CopyButton: View {
    let help: String
    var variant: ButtonVariant = .ghost
    var size: ControlSize = .regular
    var symbolSize: CGFloat?
    var round = false
    var copies = 0
    let copy: () -> Void
    @State private var copied = false
    @State private var shown = 0

    var body: some View {
        ActionButton(icon: copied ? .check : .copy, help: help, variant: variant, size: size, symbolSize: symbolSize, round: round) {
            copy()
            showCopied()
        }
        .onChange(of: copies) { showCopied() }
        .task(id: shown) {
            guard copied, (try? await Task.sleep(for: .seconds(1.2))) != nil else { return }
            copied = false
        }
    }

    private func showCopied() {
        copied = true
        shown += 1
    }
}
