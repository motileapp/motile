import SwiftUI

/// A command to run in a terminal, in the theme's code colours, with the button that copies it
/// and, on iOS, the one that shares it. Without a command yet it says what it waits for. A link
/// is shown on one line, cut short, with the buttons that open and copy it.
struct CommandBox: View {
    @Environment(\.surface) private var surface
    let command: String?
    /// What `Theme.syntax` colours each stretch: its start, its length and the colour, in threes.
    var spans: [Int] = []
    var placeholder = "Preparing the command…"
    var link: URL?

    private var noun: String { link == nil ? "command" : "link" }

    /// The room between the buttons' lights, which their presses share.
    private static let buttonGap: CGFloat = 4

    private var copyMargin: EdgeInsets {
        #if os(iOS)
        return .beside(leading: Self.buttonGap / 2)
        #else
        return .beside(leading: link == nil ? 0 : Self.buttonGap / 2)
        #endif
    }

    /// Puts the first line of the command level with the middle of the buttons.
    private static let textInset = ((ControlSize.regular.height - PlatformFont.uiMono(12.5).textLineHeight) / 2).rounded()

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(command.map { Self.highlighted($0, spans: spans) } ?? AttributedString(placeholder))
                .font(.ui(size: 12.5, design: .monospaced))
                .foregroundStyle(command == nil ? Color.themeMutedStrongerForeground : Color.themeForeground)
                .lineSpacing(3)
                .lineLimit(link == nil ? nil : 1)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding([.vertical, .leading], Self.textInset)
            HStack(spacing: 0) {
                if let link {
                    ActionButton("Open", icon: .squareArrowOutUpRight, variant: .ghost, margin: .beside(trailing: Self.buttonGap / 2)) {
                        Platform.open(link)
                    }
                }
                #if os(iOS)
                if link == nil {
                    let shared = EdgeInsets.beside(trailing: Self.buttonGap / 2)
                    ShareLink(item: command ?? "") {
                        ControlLabel(title: nil, icon: .symbol(.share), size: .regular)
                            .padding(shared)
                    }
                    .buttonStyle(ControlButtonStyle(look: ControlLook(variant: .ghost, size: .regular), margin: shared))
                    .accessibilityLabel("Share the command")
                }
                #endif
                CopyButton(help: "Copy the \(noun)", margin: copyMargin) {
                    guard let command else { return }
                    Platform.copy(command)
                }
            }
            .disabled(command == nil)
        }
        .padding(scaled(4))
        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous), bordered: true)
    }

    /// The command in its colours. It wraps between any two characters, as CSS's `break-all`
    /// does, so it is not selectable: a copy would carry the zero-width spaces into the shell.
    private static func highlighted(_ command: String, spans: [Int]) -> AttributedString {
        var colours = [Int](repeating: 0, count: command.utf16.count)
        for index in stride(from: 0, to: spans.count - 2, by: 3) {
            let start = spans[index], end = start + spans[index + 1]
            guard end <= colours.count else { continue }
            colours.replaceSubrange(start..<end, with: repeatElement(spans[index + 2], count: end - start))
        }
        var result = AttributedString()
        var offset = 0
        for character in command {
            var piece = AttributedString(String(character) + "\u{200B}")
            let colour = colours[offset]
            piece.foregroundColor = Color(platform: Theme.syntax[colour < Theme.syntax.count ? colour : 0])
            result += piece
            offset += character.utf16.count
        }
        return result
    }
}
