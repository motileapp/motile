import SwiftUI

/// A bar where a line of text will be, in its line and as tall as its capitals, fading in and out.
struct PlaceholderBar: View {
    @Environment(\.surface) private var surface
    let width: CGFloat
    let size: TextSize
    var weight: Font.Weight = .regular

    var body: some View {
        Text(verbatim: " ")
            .font(.ui(size, weight: weight))
            .frame(width: width, alignment: .leading)
            .hidden()
            .overlay {
                Pulsing {
                    RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
                        .fill(surface.color(.control))
                        .frame(height: PlatformFont.ui(size).capHeight.rounded())
                }
            }
    }
}

/// What stands in for what is yet to come, fading in and out.
struct Pulsing<Content: View>: View {
    @ViewBuilder var content: Content
    @State private var faded = false

    var body: some View {
        content
            .animation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) { $0.opacity(.disabled, when: faded) }
            .onAppear { faded = true }
    }
}
