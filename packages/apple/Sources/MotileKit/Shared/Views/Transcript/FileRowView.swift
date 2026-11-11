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
        self.init(id: json.string("media"), bytes: (json["size"] as? NSNumber)?.int64Value ?? 0, alt: json.string("alt"), name: json.string("name"))
    }

    init(id: String, bytes: Int64, alt: String, name: String) {
        self.id = id
        self.bytes = bytes
        self.alt = alt
        self.name = name
    }

    var size: String { ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file) }

    /// What kind of file it is and how large, as "PDF · 2.4 MB".
    var detail: String {
        let kind = (name as NSString).pathExtension.uppercased()
        guard bytes > 0 else { return kind }
        return kind.isEmpty ? size : "\(kind) · \(size)"
    }

    var symbol: Symbol {
        switch (name as NSString).pathExtension.lowercased() {
        case "pdf", "md", "markdown", "txt", "rst", "csv", "tsv", "doc", "docx", "rtf", "pages", "odt", "log": .fileText
        default: .file
        }
    }
}

/// The files agents sent that are downloading or were downloaded, by their names on the server. A
/// download goes on while its row is off screen, and where it went is remembered across launches.
enum FileDownloads {
    enum State {
        case remote
        case fetching(Double)
        case failed(String)
        case downloaded(URL)
    }

    static let changed = Notification.Name("motile.fileDownloads")
    private static let key = "downloadedFiles"
    private static var fetching: [String: Double] = [:]
    /// Files fetched that are being put where downloads go.
    private static var saving: Set<String> = []
    private static var failures: [String: String] = [:]
    /// Goes up when a fetch is stopped, so that its answer is not taken for a later one's.
    private static var generations: [String: Int] = [:]
    private static var progressWatch: NSObjectProtocol?

    static func state(of id: String) -> State {
        if saving.contains(id) { return .fetching(1) }
        if let fraction = fetching[id] { return .fetching(fraction) }
        if let file = downloaded(id) { return .downloaded(file) }
        if let failure = failures[id] { return .failed(failure) }
        return .remote
    }

    /// Fetches the file and puts it where downloads go: Downloads on a Mac, the client on iOS.
    static func download(_ file: SentFile, owner: RowOwner, then: ((URL) -> Void)? = nil) {
        fetch(file, owner: owner) { fetched in
            saving.insert(file.id)
            MediaFiles.download(fetched, named: file.name) { saved in
                saving.remove(file.id)
                guard let saved else { return fail(file.id, "This file couldn't be saved in Downloads.") }
                remember(saved, as: file.id)
                tell(file.id)
                then?(saved)
            }
        }
    }

    /// Hands over the client's copy of the file, which the core fetches if this device doesn't
    /// have it.
    static func fetch(_ file: SentFile, owner: RowOwner, done: @escaping (URL) -> Void) {
        watchProgress()
        let generation = generations[file.id, default: 0]
        if fetching[file.id] == nil { fetching[file.id] = 0 }
        failures[file.id] = nil
        tell(file.id)
        owner.fetchFile(id: file.id) { result in
            guard generations[file.id, default: 0] == generation else { return }
            fetching[file.id] = nil
            switch result {
            case .success(let fetched):
                done(fetched)
                tell(file.id)
            case .failure(let error):
                fail(file.id, error.localizedDescription)
            }
        }
    }

    static func cancel(_ id: String, owner: RowOwner) {
        guard fetching.removeValue(forKey: id) != nil else { return }
        generations[id, default: 0] += 1
        owner.cancelFetch(id: id)
        tell(id)
    }

    /// Downloads the file, stops its download, or opens it once it is downloaded.
    static func press(_ file: SentFile, owner: RowOwner, from view: PlatformView) {
        switch state(of: file.id) {
        case .remote, .failed:
            download(file, owner: owner)
        case .fetching:
            cancel(file.id, owner: owner)
        case .downloaded(let downloaded):
            MediaFiles.open(downloaded, named: file.name, from: view)
        }
    }

    static func reveal(_ file: SentFile, from view: PlatformView) {
        guard case .downloaded(let downloaded) = state(of: file.id) else { return }
        MediaFiles.reveal(downloaded, named: file.name, from: view)
    }

    /// The menu of the file as far as its download is.
    static func actions(_ file: SentFile, owner: RowOwner, from view: PlatformView) -> [MenuAction] {
        let copy = MenuAction(title: "Copy File", symbol: .copy) { [weak owner] in
            guard let owner else { return }
            withFile(file, owner: owner) { fetched in
                DispatchQueue.global(qos: .userInitiated).async {
                    let named = MediaFiles.namedCopy(of: fetched, name: file.name)
                    DispatchQueue.main.async { MediaFiles.copyFile(named) }
                }
            }
        }
        switch state(of: file.id) {
        case .fetching:
            return [MenuAction(title: "Cancel Download", symbol: .x) { cancel(file.id, owner: owner) }]
        case .downloaded(let downloaded):
            return [
                MenuAction(title: "Open", symbol: .squareArrowOutUpRight) { [weak view] in
                    guard let view else { return }
                    MediaFiles.open(downloaded, named: file.name, from: view)
                },
                MenuAction(title: MediaFiles.revealTitle, symbol: .folder) { [weak view] in
                    guard let view else { return }
                    reveal(file, from: view)
                },
                copy,
            ]
        case .remote, .failed:
            return [
                MenuAction(title: "Download", symbol: .download) { download(file, owner: owner) },
                MenuAction(title: MediaFiles.saveTitle, symbol: .download) { [weak view, weak owner] in
                    guard let owner else { return }
                    withFile(file, owner: owner) { fetched in
                        guard let view else { return }
                        MediaFiles.save(fetched, named: file.name, from: view)
                    }
                },
                copy,
            ]
        }
    }

    /// Hands over the downloaded file, or the client's copy, which the core fetches if need be.
    private static func withFile(_ file: SentFile, owner: RowOwner, _ done: @escaping (URL) -> Void) {
        if case .downloaded(let downloaded) = state(of: file.id) { return done(downloaded) }
        fetch(file, owner: owner, done: done)
    }

    private static func fail(_ id: String, _ reason: String) {
        failures[id] = reason
        tell(id)
    }

    private static func tell(_ id: String) {
        NotificationCenter.default.post(name: changed, object: nil, userInfo: ["id": id])
    }

    private static func watchProgress() {
        guard progressWatch == nil else { return }
        progressWatch = NotificationCenter.default.addObserver(forName: .mediaProgress, object: nil, queue: .main) { note in
            guard let id = note.userInfo?["id"] as? String, fetching[id] != nil else { return }
            guard let fraction = note.userInfo?["fraction"] as? Double else { return }
            fetching[id] = fraction
            tell(id)
        }
    }

    /// Where the file was downloaded, while it is still there. Paths are kept from the home
    /// folder, which moves on iOS when the client is updated.
    private static func downloaded(_ id: String) -> URL? {
        guard let path = (UserDefaults.standard.dictionary(forKey: key) as? [String: String])?[id] else { return nil }
        let file = URL(fileURLWithPath: path.hasPrefix("/") ? path : NSHomeDirectory() + "/" + path)
        return FileManager.default.fileExists(atPath: file.path) ? file : nil
    }

    private static func remember(_ file: URL, as id: String) {
        var files = UserDefaults.standard.dictionary(forKey: key) as? [String: String] ?? [:]
        let home = NSHomeDirectory() + "/"
        files[id] = file.path.hasPrefix(home) ? String(file.path.dropFirst(home.count)) : file.path
        UserDefaults.standard.set(files, forKey: key)
    }
}

/// A file a reply sends, as its card.
final class FileRowView: RowView {
    private static let gap: CGFloat = 6
    private let card = FileCard()

    static var height: CGFloat { FileCard.height + gap * 2 }

    /// The words that say what went wrong, without the system's reasons after them.
    static func firstSentence(of reason: String) -> String {
        guard let end = reason.range(of: ". ") else { return reason }
        return String(reason[..<end.lowerBound]) + "."
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(card)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ row: RowModel) {
        super.configure(row)
        guard case .file(let content) = row.kind else { return }
        card.show(content, owner: owner)
    }

    override func layout(width: CGFloat) -> CGFloat {
        card.frame = CGRect(x: 0, y: Self.gap, width: min(width, FileCard.widest), height: FileCard.height)
        return Self.height
    }
}

/// A file as a card with its name, its kind and size, where an agent sends one and in a message's
/// bubble. Its icon says where the download is, as a messaging app's does: an arrow to download it,
/// the progress with a cross that stops it, then the kind of file, which the card opens. A file its
/// server didn't keep only shows its kind.
final class FileCard: FlippedView {
    static let height = scaled(56)
    static let widest = scaled(420)
    private static let tileSide = scaled(36)
    private static let ringInset = scaled(5)
    private static let inset = (height - tileSide) / 2
    private static let lineHeight = scaled(16)
    /// The button is as far from the card's side as from its top and bottom.
    private static let buttonMargin = (height - RowButton.metrics.height) / 2

    private let card = SurfaceView()
    private let tile = SurfaceView()
    private let ring = ProgressRingView()
    private let icon = SymbolView(.file, size: ControlSize.large.symbol, tint: Theme.mutedForeground)
    private let name = TextLabel(font: .ui(.sm, weight: .medium), color: Theme.foreground)
    private let detail = TextLabel(font: .uiDigits(.xs), color: Theme.mutedForeground)
    private var revealButton: RowButton!
    private weak var owner: RowOwner?
    private var content: SentFile?
    private var kept = true
    private var state = FileDownloads.State.remote
    private var hovered: CGPoint?
    private var downloadWatch: NSObjectProtocol?

    override init(frame: CGRect) {
        super.init(frame: frame)
        card.fill = Theme.backgroundSecondary
        card.stroke = Theme.border
        card.radius = Radius.lg
        addSubview(card)
        tile.radius = Radius.md
        card.addSubview(tile)
        tile.addSubview(ring)
        tile.addSubview(icon)
        name.breaks = .byTruncatingMiddle
        card.addSubview(name)
        card.addSubview(detail)
        revealButton = RowButton(
            title: MediaFiles.revealTitle,
            tooltip: MediaFiles.revealTitle,
            radius: RowButton.metrics.radius,
            bordered: true,
            hover: Theme.backgroundSecondaryAccent,
            rest: Theme.background,
            insets: PlatformEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)
        ) { [weak self] in self?.reveal() }
        revealButton.takesFirstClick = true
        takesFirstClick = true
        card.addSubview(revealButton)
        onHover = { [weak self] point in
            guard let self else { return }
            let entered = self.hovered == nil && point != nil
            self.hovered = point
            // The downloaded file may have been moved or removed since.
            guard entered else { return self.light() }
            self.show()
        }
        downloadWatch = NotificationCenter.default.addObserver(forName: FileDownloads.changed, object: nil, queue: .main) { [weak self] note in
            guard let self, self.kept, let id = self.content?.id, note.userInfo?["id"] as? String == id else { return }
            self.show()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        if let downloadWatch { NotificationCenter.default.removeObserver(downloadWatch) }
    }

    /// Shows the file, which can be downloaded when its server `kept` it.
    func show(_ file: SentFile, owner: RowOwner?, kept: Bool = true) {
        self.owner = owner
        guard file.id != content?.id || file.name != content?.name || kept != self.kept else { return }
        content = file
        self.kept = kept
        describe("\(file.name), \(file.detail)", button: kept)
        name.string = file.name
        pointer = kept ? .hand : .arrow
        onPress = kept ? { [weak self] point in self?.press(at: point) } : nil
        menuActions = kept ? { [weak self] in self?.actions() ?? [] } : nil
        revealButton.dim()
        show()
    }

    private func show() {
        guard let content else { return }
        state = kept ? FileDownloads.state(of: content.id) : .remote
        ring.isHidden = true
        tip = content.alt.isEmpty ? nil : content.alt
        detail.color = Theme.mutedForeground
        detail.string = content.detail
        switch state {
        case .remote:
            icon.show(kept ? .download : content.symbol, size: ControlSize.large.symbol)
        case .fetching(let fraction):
            ring.isHidden = false
            ring.fraction = fraction
            icon.show(.x, size: ControlSize.small.symbol)
            let received = ByteCountFormatter.string(fromByteCount: Int64(fraction * Double(content.bytes)), countStyle: .file)
            detail.string = "\(received) / \(content.size)"
        case .failed(let reason):
            icon.show(.download, size: ControlSize.large.symbol)
            tip = reason
            detail.color = Theme.destructive
            detail.string = FileRowView.firstSentence(of: reason)
        case .downloaded:
            icon.show(content.symbol, size: ControlSize.large.symbol)
        }
        revealButton.isHidden = !isDownloaded
        light()
        placeEnd()
    }

    private var isDownloaded: Bool {
        guard case .downloaded = state else { return false }
        return true
    }

    private var isFetching: Bool {
        guard case .fetching = state else { return false }
        return true
    }

    /// The card lights under the pointer where a press does something, and its icon's tile a step
    /// above it; while the file downloads only the tile lights, which stops it.
    private func light() {
        let overTile = hovered.map { tile.frame.contains($0) } ?? false
        let litCard = kept && !isFetching && (hovered.map { takesPress(at: $0) } ?? false)
        let litTile = overTile && isFetching
        card.fill = litCard ? Theme.backgroundSecondaryAccent : Theme.backgroundSecondary
        tile.fill = litCard || litTile ? Theme.backgroundSecondaryAccentStronger : Theme.backgroundSecondaryAccent
        icon.tint = litTile ? Theme.foreground : Theme.mutedForeground
    }

    private func press(at point: CGPoint) {
        guard let content, let owner else { return }
        show()
        guard !isFetching || tile.frame.contains(point) else { return }
        FileDownloads.press(content, owner: owner, from: self)
    }

    private func reveal() {
        guard let content else { return }
        guard case .downloaded = FileDownloads.state(of: content.id) else { return show() }
        FileDownloads.reveal(content, from: self)
    }

    private func actions() -> [MenuAction] {
        guard let content, let owner else { return [] }
        show()
        return FileDownloads.actions(content, owner: owner, from: self)
    }

    override func layoutNow() {
        card.frame = bounds
        tile.frame = CGRect(x: Self.inset, y: Self.inset, width: Self.tileSide, height: Self.tileSide)
        ring.frame = tile.bounds.insetBy(dx: Self.ringInset, dy: Self.ringInset)
        icon.frame = tile.bounds
        placeEnd()
    }

    /// Places the button at the card's end, when there is one, and the words in the room left.
    private func placeEnd() {
        let bounds = card.bounds
        var end = bounds.width - Self.inset
        if !revealButton.isHidden {
            let buttonWidth = revealButton.width
            revealButton.frame = CGRect(
                x: bounds.width - Self.buttonMargin - buttonWidth, y: Self.buttonMargin, width: buttonWidth, height: RowButton.metrics.height
            )
            end = revealButton.frame.minX - Self.inset
        }
        let x = tile.frame.maxX + Self.inset - 4
        let top = ((bounds.height - Self.lineHeight * 2 - 2) / 2).rounded()
        name.frame = CGRect(x: x, y: top, width: max(0, end - x), height: Self.lineHeight)
        detail.frame = CGRect(x: x, y: top + Self.lineHeight + 2, width: max(0, end - x), height: Self.lineHeight)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        guard bounds.contains(point) else { return false }
        if isFetching { return tile.frame.contains(point) }
        return revealButton.isHidden || !revealButton.frame.contains(point)
    }
}
