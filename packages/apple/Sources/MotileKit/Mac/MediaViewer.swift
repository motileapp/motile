#if os(macOS)
import AVKit
import SwiftUI

/// The images and videos of a message or of the composer, one at a time over the whole window.
/// ← and → go through them, and Esc or a click beside the picture closes it. An image zooms
/// as it does in Preview. Its buttons sit in the window's top bar, and a right click
/// on it copies or saves it. An image can be marked up, to put in the composer.
struct MediaViewer: View {
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    @Environment(AppStore.self) private var store
    let viewing: Viewing
    @State private var loaded: Loaded?
    @State private var failed = false
    @State private var fraction: Double?
    @State private var keys: Any?
    @State private var playerReady = false
    @State private var fullScreen = false
    @State private var file: URL?
    @State private var copies = 0
    @State private var markup: MarkupSession?
    @State private var discarding: Discarding?
    @State private var markupFailed = false

    private struct Discarding: Identifiable {
        var id: Int { 0 }
    }

    private enum Loaded {
        case image(CGImage, scale: CGFloat)
        case video(AVPlayer)
    }

    var body: some View {
        GeometryReader { window in
            viewer(bar: window.safeAreaInsets.top)
        }
    }

    /// `bar` is the height of the window's top bar, which the name and the buttons are drawn in.
    private func viewer(bar: CGFloat) -> some View {
        let item = viewing.item
        let count = file == nil ? 1 : markable ? 4 : 3
        return ZStack {
            Color.themeScrim
                .ignoresSafeArea()
                .onTapGesture { if markup == nil { store.closeViewer() } }
            content(item)
            if viewing.items.count > 1, markup == nil {
                HStack {
                    roundButton(.chevronLeft, help: "Previous (←)") { store.viewNext(-1) }
                    Spacer()
                    roundButton(.chevronRight, help: "Next (→)") { store.viewNext(1) }
                }
                .padding(.horizontal, 14)
            }
        }
        // One row in the top bar places the name and the buttons, so they share a middle. The name
        // is centered on the window and keeps clear of the buttons however many of them show.
        .overlay(alignment: .top) {
            ZStack {
                HStack(spacing: 8) {
                    Text(item.name)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if viewing.items.count > 1 {
                        Text("\(viewing.index + 1) of \(viewing.items.count)")
                            .foregroundStyle(Color.themeMutedForegroundOverPicture)
                    }
                }
                .font(.ui(.sm, weight: .medium))
                .foregroundStyle(Color.themeForegroundOverPicture)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(Color.themeScrim, in: Capsule())
                .padding(.horizontal, markup == nil ? Self.nameInset : Self.markupNameInset)
                if let markup {
                    markupButtons(markup)
                        .padding(.trailing, Self.cornerInset)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                } else {
                    HStack(spacing: 0) {
                        if let file {
                            if markable {
                                roundButton(.pencilLine, help: "Mark Up (M)", margin: .sharing(Self.buttonGap, at: 0, of: count)) { startMarkup() }
                            }
                            let first = markable ? 1 : 0
                            CopyButton(
                                help: "\(Self.copyTitle(item)) (⌘C)", variant: .overlay, size: .large,
                                symbolSize: ControlSize.large.smallSymbol, round: true, copies: copies,
                                margin: .sharing(Self.buttonGap, at: first, of: count)
                            ) { MediaFiles.copy(file, video: item.video, named: item.name) }
                            roundButton(.download, help: "Save As (⌘S)", margin: .sharing(Self.buttonGap, at: first + 1, of: count)) {
                                MediaFiles.save(file, named: item.name)
                            }
                        }
                        roundButton(.x, help: "Close (Esc)", margin: .sharing(Self.buttonGap, at: count - 1, of: count)) { store.closeViewer() }
                    }
                    .padding(.trailing, Self.cornerInset)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
            .frame(height: ControlSize.large.height)
            .padding(.top, Self.cornerInset)
            .offset(y: -bar)
        }
        .overlay(alignment: .bottom) {
            if let markup {
                VStack(spacing: 10) {
                    if markupFailed {
                        Text("This image couldn't be marked up.")
                            .font(.ui(.sm))
                            .foregroundStyle(Color.themeForegroundOverPicture)
                            .padding(.horizontal, 10)
                            .frame(height: 24)
                            .background(Color.themeScrim, in: Capsule())
                    }
                    MarkupBar(session: markup)
                }
                .padding(.bottom, Self.cornerInset + 8)
                .appearing(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.snappy(duration: 0.25), value: markup == nil)
        .asking($discarding) { _ in
            Confirmation(
                title: "Discard your marks?", message: "The image stays as it was.",
                answers: [Answer(title: "Discard", variant: .danger) { discardMarkup() }])
        }
        .task(id: item) {
            endMarkup()
            load(item)
        }
        .onReceive(NotificationCenter.default.publisher(for: .mediaProgress)) { note in
            guard case .media(let id) = item.source, note.userInfo?["id"] as? String == id else { return }
            fraction = note.userInfo?["fraction"] as? Double
        }
        .onAppear {
            keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                handle(event) ? nil : event
            }
        }
        .onDisappear {
            if let keys { NSEvent.removeMonitor(keys) }
            keys = nil
            pause()
            markup?.close()
        }
    }

    /// An image that is here can be marked up.
    private var markable: Bool {
        guard case .image = loaded else { return false }
        return file != nil
    }

    private func markupButtons(_ markup: MarkupSession) -> some View {
        HStack(spacing: 8) {
            ActionButton("Cancel", help: "Cancel (Esc)", variant: .overlay, size: .large, round: true) { cancelMarkup() }
            ActionButton(
                markup.attachment == nil ? "Add to Message" : "Done", help: "Return", variant: .primary, size: .large,
                pending: markup.finishing, round: true
            ) { finishMarkup() }
            .disabled(!markup.changed && markup.attachment != nil)
        }
    }

    private static var markupFolder: URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("motile-attachments/markup", isDirectory: true)
    }

    /// Marks up the image shown, or goes on with the marks it was given in the composer, over the
    /// image before them.
    private func startMarkup() {
        guard case .image(let image, let scale) = loaded, let file else { return }
        let item = viewing.item
        markupFailed = false
        guard let saved = item.attachment.flatMap({ id in store.attachments.first { $0.id == id }?.markup }) else {
            markup = MarkupSession(source: file, name: item.name, attachment: item.attachment, width: image.width, height: image.height, saved: nil)
            return
        }
        Pictures.decode(saved.original, id: "view:\(saved.original.path)", maxPixels: 8192) { original in
            guard viewing.item == item, let original else { return }
            loaded = .image(original, scale: scale)
            markup = MarkupSession(
                source: saved.original, name: saved.name, attachment: item.attachment, width: original.width, height: original.height, saved: saved)
        }
    }

    private func cancelMarkup() {
        guard let markup else { return }
        guard !markup.changed else { return discarding = Discarding() }
        discardMarkup()
    }

    /// Leaves the marks, and shows the image as it was when they were drawn over another.
    private func discardMarkup() {
        let reload = markup?.source != file
        endMarkup()
        if reload { load(viewing.item) }
    }

    private func endMarkup() {
        markup?.close()
        markup = nil
    }

    private func finishMarkup() {
        guard let markup, !markup.finishing, markup.changed || markup.attachment == nil else { return }
        markupFailed = false
        markup.finish(into: Self.markupFolder) { result in
            guard let result else { return markupFailed = true }
            store.attachMarkup(result, replacing: markup.attachment)
            endMarkup()
            store.closeViewer()
        }
    }

    /// While an image is marked up, the keys pick its tools and colours.
    private func handleMarkup(_ event: NSEvent, _ markup: MarkupSession) -> Bool {
        guard discarding == nil else { return false }
        if event.keyCode == 53 {
            markup.selected == nil ? cancelMarkup() : markup.deselect()
            return true
        }
        guard markup.selected == nil else { return false }
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        if event.modifierFlags.contains(.command) {
            guard key == "z" else { return false }
            event.modifierFlags.contains(.shift) ? markup.redo() : markup.undo()
            return true
        }
        if event.keyCode == 36 || event.keyCode == 76 {
            finishMarkup()
        } else if let tool = MarkupTool.allCases.first(where: { $0.key.lowercased() == key }) {
            markup.tool = tool
        } else if let digit = Int(key), markup.inks.indices.contains(digit - 1) {
            markup.ink = markup.inks[digit - 1]
        }
        return true
    }

    @ViewBuilder private func content(_ item: ViewedMedia) -> some View {
        switch loaded {
        case .image(let image, let scale):
            ZoomableImage(
                image: image,
                size: CGSize(width: CGFloat(image.width) / scale, height: CGFloat(image.height) / scale),
                margin: markup == nil ? ZoomingScrollView.viewerMargin : Self.markupMargin,
                radius: Radius.md,
                largestFit: ZoomingScrollView.viewerLargestFit,
                menu: menu,
                clickedBeside: { if markup == nil { store.closeViewer() } },
                markup: markup
            )
            .ignoresSafeArea()
        case .video(let player):
            PlayerView(player: player, ready: $playerReady, fullScreen: $fullScreen, menu: menu)
                .clipShape(RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
                .padding(.horizontal, ZoomingScrollView.viewerMargin.width)
                .padding(.vertical, ZoomingScrollView.viewerMargin.height)
                .overlay {
                    if !playerReady {
                        Spinner(size: ControlSize.large.symbol)
                            .foregroundStyle(Color.themeMutedForegroundOverPicture)
                    }
                }
        case nil:
            Group {
                if failed {
                    Text(item.video ? "This video couldn't be loaded." : "This image couldn't be loaded.")
                } else if let fraction {
                    Text("Downloading \(item.name) · \(Int(fraction * 100))%").monospacedDigit()
                } else {
                    Spinner(size: ControlSize.large.symbol)
                }
            }
            .font(.ui(.sm))
            .foregroundStyle(Color.themeMutedForegroundOverPicture)
        }
    }

    /// The room between the round buttons' lights, which their presses share.
    private static let buttonGap: CGFloat = 8
    /// The room between the corner's buttons and the window's top and trailing edges.
    private static let cornerInset: CGFloat = 16
    /// The room on each side of the name: what all four corner buttons take and a gap, the same
    /// on both sides so the name stays centered and doesn't move when the buttons come in.
    private static var nameInset: CGFloat {
        cornerInset + 4 * ControlSize.large.height + 3 * buttonGap + cornerInset
    }

    /// The room around a picture being marked up, which keeps it clear of the bar under it.
    private static let markupMargin = CGSize(
        width: ZoomingScrollView.viewerMargin.width, height: cornerInset + 8 + ControlSize.large.height + 12 + 12)
    /// The same beside Cancel and Add to Message.
    private static let markupNameInset: CGFloat = 300

    private static func copyTitle(_ item: ViewedMedia) -> String {
        item.video ? "Copy Video" : "Copy Image"
    }

    /// What a right click on the picture offers, as the corner's buttons do.
    private func menu() -> [MenuAction] {
        guard let file else { return [] }
        let item = viewing.item
        return [
            MenuAction(title: Self.copyTitle(item), symbol: .copy) {
                MediaFiles.copy(file, video: item.video, named: item.name)
                copies += 1
            },
            MenuAction(title: MediaFiles.saveTitle, symbol: .download) { MediaFiles.save(file, named: item.name) },
        ]
    }

    private func roundButton(_ symbol: Symbol, help: String, margin: EdgeInsets = EdgeInsets(), action: @escaping () -> Void) -> some View {
        ActionButton(
            icon: symbol, help: help, variant: .overlay, size: .large, symbolSize: ControlSize.large.smallSymbol, round: true,
            margin: margin, action: action
        )
    }

    /// The player's own full screen and the save panel take the keys while they are up.
    private func handle(_ event: NSEvent) -> Bool {
        guard !fullScreen, !(event.window is NSPanel) else { return false }
        if let markup { return handleMarkup(event, markup) }
        if event.charactersIgnoringModifiers == "m", event.modifierFlags.intersection(.deviceIndependentFlagsMask).isEmpty, markable {
            startMarkup()
            return true
        }
        if event.modifierFlags.contains(.command), let file {
            switch event.charactersIgnoringModifiers {
            case "c":
                MediaFiles.copy(file, video: viewing.item.video, named: viewing.item.name)
                copies += 1
                return true
            case "s":
                MediaFiles.save(file, named: viewing.item.name)
                return true
            default: break
            }
        }
        switch event.keyCode {
        case 53: store.closeViewer()
        case 123: store.viewNext(-1)
        case 124: store.viewNext(1)
        case 49:
            guard case .video(let player) = loaded else { return true }
            player.timeControlStatus == .paused ? player.play() : player.pause()
        default: return !event.modifierFlags.contains(.command)
        }
        return true
    }

    private func pause() {
        guard case .video(let player) = loaded else { return }
        player.pause()
    }

    private func load(_ item: ViewedMedia) {
        pause()
        (loaded, failed, fraction, playerReady, file) = (nil, false, nil, false, nil)
        let show = { (file: URL?) in
            guard viewing.item == item else { return }
            guard let file else { return failed = true }
            self.file = file
            guard item.video else {
                let scale = NSScreen.main?.backingScaleFactor ?? 2
                return Pictures.decode(file, id: "view:\(file.path)", maxPixels: 8192) { image in
                    guard viewing.item == item else { return }
                    loaded = image.map { .image($0, scale: scale) }
                    failed = image == nil
                }
            }
            // A video the Mac's player can't play opens in the app the Mac has for it.
            guard Self.playedHere.contains(file.pathExtension.lowercased()) else {
                NSWorkspace.shared.open(file)
                return store.closeViewer()
            }
            let player = AVPlayer(url: file)
            loaded = .video(player)
            player.play()
        }
        switch item.source {
        case .file(let file): show(file)
        case .media(let id): store.media(id, done: show)
        }
    }
}

/// The Mac's player with its controls and its full-screen button. `ready` is set once it draws
/// its first frame, `fullScreen` while it fills the screen.
private struct PlayerView: NSViewRepresentable {
    let player: AVPlayer
    @Binding var ready: Bool
    @Binding var fullScreen: Bool
    let menu: () -> [MenuAction]

    func makeNSView(context: Context) -> MenuPlayerView {
        let view = MenuPlayerView()
        view.controlsStyle = .floating
        view.showsFullScreenToggleButton = true
        view.delegate = context.coordinator
        context.coordinator.watch(view)
        return view
    }

    func updateNSView(_ view: MenuPlayerView, context: Context) {
        context.coordinator.parent = self
        view.menuActions = menu
        guard view.player !== player else { return }
        view.player = player
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, AVPlayerViewDelegate {
        var parent: PlayerView
        private var ready: NSKeyValueObservation?

        init(_ parent: PlayerView) { self.parent = parent }

        func watch(_ view: AVPlayerView) {
            ready = view.observe(\.isReadyForDisplay, options: [.initial, .new]) { [weak self] view, _ in
                let ready = view.isReadyForDisplay
                DispatchQueue.main.async { self?.parent.ready = ready }
            }
        }

        func playerViewWillEnterFullScreen(_ playerView: AVPlayerView) { parent.fullScreen = true }

        func playerViewDidExitFullScreen(_ playerView: AVPlayerView) { parent.fullScreen = false }
    }
}

/// The Mac's player, with what a right click on it offers.
private final class MenuPlayerView: AVPlayerView {
    var menuActions: () -> [MenuAction] = { [] }

    override func menu(for event: NSEvent) -> NSMenu? {
        let actions = menuActions()
        return actions.isEmpty ? super.menu(for: event) : NSMenu(actions)
    }
}

/// A `CGImage` or an `NSImage`, at `size` in points when it isn't zoomed.
struct ZoomableImage: NSViewRepresentable {
    let image: AnyObject
    let size: CGSize
    let margin: CGSize
    var radius: CGFloat = 0
    /// The most it is enlarged to fit, as a multiple of `size`.
    var largestFit: CGFloat = 1
    /// Whether ⌘+, ⌘−, ⌘0 and ⌘9 are this image's.
    var keys = true
    /// What a right click on the picture offers.
    var menu: () -> [MenuAction] = { [] }
    var clickedBeside: () -> Void = {}
    /// Takes the pointer to draw marks while it is set.
    var markup: MarkupSession?

    func makeNSView(context: Context) -> ZoomingScrollView {
        ZoomingScrollView(margin: margin, radius: radius, largestFit: largestFit)
    }

    func updateNSView(_ view: ZoomingScrollView, context: Context) {
        view.putAway(context.environment.putAway)
        view.keys = keys
        view.menuActions = menu
        view.clickedBeside = clickedBeside
        view.show(image, size: size)
        view.markup = markup
        view.margin = margin
    }
}

/// An image that fits the window until it is zoomed: by pinching, a double click, or ⌘+, ⌘−,
/// ⌘0 for its real size and ⌘9 to fit again. Zoomed, it is moved by scrolling or dragging, or
/// only by scrolling while it is marked up, when dragging draws.
final class ZoomingScrollView: NSScrollView {
    static let viewerMargin = CGSize(width: 64, height: 52)
    /// A small image is enlarged to fit the viewer, up to twice its size.
    static let viewerLargestFit: CGFloat = 2
    /// How much smaller than fitting, or than its real size if that is smaller, an image zooms out.
    private static let zoomOut: CGFloat = 4

    var keys = true
    var menuActions: () -> [MenuAction] = { [] }
    var clickedBeside: () -> Void = {}
    /// The room around the picture when it fits: more while it is marked up, to clear the bar.
    var margin: CGSize {
        didSet {
            guard margin != oldValue else { return }
            let wasFitted = abs(magnification - fitFor(oldValue)) < 0.001
            minMagnification = smallest
            guard wasFitted else { return }
            animator().magnification = fit
        }
    }
    private let radius: CGFloat
    private let largestFit: CGFloat
    private let picture = FlippedView()
    private var dragged = false
    private var canvas: MarkupCanvas?

    var markup: MarkupSession? {
        get { canvas?.session }
        set {
            guard newValue !== canvas?.session else { return }
            canvas?.removeFromSuperview()
            canvas = newValue.map(MarkupCanvas.init)
            if let canvas {
                canvas.frame = picture.bounds
                canvas.autoresizingMask = [.width, .height]
                picture.addSubview(canvas)
            }
            window?.invalidateCursorRects(for: self)
        }
    }

    private var fit: CGFloat { fitFor(margin) }

    private func fitFor(_ margin: CGSize) -> CGFloat {
        let size = picture.frame.size
        guard size.width > 0, size.height > 0 else { return 1 }
        let width = (bounds.width - 2 * margin.width) / size.width
        let height = (bounds.height - 2 * margin.height) / size.height
        return max(0.01, min(largestFit, width, height))
    }

    /// An image enlarged to fit can still be shown at its real size, and either can be zoomed out
    /// to a quarter of that.
    private var smallest: CGFloat { min(fit, 1) / Self.zoomOut }

    private var fitted: Bool { abs(magnification - fit) < 0.001 }

    init(margin: CGSize, radius: CGFloat, largestFit: CGFloat) {
        self.margin = margin
        self.radius = radius
        self.largestFit = largestFit
        super.init(frame: .zero)
        contentView = CenteringClipView()
        documentView = picture
        picture.wantsLayer = true
        picture.layer?.masksToBounds = true
        picture.layer?.cornerCurve = CALayerCornerCurve.continuous
        picture.layer?.minificationFilter = .trilinear
        drawsBackground = false
        allowsMagnification = true
        automaticallyAdjustsContentInsets = false
        (minMagnification, maxMagnification) = (1, 8)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    func show(_ image: AnyObject, size: CGSize) {
        guard picture.layer?.contents as AnyObject? !== image else { return }
        picture.layer?.contents = image
        picture.setFrameSize(size)
        minMagnification = smallest
        magnification = fit
    }

    override func setFrameSize(_ size: NSSize) {
        let wasFitted = fitted
        super.setFrameSize(size)
        minMagnification = smallest
        guard wasFitted || magnification < smallest else { return }
        magnification = fit
    }

    override func reflectScrolledClipView(_ clipView: NSClipView) {
        super.reflectScrolledClipView(clipView)
        picture.layer?.cornerRadius = radius / magnification
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) == nil ? nil : self
    }

    override func resetCursorRects() {
        guard canvas != nil else { return super.resetCursorRects() }
        addCursorRect(bounds, cursor: .crosshair)
    }

    /// Where the pointer is on the canvas, in the image's pixels, and how far from a pin it may be.
    private func marking(_ event: NSEvent) -> (session: MarkupSession, point: CGPoint, slop: CGFloat)? {
        guard let canvas else { return nil }
        let point = canvas.pixel(canvas.convert(event.locationInWindow, from: nil))
        return (canvas.session, point, canvas.pixels(6, zoom: magnification))
    }

    override func mouseDown(with event: NSEvent) {
        dragged = false
        if let marking = marking(event) { return marking.session.down(at: marking.point, slop: marking.slop) }
        guard event.clickCount == 2, onPicture(event) else { return }
        toggleZoom(at: contentView.convert(event.locationInWindow, from: nil))
    }

    override func mouseDragged(with event: NSEvent) {
        dragged = true
        if let marking = marking(event) { return marking.session.drag(to: marking.point) }
        var origin = contentView.bounds.origin
        origin.x -= event.deltaX / magnification
        origin.y -= event.deltaY / magnification
        contentView.setBoundsOrigin(contentView.constrainBoundsRect(NSRect(origin: origin, size: contentView.bounds.size)).origin)
    }

    override func mouseUp(with event: NSEvent) {
        if let marking = marking(event) { return marking.session.up(at: marking.point) }
        guard !dragged, !onPicture(event) else { return }
        clickedBeside()
    }

    override func scrollWheel(with event: NSEvent) {
        guard event.modifierFlags.contains(.command), !event.hasPreciseScrollingDeltas, event.scrollingDeltaY != 0 else {
            return super.scrollWheel(with: event)
        }
        let target = magnification * exp(event.scrollingDeltaY * log(1.12))
        setMagnification(min(max(target, smallest), maxMagnification), centeredAt: contentView.convert(event.locationInWindow, from: nil))
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let actions = menuActions()
        guard onPicture(event), !actions.isEmpty else { return nil }
        return NSMenu(actions)
    }

    override func smartMagnify(with event: NSEvent) {
        guard canvas == nil else { return }
        toggleZoom(at: contentView.convert(event.locationInWindow, from: nil))
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard keys, event.modifierFlags.contains(.command) else { return false }
        switch event.charactersIgnoringModifiers {
        case "=", "+": zoom(to: magnification * 1.5)
        case "-": zoom(to: magnification / 1.5)
        case "0": zoom(to: 1)
        case "9": zoom(to: fit)
        default: return false
        }
        return true
    }

    private func onPicture(_ event: NSEvent) -> Bool {
        picture.bounds.contains(picture.convert(event.locationInWindow, from: nil))
    }

    private func toggleZoom(at point: NSPoint) {
        animator().setMagnification(fitted ? max(1, fit * 2) : fit, centeredAt: point)
    }

    private func zoom(to magnification: CGFloat) {
        let center = NSPoint(x: contentView.bounds.midX, y: contentView.bounds.midY)
        animator().setMagnification(min(max(magnification, smallest), maxMagnification), centeredAt: center)
    }
}

/// Keeps a picture smaller than the window in its middle.
private final class CenteringClipView: NSClipView {
    override func constrainBoundsRect(_ proposed: NSRect) -> NSRect {
        var rect = super.constrainBoundsRect(proposed)
        guard let document = documentView else { return rect }
        if rect.width > document.frame.width { rect.origin.x = (document.frame.width - rect.width) / 2 }
        if rect.height > document.frame.height { rect.origin.y = (document.frame.height - rect.height) / 2 }
        return rect
    }
}
#endif
