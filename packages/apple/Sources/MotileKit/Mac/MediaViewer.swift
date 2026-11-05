#if os(macOS)
import AVKit
import SwiftUI

/// The images and videos of a message or of the composer, one at a time over the whole window.
/// ← and → go through them, and Esc or a click beside the picture closes it. An image zooms
/// as it does in Preview. Its buttons sit in the window's top bar, and a right click
/// on it copies or saves it.
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
        let count = file == nil ? 1 : 3
        let buttonHeight = ControlSize.large.height
        let row = max(bar, buttonHeight + 2 * Self.buttonGap)
        let inset = (row - buttonHeight) / 2
        return ZStack {
            Color.themeScrim
                .ignoresSafeArea()
                .onTapGesture { store.closeViewer() }
            content(item)
            if viewing.items.count > 1 {
                HStack {
                    roundButton(.chevronLeft, help: "Previous (←)") { store.viewNext(-1) }
                    Spacer()
                    roundButton(.chevronRight, help: "Next (→)") { store.viewNext(1) }
                }
                .padding(.horizontal, 14)
            }
        }
        .overlay(alignment: .top) {
            HStack(spacing: 8) {
                Text(item.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if viewing.items.count > 1 {
                    Text("\(viewing.index + 1) of \(viewing.items.count)")
                        .foregroundStyle(Color.themeMutedForegroundOverPicture)
                }
            }
            .font(.ui(size: 13, weight: .medium))
            .foregroundStyle(Color.themeForegroundOverPicture)
            .padding(.horizontal, 10)
            .frame(height: 24)
            .background(Color.themeScrim, in: Capsule())
            .padding(.top, 14)
            .padding(.horizontal, 80)
        }
        .overlay(alignment: .topTrailing) {
            HStack(spacing: 0) {
                if let file {
                    CopyButton(
                        help: "\(Self.copyTitle(item)) (⌘C)", variant: .overlay, size: .large,
                        symbolSize: ControlSize.large.smallSymbol, round: true, copies: copies, margin: .sharing(Self.buttonGap, at: 0, of: count)
                    ) { MediaFiles.copy(file, video: item.video, named: item.name) }
                    roundButton(.download, help: "Save As (⌘S)", margin: .sharing(Self.buttonGap, at: 1, of: count)) {
                        MediaFiles.save(file, named: item.name)
                    }
                }
                roundButton(.x, help: "Close (Esc)", margin: .sharing(Self.buttonGap, at: count - 1, of: count)) { store.closeViewer() }
            }
            .frame(height: row)
            .padding(.trailing, inset)
            .offset(y: -bar)
        }
        .task(id: item) { load(item) }
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
        }
    }

    @ViewBuilder private func content(_ item: ViewedMedia) -> some View {
        switch loaded {
        case .image(let image, let scale):
            ZoomableImage(
                image: image,
                size: CGSize(width: CGFloat(image.width) / scale, height: CGFloat(image.height) / scale),
                margin: ZoomingScrollView.viewerMargin,
                radius: Radius.md,
                largestFit: ZoomingScrollView.viewerLargestFit,
                menu: menu,
                clickedBeside: { store.closeViewer() }
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
            .font(.ui(size: 13))
            .foregroundStyle(Color.themeMutedForegroundOverPicture)
        }
    }

    /// The room between the round buttons' lights, which their presses share.
    private static let buttonGap: CGFloat = 8

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

    func makeNSView(context: Context) -> ZoomingScrollView {
        ZoomingScrollView(margin: margin, radius: radius, largestFit: largestFit)
    }

    func updateNSView(_ view: ZoomingScrollView, context: Context) {
        view.putAway(context.environment.putAway)
        view.keys = keys
        view.menuActions = menu
        view.clickedBeside = clickedBeside
        view.show(image, size: size)
    }
}

/// An image that fits the window until it is zoomed: by pinching, a double click, or ⌘+, ⌘−,
/// ⌘0 for its real size and ⌘9 to fit again. Zoomed, it is moved by scrolling or dragging.
final class ZoomingScrollView: NSScrollView {
    static let viewerMargin = CGSize(width: 64, height: 52)
    /// A small image is enlarged to fit the viewer, up to twice its size.
    static let viewerLargestFit: CGFloat = 2

    var keys = true
    var menuActions: () -> [MenuAction] = { [] }
    var clickedBeside: () -> Void = {}
    private let margin: CGSize
    private let radius: CGFloat
    private let largestFit: CGFloat
    private let picture = FlippedView()
    private var dragged = false

    private var fit: CGFloat {
        let size = picture.frame.size
        guard size.width > 0, size.height > 0 else { return 1 }
        let width = (bounds.width - 2 * margin.width) / size.width
        let height = (bounds.height - 2 * margin.height) / size.height
        return max(0.01, min(largestFit, width, height))
    }

    /// An image enlarged to fit can still be shown at its real size.
    private var smallest: CGFloat { min(fit, 1) }

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

    override func mouseDown(with event: NSEvent) {
        dragged = false
        guard event.clickCount == 2, onPicture(event) else { return }
        toggleZoom(at: contentView.convert(event.locationInWindow, from: nil))
    }

    override func mouseDragged(with event: NSEvent) {
        dragged = true
        var origin = contentView.bounds.origin
        origin.x -= event.deltaX / magnification
        origin.y -= event.deltaY / magnification
        contentView.setBoundsOrigin(contentView.constrainBoundsRect(NSRect(origin: origin, size: contentView.bounds.size)).origin)
    }

    override func mouseUp(with event: NSEvent) {
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
