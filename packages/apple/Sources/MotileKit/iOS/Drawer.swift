#if os(iOS)
import Observation
import SwiftUI
import UIKit

/// Whether the sidebar is shown beside the thread, which a swipe to the right does on a phone.
@Observable
final class Drawer {
    var isOpen = false
    /// The drawer only opens from the thread itself, not from a screen pushed over it.
    var isEnabled = true
}

/// The sidebar under the thread: swiping right anywhere on the thread slides it aside as a
/// card and shows the sidebar, and a swipe back closes it. A phone's sidebar takes the whole
/// screen; a wider window leaves the card in view, and a tap on it closes too.
struct DrawerView<Sidebar: View, Content: View>: UIViewControllerRepresentable {
    let drawer: Drawer
    @ViewBuilder let sidebar: Sidebar
    @ViewBuilder let content: Content

    func makeUIViewController(context: Context) -> DrawerController {
        let controller = DrawerController(sidebar: UIHostingController(rootView: AnyView(sidebar)), content: UIHostingController(rootView: AnyView(content)))
        controller.onChange = { [drawer] open in
            guard drawer.isOpen != open else { return }
            drawer.isOpen = open
        }
        return controller
    }

    func updateUIViewController(_ controller: DrawerController, context: Context) {
        (controller.sidebar as? UIHostingController<AnyView>)?.rootView = AnyView(sidebar)
        (controller.content as? UIHostingController<AnyView>)?.rootView = AnyView(content)
        controller.isEnabled = drawer.isEnabled
        controller.follow(drawer.isOpen)
    }
}

final class DrawerController: UIViewController, UIGestureRecognizerDelegate {
    /// A window narrower than this gives all of its width to the sidebar.
    private static let fullBelow: CGFloat = 500
    private static let widest: CGFloat = 360
    private static let cardRadius: CGFloat = 44
    /// The name of the recognizers that slide the sidebar's rows aside.
    static let rowSwipe = "row-swipe"

    let sidebar: UIViewController
    let content: UIViewController
    var isEnabled = true
    var onChange: ((Bool) -> Void)?

    private let card = UIView()
    /// Behind the card, since the card clips: it casts the card's shadow on the sidebar.
    private let cardShadow = UIView()
    /// Over the card while the sidebar shows: it dims the thread and takes the tap that closes.
    private let shade = UIControl()
    private let pan = UIPanGestureRecognizer()
    private(set) var isOpen = false
    /// How far the card is aside, from 0 to 1.
    private var progress: CGFloat = 0
    private var progressAtStart: CGFloat = 0

    init(sidebar: UIViewController, content: UIViewController) {
        self.sidebar = sidebar
        self.content = content
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var sidebarWidth: CGFloat {
        view.bounds.width < Self.fullBelow ? view.bounds.width : Self.widest
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Theme.background

        addChild(sidebar)
        sidebar.view.backgroundColor = .clear
        view.addSubview(sidebar.view)
        sidebar.didMove(toParent: self)

        card.clipsToBounds = true
        card.layer.cornerCurve = .continuous
        card.backgroundColor = Theme.background
        cardShadow.isUserInteractionEnabled = false
        cardShadow.layer.shadowColor = UIColor.black.cgColor
        cardShadow.layer.shadowOffset = .zero
        cardShadow.layer.shadowRadius = 28
        paintCardEdges()
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (controller: Self, _: UITraitCollection) in
            controller.paintCardEdges()
        }
        view.addSubview(cardShadow)
        view.addSubview(card)
        addChild(content)
        content.view.backgroundColor = Theme.background
        card.addSubview(content.view)
        content.didMove(toParent: self)

        shade.backgroundColor = Theme.background
        shade.alpha = 0
        shade.isHidden = true
        shade.addAction(UIAction { [weak self] _ in self?.setOpen(false, animated: true) }, for: .touchUpInside)
        card.addSubview(shade)

        pan.addTarget(self, action: #selector(panned(_:)))
        pan.delegate = self
        view.addGestureRecognizer(pan)
    }

    private func paintCardEdges() {
        card.layer.borderColor = Theme.border.resolvedColor(with: traitCollection).cgColor
        cardShadow.layer.shadowOpacity = traitCollection.userInterfaceStyle == .dark ? 0.7 : 0.16
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        place()
    }

    private func place() {
        let bounds = view.bounds
        let width = sidebarWidth
        sidebar.view.frame = CGRect(x: 0, y: 0, width: width, height: bounds.height)
        card.frame = CGRect(x: progress * width, y: 0, width: bounds.width, height: bounds.height)
        let lifted = min(1, max(0, progress * 6))
        card.layer.cornerRadius = lifted * Self.cardRadius
        card.layer.borderWidth = lifted
        content.view.frame = card.bounds
        placeShadow()
        shade.frame = card.bounds
        shade.alpha = 0.55 * progress
        shade.isHidden = progress == 0
        sidebar.view.isHidden = progress == 0
    }

    /// The shadow comes in as the card leaves its place and goes as it leaves the screen, so
    /// neither end of the slide shows it.
    private func placeShadow() {
        if cardShadow.bounds.size != card.bounds.size {
            cardShadow.layer.shadowPath = UIBezierPath(roundedRect: card.bounds, cornerRadius: Self.cardRadius).cgPath
        }
        cardShadow.frame = card.frame
        let onScreen = (view.bounds.width - card.frame.minX) / max(1, view.bounds.width)
        cardShadow.alpha = smoothstep(progress * 4) * smoothstep(onScreen * 4)
    }

    private func smoothstep(_ value: CGFloat) -> CGFloat {
        let clamped = min(1, max(0, value))
        return clamped * clamped * (3 - 2 * clamped)
    }

    /// Opens or closes as the client's state says, unless a finger is moving the card.
    func follow(_ open: Bool) {
        guard pan.state != .began, pan.state != .changed, isOpen != open else { return }
        setOpen(open, animated: true)
    }

    func setOpen(_ open: Bool, animated: Bool, velocity: CGFloat = 0) {
        let target: CGFloat = open ? 1 : 0
        guard isOpen != open || progress != target else { return }
        isOpen = open
        if open { view.endEditing(true) }
        onChange?(open)
        guard animated, view.window != nil else {
            stopSliding()
            progress = target
            return place()
        }
        let distance = abs(target - progress)
        let speed = min(12 * distance, abs(velocity) / sidebarWidth)
        slide = Slide(target: target, offset: progress - target, speed: velocity < 0 ? -speed : speed, start: CACurrentMediaTime())
        guard slideLink == nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(slid(_:)))
        link.add(to: .main, forMode: .common)
        slideLink = link
    }

    /// A critically damped spring, followed frame by frame so that what depends on how far
    /// the card is aside, like its shadow, follows it.
    private struct Slide {
        static let stiffness: CGFloat = 22
        let target: CGFloat
        let offset: CGFloat
        let speed: CGFloat
        let start: CFTimeInterval

        func at(_ time: CFTimeInterval) -> (progress: CGFloat, speed: CGFloat) {
            let t = CGFloat(time - start)
            let k = Self.stiffness
            let decay = exp(-k * t)
            let drift = speed + k * offset
            return (target + (offset + drift * t) * decay, (speed - k * drift * t) * decay)
        }
    }

    private var slide: Slide?
    private var slideLink: CADisplayLink?

    @objc private func slid(_ link: CADisplayLink) {
        guard let slide else { return stopSliding() }
        let now = slide.at(link.targetTimestamp)
        let settled = abs(now.progress - slide.target) < 0.0005 && abs(now.speed) < 0.01
        progress = settled ? slide.target : now.progress
        place()
        guard settled else { return }
        stopSliding()
    }

    private func stopSliding() {
        slideLink?.invalidate()
        slideLink = nil
        slide = nil
    }

    @objc private func panned(_ recognizer: UIPanGestureRecognizer) {
        let moved = recognizer.translation(in: view).x
        switch recognizer.state {
        case .began:
            stopSliding()
            progressAtStart = progress
            view.endEditing(true)
        case .changed:
            let wanted = progressAtStart + moved / sidebarWidth
            // Past its ends the card gives a little and no more.
            let over = wanted > 1 ? wanted - 1 : wanted < 0 ? wanted : 0
            progress = min(1, max(0, wanted)) + over * 0.12
            place()
        case .ended, .cancelled:
            let speed = recognizer.velocity(in: view).x
            let open = abs(speed) > 280 ? speed > 0 : progress > 0.5
            setOpen(open, animated: true, velocity: speed)
        default:
            break
        }
    }

    // MARK: Whose swipe it is

    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        guard isEnabled || isOpen else { return false }
        let speed = pan.velocity(in: view)
        guard abs(speed.x) > abs(speed.y) * 1.3 else { return false }
        // Opened, only the swipe back is the drawer's: one to the right is the sidebar's rows'.
        guard !isOpen else { return speed.x < 0 }
        guard speed.x > 0 else { return false }
        return !scrollsSideways(under: pan.location(in: view))
    }

    /// Whether what the finger is on has a swipe to the right of its own: code or a table that
    /// was scrolled sideways and scrolls back, or text whose selection is being dragged.
    private func scrollsSideways(under point: CGPoint) -> Bool {
        var view = self.view.hitTest(point, with: nil)
        while let current = view, current !== self.view {
            if let text = current as? UITextView, text.selectedRange.length > 0 { return true }
            if let scroll = current as? UIScrollView, scroll.isScrollEnabled,
                scroll.contentSize.width > scroll.bounds.width + 1,
                scroll.contentOffset.x > -scroll.adjustedContentInset.left + 1
            {
                return true
            }
            view = current.superview
        }
        return false
    }

    /// A row of the sidebar that a swipe slides aside has the swipe first.
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRequireFailureOf other: UIGestureRecognizer) -> Bool {
        other.name == Self.rowSwipe
    }

    /// The scroll views wait to hear that this swipe isn't the drawer's, which they do the
    /// moment it sets out up or down.
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        other is UIPanGestureRecognizer && other.view is UIScrollView
    }
}
#endif
