import SwiftUI

/// The pictures on screen that the viewer's images and videos open from, for the viewer to zoom
/// out of one and back into the one it shows as it closes.
@MainActor
enum MediaSources {
    private final class Mark {
        let key: String
        let picture: CGImage?
        let fills: Bool
        let radius: CGFloat

        init(key: String, picture: CGImage?, fills: Bool, radius: CGFloat) {
            (self.key, self.picture, self.fills, self.radius) = (key, picture, fills, radius)
        }
    }

    private static let shown = NSMapTable<PlatformView, Mark>.weakToStrongObjects()
    /// The picture last pressed, which is the one a viewer opened from when several show its file.
    private static weak var pressed: PlatformView?

    static func press(_ view: PlatformView) {
        pressed = view
    }

    /// Marks the view as the picture of the image or video, or of none. A `PictureView` says what it
    /// draws itself; another view is given its picture, which it crops to fill it or fits in it.
    static func mark(_ view: PlatformView, as media: ViewedMedia?, picture: CGImage? = nil, fills: Bool = false, radius: CGFloat = 0) {
        guard let media else { return shown.removeObject(forKey: view) }
        shown.setObject(Mark(key: media.sourceKey, picture: picture, fills: fills, radius: radius), forKey: view)
    }

    #if os(iOS)
    /// A picture on screen of an image or a video: its view, what it draws, and where and with what
    /// corners, in the view.
    struct Source {
        let view: UIView
        let picture: CGImage?
        let frame: CGRect
        let radius: CGFloat
    }

    /// The picture of it that is on screen.
    static func source(of media: ViewedMedia) -> Source? {
        let views = [pressed].compactMap { $0 } + shown.keyEnumerator().allObjects.compactMap { $0 as? UIView }
        let view = views.first { view in
            guard shown.object(forKey: view)?.key == media.sourceKey, !view.isHidden, let window = view.window else { return false }
            return view.convert(view.bounds, to: window).intersects(window.bounds)
        }
        guard let view, let mark = shown.object(forKey: view) else { return nil }
        if let box = view as? PictureView {
            let frame = box.shownFrame
            return Source(view: view, picture: box.picture, frame: frame, radius: frame == box.bounds ? box.radius : 0)
        }
        let size = mark.picture.map { CGSize(width: $0.width, height: $0.height) }
        let frame = mark.fills ? view.bounds : (size.map { fitted($0, in: view.bounds) } ?? view.bounds)
        return Source(view: view, picture: mark.picture, frame: frame, radius: mark.radius)
    }
    #endif

    /// The size, scaled to fit in the box, in its middle.
    static func fitted(_ size: CGSize, in box: CGRect) -> CGRect {
        guard size.width > 0, size.height > 0 else { return box }
        let scale = min(box.width / size.width, box.height / size.height)
        let shown = CGSize(width: size.width * scale, height: size.height * scale)
        return CGRect(x: box.midX - shown.width / 2, y: box.midY - shown.height / 2, width: shown.width, height: shown.height)
    }
}

extension ViewedMedia {
    /// What its pictures are found by: the attachment it is, or its file, in its row when it has one.
    var sourceKey: String {
        if let attachment { return "attachment:\(attachment)" }
        let file = switch source {
        case .file(let url): "file:\(url.path)"
        case .media(let id): "media:\(id)"
        }
        return shownIn.map { "\(file)@\($0.row)" } ?? file
    }
}

extension View {
    /// Marks the view as the picture of the image or video, for the viewer to zoom out of and back
    /// into on iOS. It shows `picture` cropped to fill it, or fitted in it.
    @ViewBuilder func mediaSource(_ media: ViewedMedia?, picture: CGImage?, fills: Bool, radius: CGFloat = 0) -> some View {
        #if os(iOS)
        background(MediaSourceMark(media: media, picture: picture, fills: fills, radius: radius))
        #else
        self
        #endif
    }
}

#if os(iOS)
private struct MediaSourceMark: UIViewRepresentable {
    let media: ViewedMedia?
    let picture: CGImage?
    let fills: Bool
    let radius: CGFloat

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.isUserInteractionEnabled = false
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        MediaSources.mark(view, as: media, picture: picture, fills: fills, radius: radius)
    }
}
#endif
