#if os(iOS)
import SwiftUI
import UIKit

/// What the viewer and what shows it tell each other.
@Observable
final class ViewerPresentation {
    /// It is opening, pulled down or closing, which fades its buttons.
    var closing = true
    /// A pull doesn't close it: its picture is zoomed in or marked up.
    var holds = false
    /// Its pages show: not while its picture flies out of the one it was opened from.
    var pagesShown = true
    /// What it shows, which it closes into.
    var item: ViewedMedia?
}

/// Shows the viewer over everything as Photos does: its picture grows out of the one it was
/// opened from while the black fades in, and flies back into the one it shows as it closes. A pull
/// down moves and shrinks the picture under the finger and fades the black. The picture starts
/// growing in the frame after the tap, while the viewer is still being made under it.
struct MediaViewerPresenter: UIViewControllerRepresentable {
    let store: AppStore
    let open: Bool

    func makeCoordinator() -> Coordinator { Coordinator(store: store) }

    func makeUIViewController(context: Context) -> UIViewController {
        let anchor = UIViewController()
        anchor.view.isUserInteractionEnabled = false
        return anchor
    }

    func updateUIViewController(_ anchor: UIViewController, context: Context) {
        let coordinator = context.coordinator
        coordinator.anchor = anchor
        DispatchQueue.main.async {
            coordinator.warmUp()
            coordinator.follow()
        }
    }

    @MainActor
    final class Coordinator {
        let store: AppStore
        weak var anchor: UIViewController?
        private weak var shown: ViewerController?
        private var warmed = false

        init(store: AppStore) {
            self.store = store
            store.opensViewer = { [weak self] in self?.follow() }
        }

        /// Shows the viewer while the store has one open, and takes it away once it has none.
        func follow() {
            guard let viewing = store.viewing else {
                shown?.close()
                return
            }
            guard shown == nil, let window = anchor?.view.window, let presenter = topmost() else { return }
            let flight = ViewerFlight(item: viewing.item, in: window)
            let controller = ViewerController(store: store, viewing: viewing, flight: flight)
            controller.onClosed = { [weak self] in
                self?.shown = nil
                self?.follow()
            }
            shown = controller
            presenter.present(controller, animated: false) { controller.open() }
        }

        /// Makes a viewer once, off the screen, as the client starts, so that what the first one
        /// costs isn't paid when an image is opened.
        func warmUp() {
            guard !warmed, let window = anchor?.view.window, let scene = window.windowScene else { return }
            warmed = true
            let spare = UIWindow(windowScene: scene)
            spare.frame = window.bounds.offsetBy(dx: window.bounds.width * 2, dy: 0)
            spare.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue - 1)
            spare.isUserInteractionEnabled = false
            let nothing = ViewedMedia(name: "", video: false, source: .file(URL(fileURLWithPath: "/dev/null")))
            spare.rootViewController = ViewerController(store: store, viewing: Viewing(items: [nothing], index: 0), flight: nil)
            spare.isHidden = false
            spare.layoutIfNeeded()
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                spare.isHidden = true
                spare.rootViewController = nil
            }
        }

        private func topmost() -> UIViewController? {
            var top = anchor?.view.window?.rootViewController
            while let presented = top?.presentedViewController, !presented.isBeingDismissed { top = presented }
            return top
        }
    }
}

/// The viewer on its black. The picture and the buttons lie on a stage, which is moved, scaled
/// and clipped to the picture it opens from or closes into, while the black stays put and fades.
/// It moves as Telegram's viewer does, and fades and shrinks with a pull as Photos does.
private final class ViewerController: UIViewController, UIGestureRecognizerDelegate {
    /// How long the picture takes to fly out of its thumbnail or back into it.
    static let duration: TimeInterval = 0.25
    /// Of the screen's height, how far a pull goes before the black starts to fade, and how much
    /// further until it is gone.
    private static let fadeStart: CGFloat = 0.06
    private static let fadeLength: CGFloat = 0.47
    /// How much smaller the picture gets for a pull the screen's height long.
    private static let shrink: CGFloat = 0.5
    /// Let go after a pull longer than this share of the screen's height, or faster than this many
    /// points a second, the viewer closes.
    private static let closeDistance: CGFloat = 1 / 12
    private static let closeSpeed: CGFloat = 1000

    let presentation = ViewerPresentation()
    var onClosed: () -> Void = {}
    private let store: AppStore
    private let hosting: UIHostingController<AnyView>
    private let backdrop = UIView()
    private let stage = UIView()
    /// Clips the stage to the picture opened from or closed into while it zooms.
    private let clip = UIView()
    /// The picture opened from or pulled toward, hidden while the viewer's picture is over it.
    private weak var source: UIView?
    private var closing = false
    /// Which way the last pull went: down is positive.
    private var pulledDown = true
    /// The picture growing out of the one it was opened from, until the viewer shows it.
    private var flight: ViewerFlight?

    init(store: AppStore, viewing: Viewing, flight: ViewerFlight?) {
        self.store = store
        self.flight = flight
        presentation.item = viewing.item
        hosting = UIHostingController(rootView: AnyView(ShownViewer(first: viewing).environment(store).environment(presentation)))
        super.init(nibName: nil, bundle: nil)
        // Over the client, which stays in the window and so goes on following the store.
        modalPresentationStyle = .overFullScreen
        modalPresentationCapturesStatusBarAppearance = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    override func viewDidLoad() {
        super.viewDidLoad()
        backdrop.backgroundColor = Theme.overlay
        clip.backgroundColor = .black
        for shown in [backdrop, stage, hosting.view!] {
            shown.frame = view.bounds
            shown.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        }
        hosting.view.backgroundColor = .clear
        view.addSubview(backdrop)
        view.addSubview(stage)
        addChild(hosting)
        stage.addSubview(hosting.view)
        hosting.didMove(toParent: self)
        let pull = UIPanGestureRecognizer(target: self, action: #selector(pulled(_:)))
        pull.delegate = self
        view.addGestureRecognizer(pull)
    }

    override var childForStatusBarHidden: UIViewController? { hosting }
    override var childForStatusBarStyle: UIViewController? { hosting }
    override var childForHomeIndicatorAutoHidden: UIViewController? { hosting }

    /// Shows nothing until the page has been laid out, which `open` zooms out of the picture.
    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        guard isBeingPresented else { return }
        backdrop.alpha = 0
        stage.alpha = 0
    }

    /// Shows the viewer in place of the picture that grew out of the one it was opened from, once
    /// that has landed and the viewer shows its picture; without it, the viewer fades in.
    func open() {
        view.layoutIfNeeded()
        guard let flight else {
            return UIView.animate(withDuration: Self.duration) { [self] in
                backdrop.alpha = 1
                stage.alpha = 1
            } completion: { [self] _ in presentation.closing = false }
        }
        // The buttons fade in as the picture flies; the pages wait under it until it has landed.
        presentation.pagesShown = false
        stage.alpha = 1
        presentation.closing = false
        let started = CACurrentMediaTime()
        EveryFrame.until { [weak self] in
            guard let self, !self.closing else { return true }
            guard flight.landed else { return false }
            self.view.layoutIfNeeded()
            if let shown = self.laidOutPicture {
                flight.settle(on: shown) { self.takeOver(from: flight) }
                return true
            }
            guard CACurrentMediaTime() - started > 1 else { return false }
            self.takeOver(from: flight)
            return true
        }
    }

    private func takeOver(from flight: ViewerFlight) {
        guard !closing else { return }
        let checkers = checkers
        UIView.performWithoutAnimation {
            checkers.forEach { $0.alpha = 0 }
            backdrop.alpha = 1
        }
        presentation.pagesShown = true
        flight.end()
        self.flight = nil
        UIView.animate(withDuration: 0.2) { checkers.forEach { $0.alpha = 1 } }
    }

    /// Where the page shown draws its picture, once it has laid it out in the middle of the screen,
    /// or a video's, the whole screen.
    private var laidOutPicture: CGRect? {
        guard let item = shownItem, !item.video else { return view.bounds }
        let center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        let pages = Self.views(ZoomingScrollView.self, in: stage)
        guard let page = pages.first(where: { $0.window != nil && $0.convert($0.bounds, to: view).contains(center) }), page.showsPicture else { return nil }
        let picture = page.pictureView.convert(page.pictureView.bounds, to: view)
        guard abs(picture.midX - center.x) < 1, abs(picture.midY - center.y) < 1 else { return nil }
        return picture
    }

    /// Flies the picture into the one it shows in the thread, from wherever a pull has taken it,
    /// as the black fades out; without one on screen, the picture slides off the screen the way it
    /// was pulled.
    func close() {
        guard !closing else { return }
        closing = true
        presentation.closing = true
        if let flight {
            UIView.performWithoutAnimation { backdrop.alpha = 1 }
            presentation.pagesShown = true
            flight.end()
            self.flight = nil
        }
        let source = shownItem.flatMap(MediaSources.source(of:))
        let zoom = zoom(into: source)
        if zoom != nil {
            clip.frame = stage.bounds
            clip.layer.cornerRadius = 0
            stage.mask = clip
            source?.view.alpha = 0
        }
        let checkers = checkers
        UIView.animate(withDuration: 0.2, delay: 0, options: .curveLinear) { [self] in
            backdrop.alpha = 0
            checkers.forEach { $0.alpha = 0 }
        }
        let away = stage.transform.translatedBy(x: 0, y: (pulledDown ? 1 : -1) * view.bounds.height / stage.transform.a)
        let animator = zoom == nil
            ? UIViewPropertyAnimator(duration: 0.2, curve: .linear) { [self] in stage.transform = away }
            : UIViewPropertyAnimator(duration: Self.duration, dampingRatio: 0.9) { [self] in zoom.map(place) }
        animator.addCompletion { [self] _ in
            source?.view.alpha = 1
            dismiss(animated: false) { [self] in
                if store.viewing != nil { store.closeViewer() }
                onClosed()
            }
        }
        animator.startAnimation()
    }

    /// Moves the picture with the finger, shrinking it and fading the black the further it goes;
    /// let go, it closes when pulled far or fast enough, and otherwise springs back.
    @objc private func pulled(_ pull: UIPanGestureRecognizer) {
        guard !closing else { return }
        let moved = pull.translation(in: view)
        let height = max(1, view.bounds.height)
        let distance = abs(moved.y) / height
        switch pull.state {
        case .began:
            presentation.closing = true
            source = shownItem.flatMap(MediaSources.source(of:))?.view
            source?.alpha = 0
            let checkers = checkers
            UIView.animate(withDuration: 0.2) { checkers.forEach { $0.alpha = 0 } }
        case .changed:
            let scale = max(0.5, 1 - distance * Self.shrink)
            stage.transform = CGAffineTransform(translationX: moved.x, y: moved.y).scaledBy(x: scale, y: scale)
            backdrop.alpha = 1 - min(1, max(0, (distance - Self.fadeStart) / Self.fadeLength))
        default:
            pulledDown = moved.y >= 0
            let speed = abs(pull.velocity(in: view).y)
            let closes = pull.state == .ended && (distance > Self.closeDistance || speed > Self.closeSpeed)
            guard !closes else { return store.closeViewer() }
            let checkers = checkers
            let animator = UIViewPropertyAnimator(duration: Self.duration, dampingRatio: 0.9) { [self] in
                stage.transform = .identity
                backdrop.alpha = 1
                checkers.forEach { $0.alpha = 1 }
            }
            animator.addCompletion { [self] _ in source?.alpha = 1 }
            animator.startAnimation()
            presentation.closing = false
        }
    }

    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let pull = gesture as? UIPanGestureRecognizer, !presentation.holds, !closing else { return false }
        let velocity = pull.velocity(in: view)
        return abs(velocity.y) > abs(velocity.x)
    }

    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }

    private var shownItem: ViewedMedia? { store.viewing?.item ?? presentation.item }

    private struct Zoom {
        let transform: CGAffineTransform
        let scale: CGFloat
        let clip: CGRect
        let radius: CGFloat
    }

    /// What lays the stage's picture over `source`: covering it, as the thread's picture does,
    /// clipped to it, with its corners.
    private func zoom(into source: MediaSources.Source?) -> Zoom? {
        guard let source, source.view.window != nil else { return nil }
        let target = source.view.convert(source.frame, to: view)
        let picture = pictureRect(in: stage)
        guard target.width > 0, target.height > 0, picture.width > 0, picture.height > 0 else { return nil }
        let scale = max(target.width / picture.width, target.height / picture.height)
        let shown = CGSize(width: target.width / scale, height: target.height / scale)
        let center = CGPoint(x: stage.bounds.midX, y: stage.bounds.midY)
        let transform = CGAffineTransform(
            translationX: target.midX - center.x - scale * (picture.midX - center.x),
            y: target.midY - center.y - scale * (picture.midY - center.y)
        ).scaledBy(x: scale, y: scale)
        let clip = CGRect(x: picture.midX - shown.width / 2, y: picture.midY - shown.height / 2, width: shown.width, height: shown.height)
        return Zoom(transform: transform, scale: scale, clip: clip, radius: source.radius / scale)
    }

    private func place(_ zoom: Zoom) {
        stage.mask = clip
        stage.transform = zoom.transform
        clip.frame = zoom.clip
        clip.layer.cornerRadius = zoom.radius
        clip.layer.cornerCurve = .continuous
    }

    private var checkers: [UIView] { Self.views(ZoomingScrollView.self, in: stage).map(\.checkerView) }

    /// Where the picture shown lies on the stage: the page's own, or its size fitted to the screen
    /// before the page is laid out.
    private func pictureRect(in stage: UIView) -> CGRect {
        let bounds = stage.bounds
        let center = CGPoint(x: bounds.midX, y: bounds.midY)
        let pages = Self.views(ZoomingScrollView.self, in: stage)
        if let page = pages.first(where: { $0.window != nil && $0.convert($0.bounds, to: stage).contains(center) }) {
            let picture = page.pictureView
            let rect = picture.convert(picture.bounds, to: stage)
            if rect.width > 0, rect.height > 0 { return rect }
        }
        guard let pixels = shownItem?.pixels, pixels.width > 0, pixels.height > 0 else { return bounds }
        let scale = min(bounds.width / pixels.width, bounds.height / pixels.height)
        let size = CGSize(width: pixels.width * scale, height: pixels.height * scale)
        return CGRect(x: center.x - size.width / 2, y: center.y - size.height / 2, width: size.width, height: size.height)
    }

    private static func views<T: UIView>(_ type: T.Type, in view: UIView) -> [T] {
        view.subviews.flatMap { subview in (subview as? T).map { [$0] } ?? views(type, in: subview) }
    }
}

/// The picture of the thumbnail a viewer opens from, flown to where the viewer shows it as the
/// black fades in, over the window. It is only a picture, so it starts in the frame after the tap
/// and goes on moving, on its own, while the viewer is made.
@MainActor
private final class ViewerFlight {
    private(set) var landed = false
    private let backdrop = UIView()
    private let picture = UIImageView()
    private weak var source: UIView?

    init?(item: ViewedMedia, in window: UIWindow) {
        guard let source = MediaSources.source(of: item), let image = source.picture else { return nil }
        let from = source.view.convert(source.frame, to: window)
        guard from.width > 0, from.height > 0 else { return nil }
        self.source = source.view
        backdrop.frame = window.bounds
        backdrop.backgroundColor = Theme.overlay
        backdrop.alpha = 0
        picture.image = UIImage(cgImage: image)
        picture.contentMode = .scaleAspectFill
        picture.clipsToBounds = true
        picture.frame = from
        picture.layer.cornerRadius = source.radius
        picture.layer.cornerCurve = .continuous
        window.addSubview(backdrop)
        window.addSubview(picture)
        source.view.alpha = 0
        let to = Self.shown(item, picture: CGSize(width: image.width, height: image.height), in: window.bounds)
        UIView.animate(withDuration: ViewerController.duration) { [backdrop] in backdrop.alpha = 1 }
        let animator = UIViewPropertyAnimator(duration: ViewerController.duration, dampingRatio: 0.9) { [picture] in
            picture.frame = to
            picture.layer.cornerRadius = 0
        }
        animator.addCompletion { [weak self] _ in self?.landed = true }
        animator.startAnimation()
        // Hands the animations over now, before the viewer is made on this thread.
        CATransaction.flush()
    }

    /// Moves the picture to where the viewer shows it, when that isn't where it landed.
    func settle(on frame: CGRect, then done: @escaping () -> Void) {
        let landed = picture.frame
        guard abs(landed.minX - frame.minX) > 0.5 || abs(landed.minY - frame.minY) > 0.5 || abs(landed.width - frame.width) > 0.5 else { return done() }
        let animator = UIViewPropertyAnimator(duration: 0.15, dampingRatio: 1) { [picture] in picture.frame = frame }
        animator.addCompletion { _ in done() }
        animator.startAnimation()
    }

    func end() {
        backdrop.removeFromSuperview()
        picture.removeFromSuperview()
        source?.alpha = 1
    }

    /// Where the viewer shows the picture, as its page fits it: an SVG up to the size its row shows
    /// it at, another picture up to twice its size in points, without its size as large as fits.
    static func shown(_ item: ViewedMedia, picture: CGSize, in bounds: CGRect) -> CGRect {
        var largest = ZoomingScrollView.viewerLargestFit
        let size: CGSize
        if case .media(let id) = item.source, let svg = Pictures.svgSize(id), svg.width > 0 {
            size = svg
            if let drawn = item.drawnSize { largest = max(1, drawn.width / svg.width) }
        } else if let pixels = item.pixels, pixels.width > 0, pixels.height > 0 {
            size = CGSize(width: pixels.width / Platform.pixelsPerPoint, height: pixels.height / Platform.pixelsPerPoint)
        } else {
            return MediaSources.fitted(picture, in: bounds)
        }
        let scale = min(largest, bounds.width / size.width, bounds.height / size.height)
        let shown = CGSize(width: size.width * scale, height: size.height * scale)
        return CGRect(x: bounds.midX - shown.width / 2, y: bounds.midY - shown.height / 2, width: shown.width, height: shown.height)
    }
}

/// Calls `step` on every frame until it says it is done.
private final class EveryFrame: NSObject {
    private let step: () -> Bool

    private init(_ step: @escaping () -> Bool) {
        self.step = step
    }

    static func until(_ step: @escaping () -> Bool) {
        CADisplayLink(target: EveryFrame(step), selector: #selector(tick(_:))).add(to: .main, forMode: .common)
    }

    @objc private func tick(_ link: CADisplayLink) {
        if step() { link.invalidate() }
    }
}

/// The viewer of what the store has open, or of what it had while it closes.
private struct ShownViewer: View {
    @Environment(AppStore.self) private var store
    @Environment(ViewerPresentation.self) private var presentation
    @State private var last: Viewing

    init(first: Viewing) {
        _last = State(initialValue: first)
    }

    var body: some View {
        MediaViewer(viewing: store.viewing ?? last)
            .onChange(of: store.viewing) {
                guard let viewing = store.viewing else { return }
                last = viewing
                presentation.item = viewing.item
            }
    }
}
#endif
