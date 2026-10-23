#if os(macOS)
import AppKit
import WebKit

/// A web view that gives its page the wheel gestures that start over it and that the page takes,
/// and the transcript around it the rest, as a browser does with a frame.
final class PageWebView: WKWebView {
    private enum Wheel {
        /// The page was sent the gesture's events and hasn't said yet whether it takes them.
        case asking([NSEvent])
        case page
        case transcript
    }

    /// The page says whether it takes a wheel; YouTube's player doesn't, and leaves them all to the
    /// transcript.
    fileprivate private(set) var answers = false
    private var wheel = Wheel.transcript
    private var asked = 0
    /// How long a page that doesn't answer keeps a gesture from the transcript.
    private static let patience: TimeInterval = 0.15

    override init(frame: CGRect, configuration: WKWebViewConfiguration) {
        super.init(frame: frame, configuration: configuration)
        WheelGesture.watch()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func scrollWheel(with event: NSEvent) {
        guard WheelGesture.owner === self else { return toTranscript(event) }
        switch wheel {
        case .asking(let held):
            wheel = .asking(held + [event])
            super.scrollWheel(with: event)
        case .page:
            super.scrollWheel(with: event)
        case .transcript:
            toTranscript(event)
        }
    }

    func heard(_ message: WebPage.Message) {
        answers = true
        guard case .wheel(let taken) = message else { return }
        answered(taken: taken)
    }

    /// A gesture starts over the page, which is asked whether it takes it.
    fileprivate func ask() {
        wheel = .asking([])
        asked += 1
        let gesture = asked
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.patience) { [weak self] in
            guard let self, self.asked == gesture else { return }
            self.answered(taken: false)
        }
    }

    private func answered(taken: Bool) {
        guard case .asking(let held) = wheel else { return }
        wheel = taken ? .page : .transcript
        guard !taken else { return }
        held.forEach(toTranscript)
    }

    private func toTranscript(_ event: NSEvent) {
        nextResponder?.scrollWheel(with: event)
    }
}

/// Whose the wheel's gesture is: the page's it started over, or the transcript's, which keeps it
/// when a page slides under the pointer. A gesture starts with the fingers on the trackpad, or with
/// a mouse wheel's first tick after a pause.
@MainActor
private enum WheelGesture {
    static weak var owner: PageWebView?
    private static var lastTick: TimeInterval = 0
    private static var monitor: Any?
    private static let pause: TimeInterval = 0.3

    static func watch() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { event in
            guard starts(event) else { return event }
            owner = page(under: event)
            owner?.ask()
            return event
        }
    }

    private static func starts(_ event: NSEvent) -> Bool {
        if event.phase.contains(.began) { return true }
        guard event.phase.isEmpty, event.momentumPhase.isEmpty else { return false }
        defer { lastTick = event.timestamp }
        return event.timestamp - lastTick > pause
    }

    private static func page(under event: NSEvent) -> PageWebView? {
        guard let content = event.window?.contentView else { return nil }
        let hit = content.hitTest(content.superview?.convert(event.locationInWindow, from: nil) ?? event.locationInWindow)
        let page = sequence(first: hit, next: { $0?.superview }).lazy.compactMap { $0 as? PageWebView }.first
        return page?.answers == true ? page : nil
    }
}

extension WKWebView {
    /// Shows the view behind it until the page draws its own background.
    func clearsBackground() {
        setValue(false, forKey: "drawsBackground")
    }
}

extension WKWebViewConfiguration {
    /// A video plays where it is, once pressed.
    func allowsPlayback() {
        mediaTypesRequiringUserActionForPlayback = .all
    }
}

extension NSColor {
    /// The colour as CSS takes it in one appearance, `#rrggbbaa`.
    func css(dark: Bool) -> String {
        guard let color = Theme.resolved(self, dark: dark).usingColorSpace(.sRGB) else { return "" }
        return cssHex(color.redComponent, color.greenComponent, color.blueComponent, color.alphaComponent)
    }
}
#endif
