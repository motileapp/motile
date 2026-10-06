import SwiftUI

/// On or off: a knob at one end of a track, which is in the primary colour while it is on.
struct Switch: View {
    private static let height = scaled(20)
    private static let width = scaled(34)
    private static let inset: CGFloat = 2

    @Binding var isOn: Bool
    @Environment(\.surface) private var surface
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            Capsule()
                .fill(isOn ? Color.themePrimary : surface.next.next.color)
                .frame(width: Self.width, height: Self.height)
                .overlay(alignment: isOn ? .trailing : .leading) {
                    Circle()
                        .fill(Color.white)
                        .padding(Self.inset)
                        .shadow(color: .black.opacity(0.18), radius: 1, y: 1)
                }
                .contentShape(Capsule())
        }
        .buttonStyle(DimButtonStyle())
        .opacity(enabled ? 1 : 0.5)
        .animation(.easeOut(duration: 0.15), value: isOn)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(isOn ? "On" : "Off")
    }
}
