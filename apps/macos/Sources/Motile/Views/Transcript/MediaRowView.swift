import AVKit
import AppKit
import ImageIO

/// An image or a video a reply shows. The core says how large it is, so the row has its place
/// before the file is on this Mac.
struct MediaContent {
    static let maxHeight: CGFloat = 480
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    let id: String
    let video: Bool
    /// Unknown for a video.
    let pixels: NSSize?
    let bytes: Int64
    let alt: String
    let name: String

    init(json: JSON) {
        id = json.string("media")
        video = json.bool("video")
        let (width, height) = (json.double("width"), json.double("height"))
        pixels = width > 0 && height > 0 ? NSSize(width: width, height: height) : nil
        bytes = (json["size"] as? NSNumber)?.int64Value ?? 0
        alt = json.string("alt")
        name = json.string("name")
    }

    /// Whether the Mac's own player plays it. Other videos open in the app the Mac has for them.
    var playsHere: Bool { Self.playedHere.contains((id as NSString).pathExtension.lowercased()) }

    /// The box it is shown in, in a column `width` wide: its own size, or smaller to fit.
    func box(width: CGFloat) -> NSSize {
        guard let pixels else {
            let boxWidth = min(width, 640)
            return NSSize(width: boxWidth, height: (boxWidth * 9 / 16).rounded())
        }
        let scale = min(1, width / pixels.width, Self.maxHeight / pixels.height)
        return NSSize(width: max(1, (pixels.width * scale).rounded()), height: max(1, (pixels.height * scale).rounded()))
    }
}

extension Notification.Name {
    /// A download of an image or a video went on. `userInfo` has its `id` and its `fraction`.
    static let mediaProgress = Notification.Name("motile.mediaProgress")
}

/// Decodes images off the main thread, at the size they are shown, and keeps the latest ones.
enum Pictures {
    private static let cache: NSCache<NSString, CGImage> = {
        let cache = NSCache<NSString, CGImage>()
        cache.totalCostLimit = 256 * 1024 * 1024
        return cache
    }()
    private static let decoding = DispatchQueue(label: "app.motile.pictures", qos: .userInitiated, attributes: .concurrent)

    static func cached(_ id: String) -> CGImage? {
        cache.object(forKey: id as NSString)
    }

    /// Calls `done` on the main thread with the image, no larger than `maxPixels` on a side.
    static func decode(_ file: URL, id: String, maxPixels: CGFloat, done: @escaping (CGImage?) -> Void) {
        decoding.async {
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixels,
            ]
            let source = CGImageSourceCreateWithURL(file as CFURL, nil)
            let image = source.flatMap { CGImageSourceCreateThumbnailAtIndex($0, 0, options as CFDictionary) }
            if let image {
                cache.setObject(image, forKey: id as NSString, cost: image.bytesPerRow * image.height)
            }
            DispatchQueue.main.async { done(image) }
        }
    }
}

/// The box an image or a video is shown in: empty until the picture is there.
final class PictureView: NSView {
    static let radius: CGFloat = 10

    var picture: CGImage? { didSet { needsDisplay = true } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.contents = picture
        layer?.contentsGravity = .resizeAspect
        layer?.backgroundColor = Theme.codeBackground.cgColor
        layer?.cornerRadius = Self.radius
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        layer?.borderColor = Theme.border.cgColor
        layer?.borderWidth = 1
    }
}

final class MediaRowView: RowView {
    private static let gap: CGFloat = 6

    private let picture = PictureView()
    private let playSymbol = NSImageView()
    private let caption = NSTextField(labelWithString: "")
    private var player: AVPlayerView?
    private var content: MediaContent?
    private var file: URL?
    private var downloading = false
    private var progress: NSObjectProtocol?

    static func height(_ content: MediaContent, width: CGFloat) -> CGFloat {
        content.box(width: width).height + gap * 2
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(picture)
        let configuration = NSImage.SymbolConfiguration(pointSize: 40, weight: .regular)
        playSymbol.image = NSImage(systemSymbolName: "play.circle.fill", accessibilityDescription: "Play")?
            .withSymbolConfiguration(configuration)
        playSymbol.contentTintColor = Theme.secondary
        addSubview(playSymbol)
        caption.font = Theme.smallFont
        caption.textColor = Theme.secondary
        caption.lineBreakMode = .byTruncatingMiddle
        addSubview(caption)
        progress = NotificationCenter.default.addObserver(forName: .mediaProgress, object: nil, queue: .main) { [weak self] note in
            guard let self, self.downloading, let content = self.content,
                note.userInfo?["id"] as? String == content.id,
                let fraction = note.userInfo?["fraction"] as? Double
            else { return }
            self.caption.stringValue = "Downloading \(content.name) · \(Int(fraction * 100))%"
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        if let progress { NotificationCenter.default.removeObserver(progress) }
    }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .media(let media) = row.kind, media.id != content?.id else { return }
        removePlayer()
        content = media
        file = nil
        downloading = false
        toolTip = media.alt.isEmpty ? nil : media.alt
        setAccessibilityLabel(media.alt.isEmpty ? media.name : media.alt)
        playSymbol.isHidden = !media.video
        picture.picture = media.video ? nil : Pictures.cached(media.id)
        caption.stringValue = media.video ? Self.described(media) : ""
        guard !media.video, picture.picture == nil else { return }
        loadPicture(media)
    }

    private static func described(_ media: MediaContent) -> String {
        "\(media.name) · \(ByteCountFormatter.string(fromByteCount: media.bytes, countStyle: .file))"
    }

    private func loadPicture(_ media: MediaContent) {
        let box = media.box(width: Theme.contentWidth)
        let maxPixels = max(box.width, box.height) * (window?.backingScaleFactor ?? 2)
        fetch { [weak self] file in
            guard let file else {
                self?.caption.stringValue = "This image couldn't be loaded. Click to try again."
                return
            }
            Pictures.decode(file, id: media.id, maxPixels: maxPixels) { image in
                guard let self, self.content?.id == media.id else { return }
                self.picture.picture = image
                self.caption.stringValue = image == nil ? "This image couldn't be shown" : ""
            }
        }
    }

    /// Hands over the file, which the core fetches from the server if this Mac doesn't have it.
    private func fetch(_ done: @escaping (URL?) -> Void) {
        guard let media = content else { return }
        if let file { return done(file) }
        guard let owner else { return done(nil) }
        owner.media(id: media.id) { [weak self] file in
            guard let self, self.content?.id == media.id else { return }
            self.file = file
            done(file)
        }
    }

    override func layout(width: CGFloat) -> CGFloat {
        guard let content else { return 0 }
        let box = content.box(width: width)
        let frame = NSRect(x: 0, y: Self.gap, width: box.width, height: box.height)
        picture.frame = frame
        player?.frame = frame
        playSymbol.frame = NSRect(x: frame.midX - 24, y: frame.midY - 24, width: 48, height: 48)
        caption.frame = NSRect(x: 12, y: frame.maxY - 26, width: max(0, frame.width - 24), height: 16)
        return box.height + Self.gap * 2
    }

    // MARK: Opening and playing

    /// The box takes the clicks, until a player with controls of its own is in it.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard player == nil, picture.frame.contains(convert(point, from: superview)) else { return super.hitTest(point) }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        guard let media = content else { return }
        guard media.video else {
            guard picture.picture != nil else { return loadPicture(media) }
            fetch { file in
                guard let file else { return }
                NSWorkspace.shared.open(file)
            }
            return
        }
        guard !downloading else { return }
        downloading = true
        caption.stringValue = "Downloading \(media.name)…"
        fetch { [weak self] file in
            guard let self else { return }
            self.downloading = false
            guard let file else {
                self.caption.stringValue = "This video couldn't be loaded. Click to try again."
                return
            }
            self.caption.stringValue = Self.described(media)
            guard media.playsHere else {
                NSWorkspace.shared.open(file)
                return
            }
            self.play(file)
        }
    }

    private func play(_ file: URL) {
        let view = AVPlayerView(frame: picture.frame)
        view.controlsStyle = .inline
        view.videoGravity = .resizeAspect
        view.wantsLayer = true
        view.layer?.cornerRadius = PictureView.radius
        view.layer?.masksToBounds = true
        view.player = AVPlayer(url: file)
        addSubview(view)
        player = view
        playSymbol.isHidden = true
        caption.isHidden = true
        view.player?.play()
    }

    private func removePlayer() {
        player?.player?.pause()
        player?.removeFromSuperview()
        player = nil
        caption.isHidden = false
    }

    /// A row that scrolled away stops playing.
    override func viewDidHide() {
        super.viewDidHide()
        player?.player?.pause()
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard superview == nil else { return }
        player?.player?.pause()
    }

    // MARK: Menu

    override func menu(for event: NSEvent) -> NSMenu? {
        guard let media = content else { return nil }
        let menu = NSMenu()
        if !media.video {
            menu.addItem(withTitle: "Copy Image", action: #selector(copyImage), keyEquivalent: "").target = self
        }
        menu.addItem(withTitle: "Save As…", action: #selector(save), keyEquivalent: "").target = self
        return menu
    }

    @objc private func copyImage() {
        fetch { file in
            guard let file else { return }
            DispatchQueue.global(qos: .userInitiated).async {
                guard let tiff = NSImage(contentsOf: file)?.tiffRepresentation else { return }
                DispatchQueue.main.async {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setData(tiff, forType: .tiff)
                }
            }
        }
    }

    @objc private func save() {
        guard let media = content else { return }
        fetch { file in
            guard let file else { return }
            let panel = NSSavePanel()
            panel.nameFieldStringValue = media.name
            panel.begin { response in
                guard response == .OK, let destination = panel.url else { return }
                DispatchQueue.global(qos: .userInitiated).async {
                    try? FileManager.default.removeItem(at: destination)
                    try? FileManager.default.copyItem(at: file, to: destination)
                }
            }
        }
    }
}
