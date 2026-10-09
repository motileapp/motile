import SwiftUI

/// One of the things a `CheckMenu` turns on and off.
struct Check: Identifiable {
    let id: String
    let title: String
    let checked: Bool
    let toggle: () -> Void
}

/// A menu of things to turn on and off. It stays open while they are picked, which no system
/// menu does on the Mac, so it is drawn by the view with `dropdowns()` around it. A line parts
/// its `groups`.
struct CheckMenu: View {
    private static let padding: CGFloat = 4

    private let title: String
    private let icon: Symbol?
    private let help: String
    private let variant: ButtonVariant
    private let size: ControlSize
    private let tint: Color?
    private let groups: [[Check]]
    @State private var open = false

    init(
        _ title: String, icon: Symbol? = nil, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular,
        tint: Color? = nil, groups: [[Check]]
    ) {
        self.title = title
        self.icon = icon
        self.help = help
        self.variant = variant
        self.size = size
        self.tint = tint
        self.groups = groups
    }

    var body: some View {
        ActionButton(title, icon: icon, help: help, variant: variant, size: size, selected: open, opens: true, tint: tint) {
            show(true)
        }
        .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
            guard open else { return nil }
            return OpenDropdown.Shown(anchor: anchor, content: AnyView(card)) { show(false) }
        }
    }

    private func show(_ shown: Bool) {
        withAnimation(.easeOut(duration: 0.12)) { open = shown }
    }

    private var card: some View {
        ViewThatFits(in: .vertical) {
            list
            ScrollView { list }
        }
        .frame(minWidth: 160)
        .fixedSize(horizontal: true, vertical: false)
        .dropdownCard()
    }

    private var list: some View {
        VStack(spacing: 0) {
            ForEach(groups.indices, id: \.self) { index in
                if index > 0 {
                    Color.themeBorderCard
                        .frame(height: 1)
                        .padding(.horizontal, -Self.padding)
                        .padding(.vertical, Self.padding)
                }
                ForEach(groups[index]) { check in
                    row(check)
                        .button(.highlight(radius: Radius.small)) { check.toggle() }
                }
            }
        }
        .padding(Self.padding)
    }

    private func row(_ check: Check) -> some View {
        HStack(spacing: 8) {
            Image(.check, size: 13)
                .opacity(check.checked ? 1 : 0)
            Text(check.title)
                .lineLimit(1)
            Spacer(minLength: 12)
        }
        .font(.ui(size: 13))
        .foregroundStyle(Color.themeForeground)
        .padding(.horizontal, 8)
        .frame(height: pressable(28))
        .contentShape(Rectangle())
    }
}

/// The dropdown that is open: what it shows, the button it hangs from, and how to close it.
struct OpenDropdown: PreferenceKey {
    struct Shown {
        let anchor: Anchor<CGRect>
        let content: AnyView
        let close: () -> Void
    }

    static var defaultValue: Shown? { nil }

    static func reduce(value: inout Shown?, nextValue: () -> Shown?) {
        value = nextValue() ?? value
    }
}

extension View {
    /// Draws the dropdown open inside it under its button, over everything else. A click or a tap
    /// anywhere else closes it, and so does Esc.
    func dropdowns() -> some View {
        overlayPreferenceValue(OpenDropdown.self) { open in
            if let open {
                DropdownLayer(open: open)
                    .appearing()
            }
        }
    }

    /// The look of what drops from a button: the popover's colour, a line around it and a faint
    /// shadow under it.
    fileprivate func dropdownCard() -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        return clipShape(shape)
            .background {
                shape
                    .fill(Color.themePopover)
                    .shadow(.regular)
            }
            .overlay { shape.strokeBorder(Color.themeBorderCard, lineWidth: 1) }
            .environment(\.surface, .popover)
    }
}

private struct DropdownLayer: View {
    private static let gap: CGFloat = 4

    let open: OpenDropdown.Shown
    #if os(macOS)
    @State private var escape: Any?
    #endif

    var body: some View {
        GeometryReader { proxy in
            let button = proxy[open.anchor]
            ZStack(alignment: .topLeading) {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture(perform: open.close)
                open.content
                    .frame(maxHeight: max(0, proxy.size.height - button.maxY - 3 * Self.gap), alignment: .top)
                    .offset(x: button.minX, y: button.maxY + Self.gap)
            }
        }
        #if os(macOS)
        // Before the shortcuts, so that Esc closes the dropdown and not what it lies on.
        .onAppear {
            escape = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53 else { return event }
                open.close()
                return nil
            }
        }
        .onDisappear {
            guard let escape else { return }
            NSEvent.removeMonitor(escape)
        }
        #endif
    }
}
