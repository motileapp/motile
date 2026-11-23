import AVKit
import CMotileCore
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
    /// The image that stands for a video until it plays.
    let poster: String?

    init(json: JSON) {
        id = json.string("media")
        video = json.bool("video")
        let (width, height) = (json.double("width"), json.double("height"))
        pixels = width > 0 && height > 0 ? CGSize(width: width, height: height) : nil
        bytes = (json["size"] as? NSNumber)?.int64Value ?? 0
        alt = json.string("alt")
        name = json.string("name")
        poster = json.optionalString("poster")
    }

    /// The size an SVG is shown at, which the viewer opens it at too.
    var svgSize: CGSize? { id.lowercased().hasSuffix(".svg") ? pixels : nil }

    /// The image the box shows: the image itself, or a video's poster.
    var picture: String? { video ? poster : id }

    /// The box it is shown in, in a column `width` wide: the picture's size, and at least
    /// `smallestBox`, so that a small picture lies on a checkerboard that can be made out and pressed.
    func box(width: CGFloat) -> CGSize {
        guard let picture = drawn(width: width) else {
            let boxWidth = min(width, 640)
            return CGSize(width: boxWidth, height: (boxWidth * 9 / 16).rounded())
        }
        return CGSize(width: min(width, max(picture.width, Self.smallestBox)), height: max(picture.height, Self.smallestBox))
    }

    static let smallestBox: CGFloat = 96

    /// How large the picture is drawn in a column `width` wide: its own size, or smaller to fit.
    func drawn(width: CGFloat) -> CGSize? {
        guard let pixels else { return nil }
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
    /// What is learned of each picture as it is decoded, by its key: kept, unlike the pictures,
    /// since a picture shown again from its row must lie on the same checkerboard.
    private static var facts: [String: (ink: Double?, svgSize: CGSize?)] = [:]
    private static let factsLock = NSLock()

    private static func fact(_ id: String) -> (ink: Double?, svgSize: CGSize?)? {
        factsLock.withLock { facts[id] ?? facts[svgKey(id, dark: Platform.isDark) as String] }
    }

    private static func learn(_ key: NSString, ink: Double? = nil, svgSize: CGSize? = nil) {
        factsLock.withLock {
            let known = facts[key as String]
            facts[key as String] = (ink ?? known?.ink, svgSize ?? known?.svgSize)
        }
    }
    private static let decoding = DispatchQueue(label: "app.motile.pictures", qos: .userInitiated, attributes: .concurrent)

    /// An SVG is kept for the appearance it was drawn in, since its `currentColor` is the text's.
    private static func svgKey(_ id: String, dark: Bool) -> NSString {
        "\(id)#\(dark ? "dark" : "light")" as NSString
    }

    static func cached(_ id: String) -> CGImage? {
        cache.object(forKey: id as NSString) ?? cache.object(forKey: svgKey(id, dark: Platform.isDark))
    }

    /// The checkerboard that lies behind the picture: the dark one under what draws in light colours,
    /// the light one under anything else.
    static func checker(behind id: String) -> Checker {
        guard let ink = ink(id) else { return .light }
        return contrast(ink, Checker.dark.luminance) > contrast(ink, Checker.light.luminance) ? .dark : .light
    }

    /// Whether the picture has parts that are seen through.
    static func isSeenThrough(_ id: String) -> Bool {
        ink(id) != nil
    }

    /// The size an SVG asks to be drawn at, which is its actual size in the viewer.
    static func svgSize(_ id: String) -> CGSize? {
        fact(id)?.svgSize
    }

    private static func ink(_ id: String) -> Double? {
        fact(id)?.ink
    }

    /// What a picture's see-through parts show, as image editors do.
    enum Checker {
        case light, dark

        static let square: CGFloat = 8

        private var colors: (PlatformColor, PlatformColor) {
            switch self {
            case .light: (Theme.checkerLight, Theme.checkerLightAlternate)
            case .dark: (Theme.checkerDark, Theme.checkerDarkAlternate)
            }
        }

        /// The line around a picture that lies on it, which shows where the picture ends.
        var frame: PlatformColor {
            switch self {
            case .light: Theme.checkerLightFrame
            case .dark: Theme.checkerDarkFrame
            }
        }

        /// How light it is on average.
        var luminance: Double {
            let (one, other) = colors
            return (one.luminance + other.luminance) / 2
        }

        private static var patterns: [String: PlatformColor] = [:]

        /// The checkerboard as a colour to fill with.
        func pattern() -> PlatformColor {
            let scale = Platform.pixelsPerPoint
            let side = max(1, Int((Checker.square * scale).rounded()))
            let key = "\(self)\(side)"
            if let made = Self.patterns[key] { return made }
            let made = draw(side: side, scale: scale)
            Self.patterns[key] = made
            return made
        }

        private func draw(side: Int, scale: CGFloat) -> PlatformColor {
            let (one, other) = colors
            guard let canvas = CGContext(
                data: nil, width: side * 2, height: side * 2, bitsPerComponent: 8, bytesPerRow: 0,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return one }
            canvas.setFillColor(Theme.resolved(one, dark: false).cgColor)
            canvas.fill(CGRect(x: 0, y: 0, width: side * 2, height: side * 2))
            canvas.setFillColor(Theme.resolved(other, dark: false).cgColor)
            canvas.fill([CGRect(x: side, y: 0, width: side, height: side), CGRect(x: 0, y: side, width: side, height: side)])
            guard let image = canvas.makeImage() else { return one }
            #if os(macOS)
            let points = CGFloat(side * 2) / scale
            return NSColor(patternImage: NSImage(cgImage: image, size: CGSize(width: points, height: points)))
            #else
            return UIColor(patternImage: UIImage(cgImage: image, scale: scale, orientation: .up))
            #endif
        }
    }

    /// Calls `done` on the main thread with the image, no larger than `maxPixels` on a side. An SVG
    /// is drawn that large, up to `svgMaxPixels`, with `currentColor` in the text's colour.
    static func decode(_ file: URL, id: String, maxPixels: CGFloat, done: @escaping (CGImage?) -> Void) {
        let svg = file.pathExtension.lowercased() == "svg"
        let dark = Platform.isDark
        let key = svg ? svgKey(id, dark: dark) : id as NSString
        let color = Theme.resolved(Theme.foreground, dark: dark).hex
        let finish = { (image: CGImage?) in
            decoding.async {
                if let image {
                    cache.setObject(image, forKey: key, cost: image.bytesPerRow * image.height)
                    if let ink = inkLuminance(image) { learn(key, ink: ink) }
                }
                DispatchQueue.main.async { done(image) }
            }
        }
        decoding.async {
            guard !svg else {
                guard let data = try? Data(contentsOf: file) else { return finish(nil) }
                if let size = svgSize(of: data) { learn(key, svgSize: size) }
                return finish(drawSVG(inked(data, color: color), side: min(maxPixels, svgMaxPixels)))
            }
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixels,
            ]
            let source = CGImageSourceCreateWithURL(file as CFURL, nil)
            finish(source.flatMap { CGImageSourceCreateThumbnailAtIndex($0, 0, options as CFDictionary) })
        }
    }

    /// How large an SVG is drawn at most, on its longer side.
    private static let svgMaxPixels: CGFloat = 4096

    /// The SVG with its `currentColor` in `color`, as a web page gives it the text's colour, unless
    /// its root says a colour of its own.
    static func inked(_ data: Data, color: String) -> Data {
        guard let text = String(data: data, encoding: .utf8), let root = text.range(of: "<svg") else { return data }
        let tag = text[root.upperBound...].prefix { $0 != ">" }
        guard tag.range(of: #"\scolor\s*="#, options: .regularExpression) == nil else { return data }
        return Data((text[..<root.upperBound] + " color=\"\(color)\"" + text[root.upperBound...]).utf8)
    }

    private static func svgSize(of data: Data) -> CGSize? {
        #if os(macOS)
        guard let size = NSImage(data: data)?.size, size.width > 0, size.height > 0 else { return nil }
        return size
        #else
        return SVGSize.size(of: data)
        #endif
    }

    /// The SVG drawn `side` pixels on its longer side, by the core: on the thread it is asked on.
    static func drawSVG(_ data: Data, side: CGFloat) -> CGImage? {
        var (width, height): (UInt32, UInt32) = (0, 0)
        let drawn = data.withUnsafeBytes { bytes -> UnsafeMutablePointer<UInt8>? in
            guard let base = bytes.bindMemory(to: UInt8.self).baseAddress else { return nil }
            return motile_svg_draw(base, UInt(bytes.count), Float(side), &width, &height)
        }
        guard let drawn else { return nil }
        let length = Int(width) * Int(height) * 4
        let release: CGDataProviderReleaseDataCallback = { _, pixels, size in
            motile_free_pixels(UnsafeMutablePointer(mutating: pixels.assumingMemoryBound(to: UInt8.self)), UInt(size))
        }
        guard let provider = CGDataProvider(dataInfo: nil, data: drawn, size: length, releaseData: release) else {
            motile_free_pixels(drawn, UInt(length))
            return nil
        }
        return CGImage(
            width: Int(width), height: Int(height), bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: Int(width) * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    /// How light what the picture draws is on average, weighed by how opaque it is: nil when
    /// nothing in it is seen through.
    private static func inkLuminance(_ image: CGImage) -> Double? {
        switch image.alphaInfo {
        case .none, .noneSkipFirst, .noneSkipLast: return nil
        default: break
        }
        let side = 32
        guard let canvas = CGContext(
            data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ), let data = canvas.data else { return nil }
        canvas.interpolationQuality = .medium
        canvas.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
        let pixels = data.bindMemory(to: UInt8.self, capacity: side * side * 4)
        var (sum, weight, seenThrough) = (0.0, 0.0, false)
        for index in stride(from: 0, to: side * side * 4, by: 4) {
            let alpha = Double(pixels[index + 3]) / 255
            seenThrough = seenThrough || alpha < 0.98
            guard alpha > 0.02 else { continue }
            let channel = { (offset: Int) in Double(pixels[index + offset]) / 255 / alpha }
            sum += relativeLuminance(channel(0), channel(1), channel(2)) * alpha
            weight += alpha
        }
        return seenThrough && weight > 0 ? sum / weight : nil
    }

    static func relativeLuminance(_ red: Double, _ green: Double, _ blue: Double) -> Double {
        let linear = { (value: Double) in value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4) }
        return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }

    /// WCAG's contrast ratio between two relative luminances.
    static func contrast(_ one: Double, _ other: Double) -> Double {
        (max(one, other) + 0.05) / (min(one, other) + 0.05)
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
        guard video else { return isSVG(file, named: name) ? copySVG(file, named: name) : copyImage(at: file) }
        DispatchQueue.global(qos: .userInitiated).async {
            let named = namedCopy(of: file, name: name)
            DispatchQueue.main.async { copyFile(named) }
        }
    }

    static func isSVG(_ file: URL, named name: String) -> Bool {
        file.pathExtension.lowercased() == "svg" || name.lowercased().hasSuffix(".svg")
    }

    /// How large the picture an SVG is copied with is drawn, for what only takes pictures.
    private static let copiedSVGPixels: CGFloat = 1024

    /// Hands over an SVG's file under its name, its text, and a picture of it, as `copySVG` puts
    /// them on the pasteboard: the file for where files go, the SVG for what reads it, the picture
    /// for the rest.
    static func svgParts(_ file: URL, named name: String, done: @escaping (_ named: URL, _ svg: Data, _ png: Data?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            guard let svg = try? Data(contentsOf: file) else { return }
            let named = namedCopy(of: file, name: name)
            let png = Pictures.drawSVG(svg, side: copiedSVGPixels).flatMap(pngData)
            DispatchQueue.main.async { done(named, svg, png) }
        }
    }

    private static func pngData(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data as CFMutableData, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
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

    private let box = MediaBox()
    private var content: MediaContent?

    static func height(_ content: MediaContent, width: CGFloat) -> CGFloat {
        content.box(width: width).height + gap * 2
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(box)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .media(let media) = row.kind else { return }
        content = media
        box.show(media, owner: owner)
    }

    override func layout(width: CGFloat) -> CGFloat {
        guard let content else { return 0 }
        let size = content.box(width: width)
        box.frame = CGRect(x: 0, y: Self.gap, width: size.width, height: size.height)
        return size.height + Self.gap * 2
    }
}

/// The picture of an image or a video, as large as its frame: a reply's, or one a tool call
/// showed its agent. It loads the file, says when it can't, and opens in the viewer.
final class MediaBox: FlippedView {
    private static let stackGap: CGFloat = 10
    private static let lineHeight = scaled(16)

    private weak var owner: RowOwner?
    private let picture = PictureView()
    private var playButton: OverlayButton!
    private let caption = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
    private let spinner = SpinnerView(size: ControlSize.large.symbol)
    private let progress = TextLabel(font: .uiDigits(.xs), color: Theme.mutedForeground)
    private let message = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
    private var retryButton: RowButton!
    private var content: MediaContent?
    private var file: URL?
    private var progressWatch: NSObjectProtocol?

    private enum State {
        case loading(fraction: Double?)
        case shown
        case failed(String)
    }

    private var state = State.shown { didSet { show(state) } }

    /// Its corners: a reply's are `PictureView.radius`, one inside a tool call's box less.
    var radius: CGFloat {
        get { picture.radius }
        set { picture.radius = newValue }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(picture)
        playButton = OverlayButton(.play, size: .large, tooltip: "Play") { [weak self] in self?.pressed() }
        addSubview(playButton)
        caption.breaks = .byTruncatingMiddle
        addSubview(caption)
        addSubview(spinner)
        progress.centered = true
        addSubview(progress)
        message.centered = true
        addSubview(message)
        retryButton = RowButton(
            title: "Try again",
            tooltip: "Load the image again",
            radius: RowButton.metrics.radius,
            bordered: true,
            hover: Theme.backgroundSecondaryAccent,
            insets: PlatformEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)
        ) { [weak self] in self?.retry() }
        addSubview(retryButton)
        show(state)
        onPress = { [weak self] _ in self?.pressed() }
        menuActions = { [weak self] in
            guard let self, let media = self.content else { return [] }
            return MediaFiles.actions(video: media.video, name: media.name, from: self) { [weak self] done in
                self?.fetch { fetched in
                    guard case .success(let file) = fetched else { return }
                    done(file)
                }
            }
        }
        #if os(iOS)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (box: Self, _: UITraitCollection) in box.appearanceChanged() }
        #endif
        progressWatch = NotificationCenter.default.addObserver(forName: .mediaProgress, object: nil, queue: .main) { [weak self] note in
            guard let self, let id = self.content?.id, note.userInfo?["id"] as? String == id else { return }
            guard case .loading = self.state, let fraction = note.userInfo?["fraction"] as? Double else { return }
            self.state = .loading(fraction: fraction)
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        if let progressWatch { NotificationCenter.default.removeObserver(progressWatch) }
    }

    func show(_ media: MediaContent, owner: RowOwner?) {
        self.owner = owner
        guard media.id != content?.id else { return }
        content = media
        file = nil
        tip = media.alt.isEmpty ? nil : media.alt
        describe(media.alt.isEmpty ? media.name : media.alt)
        playButton.isHidden = !media.video
        picture.picture = media.picture.flatMap(Pictures.cached)
        picture.tile = Pictures.checker(behind: media.picture ?? media.id).pattern()
        relayout()
        let unpictured = media.video && media.poster == nil
        caption.string = unpictured ? "\(media.name) · \(ByteCountFormatter.string(fromByteCount: media.bytes, countStyle: .file))" : ""
        state = .shown
        guard media.picture != nil, picture.picture == nil else { return }
        loadPicture(media)
    }

    #if os(macOS)
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        appearanceChanged()
    }
    #endif

    /// Draws an SVG again in the other appearance, which its `currentColor` follows.
    private func appearanceChanged() {
        guard let media = content, !media.video, media.id.lowercased().hasSuffix(".svg") else { return }
        picture.picture = Pictures.cached(media.id)
        picture.tile = Pictures.checker(behind: media.id).pattern()
        guard picture.picture == nil else { return }
        loadPicture(media)
    }

    /// Shows the image, or the video's poster. A video whose poster can't be had stays a box to play.
    private func loadPicture(_ media: MediaContent) {
        guard let pictureID = media.picture else { return }
        let box = media.box(width: Theme.contentWidth)
        let maxPixels = max(box.width, box.height) * Platform.pixelsPerPoint
        state = .loading(fraction: nil)
        let decode = { [weak self] (fetched: Result<URL, Error>) in
            guard let self, self.content?.id == media.id else { return }
            guard case .success(let file) = fetched else { return self.state = media.video ? .shown : self.failure(fetched) }
            Pictures.decode(file, id: pictureID, maxPixels: maxPixels) { image in
                guard self.content?.id == media.id else { return }
                self.picture.picture = image
                self.picture.tile = Pictures.checker(behind: pictureID).pattern()
                self.state = image == nil && !media.video ? .failed("This image couldn't be shown.") : .shown
            }
        }
        guard media.video else { return fetch(decode) }
        guard let owner else { return decode(.failure(CoreBridge.CoreError(message: "Something went wrong."))) }
        owner.fetchFile(id: pictureID, done: decode)
    }

    /// Why the image isn't shown: what the core said when it can't be asked for again, as from a
    /// removed server, or else that it can be tried again.
    private func failure(_ fetched: Result<URL, Error>) -> State {
        guard case .failure(let error) = fetched, owner?.fetchesAgain == false else { return .failed("This image couldn't be loaded.") }
        return .failed(error.localizedDescription)
    }

    private func retry() {
        guard let media = content else { return }
        file = nil
        retryButton.dim()
        loadPicture(media)
    }

    private func show(_ state: State) {
        var loading = false
        var failed = false
        switch state {
        case .loading(let fraction):
            loading = true
            progress.string = fraction.map { "\(Int($0 * 100))%" } ?? ""
        case .shown:
            break
        case .failed(let why):
            failed = true
            message.string = why
        }
        spinner.isHidden = !loading
        progress.isHidden = !loading || progress.string.isEmpty
        message.isHidden = !failed
        retryButton.isHidden = !failed || owner?.fetchesAgain == false
        placeStates()
    }

    /// Hands over the file, which the core fetches from the server if this device doesn't have it.
    private func fetch(_ done: @escaping (Result<URL, Error>) -> Void) {
        guard let media = content else { return }
        if let file { return done(.success(file)) }
        guard let owner else { return done(.failure(CoreBridge.CoreError(message: "Something went wrong."))) }
        owner.fetchFile(id: media.id) { [weak self] fetched in
            guard let self, self.content?.id == media.id else { return }
            self.file = try? fetched.get()
            done(fetched)
        }
    }

    override func layoutNow() {
        picture.frame = bounds
        picture.drawn = content?.drawn(width: bounds.width)
        let side = playButton.size.height
        playButton.frame = CGRect(x: (bounds.midX - side / 2).rounded(), y: (bounds.midY - side / 2).rounded(), width: side, height: side)
        caption.frame = CGRect(x: 12, y: bounds.maxY - 10 - scaled(16), width: max(0, bounds.width - 24), height: scaled(16))
        placeStates()
    }

    /// Centres the spinner with its percentage, or the message with its button, in the box.
    private func placeStates() {
        let box = picture.frame
        let spinnerSide = ControlSize.large.symbolSide
        let spinnerStack = spinnerSide + (progress.isHidden ? 0 : Self.stackGap + Self.lineHeight)
        var y = (box.midY - spinnerStack / 2).rounded()
        spinner.frame = CGRect(x: (box.midX - spinnerSide / 2).rounded(), y: y, width: spinnerSide, height: spinnerSide)
        progress.frame = CGRect(x: box.minX, y: y + spinnerSide + Self.stackGap, width: box.width, height: Self.lineHeight)
        let buttonHeight = RowButton.metrics.height
        let messageStack = Self.lineHeight + (retryButton.isHidden ? 0 : Self.stackGap + buttonHeight)
        y = (box.midY - messageStack / 2).rounded()
        message.frame = CGRect(x: box.minX + 12, y: y, width: max(0, box.width - 24), height: Self.lineHeight)
        let buttonWidth = retryButton.width
        retryButton.frame = CGRect(
            x: (box.midX - buttonWidth / 2).rounded(), y: y + Self.lineHeight + Self.stackGap, width: buttonWidth, height: buttonHeight
        )
    }

    override func takesPress(at point: CGPoint) -> Bool {
        guard bounds.contains(point) else { return false }
        let buttons = [retryButton!, playButton!].filter { !$0.isHidden }
        return !buttons.contains { $0.frame.contains(point) }
    }

    /// Opens the image or the video in the viewer, which plays a video and fills the screen with it.
    private func pressed() {
        guard let media = content, media.video || picture.picture != nil else { return }
        owner?.view([ViewedMedia(name: media.name, video: media.video, source: .media(media.id), drawnSize: media.svgSize, pixels: media.pixels)], at: 0)
    }
}
