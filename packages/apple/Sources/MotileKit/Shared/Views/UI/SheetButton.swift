#if os(iOS)
import SwiftUI

/// What a sheet on iOS is for, across its bottom: the system's prominent glass capsule, or its
/// filled button before iOS 26. A spinner while it waits.
struct SheetButton: View {
    let title: String
    var pending = false
    let action: () -> Void

    init(_ title: String, pending: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.pending = pending
        self.action = action
    }

    var body: some View {
        let button = Button(action: action) {
            ZStack {
                Text(title)
                    .opacity(pending ? 0 : 1)
                if pending {
                    Spinner(size: 17)
                }
            }
            .font(.ui(size: 15, weight: .semibold))
            .frame(maxWidth: .infinity)
        }
        .controlSize(.large)
        .tint(Color.themePrimary)
        .allowsHitTesting(!pending)
        if #available(iOS 26, *) {
            button.buttonStyle(.glassProminent)
        } else {
            button.buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
        }
    }
}
#endif
