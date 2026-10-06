import AVKit
import ImageIO

#if os(macOS)
import AppKit
#else
import UIKit
#endif
import UniformTypeIdentifiers

/// An image or a video a reply shows. The core says how large it is, so the row has its place
/// before the file is on this device.
struct MediaContent {
    static let maxHeight: CGFloat = 480
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    let id: String
    let video: Bool
    /// Unknown for a video.
    let pixels: CGSize?
    let bytes: Int64
    let alt: String
    let name: String

    init(json: JSON) {
        id = json.string("media")
        video = json.bool("video")
        let (width, height) = (json.double("width"), json.double("height"))
        pixels = width > 0 && height > 0 ? CGSize(width: width, height: height) : nil
        bytes = (json["size"] as? NSNumber)?.int64Value ?? 0
        alt = json.string("alt")
        name = json.string("name")
    }

    /// Whether the system's own player plays it. Other videos open in the app the Mac has for them.
    var playsHere: Bool { Self.playedHere.contains((id as NSString).pathExtension.lowercased()) }

    /// The box it is shown in, in a column `width` wide: its own size, or smaller to fit.
    func box(width: CGFloat) -> CGSize {
        guard let pixels else {
            let boxWidth = min(width, 640)
            return CGSize(width: boxWidth, height: (boxWidth * 9 / 16).rounded())
        }
        let scale = min(1, width / pixels.width, Self.maxHeight / pixels.height)
        return CGSize(width: max(1, (pixels.width * scale).rounded()), height: max(1, (pixels.height * scale).rounded()))
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

    /// Calls `done` on the main thread with the first frame of a video, no larger than `maxPixels`
    /// on a side.
    static func firstFrame(_ video: URL, id: String, maxPixels: CGFloat, done: @escaping (CGImage?) -> Void) {
        let frames = AVAssetImageGenerator(asset: AVURLAsset(url: video))
        frames.appliesPreferredTrackTransform = true
        frames.maximumSize = CGSize(width: maxPixels, height: maxPixels)
        frames.generateCGImageAsynchronously(for: .zero) { image, _, _ in
            if let image {
                cache.setObject(image, forKey: id as NSString, cost: image.bytesPerRow * image.height)
            }
            DispatchQueue.main.async { done(image) }
        }
    }

    /// Writes the image as a JPEG to a file that goes when the Mac clears its temporary files.
    static func writeJPEG(_ image: CGImage) -> URL? {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("motile-poster-\(UUID().uuidString).jpg")
        guard let destination = CGImageDestinationCreateWithURL(file as CFURL, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        return CGImageDestinationFinalize(destination) ? file : nil
    }
}

final class MediaRowView: RowView {
    private static let gap: CGFloat = 6

    private let picture = PictureView()
    private let playSymbol = SymbolView(.circlePlay, size: 40, tint: Theme.secondary)
    private let spinner = SpinnerView(size: 24)
    private let caption = TextLabel(font: Theme.smallFont, color: Theme.secondary)
    private var content: MediaContent?
    private var file: URL?
    private var downloading = false
    private var progress: NSObjectProtocol?
    #if os(macOS)
    private var player: AVPlayerView?
    private var playerReady: NSKeyValueObservation?
    #endif

    static func height(_ content: MediaContent, width: CGFloat) -> CGFloat {
        content.box(width: width).height + gap * 2
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(picture)
        addSubview(playSymbol)
        spinner.isHidden = true
        addSubview(spinner)
        caption.breaks = .byTruncatingMiddle
        addSubview(caption)
        onPress = { [weak self] _ in self?.pressed() }
        menuActions = { [weak self] in self?.actions ?? [] }
        progress = NotificationCenter.default.addObserver(forName: .mediaProgress, object: nil, queue: .main) { [weak self] note in
            guard let self, self.downloading, let content = self.content,
                note.userInfo?["id"] as? String == content.id,
                let fraction = note.userInfo?["fraction"] as? Double
            else { return }
            self.caption.string = "Downloading \(content.name) · \(Int(fraction * 100))%"
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
        tip = media.alt.isEmpty ? nil : media.alt
        describe(media.alt.isEmpty ? media.name : media.alt)
        playSymbol.isHidden = !media.video
        spinner.isHidden = true
        picture.picture = media.video ? nil : Pictures.cached(media.id)
        caption.string = media.video ? Self.described(media) : ""
        guard !media.video, picture.picture == nil else { return }
        loadPicture(media)
    }

    private static func described(_ media: MediaContent) -> String {
        "\(media.name) · \(ByteCountFormatter.string(fromByteCount: media.bytes, countStyle: .file))"
    }

    private func loadPicture(_ media: MediaContent) {
        let box = media.box(width: Theme.contentWidth)
        let maxPixels = max(box.width, box.height) * Platform.pixelsPerPoint
        fetch { [weak self] file in
            guard let file else {
                self?.caption.string = "This image couldn't be loaded. \(Self.pressWord) to try again."
                return
            }
            Pictures.decode(file, id: media.id, maxPixels: maxPixels) { image in
                guard let self, self.content?.id == media.id else { return }
                self.picture.picture = image
                self.caption.string = image == nil ? "This image couldn't be shown" : ""
            }
        }
    }

    private static let pressWord = Platform.scale > 1 ? "Tap" : "Click"

    /// Hands over the file, which the core fetches from the server if this device doesn't have it.
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
        let frame = CGRect(x: 0, y: Self.gap, width: box.width, height: box.height)
        picture.frame = frame
        #if os(macOS)
        player?.frame = frame
        #endif
        playSymbol.frame = CGRect(x: frame.midX - 24, y: frame.midY - 24, width: 48, height: 48)
        spinner.frame = playSymbol.frame
        caption.frame = CGRect(x: 12, y: frame.maxY - 10 - scaled(16), width: max(0, frame.width - 24), height: scaled(16))
        return box.height + Self.gap * 2
    }

    // MARK: Opening and playing

    /// The box takes the clicks, until a player with controls of its own is in it.
    override func takesPress(at point: CGPoint) -> Bool {
        #if os(macOS)
        guard player == nil else { return false }
        #endif
        return picture.frame.contains(point)
    }

    private func pressed() {
        guard let media = content else { return }
        guard media.video else {
            guard picture.picture != nil else { return loadPicture(media) }
            owner?.view([ViewedMedia(name: media.name, video: false, source: .media(media.id))], at: 0)
            return
        }
        #if os(macOS)
        guard !downloading else { return }
        downloading = true
        wait(true)
        caption.string = "Downloading \(media.name)"
        fetch { [weak self] file in
            guard let self else { return }
            self.downloading = false
            guard let file else {
                self.wait(false)
                self.caption.string = "This video couldn't be loaded. Click to try again."
                return
            }
            self.caption.string = Self.described(media)
            guard media.playsHere else {
                self.wait(false)
                Platform.open(file)
                return
            }
            self.play(file)
        }
        #else
        // A phone plays a video over the whole screen.
        owner?.view([ViewedMedia(name: media.name, video: true, source: .media(media.id))], at: 0)
        #endif
    }

    /// While the video downloads and until its first frame is drawn, the spinner stands where the
    /// play symbol was.
    private func wait(_ waiting: Bool) {
        spinner.isHidden = !waiting
        playSymbol.isHidden = waiting || !(content?.video ?? false)
    }

    #if os(macOS)
    private func play(_ file: URL) {
        let view = AVPlayerView(frame: picture.frame)
        view.controlsStyle = .inline
        view.showsFullScreenToggleButton = true
        view.videoGravity = .resizeAspect
        view.wantsLayer = true
        view.layer?.cornerRadius = PictureView.radius
        view.layer?.masksToBounds = true
        view.player = AVPlayer(url: file)
        addSubview(view, positioned: .below, relativeTo: spinner)
        player = view
        playerReady = view.observe(\.isReadyForDisplay, options: [.initial, .new]) { [weak self] view, _ in
            guard view.isReadyForDisplay else { return }
            DispatchQueue.main.async {
                guard let self, self.player === view else { return }
                self.spinner.isHidden = true
            }
        }
        caption.isHidden = true
        view.player?.play()
    }

    private func removePlayer() {
        playerReady = nil
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
    #else
    private func removePlayer() {}
    #endif

    // MARK: Menu

    private var actions: [MenuAction] {
        guard let media = content else { return [] }
        var actions: [MenuAction] = []
        if !media.video {
            actions.append(MenuAction(title: "Copy Image", symbol: .copy) { [weak self] in self?.copyImage() })
        }
        actions.append(MenuAction(title: MediaFiles.saveTitle, symbol: .download) { [weak self] in self?.save() })
        return actions
    }

    private func copyImage() {
        fetch { file in
            guard let file else { return }
            MediaFiles.copyImage(at: file)
        }
    }

    private func save() {
        guard let media = content else { return }
        fetch { [weak self] file in
            guard let self, let file else { return }
            MediaFiles.save(file, named: media.name, from: self)
        }
    }
}
