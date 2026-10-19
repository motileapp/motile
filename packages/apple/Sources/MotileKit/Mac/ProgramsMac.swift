#if os(macOS)
import AppKit

/// A carousel's track: its items, moved sideways by the trackpad or the wheel and stopping at an
/// item's edge once the gesture ends, or a page at a time by its buttons.
final class CarouselTrack: NSView {
    let content = FlippedView()
    var onScroll: (() -> Void)?
    private(set) var offset: CGFloat = 0
    private var width: CGFloat = 0
    private var inset: CGFloat = 0
    private var step: CGFloat = 1
    private var sideways: Bool?

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true
        addSubview(content)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    var most: CGFloat { max(0, width - bounds.width) }

    func set(width: CGFloat, height: CGFloat, inset: CGFloat, step: CGFloat) {
        self.width = width
        self.inset = inset
        self.step = max(1, step)
        offset = min(offset, most)
        content.frame = NSRect(x: -offset, y: inset, width: width, height: height)
    }

    override func scrollWheel(with event: NSEvent) {
        if event.phase.contains(.began) || event.phase.contains(.mayBegin) { sideways = nil }
        if sideways == nil, event.scrollingDeltaX != 0 || event.scrollingDeltaY != 0 {
            sideways = abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
        }
        guard sideways == true || event.phase.isEmpty && event.momentumPhase.isEmpty && abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY),
            most > 0
        else { return nextResponder?.scrollWheel(with: event) ?? () }
        move(to: offset - event.scrollingDeltaX, animated: false)
        let ended = event.momentumPhase.contains(.ended) || (event.phase.contains(.ended) && event.momentumPhase.isEmpty && abs(event.scrollingDeltaX) < 1)
        if ended { move(to: (offset / step).rounded() * step, animated: true) }
    }

    /// Moves by 85% of what shows, to the nearest item edge that way.
    func page(_ direction: Int) {
        let target = offset + CGFloat(direction) * bounds.width * 0.85
        let snapped = direction > 0 ? (target / step).rounded(.down) * step : (target / step).rounded(.up) * step
        move(to: snapped == offset ? target : snapped, animated: true)
    }

    private func move(to target: CGFloat, animated: Bool) {
        offset = min(max(0, target), most)
        let origin = NSPoint(x: -offset, y: inset)
        if animated, !Platform.reducesMotion {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.3
                context.allowsImplicitAnimation = true
                content.animator().setFrameOrigin(origin)
            }
        } else {
            content.setFrameOrigin(origin)
        }
        onScroll?()
    }
}

/// A popover's panel over the window, under its trigger or above it, which a click outside it, Esc
/// or a scroll of the transcript closes.
final class ProgramPopoverPanel {
    private weak var anchor: NSView?
    private let view = ProgramPanelView()
    private let onClose: () -> Void
    private var monitor: Any?

    init(anchor: NSView, onClose: @escaping () -> Void) {
        self.anchor = anchor
        self.onClose = onClose
    }

    func show(_ slot: UiSlot, host: ProgramHost?, surface: Surface) {
        guard let anchor, let content = anchor.window?.contentView else { return }
        let size = view.show(slot, host: host, window: content.bounds.size)
        place(size, anchor: anchor, in: content)
        if view.superview !== content { content.addSubview(view) }
        watch()
    }

    private func place(_ size: CGSize, anchor: NSView, in host: NSView) {
        let trigger = anchor.convert(anchor.bounds, to: host)
        let flipped = host.isFlipped
        let below = flipped ? host.bounds.height - trigger.maxY : trigger.minY
        let above = flipped ? trigger.minY : host.bounds.height - trigger.maxY
        let up = below < 220 + 8 && above > 240
        var x = min(max(12, trigger.minX), host.bounds.width - 12 - size.width)
        x = max(12, x)
        let y: CGFloat
        if flipped {
            y = up ? trigger.minY - 8 - size.height : trigger.maxY + 8
        } else {
            y = up ? trigger.maxY + 8 : trigger.minY - 8 - size.height
        }
        view.frame = NSRect(x: x, y: y, width: size.width, height: size.height)
    }

    private func watch() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .keyDown, .scrollWheel]) { [weak self] event in
            guard let self else { return event }
            switch event.type {
            case .keyDown where event.keyCode == 53:
                dismiss()
                return nil
            case .leftMouseDown, .rightMouseDown, .scrollWheel:
                let point = view.convert(event.locationInWindow, from: nil)
                let onAnchor = anchor.map { $0.bounds.contains($0.convert(event.locationInWindow, from: nil)) } ?? false
                if !view.bounds.contains(point), !onAnchor || event.type == .scrollWheel { dismiss() }
            default:
                break
            }
            return event
        }
    }

    private func dismiss() {
        close()
        onClose()
    }

    func close() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        view.removeFromSuperview()
    }
}
#endif
