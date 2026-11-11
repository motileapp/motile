#if os(iOS)
import AVKit
import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// The images and videos of a message or of the composer, one at a time over the whole screen,
/// with its name and a close button on glass at the top left and copy and share at the bottom right. A
/// swipe goes through them, a pinch or a double tap zooms an image, a tap hides or shows the
/// buttons, and a pull down closes it. An image can be marked up, to put in the composer: one finger
/// draws, two move and zoom it.
struct MediaViewer: View {
    private static let barGap: CGFloat = 6

    @Environment(AppStore.self) private var store
    let viewing: Viewing
    @State private var pulled: CGFloat = 0
    @State private var zoomed = false
    @State private var shared: URL?
    @State private var chrome = true
    @State private var copied = false
    /// The image shown, decoded, which marking up starts from.
    @State private var picture: CGImage?
    @State private var markup: MarkupSession?
    /// The image before its marks, when a marked-up attachment is marked up again.
    @State private var markupPicture: CGImage?
    @State private var discarding = false
    @State private var markupFailed = false

    var body: some View {
        let item = viewing.item
        ZStack {
            Color.themeOverlay
                .opacity(max(Opacity.overlayPulled.fixed, 1 - Double(abs(pulled)) / 500))
                .ignoresSafeArea()
            TabView(selection: index) {
                ForEach(Array(viewing.items.enumerated()), id: \.offset) { position, item in
                    let current = position == viewing.index
                    MediaPage(
                        item: item, shown: current, zoomed: $zoomed,
                        file: current ? $shared : .constant(nil), picture: current ? $picture : .constant(nil), room: room,
                        markup: current ? markup : nil, markupPicture: current ? markupPicture : nil,
                        controls: current && markup != nil ? Self.markupControls : EdgeInsets(), tapped: toggleChrome
                    )
                    .tag(position)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .scrollDisabled(markup != nil)
            .ignoresSafeArea()
            .offset(y: pulled)
        }
        .overlay(alignment: .top) {
            if let markup {
                markupTop(markup)
            } else {
                top(item)
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if markup == nil { bottom(item) }
        }
        .overlay(alignment: .bottom) {
            if let markup {
                VStack(spacing: 10) {
                    if markupFailed {
                        Text("This image couldn't be marked up.")
                            .font(.ui(.sm))
                            .foregroundStyle(Color.themeForegroundOverPicture)
                    }
                    MarkupBar(session: markup)
                }
                .padding(.horizontal, 12)
                .padding(.bottom, Self.barGap)
                .appearing(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.snappy(duration: 0.25), value: markup == nil)
        .alert("Discard your marks?", isPresented: $discarding) {
            Button("Discard", role: .destructive) { endMarkup() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The image stays as it was.")
        }
        .onChange(of: viewing.index) { endMarkup() }
        .onChange(of: store.markupStep) { follow(store.markupStep) }
        .onDisappear { markup?.close() }
        .simultaneousGesture(pull)
        .environment(\.colorScheme, .dark)
        .statusBarHidden(!buttonsShown(item))
        .persistentSystemOverlays(.hidden)
        .task(id: copied) {
            guard copied, (try? await Task.sleep(for: .seconds(1.2))) != nil else { return }
            copied = false
        }
    }

    private var index: Binding<Int> {
        Binding {
            viewing.index
        } set: { new in
            (shared, picture) = (nil, nil)
            store.viewNext(new - viewing.index)
        }
    }

    /// What a video keeps clear of, inside the safe area: the buttons.
    private var room: CGFloat { GlassButtonStyle.side + Self.barGap }

    /// The system's player draws its own controls, so a video keeps the buttons.
    private func buttonsShown(_ item: ViewedMedia) -> Bool { chrome || item.video }

    private func toggleChrome() {
        guard markup == nil else { return }
        withAnimation(.easeInOut(duration: 0.3)) { chrome.toggle() }
    }

    /// Pulling the picture down closes the viewer, unless the picture is zoomed and the pull moves it.
    private var pull: some Gesture {
        DragGesture(minimumDistance: 24)
            .onChanged { drag in
                guard markup == nil, !zoomed, abs(drag.translation.height) > abs(drag.translation.width) else { return }
                pulled = max(0, drag.translation.height)
            }
            .onEnded { drag in
                guard markup == nil else { return }
                guard pulled > 110 || drag.predictedEndTranslation.height > 400, !zoomed else {
                    return withAnimation(.spring(duration: 0.3)) { pulled = 0 }
                }
                store.closeViewer()
            }
    }

    private func top(_ item: ViewedMedia) -> some View {
        ZStack {
            title(item)
                .padding(.horizontal, GlassButtonStyle.side + 8)
            Button { store.closeViewer() } label: { Image(.x, size: .xl) }
                .buttonStyle(.glassSymbol)
                .glassButton(in: Circle())
                .accessibilityLabel("Close")
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 16)
        .padding(.top, Self.barGap)
        .shown(pulled == 0 && buttonsShown(item), edge: .top)
    }

    @ViewBuilder private func bottom(_ item: ViewedMedia) -> some View {
        if let shared {
            HStack(spacing: 0) {
                if picture != nil, !item.video {
                    Button { startMarkup() } label: { Image(.pencilLine, size: .xl) }
                        .accessibilityLabel("Mark Up")
                }
                Button {
                    MediaFiles.copy(shared, video: item.video, named: item.name)
                    copied = true
                } label: {
                    Image(copied ? .check : .copy, size: .xl)
                }
                .accessibilityLabel(item.video ? "Copy Video" : "Copy Image")
                ShareLink(item: shared) { Image(.share, size: .xl) }
                    .accessibilityLabel("Share")
            }
            .buttonStyle(.glassSymbol)
            .padding(.horizontal, 4)
            .glassButton(in: Capsule())
            .padding(.trailing, 16)
            .padding(.bottom, Self.barGap)
            .shown(pulled == 0 && buttonsShown(item), edge: .bottom)
        }
    }

    private func markupTop(_ markup: MarkupSession) -> some View {
        HStack {
            Button { cancelMarkup() } label: { Image(.x, size: .xl) }
                .buttonStyle(.glassSymbol)
                .glassButton(in: Circle())
                .accessibilityLabel("Cancel")
            Spacer()
            ActionButton(
                markup.attachment == nil ? "Add to Message" : "Done", variant: .primary, size: .large, pending: markup.finishing,
                round: true, glass: true
            ) { finishMarkup() }
            .disabled(!markup.changed && markup.attachment != nil)
        }
        .padding(.horizontal, 16)
        .padding(.top, Self.barGap)
    }

    /// What the buttons at the top and the bar at the bottom take while an image is marked up, and
    /// a gap.
    private static let markupControls = EdgeInsets(
        top: barGap + GlassButtonStyle.side + 8, leading: 0, bottom: barGap + floatingHeight + 8, trailing: 0)

    private static var markupFolder: URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("motile-attachments/markup", isDirectory: true)
    }

    /// Marks up the image shown, or goes on with the marks it was given in the composer, over the
    /// image before them.
    private func startMarkup() {
        guard let shared, let picture else { return }
        let item = viewing.item
        markupFailed = false
        guard let saved = item.attachment.flatMap({ id in store.attachments.first { $0.id == id }?.markup }) else {
            markup = MarkupSession(source: shared, name: item.name, attachment: item.attachment, width: picture.width, height: picture.height, saved: nil)
            return
        }
        Pictures.decode(saved.original, id: "view:\(saved.original.path)", maxPixels: 6144) { original in
            guard viewing.item == item, let original else { return }
            markupPicture = original
            markup = MarkupSession(
                source: saved.original, name: saved.name, attachment: item.attachment, width: original.width, height: original.height, saved: saved)
        }
    }

    private func cancelMarkup() {
        guard let markup else { return }
        guard !markup.changed else { return discarding = true }
        endMarkup()
    }

    private func endMarkup() {
        markup?.close()
        (markup, markupPicture) = (nil, nil)
    }

    private func finishMarkup() {
        guard let markup, !markup.finishing, markup.changed || markup.attachment == nil else { return }
        markupFailed = false
        markup.finish(into: Self.markupFolder) { result in
            guard let result else { return markupFailed = true }
            store.attachMarkup(result, replacing: markup.attachment)
            endMarkup()
            store.closeViewer()
        }
    }

    /// What `do.sh` asks of the markup.
    private func follow(_ step: String?) {
        guard let step else { return }
        store.markupStep = nil
        let words = step.split(separator: " ").map(String.init)
        let rest = words.dropFirst().joined(separator: " ")
        switch words.first {
        case "start": startMarkup()
        case "tool": markup?.tool = MarkupTool(rawValue: rest) ?? .pen
        case "ink": markup?.ink = rest
        case "inks": markup?.inksOpen = true
        case "draw":
            guard let markup else { return }
            let values = words.dropFirst().compactMap(Double.init)
            let points = stride(from: 0, to: values.count - 1, by: 2).map {
                CGPoint(x: values[$0] * markup.width, y: values[$0 + 1] * markup.height)
            }
            guard let first = points.first, let last = points.last else { return }
            markup.down(at: first, slop: 8)
            points.dropFirst().forEach { markup.drag(to: $0) }
            markup.up(at: last)
        case "note": markup?.note = rest
        case "keep": markup?.deselect()
        case "done": finishMarkup()
        case "cancel": cancelMarkup()
        default: break
        }
    }

    private func title(_ item: ViewedMedia) -> some View {
        VStack(spacing: 1) {
            Text(item.name)
                .font(.ui(.sm, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.middle)
            if viewing.items.count > 1 {
                Text("\(viewing.index + 1) of \(viewing.items.count)")
                    .font(.ui(.xxs))
                    .foregroundStyle(Color.themeMutedForegroundOverPicture)
            }
        }
        .foregroundStyle(Color.themeForegroundOverPicture)
    }
}

private extension View {
    /// Hidden, the buttons fade as they slide off their edge on a spring, as Telegram's do: those on
    /// top out of the screen, those at the bottom by their own height.
    func shown(_ shown: Bool, edge: VerticalEdge) -> some View {
        animation(.easeInOut(duration: 0.3)) { $0.opacity(shown ? 1 : 0) }
            .animation(.spring(duration: 0.4)) {
                $0.visualEffect { content, proxy in
                    let away = edge == .top ? -proxy.frame(in: .global).maxY : proxy.size.height
                    return content.offset(y: shown ? 0 : away)
                }
            }
            .allowsHitTesting(shown)
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
    /// The image, once it is decoded, for the viewer to mark up.
    @Binding var picture: CGImage?
    /// What the viewer's buttons take above and below, for a video to stay clear of.
    let room: CGFloat
    let markup: MarkupSession?
    /// Shown in place of the image while it is marked up again.
    let markupPicture: CGImage?
    /// The room the buttons over it take, which it keeps clear of.
    let controls: EdgeInsets
    let tapped: () -> Void
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
                guard loaded == nil else {
                    if case .image(let image, _) = loaded { picture = image }
                    return play()
                }
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
            let shown = markupPicture ?? image
            ZoomableImage(
                image: shown,
                size: CGSize(width: CGFloat(shown.width) / scale, height: CGFloat(shown.height) / scale),
                margin: .zero,
                largestFit: ZoomingScrollView.viewerLargestFit,
                onZoom: { zoomed = $0 },
                tapped: tapped,
                markup: markup,
                controls: controls
            )
            .ignoresSafeArea()
        case .video(let player):
            PlayerView(player: player)
                .padding(.vertical, room)
        case .other(let url):
            VStack(spacing: 14) {
                Text("This video can't be played here.")
                ShareLink(item: url) {
                    ControlLabel(title: "Open in Another App", icon: nil, size: .large)
                }
                .buttonStyle(.control(.primary, size: .large))
            }
            .font(.ui(.sm))
            .foregroundStyle(Color.themeForegroundOverPicture)
            .tappable(tapped)
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
            .font(.ui(.sm))
            .foregroundStyle(Color.themeMutedForegroundOverPicture)
            .tappable(tapped)
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
                    if shown { picture = image }
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
    /// The most it is enlarged to fit, as a multiple of `size`.
    var largestFit: CGFloat = 1
    /// The Mac's zoom keys. A screen is zoomed with fingers.
    var keys = true
    var onZoom: (Bool) -> Void = { _ in }
    var tapped: () -> Void = {}
    /// Takes one finger to draw marks while it is set.
    var markup: MarkupSession?
    /// The room the viewer's buttons take inside the safe area, and above the keyboard at the
    /// bottom, which the picture keeps clear of.
    var controls = EdgeInsets()

    func makeUIView(context: Context) -> ZoomingScrollView {
        ZoomingScrollView(margin: margin, radius: radius, largestFit: largestFit)
    }

    func updateUIView(_ view: ZoomingScrollView, context: Context) {
        view.onZoom = onZoom
        view.tapped = tapped
        view.show(image, size: size)
        view.markup = markup
        view.controls = UIEdgeInsets(top: controls.top, left: controls.leading, bottom: controls.bottom, right: controls.trailing)
    }
}

/// An image that fits the screen until it is zoomed: by pinching or a double tap. Zoomed, it is
/// moved by dragging, with two fingers while it is marked up, when one finger draws. It fits
/// between what covers its edges, and keeps the pin whose note is written in sight.
final class ZoomingScrollView: UIScrollView, UIScrollViewDelegate, UIGestureRecognizerDelegate {
    /// A small image is enlarged to fit the viewer, up to twice its size.
    static let viewerLargestFit: CGFloat = 2
    var onZoom: (Bool) -> Void = { _ in }
    var tapped: () -> Void = {}
    private let margin: CGSize
    private let largestFit: CGFloat
    private let picture = UIImageView()
    private var shown: AnyObject?
    private var laidOut = CGSize.zero
    /// The scale that fit the last layout.
    private var fitted: CGFloat = 1
    private var taps: [UIGestureRecognizer] = []
    private var canvas: MarkupCanvas?
    private lazy var drawing: OneFinger = {
        let drawing = OneFinger(target: self, action: #selector(drew(_:)))
        drawing.delegate = self
        return drawing
    }()

    var controls = UIEdgeInsets.zero {
        didSet { measureClear() }
    }
    private var keyboard: CGFloat = 0

    /// What covers the edges: the controls, past the safe area or the keyboard.
    private var clear = UIEdgeInsets.zero {
        didSet {
            guard clear != oldValue else { return }
            let wasFitted = abs(zoomScale - fitted) < 0.001
            UIView.animate(withDuration: 0.3) {
                self.refit(reset: wasFitted)
                self.showSelectedPin()
            }
        }
    }

    var markup: MarkupSession? {
        get { canvas?.session }
        set {
            guard newValue !== canvas?.session else { return }
            canvas?.removeFromSuperview()
            canvas = newValue.map(MarkupCanvas.init)
            canvas?.changed = { [weak self] in self?.selectionChanged() }
            let marking = canvas != nil
            panGestureRecognizer.minimumNumberOfTouches = marking ? 2 : 1
            taps.forEach { $0.isEnabled = !marking }
            if let canvas {
                canvas.frame = picture.bounds
                canvas.autoresizingMask = [.flexibleWidth, .flexibleHeight]
                canvas.contentScaleFactor = Platform.pixelsPerPoint * zoomScale
                picture.addSubview(canvas)
                addGestureRecognizer(drawing)
            } else {
                removeGestureRecognizer(drawing)
            }
        }
    }

    init(margin: CGSize, radius: CGFloat, largestFit: CGFloat) {
        self.margin = margin
        self.largestFit = largestFit
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
        let single = UITapGestureRecognizer(target: self, action: #selector(singleTapped))
        single.require(toFail: double)
        addGestureRecognizer(single)
        taps = [double, single]
        let center = NotificationCenter.default
        center.addObserver(self, selector: #selector(keyboardMoved(_:)), name: UIResponder.keyboardWillChangeFrameNotification, object: nil)
        center.addObserver(self, selector: #selector(keyboardMoved(_:)), name: UIResponder.keyboardWillHideNotification, object: nil)
    }

    override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        measureClear()
    }

    @objc private func keyboardMoved(_ note: Notification) {
        let frame = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
        let hides = note.name == UIResponder.keyboardWillHideNotification
        keyboard = hides ? 0 : frame.map { max(0, convert(bounds, to: nil).maxY - $0.minY) } ?? 0
        measureClear()
    }

    private func measureClear() {
        guard controls != .zero else { return clear = .zero }
        clear = UIEdgeInsets(
            top: safeAreaInsets.top + controls.top, left: safeAreaInsets.left + controls.left,
            bottom: max(safeAreaInsets.bottom, keyboard) + controls.bottom, right: safeAreaInsets.right + controls.right)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var fit: CGFloat {
        let size = picture.bounds.size
        guard size.width > 0, size.height > 0, bounds.width > 0 else { return 1 }
        let width = (bounds.width - 2 * margin.width - clear.left - clear.right) / size.width
        let height = (bounds.height - 2 * margin.height - clear.top - clear.bottom) / size.height
        return max(0.01, min(largestFit, width, height))
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
            refit(reset: abs(zoomScale - fitted) < 0.001)
        }
        center()
    }

    private func refit(reset: Bool) {
        guard bounds.width > 0 else { return }
        fitted = fit
        // An image enlarged to fit can still be shown at its real size.
        minimumZoomScale = min(fitted, 1)
        maximumZoomScale = max(4, fitted * 8)
        if reset || zoomScale < minimumZoomScale { zoomScale = fitted }
        center()
    }

    /// Keeps a picture smaller than what is left clear in its middle, and lets a larger one move
    /// out from under what covers the edges.
    private func center() {
        let across = max(0, (bounds.width - clear.left - clear.right - contentSize.width) / 2)
        let down = max(0, (bounds.height - clear.top - clear.bottom - contentSize.height) / 2)
        contentInset = UIEdgeInsets(top: clear.top + down, left: clear.left + across, bottom: clear.bottom + down, right: clear.right + across)
    }

    private var shownPin: Int?

    private func selectionChanged() {
        guard canvas?.session.selected != shownPin else { return }
        shownPin = canvas?.session.selected
        UIView.animate(withDuration: 0.3) { self.showSelectedPin() }
    }

    /// Moves the picture just enough for the selected pin to be clear of what covers the edges.
    private func showSelectedPin() {
        guard let canvas, let pin = canvas.session.selectedPin else { return }
        let center = canvas.convert(canvas.point(CGPoint(x: pin.x, y: pin.y)), to: self)
        let reach = canvas.point(CGPoint(x: pin.radius, y: 0)).x * zoomScale + 16
        let top = contentOffset.y + clear.top
        let bottom = contentOffset.y + bounds.height - clear.bottom
        var offset = contentOffset
        if center.y + reach > bottom {
            offset.y += center.y + reach - bottom
        } else if center.y - reach < top {
            offset.y -= top - (center.y - reach)
        }
        let lowest = -contentInset.top
        let highest = max(lowest, contentSize.height + contentInset.bottom - bounds.height)
        offset.y = min(max(offset.y, lowest), highest)
        contentOffset = offset
    }

    func viewForZooming(in scrollView: UIScrollView) -> UIView? { picture }

    func scrollViewDidZoom(_ scrollView: UIScrollView) {
        center()
        onZoom(zoomScale > fitted + 0.001)
    }

    /// Marks are drawn again at the zoom's sharpness once it settles.
    func scrollViewDidEndZooming(_ scrollView: UIScrollView, with view: UIView?, atScale scale: CGFloat) {
        guard let canvas else { return }
        canvas.contentScaleFactor = Platform.pixelsPerPoint * scale
        canvas.setNeedsDisplay()
    }

    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        recognizer === drawing && other === pinchGestureRecognizer
    }

    /// Where the finger landed, until it moves far enough to draw: a second finger before then
    /// only zooms.
    private var landed: CGPoint?
    private static let drawingSlop: CGFloat = 8

    @objc private func drew(_ recognizer: OneFinger) {
        guard let canvas else { return }
        let at = recognizer.location(in: canvas)
        let session = canvas.session
        switch recognizer.state {
        case .began:
            landed = at
        case .changed:
            if let start = landed {
                guard hypot(at.x - start.x, at.y - start.y) * zoomScale > Self.drawingSlop else { return }
                landed = nil
                session.down(at: canvas.pixel(start), slop: canvas.pixels(12, zoom: zoomScale))
            }
            session.drag(to: canvas.pixel(at))
        case .ended:
            if let start = landed { session.down(at: canvas.pixel(start), slop: canvas.pixels(12, zoom: zoomScale)) }
            landed = nil
            session.up(at: canvas.pixel(at))
        case .cancelled:
            if landed == nil { session.cancel() }
            landed = nil
        default: break
        }
    }

    @objc private func doubleTapped(_ recognizer: UITapGestureRecognizer) {
        guard abs(zoomScale - fitted) < 0.001 else { return setZoomScale(fitted, animated: true) }
        let point = recognizer.location(in: picture)
        let scale = min(maximumZoomScale, max(fitted * 2.5, 1))
        let size = CGSize(width: bounds.width / scale, height: bounds.height / scale)
        zoom(to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2, width: size.width, height: size.height), animated: true)
    }

    @objc private func singleTapped() {
        tapped()
    }
}

/// One finger on the screen, from the moment it lands; a second one ends it, to pinch or pan.
final class OneFinger: UIGestureRecognizer {
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard state == .possible, touches.count == 1, event.allTouches?.count == 1 else {
            state = state == .possible ? .failed : .cancelled
            return
        }
        state = .began
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .changed
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .ended
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .cancelled
    }
}

private extension View {
    func tappable(_ tapped: @escaping () -> Void) -> some View {
        frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .onTapGesture(perform: tapped)
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
