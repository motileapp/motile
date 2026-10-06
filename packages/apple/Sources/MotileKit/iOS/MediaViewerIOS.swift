#if os(iOS)
import AVKit
import SwiftUI
import UIKit

/// The images and videos of a message or of the composer, one at a time over the whole screen.
/// A swipe goes through them, a pinch or a double tap zooms an image, and a pull down closes it.
struct MediaViewer: View {
    @Environment(AppStore.self) private var store
    let viewing: Viewing
    @State private var pulled: CGFloat = 0
    @State private var zoomed = false
    @State private var shared: URL?

    var body: some View {
        let item = viewing.item
        ZStack {
            Color.black
                .opacity(1 - min(0.6, Double(abs(pulled)) / 500))
                .ignoresSafeArea()
            TabView(selection: index) {
                ForEach(Array(viewing.items.enumerated()), id: \.offset) { position, item in
                    MediaPage(item: item, shown: position == viewing.index, zoomed: $zoomed, file: position == viewing.index ? $shared : .constant(nil))
                        .tag(position)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
            .offset(y: pulled)
        }
        .overlay(alignment: .top) { bar(item) }
        .simultaneousGesture(pull)
        .statusBarHidden()
        .persistentSystemOverlays(.hidden)
    }

    private var index: Binding<Int> {
        Binding {
            viewing.index
        } set: { new in
            shared = nil
            store.viewNext(new - viewing.index)
        }
    }

    /// Pulling the picture down closes the viewer, unless the picture is zoomed and the pull moves it.
    private var pull: some Gesture {
        DragGesture(minimumDistance: 24)
            .onChanged { drag in
                guard !zoomed, abs(drag.translation.height) > abs(drag.translation.width) else { return }
                pulled = max(0, drag.translation.height)
            }
            .onEnded { drag in
                guard pulled > 110 || drag.predictedEndTranslation.height > 400, !zoomed else {
                    return withAnimation(.spring(duration: 0.3)) { pulled = 0 }
                }
                store.closeViewer()
            }
    }

    private func bar(_ item: ViewedMedia) -> some View {
        HStack(spacing: 8) {
            button(.x, label: "Close") { store.closeViewer() }
            Color.clear.frame(width: 40, height: 40)
            Spacer(minLength: 8)
            VStack(spacing: 1) {
                Text(item.name)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if viewing.items.count > 1 {
                    Text("\(viewing.index + 1) of \(viewing.items.count)")
                        .font(.system(size: 12))
                        .foregroundStyle(.white.opacity(0.6))
                }
            }
            .foregroundStyle(.white)
            Spacer(minLength: 8)
            if let shared {
                button(.copy, label: item.video ? "Copy Video" : "Copy Image") {
                    MediaFiles.copy(shared, video: item.video, named: item.name)
                }
                ShareLink(item: shared) { symbol(.share) }
            } else {
                Color.clear.frame(width: 88, height: 40)
            }
        }
        .padding(.horizontal, 14)
        .padding(.top, 6)
        .opacity(pulled > 0 ? 0 : 1)
    }

    private func button(_ name: Symbol, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { symbol(name) }
            .accessibilityLabel(label)
    }

    private func symbol(_ name: Symbol) -> some View {
        Image(name, size: 13)
            .foregroundStyle(.white)
            .frame(width: 40, height: 40)
            .background(.white.opacity(0.16), in: Circle())
            .contentShape(Circle())
    }
}

/// One image or video of the viewer, fetched when it is the one shown.
private struct MediaPage: View {
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    @Environment(AppStore.self) private var store
    let item: ViewedMedia
    let shown: Bool
    @Binding var zoomed: Bool
    /// The file, once it is here, for the viewer to share.
    @Binding var file: URL?
    @State private var loaded: Loaded?
    @State private var failed = false
    @State private var fraction: Double?

    private enum Loaded {
        case image(CGImage, scale: CGFloat)
        case video(AVPlayer)
        /// A video the system's player can't play.
        case other(URL)
    }

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .task(id: shown) {
                guard shown else { return pause() }
                guard loaded == nil else { return play() }
                load()
            }
            .onReceive(NotificationCenter.default.publisher(for: .mediaProgress)) { note in
                guard case .media(let id) = item.source, note.userInfo?["id"] as? String == id else { return }
                fraction = note.userInfo?["fraction"] as? Double
            }
            .onDisappear(perform: pause)
    }

    @ViewBuilder private var content: some View {
        switch loaded {
        case .image(let image, let scale):
            ZoomableImage(
                image: image,
                size: CGSize(width: CGFloat(image.width) / scale, height: CGFloat(image.height) / scale),
                margin: .zero,
                onZoom: { zoomed = $0 }
            ) { store.closeViewer() }
            .ignoresSafeArea()
        case .video(let player):
            PlayerView(player: player)
                .padding(.top, 60)
        case .other(let url):
            VStack(spacing: 14) {
                Text("This video can't be played here.")
                ShareLink(item: url) {
                    ControlLabel(title: "Open in Another App", icon: nil, size: .large)
                }
                .buttonStyle(.control(.primary, size: .large))
            }
            .font(.system(size: 15))
            .foregroundStyle(.white.opacity(0.8))
        case nil:
            Group {
                if failed {
                    Text(item.video ? "This video couldn't be loaded." : "This image couldn't be loaded.")
                } else if let fraction {
                    Text("Downloading \(item.name) · \(Int(fraction * 100))%").monospacedDigit()
                } else {
                    Spinner(size: ControlSize.large.symbol)
                }
            }
            .font(.system(size: 15))
            .foregroundStyle(.white.opacity(0.7))
        }
    }

    private func play() {
        guard case .video(let player) = loaded else { return }
        player.play()
    }

    private func pause() {
        guard case .video(let player) = loaded else { return }
        player.pause()
    }

    private func load() {
        (failed, fraction) = (false, nil)
        let show = { (url: URL?) in
            guard let url else { return failed = true }
            file = url
            guard item.video else {
                let scale = Platform.pixelsPerPoint
                return Pictures.decode(url, id: "view:\(url.path)", maxPixels: 6144) { image in
                    loaded = image.map { .image($0, scale: scale) }
                    failed = image == nil
                }
            }
            guard Self.playedHere.contains(url.pathExtension.lowercased()) else { return loaded = .other(url) }
            let player = AVPlayer(url: url)
            loaded = .video(player)
            if shown { player.play() }
        }
        switch item.source {
        case .file(let url): show(url)
        case .media(let id): store.media(id, done: show)
        }
    }
}

/// A `CGImage` or a `UIImage`, at `size` in points when it isn't zoomed.
struct ZoomableImage: UIViewRepresentable {
    let image: AnyObject
    let size: CGSize
    let margin: CGSize
    var radius: CGFloat = 0
    /// The Mac's zoom keys. A screen is zoomed with fingers.
    var keys = true
    var onZoom: (Bool) -> Void = { _ in }
    var clickedBeside: () -> Void = {}

    func makeUIView(context: Context) -> ZoomingScrollView {
        ZoomingScrollView(margin: margin, radius: radius)
    }

    func updateUIView(_ view: ZoomingScrollView, context: Context) {
        view.onZoom = onZoom
        view.tappedBeside = clickedBeside
        view.show(image, size: size)
    }
}

/// An image that fits the screen until it is zoomed: by pinching or a double tap. Zoomed, it is
/// moved by dragging.
final class ZoomingScrollView: UIScrollView, UIScrollViewDelegate {
    var onZoom: (Bool) -> Void = { _ in }
    var tappedBeside: () -> Void = {}
    private let margin: CGSize
    private let picture = UIImageView()
    private var shown: AnyObject?
    private var laidOut = CGSize.zero

    init(margin: CGSize, radius: CGFloat) {
        self.margin = margin
        super.init(frame: .zero)
        delegate = self
        showsVerticalScrollIndicator = false
        showsHorizontalScrollIndicator = false
        contentInsetAdjustmentBehavior = .never
        decelerationRate = .fast
        picture.layer.cornerRadius = radius
        picture.layer.cornerCurve = .continuous
        picture.layer.masksToBounds = radius > 0
        picture.layer.minificationFilter = .trilinear
        addSubview(picture)
        let double = UITapGestureRecognizer(target: self, action: #selector(doubleTapped(_:)))
        double.numberOfTapsRequired = 2
        addGestureRecognizer(double)
        let single = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
        single.require(toFail: double)
        addGestureRecognizer(single)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var fit: CGFloat {
        let size = picture.bounds.size
        guard size.width > 0, size.height > 0, bounds.width > 0 else { return 1 }
        let width = (bounds.width - 2 * margin.width) / size.width
        let height = (bounds.height - 2 * margin.height) / size.height
        return max(0.01, min(1, width, height))
    }

    func show(_ image: AnyObject, size: CGSize) {
        guard shown !== image else { return }
        shown = image
        zoomScale = 1
        picture.image = (image as? UIImage) ?? UIImage(cgImage: image as! CGImage)
        picture.frame = CGRect(origin: .zero, size: size)
        contentSize = size
        refit(reset: true)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if bounds.size != laidOut {
            laidOut = bounds.size
            refit(reset: zoomScale <= minimumZoomScale + 0.001)
        }
        center()
    }

    private func refit(reset: Bool) {
        guard bounds.width > 0 else { return }
        minimumZoomScale = fit
        maximumZoomScale = max(4, fit * 8)
        if reset || zoomScale < fit { zoomScale = fit }
        center()
    }

    /// Keeps a picture smaller than the screen in its middle.
    private func center() {
        let across = max(0, (bounds.width - contentSize.width) / 2)
        let down = max(0, (bounds.height - contentSize.height) / 2)
        contentInset = UIEdgeInsets(top: down, left: across, bottom: down, right: across)
    }

    func viewForZooming(in scrollView: UIScrollView) -> UIView? { picture }

    func scrollViewDidZoom(_ scrollView: UIScrollView) {
        center()
        onZoom(zoomScale > minimumZoomScale + 0.001)
    }

    @objc private func doubleTapped(_ recognizer: UITapGestureRecognizer) {
        guard zoomScale <= minimumZoomScale + 0.001 else { return setZoomScale(minimumZoomScale, animated: true) }
        let point = recognizer.location(in: picture)
        let scale = min(maximumZoomScale, max(minimumZoomScale * 2.5, 1))
        let size = CGSize(width: bounds.width / scale, height: bounds.height / scale)
        zoom(to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2, width: size.width, height: size.height), animated: true)
    }

    @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
        guard !picture.frame.contains(recognizer.location(in: self)) else { return }
        tappedBeside()
    }
}

/// The system's player with its controls and its full-screen button.
private struct PlayerView: UIViewControllerRepresentable {
    let player: AVPlayer

    func makeUIViewController(context: Context) -> AVPlayerViewController {
        let controller = AVPlayerViewController()
        controller.view.backgroundColor = .clear
        controller.player = player
        return controller
    }

    func updateUIViewController(_ controller: AVPlayerViewController, context: Context) {
        guard controller.player !== player else { return }
        controller.player = player
    }
}
#endif
