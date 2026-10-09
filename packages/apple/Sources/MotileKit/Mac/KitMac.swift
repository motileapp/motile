#if os(macOS)
import AppKit
import QuartzCore

/// An entry of the menu a view opens under a right click.
struct MenuAction {
    let title: String
    let symbol: Symbol
    let run: () -> Void
}

extension NSView {
    var opacity: CGFloat {
        get { alphaValue }
        set { alphaValue = newValue }
    }

    func redraw() { needsDisplay = true }

    var tip: String? {
        get { toolTip }
        set { toolTip = newValue }
    }

    func describe(_ label: String, button: Bool = false) {
        setAccessibilityElement(true)
        if button { setAccessibilityRole(.button) }
        setAccessibilityLabel(label)
    }

    func fade(to opacity: CGFloat, duration: TimeInterval) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = duration
            animator().alphaValue = opacity
        }
    }

    /// A shadow of the size, in the colour shadow at one of its opacities, as the appearance
    /// has it now. A view applies it again when its appearance changes.
    func applyShadow(_ size: ShadowSize, _ strength: Opacity) {
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let shadow = NSShadow()
        shadow.shadowColor = Theme.resolved(Theme.thinned(Theme.shadow, strength), dark: dark)
        shadow.shadowBlurRadius = size.radius
        shadow.shadowOffset = NSSize(width: 0, height: -size.down)
        self.shadow = shadow
    }

    func ticker(target: Any, selector: Selector) -> CADisplayLink {
        displayLink(target: target, selector: selector)
    }
}

/// A view whose origin is its top left corner, like the text inside it. It can take clicks, say
/// where the pointer is over it and open a menu.
class FlippedView: NSView {
    enum Pointer {
        case arrow, hand
    }

    /// Called with where the view was clicked. A view with one takes the clicks on what is inside it.
    var onPress: ((CGPoint) -> Void)?
    /// Called with where the pointer is over the view, and with nothing when it leaves.
    var onHover: ((CGPoint?) -> Void)? {
        didSet { updateTrackingAreas() }
    }
    var menuActions: (() -> [MenuAction])?
    /// The cursor over a view that takes clicks.
    var pointer = Pointer.arrow
    private var tracking: NSTrackingArea?

    override var isFlipped: Bool { true }

    /// Places what is inside the view.
    func layoutNow() {}

    override func layout() {
        super.layout()
        layoutNow()
    }

    /// Whether a click at the point is the view's.
    func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard onPress != nil, !isHidden, takesPress(at: convert(point, from: superview)) else { return super.hitTest(point) }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard let onPress, takesPress(at: point) else { return super.mouseDown(with: event) }
        onPress(point)
    }

    // One area that follows the view, so that moving a row doesn't make another.
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        guard tracking == nil, onHover != nil else { return }
        let options: NSTrackingArea.Options = [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect]
        let area = NSTrackingArea(rect: .zero, options: options, owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        onHover?(convert(event.locationInWindow, from: nil))
    }

    override func mouseMoved(with event: NSEvent) {
        onHover?(convert(event.locationInWindow, from: nil))
    }

    override func mouseExited(with event: NSEvent) {
        onHover?(nil)
    }

    /// A view that takes clicks keeps its cursor, also when it floats over text.
    override func resetCursorRects() {
        guard onPress != nil else { return }
        addCursorRect(bounds, cursor: pointer == .hand ? .pointingHand : .arrow)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        guard let actions = menuActions?(), !actions.isEmpty else { return super.menu(for: event) }
        let menu = NSMenu()
        for action in actions { menu.addItem(ClosureMenuItem(action)) }
        return menu
    }
}

private final class ClosureMenuItem: NSMenuItem {
    private let run: () -> Void

    init(_ action: MenuAction) {
        run = action.run
        super.init(title: action.title, action: #selector(chosen), keyEquivalent: "")
        target = self
    }

    required init(coder: NSCoder) { fatalError("not used") }

    @objc private func chosen() { run() }
}

/// A view that shows its layer, which `paint` sets up in the colours of the appearance.
class LayerView: FlippedView {
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        paint(layer)
    }

    func paint(_ layer: CALayer) {}

    func repaint() { needsDisplay = true }

    func resolved(_ color: NSColor) -> CGColor { color.cgColor }
}

/// One line of text.
class TextLabel: NSTextField {
    convenience init(font: NSFont, color: NSColor) {
        self.init(labelWithString: "")
        self.font = font
        textColor = color
        lineBreakMode = .byTruncatingTail
        maximumNumberOfLines = 1
    }

    var string: String {
        get { stringValue }
        set { stringValue = newValue }
    }

    var attributed: NSAttributedString {
        get { attributedStringValue }
        set { attributedStringValue = newValue }
    }

    var color: NSColor? {
        get { textColor }
        set { textColor = newValue }
    }

    var breaks: NSLineBreakMode {
        get { lineBreakMode }
        set { lineBreakMode = newValue }
    }

    var centered: Bool {
        get { alignment == .center }
        set { alignment = newValue ? .center : .natural }
    }

    var naturalWidth: CGFloat { intrinsicContentSize.width }

    /// The size the words take with the room the field draws around them, whatever width it
    /// was last given.
    var natural: CGSize {
        CGSize(width: ceil(cell?.cellSize.width ?? 0), height: intrinsicContentSize.height)
    }
}

/// A bright copy of a label, laid over it and seen only through a soft band that sweeps across.
final class ShimmerLabel: TextLabel {
    private static let bandWidth: CGFloat = 72
    private static let period: CFTimeInterval = 2.2

    private let band = CAGradientLayer()

    var sweeps = false {
        didSet { restart() }
    }

    override var frame: NSRect {
        didSet {
            guard frame.size != oldValue.size else { return }
            restart()
        }
    }

    override var stringValue: String {
        didSet {
            guard stringValue != oldValue else { return }
            restart()
        }
    }

    static func make(_ font: NSFont) -> ShimmerLabel {
        let field = ShimmerLabel(font: font, color: Theme.emphasizedForeground)
        field.setAccessibilityElement(false)

        let (edge, middle) = (Opacity.shimmerBandEdge.fixed, Opacity.shimmerBandMiddle.fixed)
        let alphas: [CGFloat] = [0, edge, middle, 1, middle, edge, 0]
        field.band.colors = alphas.map { NSColor.black.withAlphaComponent($0).cgColor }
        field.band.locations = [0, 0.15, 0.35, 0.5, 0.65, 0.85, 1]
        field.band.startPoint = CGPoint(x: 0, y: 0.5)
        field.band.endPoint = CGPoint(x: 1, y: 0.5)
        field.wantsLayer = true
        field.layer?.mask = field.band
        return field
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        restart()
    }

    override func viewDidHide() {
        super.viewDidHide()
        restart()
    }

    override func viewDidUnhide() {
        super.viewDidUnhide()
        restart()
    }

    private func restart() {
        band.removeAllAnimations()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        band.frame = NSRect(x: -Self.bandWidth, y: 0, width: Self.bandWidth, height: bounds.height)
        CATransaction.commit()
        guard sweeps, window != nil, !isHiddenOrHasHiddenAncestor else { return }
        guard !Platform.reducesMotion else { return }

        let sweep = CABasicAnimation(keyPath: "position.x")
        sweep.fromValue = -Self.bandWidth / 2
        // Across the words, however much room the label has.
        sweep.toValue = min(bounds.width, intrinsicContentSize.width) + Self.bandWidth / 2
        sweep.duration = Self.period
        sweep.repeatCount = .infinity
        // Every sweep starts on the same beat, so labels move together and a restart doesn't show.
        let now = band.convertTime(CACurrentMediaTime(), from: nil)
        sweep.beginTime = now - now.truncatingRemainder(dividingBy: Self.period)
        band.add(sweep, forKey: "sweep")
    }
}

/// A symbol in one colour, in the middle of its frame.
final class SymbolView: NSImageView {
    convenience init(_ symbol: Symbol? = nil, size: CGFloat = 12, tint: NSColor = Theme.mutedForeground) {
        self.init(frame: .zero)
        imageScaling = .scaleNone
        contentTintColor = tint
        if let symbol { show(symbol, size: size) }
    }

    func show(_ symbol: Symbol, size: CGFloat = 12) {
        image = .symbol(symbol, size: size)
    }

    var tint: NSColor? {
        get { contentTintColor }
        set { contentTintColor = newValue }
    }

    private var shadowSpec: (size: ShadowSize, strength: Opacity)?

    /// A shadow of the size, which follows the appearance.
    func dropShadow(_ size: ShadowSize, _ strength: Opacity) {
        shadowSpec = (size, strength)
        applyShadow(size, strength)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        guard let shadowSpec else { return }
        applyShadow(shadowSpec.size, shadowSpec.strength)
    }
}

/// The transcript's button: a symbol and, optionally, a title, sized and lit as `ActionButton`
/// is. Under the pointer and while it is pressed it has a background and its symbol is in the
/// colour of text.
final class IconButton: NSButton {
    static let metrics = MotileKit.ControlSize.regular
    static let side = metrics.height

    /// The layer it lies on.
    var surface = Surface.background
    private var symbolSize = IconButton.metrics.symbol
    private var action_: (() -> Void)?
    private var tracking: NSTrackingArea?
    private var hovering = false { didSet { light() } }
    private var pressing = false { didSet { light() } }

    convenience init(symbol: Symbol, title: String = "", symbolSize: CGFloat? = nil, tooltip: String, action: @escaping () -> Void) {
        self.init(frame: .zero)
        self.symbolSize = symbolSize ?? Self.metrics.symbol
        isBordered = false
        bezelStyle = .inline
        wantsLayer = true
        layer?.cornerRadius = Self.metrics.radius
        layer?.cornerCurve = .continuous
        image = .symbol(symbol, size: self.symbolSize)
        imagePosition = title.isEmpty ? .imageOnly : .imageLeading
        self.title = title
        font = Theme.smallFont
        contentTintColor = Theme.mutedForeground
        toolTip = tooltip
        target = self
        self.action = #selector(pressed)
        action_ = action
        setButtonType(.momentaryChange)
    }

    func set(symbol: Symbol, title: String = "") {
        image = .symbol(symbol, size: symbolSize)
        self.title = title
    }

    // One area that follows the view, so that moving a row doesn't make another.
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        guard tracking == nil else { return }
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        hovering = true
    }

    override func mouseExited(with event: NSEvent) {
        hovering = false
    }

    // The button tracks the mouse inside `mouseDown` until it is let go.
    override func mouseDown(with event: NSEvent) {
        pressing = true
        super.mouseDown(with: event)
        pressing = false
    }

    private func light() {
        let lit = hovering || pressing
        contentTintColor = lit ? Theme.foreground : Theme.mutedForeground
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = lit ? surface.platform(.control).cgColor : nil
        }
    }

    @objc private func pressed() {
        action_?()
    }
}

/// A read-only text view that the transcript sizes itself. It never scrolls; wheel events go on
/// to the transcript.
final class RowTextView: NSTextView {
    /// Called when the user starts selecting here, so other rows can let go of their selection.
    var onSelect: (() -> Void)?
    private var system: TextSystem!

    static func make(wraps: Bool = true) -> RowTextView {
        let system = TextSystem()
        // The view takes the storage as its own while it is made.
        let view = withExtendedLifetime(system.storage) { RowTextView(frame: .zero, textContainer: system.container) }
        view.system = system
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = false
        view.textContainerInset = .zero
        view.isVerticallyResizable = false
        view.isHorizontallyResizable = false
        view.isAutomaticLinkDetectionEnabled = false
        view.usesFontPanel = false
        view.usesFindBar = false
        view.isRichText = true
        view.linkTextAttributes = [.foregroundColor: Theme.primary, .cursor: NSCursor.pointingHand]
        if !wraps { system.unwrap() }
        return view
    }

    var content: NSAttributedString {
        get { system.storage }
        set { system.update(to: newValue) }
    }

    /// The room beside the text, inside the view.
    var sideInset: CGFloat {
        get { textContainerInset.width }
        set { textContainerInset = NSSize(width: newValue, height: 0) }
    }

    func height(forWidth width: CGFloat) -> CGFloat {
        system.height(forWidth: width)
    }

    /// Fades in the text under `oldHeight`, which was just added, and leaves the rest as it is.
    func fadeIn(below oldHeight: CGFloat) {
        wantsLayer = true
        guard let layer else { return }
        GrowthFade.run(on: layer, size: bounds.size, below: oldHeight)
    }

    func codeBoxes() -> [(frame: CGRect, language: String, code: String)] {
        system.codeBoxes(origin: textContainerOrigin)
    }

    override func writeSelection(to pboard: NSPasteboard, types: [NSPasteboard.PasteboardType]) -> Bool {
        guard super.writeSelection(to: pboard, types: types) else { return false }
        guard let text = pboard.string(forType: .string), text.contains("\u{2028}") else { return true }
        pboard.setString(TextSystem.withLineBreaks(text), forType: .string)
        return true
    }

    func clearSelection() {
        guard selectedRange().length > 0 else { return }
        setSelectedRange(NSRange(location: 0, length: 0))
    }

    override func mouseDown(with event: NSEvent) {
        onSelect?()
        super.mouseDown(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        nextResponder?.scrollWheel(with: event)
    }

    // What floats over the text, like the jump button, keeps its own cursor.
    override func mouseMoved(with event: NSEvent) {
        guard isUnderPointer(event) else { return }
        super.mouseMoved(with: event)
    }

    override func cursorUpdate(with event: NSEvent) {
        guard isUnderPointer(event) else { return }
        super.cursorUpdate(with: event)
    }

    private func isUnderPointer(_ event: NSEvent) -> Bool {
        window?.contentView?.hitTest(event.locationInWindow)?.isDescendant(of: self) ?? false
    }

    // The transcript decides the size; the text view must not grow itself to fit.
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: NSView.noIntrinsicMetric) }
}

/// Shows code that is wider than the column and moves it sideways under the pointer. It is a
/// plain clipping view rather than a scroll view: a scroll view inside the transcript's own
/// doesn't redraw what the transcript scrolls into view.
final class SidewaysClipView: NSView {
    private weak var content: NSView?
    private var contentWidth: CGFloat = 0
    private var offset: CGFloat = 0
    /// Whether the gesture under way moves the code sideways. A gesture keeps the way it set out,
    /// as it does in a scroll view, so one that scrolls the transcript never drags the code along.
    private var gestureIsSideways: Bool?

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func setContent(_ view: NSView, size: NSSize) {
        content = view
        contentWidth = size.width
        offset = min(offset, max(0, contentWidth - bounds.width))
        view.frame = NSRect(x: -offset, y: 0, width: size.width, height: size.height)
    }

    private func isSideways(_ event: NSEvent) -> Bool {
        let sideways = abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
        // A wheel that isn't a gesture says so with every notch.
        guard !event.phase.isEmpty || !event.momentumPhase.isEmpty else { return sideways }
        if event.phase.contains(.began) || event.phase.contains(.mayBegin) { gestureIsSideways = nil }
        if gestureIsSideways == nil, event.scrollingDeltaX != 0 || event.scrollingDeltaY != 0 { gestureIsSideways = sideways }
        return gestureIsSideways ?? false
    }

    override func scrollWheel(with event: NSEvent) {
        let overflow = contentWidth - bounds.width
        guard isSideways(event), overflow > 0, let content else {
            nextResponder?.scrollWheel(with: event)
            return
        }
        offset = min(max(0, offset - event.scrollingDeltaX), overflow)
        content.frame.origin.x = -offset
    }
}

/// The scroll view the transcript's rows are in, as the transcript drives it.
final class TranscriptScroller: NSView {
    let document = FlippedView()
    /// The viewport moved, by the user's hand or not.
    var onScroll: (() -> Void)?
    /// The user's fingers, or the momentum they gave it, started or stopped moving the viewport.
    var onUserScroll: ((Bool) -> Void)?

    private let scrollView = NSScrollView()

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.automaticallyAdjustsContentInsets = false
        scrollView.documentView = document
        scrollView.contentView.postsBoundsChangedNotifications = true
        scrollView.wantsLayer = true
        addSubview(scrollView)
        NotificationCenter.default.addObserver(
            self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification, object: scrollView.contentView)
        NotificationCenter.default.addObserver(
            self, selector: #selector(userScrollBegan), name: NSScrollView.willStartLiveScrollNotification, object: scrollView)
        NotificationCenter.default.addObserver(
            self, selector: #selector(userScrollEnded), name: NSScrollView.didEndLiveScrollNotification, object: scrollView)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override func layout() {
        super.layout()
        scrollView.frame = bounds
    }

    /// Fades the rows out at the viewport's edges.
    var fadeMask: CALayer? {
        get { scrollView.layer?.mask }
        set { scrollView.layer?.mask = newValue }
    }

    var offsetY: CGFloat { scrollView.contentView.bounds.minY }
    var viewportHeight: CGFloat { scrollView.contentView.bounds.height }
    var documentSize: CGSize { document.frame.size }

    static let indicatorWidth = NSScroller.scrollerWidth(for: .regular, scrollerStyle: .overlay)

    static let blursUnderTopBar = false

    /// How tall the bar over the transcript's top is. The scroller keeps clear of it.
    func setTopBar(height: CGFloat) {
        scrollView.scrollerInsets = NSEdgeInsets(top: height, left: 0, bottom: 0, right: 0)
    }

    func setDocument(width: CGFloat, height: CGFloat) {
        guard document.frame.height != height || document.frame.width != width else { return }
        document.frame = NSRect(x: 0, y: 0, width: width, height: height)
    }

    func scroll(to y: CGFloat) {
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: y))
        scrollView.reflectScrolledClipView(scrollView.contentView)
    }

    @objc private func scrolled() { onScroll?() }
    @objc private func userScrollBegan() { onUserScroll?(true) }
    @objc private func userScrollEnded() { onUserScroll?(false) }
}
#endif

#if os(macOS)
/// What is done with the file of an image or a video outside the client.
enum MediaFiles {
    static let saveTitle = "Save As"

    static func copyImage(at file: URL) {
        DispatchQueue.global(qos: .userInitiated).async {
            guard let tiff = NSImage(contentsOf: file)?.tiffRepresentation else { return }
            DispatchQueue.main.async {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setData(tiff, forType: .tiff)
            }
        }
    }

    /// Puts the file itself on the pasteboard, as Finder copies one.
    static func copyFile(_ file: URL) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.writeObjects([file as NSURL])
    }

    static func save(_ file: URL, named name: String, from view: NSView? = nil) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = name
        let write = { (response: NSApplication.ModalResponse) in
            guard response == .OK, let destination = panel.url else { return }
            DispatchQueue.global(qos: .userInitiated).async {
                try? FileManager.default.removeItem(at: destination)
                try? FileManager.default.copyItem(at: file, to: destination)
            }
        }
        // A panel of its own can open behind the window or on another Space, out of sight.
        guard let window = view?.window ?? NSApp.keyWindow ?? NSApp.mainWindow else { return write(panel.runModal()) }
        panel.beginSheetModal(for: window, completionHandler: write)
    }
}
#endif
