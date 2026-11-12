import SwiftUI

/// The bar under an image being marked up: the tools, the colours, undo and redo. While a pin is
/// selected it is the pin's note instead, which Return keeps.
struct MarkupBar: View {
    @Environment(AppStore.self) private var store
    @Bindable var session: MarkupSession
    @FocusState private var noteFocused: Bool

    var body: some View {
        Group {
            if let pin = session.selectedPin {
                noteBar(pin)
            } else {
                toolBar
            }
        }
        .barPadding()
        .barSurface()
        .environment(\.surface, Self.surface)
        .environment(\.colorScheme, .dark)
        .animation(.snappy(duration: 0.2), value: session.selected)
    }

    #if os(macOS)
    private static let mac = true
    private static let surface = Surface.popover
    #else
    private static let mac = false
    private static let surface = Surface.glass
    #endif
    /// The composer's buttons on iOS, where the bar is as tall as the composer folded.
    private static let size = mac ? ControlSize.large : ControlSize.regular

    private var toolBar: some View {
        HStack(spacing: 2) {
            if !Self.mac, session.inksOpen {
                swatches
            } else {
                ForEach(MarkupTool.allCases, id: \.self) { tool in
                    let picked = session.tool == tool
                    ActionButton(
                        icon: tool.symbol, help: help(tool.title, tool.command),
                        variant: picked && !Self.mac ? .accent : .ghost, size: Self.size, selected: picked && Self.mac, round: true
                    ) { session.tool = tool }
                }
                divider
                if Self.mac {
                    swatches
                } else {
                    Swatch(color: Self.color(session.ink), help: "Colour", selected: true, size: Self.size) { session.inksOpen = true }
                }
                divider
                ActionButton(icon: .undo2, help: help("Undo", "markup.undo"), size: Self.size, round: true) { session.undo() }
                    .disabled(!session.canUndo)
                ActionButton(icon: .redo2, help: help("Redo", "markup.redo"), size: Self.size, round: true) { session.redo() }
                    .disabled(!session.canRedo)
            }
        }
    }

    /// On a phone they take the tools' place while they are open, and picking one closes them.
    private var swatches: some View {
        ForEach(session.inks, id: \.self) { ink in
            Swatch(
                color: Self.color(ink), help: help(ink.capitalized, "markup.ink.\(ink)"),
                selected: session.ink == ink, size: Self.size
            ) { session.ink = ink }
        }
    }

    private func noteBar(_ pin: MarkupSession.Pin) -> some View {
        HStack(spacing: 6) {
            Text("\(pin.number)")
                .font(.ui(.sm, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(pin.ink == "yellow" || pin.ink == "white" ? Color.themeMarkupInk : Color.themeMarkupWhite)
                .frame(width: Self.size.height, height: Self.size.height)
                .background(Self.color(pin.ink), in: Circle())
            InputField("Note for the agent", text: $session.note, variant: .bare, size: Self.size, focus: $noteFocused)
                .frame(width: Self.mac ? 340 : nil, height: Self.size.height)
                .onReturn { session.deselect() }
            ActionButton(icon: .trash2, help: "Remove Pin", size: Self.size, round: true) { session.removeSelected() }
            ActionButton(
                icon: .check, help: Self.mac ? "Keep Note (Return)" : "Keep Note", variant: .primary, size: Self.size, round: true, glass: true
            ) {
                session.deselect()
            }
        }
        .task(id: pin.mark) { noteFocused = true }
    }

    /// The text with the command's keys after it, on the Mac.
    private func help(_ text: String, _ command: String) -> String {
        Self.mac ? store.shortcuts.help(text, command, in: Shortcuts.markingUp) : text
    }

    private var divider: some View {
        Rectangle()
            .fill(Self.surface.border)
            .frame(width: 1, height: Self.size.height * 0.6)
            .padding(.horizontal, 4)
    }

    static func color(_ ink: String) -> Color {
        Color(platform: Theme.named("markup-\(ink)") ?? Theme.markupRed)
    }
}

private extension View {
    /// On iOS the composer's: as tall as it is folded, its buttons as far from the ends as from
    /// the top and bottom.
    @ViewBuilder
    func barPadding() -> some View {
        #if os(macOS)
        padding(6)
        #else
        padding(.horizontal, ComposerView.edge)
            .frame(height: floatingHeight)
        #endif
    }

    /// A popover on the Mac, and on iOS the composer's glass.
    @ViewBuilder
    func barSurface() -> some View {
        #if os(macOS)
        background(Color.themePopover, in: Capsule())
            .overlay(Capsule().strokeBorder(Color.themeBorderPopover))
            .shadow(.lg)
        #else
        glassSurface(in: Capsule())
        #endif
    }
}
