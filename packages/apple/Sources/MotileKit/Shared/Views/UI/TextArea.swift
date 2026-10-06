import SwiftUI

/// A box to write more than a line in: the composer's text in a field's frame. It is `lines`
/// tall while empty, grows with what is written up to `maxLines`, and scrolls past that. One
/// that `fills` is as tall as there is room instead. Return is a line break.
struct TextArea: View {
    private let placeholder: String
    @Binding private var text: String
    private let size: ControlSize
    private let monospaced: Bool
    private let fills: Bool
    private let heights: ClosedRange<CGFloat>
    @State private var height: CGFloat
    @Environment(\.surface) private var surface

    init(
        _ placeholder: String, text: Binding<String>, size: ControlSize = .regular, monospaced: Bool = false, lines: Int = 3, maxLines: Int = 12,
        fills: Bool = false
    ) {
        self.placeholder = placeholder
        _text = text
        self.size = size
        self.monospaced = monospaced
        self.fills = fills
        let font = Self.font(size, monospaced: monospaced)
        let least = ComposerTextView.height(of: lines, in: font)
        heights = least...(fills ? least : ComposerTextView.height(of: max(lines, maxLines), in: font))
        _height = State(initialValue: least)
    }

    private static func font(_ size: ControlSize, monospaced: Bool) -> PlatformFont {
        monospaced ? .uiMono(size.textSize + 0.5) : .ui(size.textSize + 0.5)
    }

    var body: some View {
        ComposerTextView(text: $text, height: $height, placeholder: placeholder, font: Self.font(size, monospaced: monospaced), heights: heights)
            .frame(height: fills ? nil : height)
            .frame(minHeight: fills ? heights.lowerBound : nil, maxHeight: fills ? .infinity : nil)
            .padding(.horizontal, size.padding - 2)
            .padding(.vertical, max(0, size.padding - ComposerTextView.verticalInset))
            .textPointer()
            .fieldFrame(size, surface: surface)
    }
}
