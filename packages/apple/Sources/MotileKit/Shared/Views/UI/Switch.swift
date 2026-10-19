import SwiftUI

/// On or off: the system's switch, in the primary colour while it is on.
struct Switch: View {
    /// How tall the Mac's small switch stands.
    static let height: CGFloat = 18

    @Binding var isOn: Bool

    var body: some View {
        Toggle("", isOn: $isOn)
            .toggleStyle(.switch)
            .labelsHidden()
            .tint(Color.themePrimary)
            .controlSize(.small)
    }
}
