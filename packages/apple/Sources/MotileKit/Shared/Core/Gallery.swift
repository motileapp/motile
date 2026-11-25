import Foundation

/// An image or a video of the open thread, as its Media tab shows it.
struct GalleryMedia: Equatable, Identifiable {
    /// Its place in the list as well: a message can show the same file twice.
    let id: String
    let media: String
    let shownIn: ShownIn
    let name: String
    let video: Bool
    /// The image that stands for a video until it plays.
    let poster: String?
    let pixels: CGSize?
    let at: Date

    init(json: JSON, index: Int) {
        media = json.string("media")
        shownIn = ShownIn(item: json.string("item"), row: json.string("row"))
        id = "\(shownIn.row)/\(media)/\(index)"
        name = json.string("name")
        video = json.bool("video")
        poster = json.optionalString("poster")
        let (width, height) = (json.double("width"), json.double("height"))
        pixels = width > 0 && height > 0 ? CGSize(width: width, height: height) : nil
        at = Date(timeIntervalSince1970: json.double("at"))
    }

    /// The image its square shows: the image itself, or a video's poster.
    var picture: String? { video ? poster : media }

    var viewed: ViewedMedia {
        ViewedMedia(name: name, video: video, source: .media(media), pixels: pixels, shownIn: shownIn)
    }
}

/// The images and videos of one thread.
struct Gallery: Equatable {
    let threadID: String
    let media: [GalleryMedia]
}

extension AppStore {
    func takeGallery(_ event: JSON) -> (() -> Void)? {
        let threadID = event.string("thread_id")
        let media = event.objects("media").enumerated().map { GalleryMedia(json: $1, index: $0) }
        return { [weak self] in
            guard let self, self.transcript.threadID == threadID else { return }
            self.gallery = Gallery(threadID: threadID, media: media)
        }
    }

    /// Has the core send the thread's images and videos, and again whenever one is added.
    func watchGallery(_ threadID: String, _ watch: Bool) {
        if !watch, gallery?.threadID == threadID { gallery = nil }
        core.send("watch_gallery", ["thread_id": threadID, "watch": watch])
    }

    /// Scrolls the open thread to where it shows the image or the video.
    func showInChat(_ shownIn: ShownIn) {
        guard let threadID = transcript.threadID else { return }
        uncoverThread()
        core.send("reveal_row", ["thread_id": threadID, "item_id": shownIn.item, "row_id": shownIn.row])
    }

    /// Takes the viewer and the panel away from over the thread.
    func uncoverThread() {
        if viewing != nil { closeViewer() }
        if rightPanel.isMaximized { rightPanel.toggleMaximized() }
        if rightPanel.coversThread, rightPanel.isOpen { rightPanel.isOpen = false }
    }

    /// Puts the image or the video in the composer, under its name.
    func addToMessage(_ media: String, name: String) {
        fetchMedia(media) { [weak self] fetched in
            guard let self, case .success(let file) = fetched else { return }
            DispatchQueue.global(qos: .userInitiated).async {
                let copy = Self.attachableCopy(of: file, name: name)
                DispatchQueue.main.async {
                    self.uncoverThread()
                    self.attach([copy])
                }
            }
        }
    }

    /// A copy of the file under its name, in a folder of its own, so that two of the same name
    /// are both attached.
    private static func attachableCopy(of file: URL, name: String) -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("motile-attachments/gallery/\(UUID().uuidString)", isDirectory: true)
        let copy = folder.appendingPathComponent(name.isEmpty ? file.lastPathComponent : name)
        guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil,
            (try? FileManager.default.copyItem(at: file, to: copy)) != nil
        else { return file }
        return copy
    }
}
