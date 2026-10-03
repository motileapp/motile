import AppKit

/// The files attached to a message, in its bubble: the images and videos as tiles that open in
/// the viewer, and the others by name under them. The tiles are all one size, so the row knows
/// its height before any picture is on this Mac.
final class AttachedFilesView: FlippedView {
    private static let tile = NSSize(width: 104, height: 78)
    private static let gap: CGFloat = 6
    private static let namesHeight: CGFloat = 16

    private weak var owner: RowOwner?
    private var files: [AttachedFile] = []
    private var tiles: [TileView] = []
    private let names = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        names.font = Theme.smallFont
        names.textColor = Theme.secondary
        names.lineBreakMode = .byTruncatingTail
        names.maximumNumberOfLines = 1
        addSubview(names)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// What the files add to the height of a row that is `width` wide, to place rows nobody has
    /// scrolled to yet.
    static func height(_ files: [AttachedFile], width: CGFloat) -> CGFloat {
        let height = size(files, width: max(120, width * 0.8) - 28).height
        return height > 0 ? height + 8 : 0
    }

    static func size(_ files: [AttachedFile], width: CGFloat) -> NSSize {
        let pictured = files.filter { $0.media != nil }.count
        let named = files.count - pictured
        let perRow = max(1, Int((width + gap) / (tile.width + gap)))
        let rows = (pictured + perRow - 1) / perRow
        let columns = min(pictured, perRow)
        let tilesWidth = CGFloat(columns) * (tile.width + gap) - (columns > 0 ? gap : 0)
        let tilesHeight = CGFloat(rows) * (tile.height + gap) - (rows > 0 ? gap : 0)
        let namesWidth = named > 0 ? min(width, ceil(namesText(files).size(withAttributes: [.font: Theme.smallFont]).width) + 4) : 0
        let between: CGFloat = rows > 0 && named > 0 ? gap : 0
        return NSSize(width: max(tilesWidth, namesWidth), height: tilesHeight + between + (named > 0 ? namesHeight : 0))
    }

    private static func namesText(_ files: [AttachedFile]) -> String {
        files.filter { $0.media == nil }.map { "📎 \($0.name)" }.joined(separator: "   ")
    }

    func show(_ files: [AttachedFile], owner: RowOwner?) {
        self.owner = owner
        guard files != self.files else { return }
        self.files = files
        names.stringValue = Self.namesText(files)
        names.isHidden = names.stringValue.isEmpty
        let pictured = files.filter { $0.media != nil }
        while tiles.count > pictured.count { tiles.removeLast().removeFromSuperview() }
        while tiles.count < pictured.count {
            let tile = TileView()
            tiles.append(tile)
            addSubview(tile)
        }
        for (index, file) in pictured.enumerated() {
            tiles[index].show(file, owner: owner) { [weak self] in self?.open(index) }
        }
    }

    private func open(_ index: Int) {
        let viewed = files.compactMap { file in
            file.media.map { ViewedMedia(name: file.name, video: file.video, source: .media($0)) }
        }
        owner?.view(viewed, at: index)
    }

    func layout(width: CGFloat) {
        let perRow = max(1, Int((width + Self.gap) / (Self.tile.width + Self.gap)))
        for (index, tile) in tiles.enumerated() {
            let origin = NSPoint(
                x: CGFloat(index % perRow) * (Self.tile.width + Self.gap),
                y: CGFloat(index / perRow) * (Self.tile.height + Self.gap)
            )
            tile.frame = NSRect(origin: origin, size: Self.tile)
        }
        let tilesBottom = tiles.last.map { $0.frame.maxY + Self.gap } ?? 0
        names.frame = NSRect(x: 0, y: tilesBottom, width: width, height: Self.namesHeight)
    }

    /// One image or video: its picture, cut to the tile, with a play sign on a video.
    private final class TileView: FlippedView {
        private let picture = PictureView()
        private let playSymbol = NSImageView()
        private var shown: AttachedFile?
        private var onClick: (() -> Void)?

        override init(frame: NSRect) {
            super.init(frame: frame)
            picture.fills = true
            addSubview(picture)
            let configuration = NSImage.SymbolConfiguration(pointSize: 26, weight: .regular)
            playSymbol.image = NSImage(systemSymbolName: "play.circle.fill", accessibilityDescription: "Play")?
                .withSymbolConfiguration(configuration)
            playSymbol.contentTintColor = .white
            playSymbol.shadow = {
                let shadow = NSShadow()
                shadow.shadowColor = NSColor.black.withAlphaComponent(0.4)
                shadow.shadowBlurRadius = 4
                return shadow
            }()
            addSubview(playSymbol)
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        func show(_ file: AttachedFile, owner: RowOwner?, onClick: @escaping () -> Void) {
            self.onClick = onClick
            toolTip = file.name
            setAccessibilityLabel(file.name)
            playSymbol.isHidden = !file.video
            guard file != shown || picture.picture == nil else { return }
            shown = file
            guard let id = file.picture else {
                picture.picture = nil
                return
            }
            let key = "tile:\(id)"
            picture.picture = Pictures.cached(key)
            guard picture.picture == nil else { return }
            let maxPixels = AttachedFilesView.tile.width * 2 * (window?.backingScaleFactor ?? 2)
            owner?.media(id: id) { [weak self] url in
                guard let url else { return }
                Pictures.decode(url, id: key, maxPixels: maxPixels) { image in
                    guard let self, self.shown == file else { return }
                    self.picture.picture = image
                }
            }
        }

        override func layout() {
            super.layout()
            picture.frame = bounds
            playSymbol.frame = NSRect(x: bounds.midX - 17, y: bounds.midY - 17, width: 34, height: 34)
        }

        override func hitTest(_ point: NSPoint) -> NSView? {
            guard !isHidden, frame.contains(point) else { return nil }
            return self
        }

        override func mouseDown(with event: NSEvent) {
            onClick?()
        }

        override func resetCursorRects() {
            addCursorRect(bounds, cursor: .pointingHand)
        }
    }
}
