import AVKit
import SwiftUI

/// The images and videos of a message or of the composer, one at a time over the whole window.
/// ← and → go through them, and Esc or a click beside the picture closes it. An image zooms
/// as it does in Preview.
struct MediaViewer: View {
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    @Environment(AppStore.self) private var store
    let viewing: Viewing
    @State private var loaded: Loaded?
    @State private var failed = false
    @State private var fraction: Double?
    @State private var keys: Any?

    private enum Loaded {
        case image(CGImage, scale: CGFloat)
        case video(AVPlayer)
    }

    var body: some View {
        let item = viewing.item
        ZStack {
            Color.black.opacity(0.86)
                .ignoresSafeArea()
                .onTapGesture { store.closeViewer() }
            content(item)
            if viewing.items.count > 1 {
                HStack {
                    arrow("chevron.left", help: "Previous (←)") { store.viewNext(-1) }
                    Spacer()
                    arrow("chevron.right", help: "Next (→)") { store.viewNext(1) }
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
                        .foregroundStyle(.white.opacity(0.6))
                }
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .frame(height: 24)
            .background(.black.opacity(0.5), in: Capsule())
            .padding(.top, 14)
            .padding(.horizontal, 80)
        }
        .overlay(alignment: .topTrailing) {
            arrow("xmark", help: "Close (Esc)") { store.closeViewer() }
                .padding(10)
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
                radius: 8
            ) { store.closeViewer() }
            .ignoresSafeArea()
        case .video(let player):
            VideoPlayer(player: player)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .padding(.horizontal, ZoomingScrollView.viewerMargin.width)
                .padding(.vertical, ZoomingScrollView.viewerMargin.height)
        case nil:
            Group {
                if failed {
                    Text(item.video ? "This video couldn't be loaded." : "This image couldn't be loaded.")
                } else if let fraction {
                    Text("Downloading \(item.name) · \(Int(fraction * 100))%").monospacedDigit()
                } else {
                    ProgressView().controlSize(.small).colorScheme(.dark)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(.white.opacity(0.7))
        }
    }

    private func arrow(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 32, height: 32)
                .background(.white.opacity(0.14), in: Circle())
                .background(.black.opacity(0.5), in: Circle())
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(help)
    }

    private func handle(_ event: NSEvent) -> Bool {
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
        (loaded, failed, fraction) = (nil, false, nil)
        let show = { (file: URL?) in
            guard viewing.item == item else { return }
            guard let file else { return failed = true }
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

/// A `CGImage` or an `NSImage`, at `size` in points when it isn't zoomed.
struct ZoomableImage: NSViewRepresentable {
    let image: AnyObject
    let size: CGSize
    let margin: CGSize
    var radius: CGFloat = 0
    /// Whether ⌘+, ⌘−, ⌘0 and ⌘9 are this image's.
    var keys = true
    var clickedBeside: () -> Void = {}

    func makeNSView(context: Context) -> ZoomingScrollView {
        ZoomingScrollView(margin: margin, radius: radius)
    }

    func updateNSView(_ view: ZoomingScrollView, context: Context) {
        view.keys = keys
        view.clickedBeside = clickedBeside
        view.show(image, size: size)
    }
}

/// An image that fits the window until it is zoomed: by pinching, a double click, or ⌘+, ⌘−,
/// ⌘0 for its real size and ⌘9 to fit again. Zoomed, it is moved by scrolling or dragging.
final class ZoomingScrollView: NSScrollView {
    static let viewerMargin = CGSize(width: 64, height: 52)

    var keys = true
    var clickedBeside: () -> Void = {}
    private let margin: CGSize
    private let radius: CGFloat
    private let picture = FlippedView()
    private var dragged = false

    private var fit: CGFloat {
        let size = picture.frame.size
        guard size.width > 0, size.height > 0 else { return 1 }
        let width = (bounds.width - 2 * margin.width) / size.width
        let height = (bounds.height - 2 * margin.height) / size.height
        return max(0.01, min(1, width, height))
    }

    init(margin: CGSize, radius: CGFloat) {
        self.margin = margin
        self.radius = radius
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
        minMagnification = fit
        magnification = fit
    }

    override func setFrameSize(_ size: NSSize) {
        let fitted = magnification <= minMagnification + 0.001
        super.setFrameSize(size)
        minMagnification = fit
        guard fitted || magnification < fit else { return }
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
        let zoomed = magnification > fit + 0.001
        animator().setMagnification(zoomed ? fit : max(1, fit * 2), centeredAt: point)
    }

    private func zoom(to magnification: CGFloat) {
        let center = NSPoint(x: contentView.bounds.midX, y: contentView.bounds.midY)
        animator().setMagnification(min(max(magnification, fit), maxMagnification), centeredAt: center)
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
