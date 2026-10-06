#if os(iOS)
import Observation
import SwiftUI
import UIKit

/// Whether the sidebar is shown beside the thread, which a swipe to the right does on a phone.
@Observable
final class Drawer {
    var isOpen = false
}

/// The sidebar and the side panel under the thread: swiping right anywhere on the thread slides
/// it aside as a card and shows the sidebar, swiping left shows the panel, and a swipe back
/// closes either. On a phone they take the whole screen; a wider window leaves some of the card
/// in view, and a tap on it closes too.
struct DrawerView<Sidebar: View, Content: View, Panel: View>: UIViewControllerRepresentable {
    let drawer: Drawer
    let sidePanel: SidePanel
    @ViewBuilder let sidebar: Sidebar
    @ViewBuilder let content: Content
    @ViewBuilder let panel: Panel

    func makeUIViewController(context: Context) -> DrawerController {
        let controller = DrawerController(sidebar: UIHostingController(rootView: AnyView(sidebar)), content: UIHostingController(rootView: AnyView(content)))
        controller.onChange = { [drawer, sidePanel] side in
            if drawer.isOpen != (side == .sidebar) { drawer.isOpen = side == .sidebar }
            if sidePanel.isOpen != (side == .panel) { sidePanel.isOpen = side == .panel }
        }
        return controller
    }

    func updateUIViewController(_ controller: DrawerController, context: Context) {
        (controller.sidebar as? UIHostingController<AnyView>)?.rootView = AnyView(sidebar)
        (controller.content as? UIHostingController<AnyView>)?.rootView = AnyView(content)
        controller.panelContent = AnyView(panel)
        controller.follow(sidebar: drawer.isOpen, panel: sidePanel.isOpen)
    }
}

final class DrawerController: UIViewController, UIGestureRecognizerDelegate {
    /// A window narrower than this gives all of its width to the sidebar and the panel.
    private static let fullBelow: CGFloat = 500
    private static let widest: CGFloat = 360
    /// What the side panel leaves of the card in a window too wide to give it all.
    private static let panelPeek: CGFloat = 64
    /// The name of the recognizers that slide the sidebar's rows aside.
    static let rowSwipe = "row-swipe"

    enum Side { case sidebar, panel }

    let sidebar: UIViewController
    let content: UIViewController
    /// The side panel is only built while it is in view, since it loads what it shows.
    var panelContent = AnyView(EmptyView()) {
        didSet { showPanelContent() }
    }
    var onChange: ((Side?) -> Void)?

    private let panel = UIHostingController(rootView: AnyView(EmptyView()))
    private var panelShown = false

    private let card = UIView()
    /// Behind the card, since the card clips: it casts the card's shadow on the side it uncovers.
    private let cardShadow = UIView()
    /// Over the card while a side shows: it dims the thread and takes the tap that closes.
    private let shade = UIControl()
    private let pan = UIPanGestureRecognizer()
    private(set) var open: Side?
    /// How far the card is aside: to the right towards 1, over the sidebar, and to the left
    /// towards -1, over the panel.
    private var progress: CGFloat = 0
    private var progressAtStart: CGFloat = 0
    private var dragged = Side.sidebar

    init(sidebar: UIViewController, content: UIViewController) {
        self.sidebar = sidebar
        self.content = content
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var sidebarWidth: CGFloat {
        view.bounds.width < Self.fullBelow ? view.bounds.width : Self.widest
    }

    private var panelWidth: CGFloat {
        view.bounds.width < Self.fullBelow ? view.bounds.width : view.bounds.width - Self.panelPeek
    }

    private func width(of side: Side) -> CGFloat {
        side == .sidebar ? sidebarWidth : panelWidth
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Theme.background

        addChild(sidebar)
        sidebar.view.backgroundColor = .clear
        view.addSubview(sidebar.view)
        sidebar.didMove(toParent: self)

        addChild(panel)
        panel.view.backgroundColor = .clear
        view.addSubview(panel.view)
        panel.didMove(toParent: self)

        card.clipsToBounds = true
        card.backgroundColor = Theme.background
        cardShadow.isUserInteractionEnabled = false
        cardShadow.layer.shadowColor = UIColor.black.cgColor
        cardShadow.layer.shadowOffset = .zero
        cardShadow.layer.shadowRadius = 20
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
        shade.addAction(UIAction { [weak self] _ in self?.setOpen(nil, animated: true) }, for: .touchUpInside)
        card.addSubview(shade)

        pan.addTarget(self, action: #selector(panned(_:)))
        pan.delegate = self
        view.addGestureRecognizer(pan)
    }

    private func paintCardEdges() {
        card.layer.borderColor = Theme.border.resolvedColor(with: traitCollection).cgColor
        cardShadow.layer.shadowOpacity = traitCollection.userInterfaceStyle == .dark ? 0.5 : 0.16
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        place()
    }

    private func place() {
        let bounds = view.bounds
        let aside = abs(progress)
        sidebar.view.frame = CGRect(x: 0, y: 0, width: sidebarWidth, height: bounds.height)
        panel.view.frame = CGRect(x: bounds.width - panelWidth, y: 0, width: panelWidth, height: bounds.height)
        card.frame = CGRect(x: progress * width(of: progress < 0 ? .panel : .sidebar), y: 0, width: bounds.width, height: bounds.height)
        let lifted = min(1, aside * 6)
        card.layer.borderWidth = lifted
        content.view.frame = card.bounds
        placeShadow()
        shade.frame = card.bounds
        shade.alpha = 0.55 * aside
        shade.isHidden = progress == 0
        sidebar.view.isHidden = progress <= 0
        panel.view.isHidden = progress >= 0
        guard panelShown != (progress < 0) else { return }
        panelShown = progress < 0
        showPanelContent()
    }

    private func showPanelContent() {
        panel.rootView = panelShown ? panelContent : AnyView(EmptyView())
    }

    /// The shadow goes as the card leaves the screen, so it doesn't fall on the side that opened.
    private func placeShadow() {
        if cardShadow.bounds.size != card.bounds.size {
            cardShadow.layer.shadowPath = UIBezierPath(rect: card.bounds).cgPath
        }
        cardShadow.frame = card.frame
        let onScreen = (view.bounds.width - abs(card.frame.minX)) / max(1, view.bounds.width)
        cardShadow.alpha = smoothstep(onScreen * 4)
    }

    private func smoothstep(_ value: CGFloat) -> CGFloat {
        let clamped = min(1, max(0, value))
        return clamped * clamped * (3 - 2 * clamped)
    }

    /// Opens or closes as the client's state says, unless a finger is moving the card. Asked
    /// for both sides, the one that isn't open yet was asked for last.
    func follow(sidebar: Bool, panel: Bool) {
        guard pan.state != .began, pan.state != .changed else { return }
        let wanted: Side? =
            switch open {
            case .sidebar: panel ? .panel : sidebar ? .sidebar : nil
            case .panel, nil: sidebar ? .sidebar : panel ? .panel : nil
            }
        guard wanted != open else { return }
        setOpen(wanted, animated: true)
    }

    func setOpen(_ side: Side?, animated: Bool, velocity: CGFloat = 0) {
        let target: CGFloat = side == .sidebar ? 1 : side == .panel ? -1 : 0
        guard open != side || progress != target else { return }
        open = side
        if side != nil { view.endEditing(true) }
        onChange?(side)
        guard animated, view.window != nil else {
            stopSliding()
            progress = target
            return place()
        }
        let distance = abs(target - progress)
        let speed = min(12 * distance, abs(velocity) / width(of: target < 0 || progress < 0 ? .panel : .sidebar))
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
            dragged = progress > 0 || (progress == 0 && recognizer.velocity(in: view).x > 0) ? .sidebar : .panel
            view.endEditing(true)
        case .changed:
            // One swipe moves the card over one side only.
            let (low, high): (CGFloat, CGFloat) = dragged == .sidebar ? (0, 1) : (-1, 0)
            let wanted = progressAtStart + moved / width(of: dragged)
            // Past its ends the card gives a little and no more.
            let over = wanted > high ? wanted - high : wanted < low ? wanted - low : 0
            progress = min(high, max(low, wanted)) + over * 0.12
            place()
        case .ended, .cancelled:
            let speed = recognizer.velocity(in: view).x
            let outwards = dragged == .sidebar ? speed : -speed
            let opens = abs(speed) > 280 ? outwards > 0 : abs(progress) > 0.5
            setOpen(opens ? dragged : nil, animated: true, velocity: speed)
        default:
            break
        }
    }

    // MARK: Whose swipe it is

    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
        let speed = pan.velocity(in: view)
        guard abs(speed.x) > abs(speed.y) * 1.3 else { return false }
        let rightwards = speed.x > 0
        let point = pan.location(in: view)
        switch open {
        // Only the swipe back is the drawer's: one to the right is the sidebar's rows'.
        case .sidebar: return !rightwards
        case .panel: return rightwards && !scrollsSideways(under: point, rightwards: true)
        case nil: return !scrollsSideways(under: point, rightwards: rightwards)
        }
    }

    /// Whether what the finger is on has a swipe that way of its own: code or a table that
    /// still scrolls that way, or text whose selection is being dragged.
    private func scrollsSideways(under point: CGPoint, rightwards: Bool) -> Bool {
        var view = self.view.hitTest(point, with: nil)
        while let current = view, current !== self.view {
            if let text = current as? UITextView, text.selectedRange.length > 0 { return true }
            if let scroll = current as? UIScrollView, scroll.isScrollEnabled, scroll.contentSize.width > scroll.bounds.width + 1 {
                let start = -scroll.adjustedContentInset.left
                let end = scroll.contentSize.width - scroll.bounds.width + scroll.adjustedContentInset.right
                if rightwards ? scroll.contentOffset.x > start + 1 : scroll.contentOffset.x < end - 1 { return true }
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
