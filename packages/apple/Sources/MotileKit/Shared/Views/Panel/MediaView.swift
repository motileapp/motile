import SwiftUI

/// The images and videos of the thread, the newest first, in squares that fill the panel's width.
/// They are watched for while the panel is in view.
struct MediaSurface: View {
    @Environment(AppStore.self) private var store
    @Environment(\.panelInView) private var inView
    let threadID: String?

    var body: some View {
        Group {
            if let threadID {
                if let gallery = store.gallery, gallery.threadID == threadID {
                    MediaGrid(media: gallery.media)
                } else {
                    PanelLoading()
                }
            } else {
                PanelMessage(text: "The images and videos of the thread show up here.")
            }
        }
        .onChange(of: watched, initial: true) { old, new in
            if let old, old != new { store.watchGallery(old, false) }
            new.map { store.watchGallery($0, true) }
        }
        .onDisappear { watched.map { store.watchGallery($0, false) } }
    }

    private var watched: String? { inView ? threadID : nil }
}

private struct MediaGrid: View {
    /// How narrow a square gets before a column goes.
    private static let smallestSide: CGFloat = 128
    private static let fewestColumns = 3

    @Environment(AppStore.self) private var store
    let media: [GalleryMedia]

    var body: some View {
        if media.isEmpty {
            PanelMessage(text: "The images and videos of the thread show up here.")
        } else {
            VStack(spacing: 0) {
                PanelBar {
                    Text(summary)
                        .font(.ui(.xs, weight: .medium))
                        .foregroundStyle(Color.themeMutedForeground)
                    Spacer()
                }
                GeometryReader { space in
                    let columns = max(Self.fewestColumns, Int(space.size.width / Self.smallestSide))
                    let side = space.size.width / CGFloat(columns)
                    ScrollView {
                        LazyVGrid(columns: Array(repeating: GridItem(.fixed(side), spacing: 0), count: columns), spacing: 0) {
                            ForEach(Array(media.enumerated()), id: \.element.id) { index, shown in
                                MediaSquare(media: shown, side: side) {
                                    store.view(media.map(\.viewed), at: index)
                                }
                            }
                        }
                        .padding(.bottom, panelEndRoom)
                    }
                }
            }
        }
    }

    private var summary: String {
        let videos = media.filter(\.video).count
        let images = media.count - videos
        let parts = [(images, "image"), (videos, "video")].filter { $0.0 > 0 }
        return parts.map { "\($0.0) \($0.1)\($0.0 == 1 ? "" : "s")" }.joined(separator: " · ")
    }
}

/// One image or video, cropped to a square. The gap around it is only drawn: the square takes
/// clicks up to its neighbours'.
private struct MediaSquare: View {
    private static let gap: CGFloat = 2

    @Environment(AppStore.self) private var store
    let media: GalleryMedia
    let side: CGFloat
    let open: () -> Void
    @State private var image: CGImage?
    @State private var failed = false
    @State private var hovering = false

    var body: some View {
        let shown = side - Self.gap
        ZStack {
            Color.themeBackgroundSecondary
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFill()
                    .frame(width: shown, height: shown)
                    .clipped()
            } else if media.picture == nil || failed {
                Image(media.video ? .play : .image, size: .lg)
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .frame(width: shown, height: shown)
        .clipped()
        .frame(width: side, height: side)
        .button(PictureButtonStyle(hovering: hovering), action: open)
        .overlay {
            if media.video, image != nil {
                // As a reply's video shows it.
                let size = ControlSize.large
                Image(.play, size: size.symbol)
                    .foregroundStyle(Color.themeForeground)
                    .frame(width: size.height, height: size.height)
                    .background(Circle().fill(Color.themeOverlayButton))
                    .overlay(Circle().strokeBorder(Color.themeBorderOverlayButton))
                    .allowsHitTesting(false)
            }
        }
        .onHover { hovering = $0 }
        .contextMenu {
            MenuItem("Show in Chat", symbol: .messageSquareText) { store.showInChat(media.shownIn) }
            MenuItem("Add to Message", symbol: .paperclip) { store.addToMessage(media.media, name: media.name) }
        }
        .tooltip(media.name)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(media.name)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { open() }
        .task(id: decodedPixels) { load() }
    }

    /// How large the picture is decoded on its longer side, for its shorter one to fill the
    /// square: in steps, so that resizing the panel doesn't decode it again at every size.
    private var decodedPixels: CGFloat {
        let step: CGFloat = 64
        let square = (side / step).rounded(.up) * step * Platform.pixelsPerPoint
        guard let pixels = media.pixels, pixels.width > 0, pixels.height > 0 else { return square * 2 }
        let ratio = max(pixels.width, pixels.height) / min(pixels.width, pixels.height)
        return (square * min(ratio, 4)).rounded()
    }

    private func load() {
        guard let picture = media.picture else { return }
        let pixels = decodedPixels
        let key = "\(picture)#square\(Int(pixels))"
        if let cached = Pictures.cached(key) {
            image = cached
            return
        }
        store.fetchMedia(picture) { fetched in
            guard case .success(let file) = fetched else { return failed = true }
            Pictures.decode(file, id: key, maxPixels: pixels) { decoded in
                image = decoded
                failed = decoded == nil
            }
        }
    }
}

/// The square's picture at opacity lit under the pointer and under a finger; the play mark over it stays.
private struct PictureButtonStyle: ButtonStyle {
    let hovering: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(.lit, when: hovering || configuration.isPressed)
    }
}
