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
                    .button(.dropdownLight()) { check.toggle() }
            }
        }
        .padding(DropdownList.padding)
    }

    private func row(_ check: Check) -> some View {
        HStack(spacing: DropdownList.gap) {
            Text(check.title)
                .lineLimit(1)
            Spacer(minLength: 12)
            Image(.check, size: DropdownList.check)
                .opacity(check.checked ? 1 : 0)
        }
        .dropdownRow()
    }
}

/// The dropdown that is open: what it shows, the button it hangs from or stands on, and how to
/// close it.
struct OpenDropdown: PreferenceKey {
    /// The room between a dropdown and its button's light.
    static let gap: CGFloat = 6

    struct Shown {
        let anchor: Anchor<CGRect>
        let content: AnyView
        /// It opens above the button where it fits, as what the composer's buttons open does.
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
            } else if let hung = HungDropdown.shared.shown {
                DropdownLayer(hung: hung)
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
                .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
                    shown ? OpenDropdown.Shown(anchor: anchor, content: AnyView(content()), trailing: trailing, close: close) : nil
                }
                .padding(inset)
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

/// A dropdown that hangs from a view outside SwiftUI, as a transcript row's button, at that
/// view's frame in its window, which follows the view while the dropdown is open.
@Observable
final class HungDropdown {
    static let shared = HungDropdown()

    struct Shown {
        /// The view's frame in its window, from the window's top left corner.
        var frame: CGRect
        let content: AnyView
    }

    private(set) var shown: Shown?
    @ObservationIgnored private weak var source: PlatformView?
    @ObservationIgnored private var closed: (() -> Void)?

    private init() {}

    func isOpen(from view: PlatformView) -> Bool {
        shown != nil && source === view
    }

    /// Opens `content` under `view`, or closes it when it is open from there already. `closed`
    /// is told when it closes.
    func toggle(from view: PlatformView, content: AnyView, closed: @escaping () -> Void) {
        guard !isOpen(from: view) else { return close() }
        close()
        guard let frame = Self.frame(of: view) else { return }
        source = view
        self.closed = closed
        shown = Shown(frame: frame, content: content)
    }

    /// Moves the dropdown with the view it hangs from, which scrolled or was laid out again.
    func follow(_ view: PlatformView) {
        guard isOpen(from: view) else { return }
        guard let frame = Self.frame(of: view) else { return close() }
        shown?.frame = frame
    }

    func close() {
        guard shown != nil else { return }
        shown = nil
        source = nil
        let closed = closed
        self.closed = nil
        closed?()
    }

    private static func frame(of view: PlatformView) -> CGRect? {
        guard let window = view.window else { return nil }
        let inWindow = view.convert(view.bounds, to: nil)
        #if os(macOS)
        let height = window.contentView?.bounds.height ?? window.frame.height
        return CGRect(x: inWindow.minX, y: height - inWindow.maxY, width: inWindow.width, height: inWindow.height)
        #else
        _ = window
        return inWindow
        #endif
    }
}

private struct DropdownLayer: View {
    private let content: AnyView
    private let up: Bool
    private let trailing: Bool
    private let close: () -> Void
    private let closeAll: (() -> Void)?
    private let place: (GeometryProxy) -> CGRect
    #if os(macOS)
    @State private var escape: Any?
    #endif

    init(open: OpenDropdown.Shown) {
        content = open.content
        up = open.up
        trailing = open.trailing
        close = open.close
        closeAll = open.closeAll
        place = { $0[open.anchor] }
    }

    init(hung: HungDropdown.Shown) {
        content = hung.content
        up = false
        trailing = false
        close = { HungDropdown.shared.close() }
        closeAll = nil
        place = { proxy in
            let origin = proxy.frame(in: .global).origin
            return hung.frame.offsetBy(dx: -origin.x, dy: -origin.y)
        }
    }

    /// Takes every press around the dropdown, which only closes it, as a menu does.
    @ViewBuilder private var backdrop: some View {
        #if os(macOS)
        DropdownBackdropView(close: closeAll ?? close)
        #else
        Color.clear
            .contentShape(Rectangle())
            .onTapGesture(perform: closeAll ?? close)
        #endif
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                backdrop
                DropdownPlacement(button: place(proxy), up: up, trailing: trailing) { content }
            }
        }
        #if os(macOS)
        // Before the shortcuts, so that Esc closes the dropdown and not what it lies on.
        .onAppear {
            escape = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 53 else { return event }
                close()
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

/// Places a dropdown against its button: on the side it prefers, `up` or under it, unless it only
/// fits on the other, which has more room; lined up with the button's leading or `trailing` edge;
/// and slid back inside the window where it would leave it. One too tall for its side is cut to
/// it, to scroll.
private struct DropdownPlacement: Layout {
    private static let gap = OpenDropdown.gap
    private static let margin: CGFloat = 8

    let button: CGRect
    let up: Bool
    let trailing: Bool

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        proposal.replacingUnspecifiedDimensions()
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let card = subviews.first else { return }
        let width = max(0, bounds.width - 2 * Self.margin)
        let below = bounds.height - button.maxY - Self.gap - Self.margin
        let above = button.minY - Self.gap - Self.margin
        let natural = card.sizeThatFits(ProposedViewSize(width: width, height: nil)).height
        let opensUp = up ? natural <= above || above >= below : natural > below && above > below
        let room = max(0, opensUp ? above : below)
        let size = card.sizeThatFits(ProposedViewSize(width: width, height: min(natural, room)))
        let aligned = trailing ? button.maxX - size.width : button.minX
        let x = max(Self.margin, min(aligned, bounds.width - Self.margin - size.width))
        let y = opensUp ? button.minY - Self.gap - size.height : button.maxY + Self.gap
        card.place(at: CGPoint(x: bounds.minX + x, y: bounds.minY + y), proposal: ProposedViewSize(size))
    }
}
