import SwiftUI

/// One of the things a `CheckMenu` turns on and off.
struct Check: Identifiable {
    let id: String
    let title: String
    let checked: Bool
    let toggle: () -> Void
}

/// A menu of things to turn on and off. It stays open while they are picked, which no system
/// menu does on the Mac, so it is drawn by the view with `dropdowns()` around it.
struct CheckMenu: View {
    private static let padding: CGFloat = 4

    private let title: String
    private let icon: Symbol?
    private let help: String
    private let variant: ButtonVariant
    private let size: ControlSize
    private let tint: Color?
    private let checks: [Check]
    /// Room around it that takes its presses, as a button's `margin`.
    private let margin: EdgeInsets
    @State private var open = false

    init(
        _ title: String, icon: Symbol? = nil, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular,
        tint: Color? = nil, margin: EdgeInsets = EdgeInsets(), checks: [Check]
    ) {
        self.title = title
        self.icon = icon
        self.help = help
        self.variant = variant
        self.size = size
        self.tint = tint
        self.margin = margin
        self.checks = checks
    }

    var body: some View {
        ActionButton(title, icon: icon, help: help, variant: variant, size: size, selected: open, opens: true, tint: tint, margin: margin) {
            show(true)
        }
        .background {
            Color.clear
                .padding(margin)
                .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
                    guard open else { return nil }
                    return OpenDropdown.Shown(anchor: anchor, content: AnyView(card)) { show(false) }
                }
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
            ForEach(checks) { check in
                row(check)
                    .button(.highlight(radius: Radius.sm)) { check.toggle() }
            }
        }
        .padding(Self.padding)
    }

    private func row(_ check: Check) -> some View {
        HStack(spacing: 8) {
            Image(.check, size: .md)
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

/// The dropdown that is open: what it shows, the button it hangs from or stands on, and how to
/// close it.
struct OpenDropdown: PreferenceKey {
    struct Shown {
        let anchor: Anchor<CGRect>
        let content: AnyView
        /// It opens above the button, as what the composer's buttons open does.
        var up = false
        /// It lines up with the button's trailing edge rather than its leading one.
        var trailing = false
        /// What Esc does: one level, as a menu open inside the dropdown.
        let close: () -> Void
        /// What a press around the dropdown does, when it closes more than Esc does.
        var closeAll: (() -> Void)?
    }

    static var defaultValue: Shown? { nil }

    static func reduce(value: inout Shown?, nextValue: () -> Shown?) {
        value = nextValue() ?? value
    }
}

extension View {
    /// Draws the dropdown open inside it under its button, over everything else. A click or a tap
    /// anywhere else only closes it, and so does Esc.
    func dropdowns() -> some View {
        overlayPreferenceValue(OpenDropdown.self) { open in
            if let open {
                DropdownLayer(open: open)
                    .appearing()
            }
        }
    }

    /// Drops `content` from it while `shown`, the button's light standing `inset` inside its
    /// frame, lined up with its `trailing` edge or its leading one, and calls `close` when a click
    /// lands elsewhere or Esc is pressed.
    func dropdown(
        _ shown: Bool, inset: EdgeInsets = EdgeInsets(), trailing: Bool = false, close: @escaping () -> Void,
        @ViewBuilder content: @escaping () -> some View
    ) -> some View {
        background {
            Color.clear
                .padding(inset)
                .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
                    shown ? OpenDropdown.Shown(anchor: anchor, content: AnyView(content()), trailing: trailing, close: close) : nil
                }
        }
    }

    /// The look of what drops from a button or lies over the window: the popover's colour, a line
    /// around it and a faint shadow under it.
    func dropdownCard() -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        return clipShape(shape)
            .background {
                shape
                    .fill(Color.themePopover)
                    .shadow(.md)
            }
            .overlay { shape.strokeBorder(Color.themeBorderPopover, lineWidth: 1) }
            .environment(\.surface, .popover)
    }
}

private struct DropdownLayer: View {
    private static let gap: CGFloat = 4
    private static let roomBelow: CGFloat = 220

    let open: OpenDropdown.Shown
    #if os(macOS)
    @State private var escape: Any?
    #endif

    @ViewBuilder private func lined(up button: CGRect) -> some View {
        if open.trailing {
            open.content
                .frame(width: button.maxX, alignment: .trailing)
        } else {
            open.content
                .offset(x: button.minX)
        }
    }

    /// Takes every press around the dropdown, which only closes it, as a menu does.
    @ViewBuilder private var backdrop: some View {
        #if os(macOS)
        DropdownBackdropView(close: open.closeAll ?? open.close)
        #else
        Color.clear
            .contentShape(Rectangle())
            .onTapGesture(perform: open.closeAll ?? open.close)
        #endif
    }

    var body: some View {
        GeometryReader { proxy in
            let button = proxy[open.anchor]
            ZStack(alignment: .topLeading) {
                backdrop
                // One that would have too little room under its button, as the strip under the
                // composer at the window's bottom, opens above it.
                let below = proxy.size.height - button.maxY
                if open.up || (below < Self.roomBelow && button.minY > below) {
                    lined(up: button)
                        .frame(maxHeight: max(0, button.minY - 3 * Self.gap), alignment: .bottom)
                        .frame(height: max(0, button.minY - Self.gap), alignment: .bottom)
                } else {
                    lined(up: button)
                        .frame(maxHeight: max(0, proxy.size.height - button.maxY - 3 * Self.gap), alignment: .top)
                        .offset(y: button.maxY + Self.gap)
                }
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
