import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The files attached to a message, in its bubble: the images and videos as tiles that open in
/// the viewer, and the others under them as the cards an agent's files are. The tiles and the cards
/// are all one size, so the row knows its height before any picture is on this device.
final class AttachedFilesView: FlippedView {
    private static let tile = CGSize(width: 96, height: 96)
    private static let gap: CGFloat = 6

    private weak var owner: RowOwner?
    private var files: [AttachedFile] = []
    private var tiles: [TileView] = []
    private var cards: [FileCard] = []

    /// What the files add to the height of a row that is `width` wide, to place rows nobody has
    /// scrolled to yet.
    static func height(_ files: [AttachedFile], width: CGFloat) -> CGFloat {
        let height = size(files, width: max(120, width * 0.8) - 28).height
        return height > 0 ? height + 8 : 0
    }

    static func size(_ files: [AttachedFile], width: CGFloat) -> CGSize {
        let pictured = files.filter(\.pictured).count
        let named = files.filter { !$0.pictured }
        let perRow = max(1, Int((width + gap) / (tile.width + gap)))
        let rows = (pictured + perRow - 1) / perRow
        let columns = min(pictured, perRow)
        let tilesWidth = CGFloat(columns) * (tile.width + gap) - (columns > 0 ? gap : 0)
        let tilesHeight = CGFloat(rows) * (tile.height + gap) - (rows > 0 ? gap : 0)
        let namesWidth = named.isEmpty ? 0 : min(width, FileCard.widest)
        let namesHeight = named.isEmpty ? 0 : CGFloat(named.count) * (FileCard.height + gap) - gap
        let between: CGFloat = rows > 0 && !named.isEmpty ? gap : 0
        return CGSize(width: max(tilesWidth, namesWidth), height: tilesHeight + between + namesHeight)
    }

    func show(_ files: [AttachedFile], owner: RowOwner?) {
        self.owner = owner
        guard files != self.files else { return }
        self.files = files
        let pictured = files.filter(\.pictured)
        while tiles.count > pictured.count { tiles.removeLast().removeFromSuperview() }
        while tiles.count < pictured.count {
            let tile = TileView()
            tiles.append(tile)
            addSubview(tile)
        }
        for (index, file) in pictured.enumerated() {
            tiles[index].show(file, owner: owner) { [weak self] in self?.open(index) }
        }
        let named = files.filter { !$0.pictured }
        while cards.count > named.count { cards.removeLast().removeFromSuperview() }
        while cards.count < named.count {
            let card = FileCard()
            cards.append(card)
            addSubview(card)
        }
        for (card, file) in zip(cards, named) {
            let download = file.download
            card.show(download ?? SentFile(id: "", bytes: file.size, alt: "", name: file.name), owner: owner, kept: download != nil)
        }
    }

    private func open(_ index: Int) {
        let viewed = files.filter(\.pictured).compactMap { file in
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
        var y = tiles.last.map { $0.frame.maxY + Self.gap } ?? 0
        for card in cards {
            card.frame = CGRect(x: 0, y: y, width: min(width, FileCard.widest), height: FileCard.height)
            y += FileCard.height + Self.gap
        }
    }

    /// One image or video: all of its picture, fitted in the tile, with a play sign on a video.
    private final class TileView: FlippedView {
        private let picture = PictureView()
        private var playButton: OverlayButton!
        private var shown: AttachedFile?

        override init(frame: CGRect) {
            super.init(frame: frame)
            addSubview(picture)
            playButton = OverlayButton(.play, size: .regular, tooltip: "Play") { [weak self] in self?.onPress?(.zero) }
            addSubview(playButton)
            pointer = .hand
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        func show(_ file: AttachedFile, owner: RowOwner?, onClick: @escaping () -> Void) {
            onPress = { _ in onClick() }
            menuActions = { [weak self, weak owner] in
                guard let self, let id = file.media else { return [] }
                return MediaFiles.actions(video: file.video, name: file.name, from: self) { done in
                    owner?.media(id: id) { url in
                        guard let url else { return }
                        done(url)
                    }
                }
            }
            tip = file.name
            describe(file.name)
            playButton.isHidden = !file.video
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
            let side = playButton.size.height
            playButton.frame = CGRect(x: (bounds.midX - side / 2).rounded(), y: (bounds.midY - side / 2).rounded(), width: side, height: side)
        }

        override func takesPress(at point: CGPoint) -> Bool {
            bounds.contains(point) && (playButton.isHidden || !playButton.frame.contains(point))
        }
    }
}
