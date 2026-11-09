import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The files attached to a message, in its bubble: the images and videos as tiles that open in
/// the viewer, and the others under them, a line each, which download as an agent's file does. The
/// tiles are all one size, so the row knows its height before any picture is on this device.
final class AttachedFilesView: FlippedView {
    private static let tile = CGSize(width: 96, height: 96)
    private static let gap: CGFloat = 6
    private static let lineHeight: CGFloat = scaled(16)
    private static let lineGap: CGFloat = 4

    private weak var owner: RowOwner?
    private var files: [AttachedFile] = []
    private var tiles: [TileView] = []
    private var lines: [FileLine] = []

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
        let namesWidth = min(width, named.map(FileLine.width).max() ?? 0)
        let namesHeight = named.isEmpty ? 0 : CGFloat(named.count) * (lineHeight + lineGap) - lineGap
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
        while lines.count > named.count { lines.removeLast().removeFromSuperview() }
        while lines.count < named.count {
            let line = FileLine()
            lines.append(line)
            addSubview(line)
        }
        for (line, file) in zip(lines, named) {
            line.show(file, owner: owner)
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
        for line in lines {
            line.frame = CGRect(x: 0, y: y, width: width, height: Self.lineHeight)
            y += Self.lineHeight + Self.lineGap
        }
    }

    /// A file that isn't an image or a video: a paperclip and its name. One its server kept
    /// downloads, shows how far, and opens once it is here.
    private final class FileLine: FlippedView {
        private static let iconGap: CGFloat = 4
        private static var iconSide: CGFloat { PlatformImage.symbolSide(.xs) }

        private let icon = SymbolView(.paperclip, size: .xs, tint: Theme.mutedForeground)
        private let name = TextLabel(font: Theme.smallFont, color: Theme.mutedForeground)
        private let status = TextLabel(font: PlatformFont.uiDigits(12), color: Theme.mutedForeground)
        private weak var owner: RowOwner?
        private var download: SentFile?
        private var hovered = false
        private var downloadWatch: NSObjectProtocol?

        static func width(_ file: AttachedFile) -> CGFloat {
            iconSide + iconGap + ceil(file.name.size(withAttributes: [.font: Theme.smallFont]).width) + 4
        }

        override init(frame: CGRect) {
            super.init(frame: frame)
            name.breaks = .byTruncatingMiddle
            addSubview(icon)
            addSubview(name)
            addSubview(status)
            onHover = { [weak self] point in
                guard let self else { return }
                self.hovered = point != nil
                self.light()
            }
            downloadWatch = NotificationCenter.default.addObserver(forName: FileDownloads.changed, object: nil, queue: .main) { [weak self] note in
                guard let self, let id = self.download?.id, note.userInfo?["id"] as? String == id else { return }
                self.light()
            }
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        deinit {
            if let downloadWatch { NotificationCenter.default.removeObserver(downloadWatch) }
        }

        func show(_ file: AttachedFile, owner: RowOwner?) {
            self.owner = owner
            download = file.download
            name.string = file.name
            describe(file.name, button: download != nil)
            pointer = download == nil ? .arrow : .hand
            onPress = download == nil ? nil : { [weak self] _ in self?.press() }
            menuActions = download == nil ? nil : { [weak self] in self?.actions() ?? [] }
            light()
        }

        private func press() {
            guard let download, let owner else { return }
            FileDownloads.press(download, owner: owner, from: self)
        }

        private func actions() -> [MenuAction] {
            guard let download, let owner else { return [] }
            return FileDownloads.actions(download, owner: owner, from: self)
        }

        /// The name lights under the pointer when a press does something, and the line says how
        /// far a download is or why it failed.
        private func light() {
            let lit = hovered && download != nil
            name.color = lit ? Theme.foreground : Theme.mutedForeground
            icon.tint = name.color
            status.color = Theme.mutedForeground
            tip = nil
            status.string = ""
            if let download {
                switch FileDownloads.state(of: download.id) {
                case .fetching(let fraction):
                    status.string = " · \(Int((fraction * 100).rounded()))%"
                case .failed(let reason):
                    status.string = " · \(FileRowView.firstSentence(of: reason))"
                    status.color = Theme.destructive
                    tip = reason
                case .remote, .downloaded:
                    break
                }
            }
            layoutNow()
        }

        override func layoutNow() {
            let side = Self.iconSide
            icon.frame = CGRect(x: 0, y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
            let x = icon.frame.maxX + Self.iconGap
            let statusNatural = status.string.isEmpty ? 0 : ceil(status.string.size(withAttributes: [.font: PlatformFont.uiDigits(12)]).width) + 2
            let statusWidth = min(statusNatural, max(0, bounds.width - x))
            let nameWidth = min(ceil(name.string.size(withAttributes: [.font: Theme.smallFont]).width) + 4, max(0, bounds.width - x - statusWidth))
            name.frame = CGRect(x: x, y: 0, width: nameWidth, height: bounds.height)
            status.frame = CGRect(x: name.frame.maxX, y: 0, width: statusWidth, height: bounds.height)
        }

        override func takesPress(at point: CGPoint) -> Bool {
            bounds.contains(point) && point.x <= status.frame.maxX
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
