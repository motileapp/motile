import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The files attached to a message, in its bubble: the images and videos as tiles that open in
/// the viewer, and the others by name under them. The tiles are all one size, so the row knows
/// its height before any picture is on this device.
final class AttachedFilesView: FlippedView {
    private static let tile = CGSize(width: 104, height: 78)
    private static let gap: CGFloat = 6
    private static let namesHeight: CGFloat = scaled(16)

    private weak var owner: RowOwner?
    private var files: [AttachedFile] = []
    private var tiles: [TileView] = []
    private let names = TextLabel(font: Theme.smallFont, color: Theme.secondary)

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(names)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// What the files add to the height of a row that is `width` wide, to place rows nobody has
    /// scrolled to yet.
    static func height(_ files: [AttachedFile], width: CGFloat) -> CGFloat {
        let height = size(files, width: max(120, width * 0.8) - 28).height
        return height > 0 ? height + 8 : 0
    }

    static func size(_ files: [AttachedFile], width: CGFloat) -> CGSize {
        let pictured = files.filter { $0.media != nil }.count
        let named = files.count - pictured
        let perRow = max(1, Int((width + gap) / (tile.width + gap)))
        let rows = (pictured + perRow - 1) / perRow
        let columns = min(pictured, perRow)
        let tilesWidth = CGFloat(columns) * (tile.width + gap) - (columns > 0 ? gap : 0)
        let tilesHeight = CGFloat(rows) * (tile.height + gap) - (rows > 0 ? gap : 0)
        let namesWidth = named > 0 ? min(width, ceil(namesText(files).size(withAttributes: [.font: Theme.smallFont]).width) + 4) : 0
        let between: CGFloat = rows > 0 && named > 0 ? gap : 0
        return CGSize(width: max(tilesWidth, namesWidth), height: tilesHeight + between + (named > 0 ? namesHeight : 0))
    }

    private static func namesText(_ files: [AttachedFile]) -> String {
        files.filter { $0.media == nil }.map { "📎 \($0.name)" }.joined(separator: "   ")
    }

    func show(_ files: [AttachedFile], owner: RowOwner?) {
        self.owner = owner
        guard files != self.files else { return }
        self.files = files
        names.string = Self.namesText(files)
        names.isHidden = names.string.isEmpty
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
            let origin = CGPoint(
                x: CGFloat(index % perRow) * (Self.tile.width + Self.gap),
                y: CGFloat(index / perRow) * (Self.tile.height + Self.gap)
            )
            tile.frame = CGRect(origin: origin, size: Self.tile)
        }
        let tilesBottom = tiles.last.map { $0.frame.maxY + Self.gap } ?? 0
        names.frame = CGRect(x: 0, y: tilesBottom, width: width, height: Self.namesHeight)
    }

    /// One image or video: its picture, cut to the tile, with a play sign on a video.
    private final class TileView: FlippedView {
        private let picture = PictureView()
        private let playSymbol = SymbolView("play.circle.fill", size: 26, tint: .white)
        private var shown: AttachedFile?

        override init(frame: CGRect) {
            super.init(frame: frame)
            picture.fills = true
            addSubview(picture)
            playSymbol.dropShadow(opacity: 0.4, radius: 4, down: 0)
            addSubview(playSymbol)
            pointer = .hand
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        func show(_ file: AttachedFile, owner: RowOwner?, onClick: @escaping () -> Void) {
            onPress = { _ in onClick() }
            tip = file.name
            describe(file.name)
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
            let maxPixels = AttachedFilesView.tile.width * 2 * Platform.pixelsPerPoint
            owner?.media(id: id) { [weak self] url in
                guard let url else { return }
                Pictures.decode(url, id: key, maxPixels: maxPixels) { image in
                    guard let self, self.shown == file else { return }
                    self.picture.picture = image
                }
            }
        }

        override func layoutNow() {
            picture.frame = bounds
            playSymbol.frame = CGRect(x: bounds.midX - 17, y: bounds.midY - 17, width: 34, height: 34)
        }
    }
}
