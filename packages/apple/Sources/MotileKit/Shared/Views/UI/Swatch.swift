import SwiftUI

/// A colour to pick: a dot as tall as a button of its size, ringed in the foreground when it is the
/// one picked.
struct Swatch: View {
    let color: Color
    let help: String
    var selected = false
    var size: ControlSize = .regular
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                Circle()
                    .strokeBorder(Color.themeForeground, lineWidth: 2)
                    .opacity(selected ? 1 : 0)
                Circle()
                    .fill(color)
                    .overlay(Circle().strokeBorder(Color.themeBorderAccent, lineWidth: 1))
                    .padding(size.height * 0.2)
            }
            .frame(width: size.height, height: size.height)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(help)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
