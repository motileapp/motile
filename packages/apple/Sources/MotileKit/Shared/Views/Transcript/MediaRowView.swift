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

extension MediaFiles {
    /// What a right click or a long press on an image or a video offers. `fetch` hands over its file.
    static func actions(video: Bool, name: String, from view: FlippedView, fetch: @escaping (@escaping (URL) -> Void) -> Void) -> [MenuAction] {
        [
            MenuAction(title: video ? "Copy Video" : "Copy Image", symbol: .copy) {
                fetch { file in copy(file, video: video, named: name) }
            },
            MenuAction(title: saveTitle, symbol: .download) { [weak view] in
                fetch { file in
                    guard let view else { return }
                    save(file, named: name, from: view)
                }
            },
        ]
    }

    static func copy(_ file: URL, video: Bool, named name: String) {
        guard video else { return copyImage(at: file) }
        DispatchQueue.global(qos: .userInitiated).async {
            let named = namedCopy(of: file, name: name)
            DispatchQueue.main.async { copyFile(named) }
        }
    }

    /// A copy of the file under its name, in a temporary folder, for what takes it to know it by it.
    static func namedCopy(of file: URL, name: String) -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("motile-shared", isDirectory: true)
        let copy = folder.appendingPathComponent(name.isEmpty ? file.lastPathComponent : name)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: copy)
        return (try? FileManager.default.copyItem(at: file, to: copy)) == nil ? file : copy
    }
}

final class MediaRowView: RowView {
    private static let gap: CGFloat = 6

    private let picture = PictureView()
    private let playSymbol = SymbolView(.circlePlay, size: 40, tint: Theme.mutedForeground)
    private let caption = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
    private var content: MediaContent?
    private var file: URL?

    static func height(_ content: MediaContent, width: CGFloat) -> CGFloat {
        content.box(width: width).height + gap * 2
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(picture)
        addSubview(playSymbol)
        caption.breaks = .byTruncatingMiddle
        addSubview(caption)
        onPress = { [weak self] _ in self?.pressed() }
        menuActions = { [weak self] in
            guard let self, let media = self.content else { return [] }
            return MediaFiles.actions(video: media.video, name: media.name, from: self) { [weak self] done in
                self?.fetch { file in
                    guard let file else { return }
                    done(file)
                }
            }
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .media(let media) = row.kind, media.id != content?.id else { return }
        content = media
        file = nil
        tip = media.alt.isEmpty ? nil : media.alt
        describe(media.alt.isEmpty ? media.name : media.alt)
        playSymbol.isHidden = !media.video
        picture.picture = media.video ? nil : Pictures.cached(media.id)
        caption.string = media.video ? "\(media.name) · \(ByteCountFormatter.string(fromByteCount: media.bytes, countStyle: .file))" : ""
        guard !media.video, picture.picture == nil else { return }
        loadPicture(media)
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
        playSymbol.frame = CGRect(x: frame.midX - 24, y: frame.midY - 24, width: 48, height: 48)
        caption.frame = CGRect(x: 12, y: frame.maxY - 10 - scaled(16), width: max(0, frame.width - 24), height: scaled(16))
        return box.height + Self.gap * 2
    }

    override func takesPress(at point: CGPoint) -> Bool {
        picture.frame.contains(point)
    }

    /// Opens the image or the video in the viewer, which plays a video and fills the screen with it.
    private func pressed() {
        guard let media = content else { return }
        guard media.video || picture.picture != nil else { return loadPicture(media) }
        owner?.view([ViewedMedia(name: media.name, video: media.video, source: .media(media.id))], at: 0)
    }
}
