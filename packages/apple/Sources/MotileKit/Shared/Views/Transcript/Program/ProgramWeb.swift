import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// HtmlView's and YouTube's geometry, as LAYOUT.md gives it.
enum ProgramWeb {
    static let startHeight: CGFloat = 360
    static let spinnerGap: CGFloat = 8
    static let waiting = "Preparing the interactive view"

    /// The tallest a page is in a window `window` tall (`web::max_height`).
    static func maxHeight(window: CGFloat) -> CGFloat {
        let margin = max(120, window * 0.2)
        return min(max(window - 2 * margin, 80), 2400)
    }

    static func youTube(_ node: UiNode, _ video: UiKind.YouTube, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let height = video.url == nil ? 0 : (width * 9 / 16).rounded()
        return ProgramBox(node, size: CGSize(width: width, height: height), surface: env.surface)
    }

    /// Its own height, else the one its page reported in this row, else the start, leaving room
    /// above and below it in the window to scroll the transcript past it.
    static func htmlView(_ node: UiNode, _ view: UiKind.HtmlView, width: CGFloat, env: ProgramEnv) -> ProgramBox {
        let wanted = view.height.map { CGFloat($0) } ?? WebHeights.height(row: env.row, node: node.id) ?? startHeight
        let height = min(wanted, maxHeight(window: env.window.height))
        let size = CGSize(width: width, height: height)
        guard view.streaming else { return ProgramBox(node, size: size, surface: env.surface) }
        let look = ProgramStyle.look(.small, narrow: env.narrow)
        let text = ProgramText.plain(waiting, look: look, color: Theme.mutedForeground)
        let side = ControlSize.small.symbolSide
        let textWidth = min(ProgramText.width(text).rounded(.up), max(0, width - side - spinnerGap))
        let x = ((width - side - spinnerGap - textWidth) / 2).rounded()
        let y = ((height - look.lineHeight) / 2).rounded()
        let part = ProgramTextPart(
            text: text, frame: CGRect(x: x + side + spinnerGap, y: y, width: textWidth, height: look.lineHeight),
            lift: ControlText.lift(look.font, lineHeight: look.lineHeight), oneLine: true)
        let spinner = CGRect(x: x, y: ((height - side) / 2).rounded(), width: side, height: side)
        return ProgramBox(node, size: size, surface: env.surface, texts: ["waiting": part], rects: ["spinner": spinner])
    }
}

/// HtmlView and YouTube: the page core builds in a sandboxed web view, made while its row is on
/// screen and let go when it leaves. A program's page has the surface's border and colour, and
/// the spinner in its place while the reply streams; YouTube's player is on black.
final class ProgramWebView: ProgramNodeView {
    private let face = SurfaceView()
    private let spinner = SpinnerView(size: ControlSize.small.symbol)
    private var page: WebPageView?
    /// What the page shown, or being built, was made from.
    private var wanted: String?

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(face)
        addSubview(spinner)
        #if os(iOS)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in
            view.paintEdge()
            view.reload()
        }
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        paintEdge()
        reload()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        reload()
    }
    #else
    override func didMoveToWindow() {
        super.didMoveToWindow()
        reload()
    }
    #endif

    override func configure(_ box: ProgramBox) {
        switch box.node.kind {
        case .youTube(let node):
            face.fill = Theme.overlay
            face.radius = ProgramStyle.radius(node.radius, side: min(box.size.width, box.size.height))
        case .htmlView(let node):
            face.fill = box.surface.platform
            face.radius = ProgramStyle.radius(node.radius, side: min(box.size.width, box.size.height))
        default:
            break
        }
        clip(radius: face.radius)
        paintEdge()
        spinner.isHidden = box.rects["spinner"] == nil
        spinner.frame = box.rects["spinner"] ?? .zero
    }

    /// A program's page has its border over it, as the layer's own, which lies over what is inside.
    private func paintEdge() {
        guard let box else { return }
        let edge: PlatformColor? = if case .htmlView = box.node.kind { box.surface.platformBorder } else { nil }
        #if os(macOS)
        wantsLayer = true
        var color = edge?.cgColor
        effectiveAppearance.performAsCurrentDrawingAppearance { color = edge?.cgColor }
        layer?.borderColor = color
        layer?.borderWidth = edge == nil ? 0 : 1
        #else
        layer.borderColor = edge?.resolvedColor(with: traitCollection).cgColor
        layer.borderWidth = edge == nil ? 0 : 1
        #endif
    }

    override func didShow(_ box: ProgramBox) {
        reload()
    }

    override func layoutNow() {
        face.frame = bounds
        page?.frame = bounds
    }

    override func leave() {
        super.leave()
        page?.removeFromSuperview()
        page = nil
        wanted = nil
    }

    /// Builds the page the node asks for, off the main thread, when it isn't the one shown.
    private func reload() {
        guard window != nil, let box, let request = request(box) else { return leave() }
        guard request.key != wanted else { return }
        wanted = request.key
        page?.removeFromSuperview()
        page = nil
        let (row, node) = (host?.rowID ?? "", box.id)
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let built = request.build()
            DispatchQueue.main.async {
                guard let self, self.wanted == request.key, let built else { return }
                self.show(built, row: row, node: node)
            }
        }
    }

    private func show(_ built: WebPage, row: String, node: String) {
        let view = WebPageView(page: built)
        view.frame = bounds
        view.onHeight = { [weak self] height in
            guard let self, case .htmlView(let html)? = self.box?.node.kind, html.height == nil else { return }
            guard WebHeights.height(row: row, node: node).map({ abs($0 - height) > 0.5 }) ?? true else { return }
            WebHeights.set(height, row: row, node: node)
            guard abs(height - self.bounds.height) > 0.5 else { return }
            self.host?.resized(node)
        }
        addSubview(view)
        page = view
    }

    private func request(_ box: ProgramBox) -> (key: String, build: () -> WebPage?)? {
        let place = "\(host?.rowID ?? "")\n\(box.id)"
        switch box.node.kind {
        case .youTube(let video):
            guard let url = video.url else { return nil }
            return ("\(place)\n\(url)\n\(video.title)", { WebPage.youtube(url: url, title: video.title) })
        case .htmlView(let view):
            guard !view.streaming else { return nil }
            let dark = drawsDark
            let background = box.surface.platform
            return ("\(place)\n\(dark)\n\(box.surface)\n\(view.html)", {
                WebPage.html(
                    view.html, dark: dark, textSize: Theme.proseSize, lineHeight: Theme.proseLineHeight, color: Theme.foreground,
                    background: background)
            })
        default:
            return nil
        }
    }
}
