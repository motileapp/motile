import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// OpenUI's `Map` for the transcript: the pins and the route core resolved on MapLibre, with glass
/// controls over it. It runs an engine (`MapEngineView`) only while it is on screen, three at most
/// at once, and shows a picture of itself otherwise.
final class MapView: FlippedView {
    /// Called with a pin's label when it is pressed.
    var onSelect: (String) -> Void = { _ in }
    /// Called with the category to show, or nil for every place, when the filter's choice changes.
    var onFilter: (String?) -> Void = { _ in }
    /// Whether the map is on screen, which is when it claims an engine. It follows its window; a
    /// transcript that keeps rows out of sight in the window says so itself.
    var onScreen = false {
        didSet {
            guard onScreen, !oldValue else { return }
            MapPool.claim(self)
        }
    }

    /// What the map remembers of how it was moved, for a view made again for the same map.
    var memory: MapMemory { pilot.memory }

    private let pilot: MapPilot
    private var node: UiKind.Map?
    private var streaming = false
    private(set) var engine: MapEngineView?
    private var expansion: MapExpansion?
    private var infoOpen = false
    private var tilesFailed = false
    private var pinPoints: [String: CGPoint] = [:]

    private let placeholder = SurfaceView()
    let stage = MapStage()
    private let still = MapStillView()
    private let filter = GlassButtonView(height: MapLayout.roundSide) {}
    private let recenter = GlassButtonView(symbol: .locateFixed, symbolSide: MapLayout.roundSymbol, help: "Show every place", height: MapLayout.roundSide) {}
    private let info = GlassButtonView(symbol: .info, symbolSide: MapLayout.roundSymbol, help: "About this map", height: MapLayout.roundSide) {}
    private let expand = GlassButtonView("Expand", symbol: .maximize2, symbolSide: MapLayout.smallSymbol, height: MapLayout.expandHeight) {}
    private let zoomIn = GlassButtonView(symbol: .plus, help: "Zoom in", height: MapLayout.zoomSide) {}
    private let zoomOut = GlassButtonView(symbol: .minus, help: "Zoom out", height: MapLayout.zoomSide) {}
    private let note = MapBoxView()
    private let failure = MapBoxView()
    private let overview = MapOverviewView()
    private let attribution = MapAttributionView()

    init(memory: MapMemory = MapMemory()) {
        pilot = MapPilot(memory: memory)
        super.init(frame: .zero)
        placeholder.fill = Theme.mapWater
        placeholder.radius = Radius.xl
        addSubview(placeholder)
        addSubview(stage)
        stage.onLayout = { [weak self] in self?.layoutStage() }
        stage.onHover = { [weak self] point in self?.hover(point != nil) }
        stage.addSubview(still)
        for view in [note, failure, overview, attribution, filter, recenter, info, expand, zoomIn, zoomOut] as [PlatformView] { stage.addSubview(view) }
        filter.opens = true
        filter.padding = MapLayout.filterPadding
        expand.symbolTrails = true
        expand.padding = (MapLayout.expandPadding, MapLayout.expandPadding)
        filter.action = { [weak self] in self?.offerFilter() }
        recenter.action = { [weak self] in self?.pilot.recenter() }
        info.action = { [weak self] in self?.toggleInfo() }
        expand.action = { [weak self] in self?.toggleExpanded() }
        zoomIn.action = { [weak self] in self?.zoom(by: 1) }
        zoomOut.action = { [weak self] in self?.zoom(by: -1) }
        failure.text = MapLayout.tilesFailed
        for view in [zoomIn, zoomOut] { view.opacity = Platform.hoverReveals ? 0 : 1 }
        zoomIn.isHidden = !Platform.hoverReveals
        zoomOut.isHidden = !Platform.hoverReveals
        watchAppearance()
        refresh()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// How tall the map is in a window this wide. The measuring pass asks it and never makes a map.
    static func height(node: UiKind.Map, windowWidth: CGFloat) -> CGFloat {
        MapLayout.height(node, windowWidth: windowWidth)
    }

    /// Shows the map as core resolved it, while its program streams or once it ended.
    func set(_ node: UiKind.Map, streaming: Bool) {
        self.node = node
        self.streaming = streaming
        pilot.show(node, streaming: streaming)
        refresh()
    }

    var isLive: Bool { engine != nil }

    /// Starts an engine where the last one left the camera.
    func goLive() {
        guard engine == nil else { return }
        let engine = MapEngineView(camera: pilot.memory.camera ?? .world, dark: isDark)
        engine.onUserMove = { [weak self] in self?.pilot.userMoved() }
        engine.onRest = { [weak self] camera, points in
            self?.pilot.rested(at: camera)
            self?.pinPoints = points
            self?.describePins()
        }
        engine.onPin = { [weak self] label in self?.pressed(label) }
        engine.onTiles = { [weak self] failing in
            self?.tilesFailed = failing
            self?.refresh()
        }
        stage.insert(engine, above: still)
        self.engine = engine
        stage.relayout()
        layoutStage()
        pilot.attach(engine)
    }

    /// Gives the engine up, showing a picture of the map in its place.
    func goStill() {
        guard let engine else { return }
        self.engine = nil
        pilot.attach(nil)
        engine.snapshot { [weak self] picture in
            if let picture { self?.still.picture = picture }
            engine.stop()
            engine.removeFromSuperview()
        }
    }

    /// Puts the expanded map back in its row.
    func collapse() {
        guard let expansion else { return }
        self.expansion = nil
        expansion.dismiss { [weak self] in
            guard let self else { return }
            addSubview(stage)
            relayout()
            refresh()
        }
        refresh()
    }

    override func layoutNow() {
        placeholder.frame = bounds
        guard expansion == nil else { return }
        stage.frame = bounds
    }

    private var narrow: Bool { stage.bounds.width < MapLayout.narrowColumn }

    private func layoutStage() {
        let bounds = stage.bounds
        guard bounds.width > 0, bounds.height > 0 else { return }
        still.frame = bounds
        engine?.frame = bounds
        engine?.narrow = narrow
        pilot.resize(to: bounds.size)
        let inset = MapLayout.inset
        var x = inset
        for button in [filter, recenter, info] where !button.isHidden {
            let width = button === filter ? button.naturalWidth : MapLayout.roundSide
            button.frame = CGRect(x: x, y: inset, width: width, height: MapLayout.roundSide)
            x += width + MapLayout.gap
        }
        let expandWidth = expand.naturalWidth
        expand.frame = CGRect(x: bounds.width - inset - expandWidth, y: inset, width: expandWidth, height: MapLayout.expandHeight)
        let side = MapLayout.zoomSide
        zoomOut.frame = CGRect(x: bounds.width - inset - side, y: bounds.height - inset - side, width: side, height: side)
        zoomIn.frame = zoomOut.frame.offsetBy(dx: 0, dy: -side - MapLayout.gap)
        let boxWidth = max(0, min(MapLayout.noteWidth, bounds.width - 2 * inset))
        let noteSize = note.size(maxWidth: boxWidth)
        note.frame = CGRect(x: inset, y: MapLayout.noteTop, width: noteSize.width, height: noteSize.height)
        let failureSize = failure.size(maxWidth: boxWidth)
        failure.frame = CGRect(x: inset, y: bounds.height - MapLayout.errorBottom - failureSize.height, width: failureSize.width, height: failureSize.height)
        let overviewSize = overview.size
        overview.frame = CGRect(x: inset, y: bounds.height - MapLayout.overviewBottom - overviewSize.height, width: overviewSize.width, height: overviewSize.height)
        let attributionSize = attribution.size
        let attributionWidth = min(attributionSize.width, bounds.width - MapLayout.attributionLeft)
        attribution.frame = CGRect(
            x: MapLayout.attributionLeft, y: bounds.height - attributionSize.height, width: max(0, attributionWidth), height: attributionSize.height)
        describePins()
    }

    private func refresh() {
        let categories = node?.categories ?? []
        filter.isHidden = categories.count < 2
        filter.title = node?.filter ?? "All places"
        info.expanded = infoOpen
        note.text = node?.note ?? ""
        note.isHidden = !infoOpen
        failure.isHidden = !tilesFailed
        overview.isHidden = node?.route != .overview
        expand.title = expansion == nil ? "Expand" : "Collapse"
        expand.symbol = expansion == nil ? .maximize2 : .minimize2
        expand.expanded = expansion != nil
        stage.relayout()
    }

    private func offerFilter() {
        guard let node else { return }
        let every = Choice("All places", chosen: node.filter == nil) { [weak self] in self?.onFilter(nil) }
        let categories = node.categories.map { category in
            Choice(category, chosen: node.filter == category) { [weak self] in self?.onFilter(category) }
        }
        Choice.offer([every] + categories, under: filter)
    }

    private func toggleInfo() {
        infoOpen.toggle()
        refresh()
    }

    private func toggleExpanded() {
        guard expansion == nil else { return collapse() }
        expansion = MapExpansion(stage: stage, from: self) { [weak self] in self?.collapse() }
        refresh()
    }

    private func pressed(_ label: String) {
        onSelect(label)
        collapse()
    }

    private func zoom(by levels: Double) {
        pilot.userMoved()
        engine?.zoom(by: levels, about: nil)
    }

    private func hover(_ over: Bool) {
        guard Platform.hoverReveals else { return }
        let opacity: CGFloat = over ? 1 : 0
        for button in [zoomIn, zoomOut] {
            guard !Platform.reducesMotion else {
                button.opacity = opacity
                continue
            }
            button.fade(to: opacity, duration: 0.15)
        }
    }

    private var isDark: Bool {
        #if os(macOS)
        effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        #else
        traitCollection.userInterfaceStyle == .dark
        #endif
    }

    private func appearanceChanged() {
        engine?.dark = isDark
    }

    #if os(macOS)
    private func watchAppearance() {}

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        appearanceChanged()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onScreen = window != nil
    }
    #else
    private func watchAppearance() {
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.appearanceChanged() }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        onScreen = window != nil
    }
    #endif

    private func describePins() {
        let pins = MapPins(node?.pins ?? []).pins.map { pin in
            MapPinAccess(label: pin.label, selected: pin.selected, center: pinPoints[pin.key]) { [weak self] in self?.pressed(pin.label) }
        }
        stage.describeMap(pins: pins)
    }
}

/// A pin as assistive technologies find it: a button with its label, selected or not.
struct MapPinAccess {
    let label: String
    let selected: Bool
    /// Where it is drawn in the map, when the engine said.
    let center: CGPoint?
    let press: () -> Void
}

/// The map's box: rounded, clipped, the colour of water until the tiles draw. It goes over the
/// window with the map when the map is expanded.
final class MapStage: LayerView {
    var onLayout: () -> Void = {}

    override func layoutNow() {
        onLayout()
    }

    override func paint(_ layer: CALayer) {
        layer.backgroundColor = resolved(Theme.mapWater)
        layer.cornerRadius = Radius.xl
        layer.cornerCurve = .continuous
        layer.masksToBounds = true
    }

    func insert(_ view: PlatformView, above sibling: PlatformView) {
        #if os(macOS)
        addSubview(view, positioned: .above, relativeTo: sibling)
        #else
        insertSubview(view, aboveSubview: sibling)
        #endif
    }
}

/// The picture of the map shown while it has no engine.
private final class MapStillView: LayerView {
    var picture: CGImage? { didSet { repaint() } }

    override func paint(_ layer: CALayer) {
        layer.contents = picture
        layer.contentsGravity = .resizeAspectFill
    }
}

/// A box of text over the map, as a popover looks: the map's note, or why the tiles are missing.
private final class MapBoxView: FlippedView {
    private static let font = Theme.smallFont
    private static let line = scaled(18)

    var text = "" { didSet { words.text = ControlText.wrapped(text, font: Self.font, lineHeight: Self.line, color: Theme.mutedForeground) } }

    private let box = SurfaceView()
    private let words = TextBlockView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        box.fill = Theme.popover
        box.stroke = Theme.borderPopover
        box.radius = Radius.lg
        box.drop = (.md, .shadow)
        addSubview(box)
        addSubview(words)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func size(maxWidth: CGFloat) -> CGSize {
        let padding = MapLayout.notePadding
        let room = max(1, maxWidth - 2 * padding)
        let width = min(room, ControlText.width(words.text))
        return CGSize(width: ceil(width + 2 * padding), height: ControlText.height(words.text, width: room) + 2 * padding)
    }

    override func layoutNow() {
        box.frame = bounds
        let padding = MapLayout.notePadding
        words.frame = bounds.insetBy(dx: padding, dy: padding)
        describe(text)
    }
}

/// "Route overview": the route is straight, as no street route was found.
private final class MapOverviewView: FlippedView {
    private let box = SurfaceView()
    private let words = TextBlockView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        box.fill = Theme.thinned(Theme.background, .lit)
        box.radius = Radius.sm
        words.oneLine = true
        words.text = ControlText.wrapped(MapLayout.routeOverview, font: .ui(10), lineHeight: scaled(16), color: Theme.mutedForeground)
        addSubview(box)
        addSubview(words)
        describe(MapLayout.routeOverview)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    var size: CGSize {
        let padding = MapLayout.overviewPadding
        return CGSize(width: ControlText.width(words.text) + 2 * padding.horizontal, height: scaled(16) + 2 * padding.vertical)
    }

    override func layoutNow() {
        box.frame = bounds
        let padding = MapLayout.overviewPadding
        words.frame = bounds.insetBy(dx: padding.horizontal, dy: padding.vertical)
    }
}

/// The style's attribution at the map's bottom left, each name a link that opens in the browser.
private final class MapAttributionView: FlippedView {
    private static let font = PlatformFont.ui(9)
    private static let line = scaled(12)

    private let box = SurfaceView()
    private let words = TextBlockView()
    /// Where each part ends, from the text's start.
    private var ends: [CGFloat] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        box.fill = Theme.thinned(Theme.background, .lit)
        words.oneLine = true
        let text = NSMutableAttributedString()
        for part in MapStyle.attribution {
            text.append(ControlText.wrapped(part.text, font: Self.font, lineHeight: Self.line, color: Theme.mutedForeground))
            ends.append(ControlText.width(text))
        }
        words.text = text
        addSubview(box)
        addSubview(words)
        pointer = .hand
        onPress = { [weak self] point in self?.open(at: point.x) }
        describe(MapStyle.attribution.map(\.text).joined())
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    var size: CGSize {
        let padding = MapLayout.attributionPadding
        return CGSize(width: ControlText.width(words.text) + 2 * padding.horizontal, height: Self.line + 2 * padding.vertical)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point) && link(at: point.x) != nil
    }

    override func layoutNow() {
        box.frame = bounds
        let padding = MapLayout.attributionPadding
        words.frame = bounds.insetBy(dx: padding.horizontal, dy: padding.vertical)
    }

    private func link(at x: CGFloat) -> URL? {
        let x = x - MapLayout.attributionPadding.horizontal
        guard let index = ends.firstIndex(where: { x < $0 }) else { return nil }
        return MapStyle.attribution[index].link
    }

    private func open(at x: CGFloat) {
        guard let link = link(at: x) else { return }
        Platform.open(link)
    }
}

/// The maps whose engines run: three at most. A map that comes on screen takes the engine of the
/// one shown longest ago, off screen first.
enum MapPool {
    static let limit = 3

    private final class Entry {
        weak var map: MapView?
        init(_ map: MapView) { self.map = map }
    }

    private static var shown: [Entry] = []

    static func claim(_ map: MapView) {
        shown.removeAll { $0.map == nil || $0.map === map }
        shown.append(Entry(map))
        let live = shown.compactMap(\.map).filter { $0.isLive && $0 !== map }
        if !map.isLive, live.count >= limit {
            (live.first { !$0.onScreen } ?? live.first)?.goStill()
        }
        map.goLive()
    }
}
