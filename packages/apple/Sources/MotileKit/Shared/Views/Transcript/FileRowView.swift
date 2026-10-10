import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A file a reply sends, other than an image or a video, for the user to download.
struct SentFile {
    let id: String
    let bytes: Int64
    let alt: String
    let name: String

    init(json: JSON) {
        id = json.string("media")
        bytes = (json["size"] as? NSNumber)?.int64Value ?? 0
        alt = json.string("alt")
        name = json.string("name")
    }

    var size: String { ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file) }

    /// What kind of file it is and how large, as "PDF · 2.4 MB".
    var detail: String {
        let kind = (name as NSString).pathExtension.uppercased()
        return kind.isEmpty ? size : "\(kind) · \(size)"
    }

    var symbol: Symbol {
        switch (name as NSString).pathExtension.lowercased() {
        case "pdf", "md", "markdown", "txt", "rst", "csv", "tsv", "doc", "docx", "rtf", "pages", "odt", "log": .fileText
        default: .file
        }
    }
}

/// A file a reply sends, as a card with its name, its kind and size, and a button that downloads
/// it. Pressing the card opens the file.
final class FileRowView: RowView {
    private static let gap: CGFloat = 6
    private static let cardHeight = scaled(56)
    private static let widest = scaled(420)
    private static let tileSide = scaled(36)
    private static let inset = (cardHeight - tileSide) / 2
    private static let lineHeight = scaled(16)
    /// The button is as far from the card's side as from its top and bottom.
    private static let buttonMargin = (cardHeight - RowButton.metrics.height) / 2
    /// Where the files downloaded since the client opened went, by their names on the server.
    private static var downloaded: [String: URL] = [:]

    private let card = SurfaceView()
    private let tile = SurfaceView()
    private let icon = SymbolView(.file, size: ControlSize.large.symbol, tint: Theme.mutedForeground)
    private let name = TextLabel(font: .ui(13, weight: .medium), color: Theme.foreground)
    private let detail = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
    private let spinner = SpinnerView(size: ControlSize.regular.symbol)
    private var downloadButton: RowButton!
    private var openButton: RowButton!
    private var content: SentFile?
    private var file: URL?
    private var loading = false
    private var fraction: Double?
    private var failed = false
    private var progressWatch: NSObjectProtocol?

    static var height: CGFloat { cardHeight + gap * 2 }

    override init(frame: CGRect) {
        super.init(frame: frame)
        card.fill = Theme.backgroundSecondary
        card.stroke = Theme.border
        card.radius = Radius.lg
        addSubview(card)
        tile.fill = Theme.backgroundSecondaryAccent
        tile.radius = Radius.md
        card.addSubview(tile)
        tile.addSubview(icon)
        name.breaks = .byTruncatingMiddle
        card.addSubview(name)
        card.addSubview(detail)
        card.addSubview(spinner)
        downloadButton = button("Download", tooltip: "Download this file") { [weak self] in self?.download() }
        openButton = button("Open", tooltip: "Open the downloaded file") { [weak self] in
            guard let self, let id = self.content?.id, let saved = Self.downloaded[id] else { return }
            Platform.open(saved)
        }
        pointer = .hand
        onPress = { [weak self] _ in self?.open() }
        onHover = { [weak self] point in
            guard let self else { return }
            let lit = point.map { self.takesPress(at: $0) } ?? false
            self.card.fill = lit ? Theme.backgroundSecondaryAccent : Theme.backgroundSecondary
        }
        menuActions = { [weak self] in
            guard let self, let content = self.content else { return [] }
            return [
                MenuAction(title: "Open", symbol: .squareArrowOutUpRight) { [weak self] in self?.open() },
                MenuAction(title: MediaFiles.saveTitle, symbol: .download) { [weak self] in
                    self?.fetch { file in
                        guard let self else { return }
                        MediaFiles.save(file, named: content.name, from: self)
                    }
                },
                MenuAction(title: "Copy File", symbol: .copy) { [weak self] in
                    self?.fetch { file in
                        DispatchQueue.global(qos: .userInitiated).async {
                            let named = MediaFiles.namedCopy(of: file, name: content.name)
                            DispatchQueue.main.async { MediaFiles.copyFile(named) }
                        }
                    }
                },
            ]
        }
        progressWatch = NotificationCenter.default.addObserver(forName: .mediaProgress, object: nil, queue: .main) { [weak self] note in
            guard let self, self.loading, let id = self.content?.id, note.userInfo?["id"] as? String == id else { return }
            guard let fraction = note.userInfo?["fraction"] as? Double else { return }
            self.fraction = fraction
            self.show()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        if let progressWatch { NotificationCenter.default.removeObserver(progressWatch) }
    }

    private func button(_ title: String, tooltip: String, action: @escaping () -> Void) -> RowButton {
        let button = RowButton(
            title: title,
            tooltip: tooltip,
            radius: RowButton.metrics.radius,
            bordered: true,
            insets: PlatformEdgeInsets(top: 0, left: 0, bottom: 0, right: 0),
            action: action
        )
        card.addSubview(button)
        return button
    }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .file(let content) = row.kind, content.id != self.content?.id else { return }
        self.content = content
        file = nil
        loading = false
        fraction = nil
        failed = false
        tip = content.alt.isEmpty ? nil : content.alt
        describe("\(content.name), \(content.detail)")
        icon.show(content.symbol, size: ControlSize.large.symbol)
        name.string = content.name
        downloadButton.dim()
        openButton.dim()
        show()
    }

    private func show() {
        guard let content else { return }
        let saved = Self.downloaded[content.id] != nil
        if failed {
            detail.string = "This file couldn't be downloaded."
        } else if loading, let fraction {
            detail.string = "\(Int(fraction * 100))% of \(content.size)"
        } else {
            detail.string = content.detail
        }
        spinner.isHidden = !loading
        downloadButton.isHidden = loading || saved
        openButton.isHidden = loading || !saved
        placeEnd()
    }

    private func download() {
        fetch { [weak self] file in
            guard let self, let content = self.content else { return }
            MediaFiles.download(file, named: content.name, from: self) { [weak self] saved in
                guard let saved else { return }
                Self.downloaded[content.id] = saved
                guard self?.content?.id == content.id else { return }
                self?.show()
            }
        }
    }

    private func open() {
        fetch { [weak self] file in
            guard let self, let content = self.content else { return }
            MediaFiles.open(file, named: content.name, from: self)
        }
    }

    /// Hands over the file, which the core fetches from the server if this device doesn't have it.
    private func fetch(_ done: @escaping (URL) -> Void) {
        guard let content else { return }
        if let file { return done(file) }
        guard let owner, !loading else { return }
        loading = true
        failed = false
        show()
        owner.media(id: content.id) { [weak self] file in
            guard let self, self.content?.id == content.id else { return }
            self.loading = false
            self.fraction = nil
            self.file = file
            self.failed = file == nil
            self.show()
            guard let file else { return }
            done(file)
        }
    }

    override func layout(width: CGFloat) -> CGFloat {
        let cardWidth = min(width, Self.widest)
        card.frame = CGRect(x: 0, y: Self.gap, width: cardWidth, height: Self.cardHeight)
        tile.frame = CGRect(x: Self.inset, y: Self.inset, width: Self.tileSide, height: Self.tileSide)
        icon.frame = tile.bounds
        placeEnd()
        return Self.height
    }

    /// Places the button or the spinner at the card's end, and the words in the room left.
    private func placeEnd() {
        let bounds = card.bounds
        let shown = [downloadButton!, openButton!].first { !$0.isHidden }
        var end = bounds.width - Self.inset
        if let shown {
            let buttonWidth = shown.width
            shown.frame = CGRect(
                x: bounds.width - Self.buttonMargin - buttonWidth, y: Self.buttonMargin, width: buttonWidth, height: RowButton.metrics.height
            )
            end = shown.frame.minX - Self.inset
        } else if !spinner.isHidden {
            let side = ControlSize.regular.symbol + 4
            spinner.frame = CGRect(x: bounds.width - Self.inset - side, y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
            end = spinner.frame.minX - Self.inset
        }
        let x = tile.frame.maxX + Self.inset - 4
        let top = ((bounds.height - Self.lineHeight * 2 - 2) / 2).rounded()
        name.frame = CGRect(x: x, y: top, width: max(0, end - x), height: Self.lineHeight)
        detail.frame = CGRect(x: x, y: top + Self.lineHeight + 2, width: max(0, end - x), height: Self.lineHeight)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        guard card.frame.contains(point) else { return false }
        let inCard = CGPoint(x: point.x - card.frame.minX, y: point.y - card.frame.minY)
        return ![downloadButton!, openButton!].contains { !$0.isHidden && $0.frame.contains(inCard) }
    }
}
