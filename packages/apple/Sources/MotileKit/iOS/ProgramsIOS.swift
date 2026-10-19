#if os(iOS)
import UIKit

/// A carousel's track: its items in a scroll view that stops at an item's edge.
final class CarouselTrack: UIScrollView, UIScrollViewDelegate {
    let content = FlippedView()
    var onScroll: (() -> Void)?
    private var step: CGFloat = 1

    override init(frame: CGRect) {
        super.init(frame: frame)
        showsHorizontalScrollIndicator = false
        showsVerticalScrollIndicator = false
        alwaysBounceVertical = false
        isDirectionalLockEnabled = true
        decelerationRate = .fast
        clipsToBounds = true
        delegate = self
        addSubview(content)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    var offset: CGFloat { contentOffset.x }
    var most: CGFloat { max(0, contentSize.width - bounds.width) }

    func set(width: CGFloat, height: CGFloat, inset: CGFloat, step: CGFloat) {
        self.step = max(1, step)
        content.frame = CGRect(x: 0, y: inset, width: width, height: height)
        contentSize = CGSize(width: width, height: height + 2 * inset)
    }

    func page(_ direction: Int) {
        let target = (contentOffset.x + CGFloat(direction) * bounds.width * 0.85) / step
        let x = min(max(0, (direction > 0 ? target.rounded(.down) : target.rounded(.up)) * step), most)
        setContentOffset(CGPoint(x: x, y: 0), animated: !Platform.reducesMotion)
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        if contentOffset.y != 0 { contentOffset.y = 0 }
        onScroll?()
    }

    func scrollViewWillEndDragging(_ scrollView: UIScrollView, withVelocity velocity: CGPoint, targetContentOffset: UnsafeMutablePointer<CGPoint>) {
        let target = targetContentOffset.pointee.x / step
        let index = velocity.x > 0 ? target.rounded(.up) : velocity.x < 0 ? target.rounded(.down) : target.rounded()
        targetContentOffset.pointee.x = min(max(0, index * step), most)
    }
}

/// A popover's panel by its trigger, which stays a popover also on a phone.
final class ProgramPopoverPanel: NSObject, UIPopoverPresentationControllerDelegate {
    private weak var anchor: UIView?
    private let onClose: () -> Void
    private var controller: UIViewController?

    init(anchor: UIView, onClose: @escaping () -> Void) {
        self.anchor = anchor
        self.onClose = onClose
    }

    func show(_ slot: UiSlot, host: ProgramHost?, surface: Surface) {
        guard let anchor, let presenter = anchor.presenter else { return }
        let controller = self.controller ?? UIViewController()
        let panel = controller.view.subviews.first as? ProgramPanelView ?? {
            let panel = ProgramPanelView()
            controller.view.addSubview(panel)
            return panel
        }()
        let window = anchor.window?.bounds.size ?? UIScreen.main.bounds.size
        let size = panel.show(slot, host: host, window: window)
        panel.frame = CGRect(origin: .zero, size: size)
        controller.view.backgroundColor = Theme.popover
        controller.preferredContentSize = size
        guard self.controller == nil else { return }
        self.controller = controller
        controller.modalPresentationStyle = .popover
        controller.popoverPresentationController?.sourceView = anchor
        controller.popoverPresentationController?.sourceRect = anchor.bounds
        controller.popoverPresentationController?.permittedArrowDirections = [.up, .down]
        controller.popoverPresentationController?.delegate = self
        presenter.present(controller, animated: true)
    }

    func close() {
        controller?.dismiss(animated: true)
        controller = nil
    }

    func adaptivePresentationStyle(for controller: UIPresentationController, traitCollection: UITraitCollection) -> UIModalPresentationStyle {
        .none
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        controller = nil
        onClose()
    }
}
#endif
