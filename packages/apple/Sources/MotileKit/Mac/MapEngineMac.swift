#if os(macOS)
import AppKit
import WebKit

/// The Mac's map: MapLibre GL JS, bundled in `Map`, in a web view that reaches nothing but
/// OpenFreeMap's tiles and glyphs. The wheel scrolls the transcript; a pinch or ctrl-scroll zooms
/// about the pointer.
final class MapEngineView: NSView, MapEngine, WKNavigationDelegate, WKUIDelegate {
    var onUserMove: () -> Void = {}
    /// The camera came to rest; where each pin is drawn, by its key.
    var onRest: (MapCamera, [String: CGPoint]) -> Void = { _, _ in }
    /// A pin was pressed, with its label.
    var onPin: (String) -> Void = { _ in }
    var onTiles: (Bool) -> Void = { _ in }
    var dark: Bool {
        didSet {
            guard dark != oldValue else { return }
            call("motile.style(\(MapPins.json(MapStyle.style(dark: dark))))")
        }
    }
    var narrow = false {
        didSet {
            guard narrow != oldValue else { return }
            call("motile.narrow(\(narrow))")
        }
    }

    private let web: MapWebView
    private let page = MapStyle.resource("map", "html")
    private var ready = false
    private var waiting: [String] = []
    private var sent = Set<String>()
    private var camera: MapCamera
    private var pins: MapPins?
    private var line: [[Double]] = []
    private var lineSerial = 0
    /// The map as last drawn whole, for when it is asked for a picture out of sight.
    private var picture: CGImage?
    private var picturing = false

    init(camera: MapCamera, dark: Bool) {
        self.camera = camera
        self.dark = dark
        let configuration = WKWebViewConfiguration()
        let scripts = WKUserContentController()
        for source in Self.scripts { scripts.addUserScript(WKUserScript(source: source, injectionTime: .atDocumentEnd, forMainFrameOnly: true)) }
        configuration.userContentController = scripts
        web = MapWebView(frame: .zero, configuration: configuration)
        super.init(frame: .zero)
        scripts.add(MapMessages(self), name: "map")
        web.customUserAgent = MapStyle.userAgent
        web.setValue(false, forKey: "drawsBackground")
        web.allowsMagnification = false
        web.allowsBackForwardNavigationGestures = false
        web.navigationDelegate = self
        web.uiDelegate = self
        web.alphaValue = 0
        web.onZoom = { [weak self] levels, point in
            self?.onUserMove()
            self?.zoom(by: levels, about: point)
        }
        addSubview(web)
        load()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        web.frame = bounds
    }

    func jump(to camera: MapCamera) {
        self.camera = camera
        call("motile.jump(\(Self.json(camera)))")
    }

    func fly(to camera: MapCamera, duration: TimeInterval) {
        self.camera = camera
        call("motile.fly(\(Self.json(camera)), \(Int(duration * 1000)))")
    }

    func show(pins: MapPins) {
        self.pins = pins
        let scale = window?.backingScaleFactor ?? Platform.pixelsPerPoint
        let added: [[String: Any]] = pins.icons.filter { !sent.contains($0) }.compactMap { emoji in
            guard let image = MapEmoji.pixels(emoji, scale: scale) else { return nil }
            sent.insert(emoji)
            return ["id": emoji, "side": image.side, "ratio": scale, "rgba": image.rgba.base64EncodedString()]
        }
        call("motile.pins(\(MapPins.json(pins.geoJSON)), \(MapPins.json(added)))")
    }

    /// Sets the route, its JSON written off the main thread; the last one set wins.
    func show(line: [[Double]]) {
        self.line = line
        lineSerial &+= 1
        let serial = lineSerial
        MapMaths.revealing.async {
            let json = MapPins.json(MapPins.route(line))
            DispatchQueue.main.async { [weak self] in
                guard let self, self.lineSerial == serial else { return }
                self.call("motile.line(\(json))")
            }
        }
    }

    /// Zooms by `levels` about a point of the map, or its centre.
    func zoom(by levels: Double, about point: CGPoint?) {
        let duration = point == nil && !Platform.reducesMotion ? 250 : 0
        let at = point.map { "\($0.x), \($0.y)" } ?? "null, null"
        call("motile.zoom(\(levels), \(at), \(duration))")
    }

    /// A picture of the map as it is drawn now, or as it was last drawn whole when it is out of sight.
    func snapshot(_ done: @escaping (CGImage?) -> Void) {
        guard ready, web.alphaValue > 0, window != nil, !isHiddenOrHasHiddenAncestor else { return done(picture) }
        web.takeSnapshot(with: nil) { [weak self] image, _ in
            let picture = image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
            if let picture { self?.picture = picture }
            done(picture ?? self?.picture)
        }
    }

    private func takePicture() {
        guard !picturing else { return }
        picturing = true
        snapshot { [weak self] _ in self?.picturing = false }
    }

    func stop() {
        web.configuration.userContentController.removeScriptMessageHandler(forName: "map")
        web.navigationDelegate = nil
        web.uiDelegate = nil
        web.stopLoading()
    }

    private func load() {
        ready = false
        sent = []
        guard let page else { return }
        web.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
        waiting = ["motile.start(\(MapPins.json(MapStyle.style(dark: dark))), \(Self.json(camera)))", "motile.narrow(\(narrow))"]
        if let pins { show(pins: pins) }
        show(line: line)
    }

    private func call(_ script: String) {
        guard ready else { return waiting.append(script) }
        web.evaluateJavaScript(script, completionHandler: nil)
    }

    fileprivate func received(_ message: [String: Any]) {
        switch message["type"] as? String {
        case "ready":
            ready = true
            let scripts = waiting
            waiting = []
            for script in scripts { call(script) }
        case "loaded":
            web.alphaValue = 1
        case "idle":
            takePicture()
        case "moved":
            onUserMove()
        case "rest":
            guard let lat = message["lat"] as? Double, let lng = message["lng"] as? Double, let zoom = message["zoom"] as? Double else { return }
            camera = MapCamera(lat: lat, lng: lng, zoom: zoom)
            let points = (message["pins"] as? [[String: Any]] ?? []).reduce(into: [String: CGPoint]()) { points, pin in
                guard let key = pin["key"] as? String, let x = pin["x"] as? Double, let y = pin["y"] as? Double else { return }
                points[key] = CGPoint(x: x, y: y)
            }
            onRest(camera, points)
        case "pin":
            guard let label = message["label"] as? String else { return }
            onPin(label)
        case "tiles":
            let failing = message["failing"] as? Bool == true
            if failing { web.alphaValue = 1 }
            onTiles(failing)
        default:
            break
        }
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = action.request.url, url.standardizedFileURL != page?.standardizedFileURL else { return decisionHandler(.allow) }
        if action.navigationType == .linkActivated, url.scheme == "https" { Platform.open(url) }
        decisionHandler(.cancel)
    }

    func webView(
        _ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        if let url = action.request.url, url.scheme == "https" { Platform.open(url) }
        return nil
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        load()
    }

    private static func json(_ camera: MapCamera) -> String {
        MapPins.json(["lat": camera.lat, "lng": camera.lng, "zoom": camera.zoom])
    }

    /// The page's scripts: GL JS's style sheet, GL JS, and the bridge to Swift.
    private static let scripts: [String] = {
        let read = { (name: String, kind: String) in MapStyle.resource(name, kind).flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? "" }
        let css = MapPins.json([read("maplibre-gl", "css")])
        let style = "(() => { const style = document.createElement('style'); style.textContent = \(css)[0]; document.head.appendChild(style); })();"
        return [style, read("maplibre-gl", "js"), read("map", "js")]
    }()
}

/// Hands the page's messages on without keeping the map alive.
private final class MapMessages: NSObject, WKScriptMessageHandler {
    private weak var engine: MapEngineView?

    init(_ engine: MapEngineView) {
        self.engine = engine
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: Any] else { return }
        engine?.received(body)
    }
}

/// The web view the map runs in. A plain scroll goes on to the transcript; a pinch, ctrl-scroll or
/// a double tap with two fingers zooms about the pointer.
private final class MapWebView: WKWebView {
    var onZoom: (Double, CGPoint) -> Void = { _, _ in }

    override func scrollWheel(with event: NSEvent) {
        guard event.modifierFlags.contains(.control) else {
            nextResponder?.scrollWheel(with: event)
            return
        }
        let levels = event.hasPreciseScrollingDeltas ? event.scrollingDeltaY / 100 : event.scrollingDeltaY / 2
        onZoom(Double(levels), point(event))
    }

    override func magnify(with event: NSEvent) {
        onZoom(log2(1 + Double(event.magnification)), point(event))
    }

    override func smartMagnify(with event: NSEvent) {
        onZoom(1, point(event))
    }

    private func point(_ event: NSEvent) -> CGPoint {
        let point = convert(event.locationInWindow, from: nil)
        return CGPoint(x: point.x, y: isFlipped ? point.y : bounds.height - point.y)
    }
}

/// The map over the window: inset from its edges, the page's colour around it, Esc puts it back.
final class MapExpansion {
    private let overlay: MapOverlayView
    private var monitor: Any?

    init?(stage: MapStage, from map: MapView, collapse: @escaping () -> Void) {
        guard let content = map.window?.contentView else { return nil }
        overlay = MapOverlayView(stage: stage)
        overlay.frame = content.bounds
        overlay.autoresizingMask = [.width, .height]
        content.addSubview(overlay, positioned: .above, relativeTo: nil)
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard event.keyCode == 53 else { return event }
            collapse()
            return nil
        }
    }

    func dismiss(_ done: @escaping () -> Void) {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        overlay.removeFromSuperview()
        done()
    }
}

private final class MapOverlayView: FlippedView {
    private let backdrop = SurfaceView()
    private let lift = SurfaceView()
    private let stage: MapStage

    init(stage: MapStage) {
        self.stage = stage
        super.init(frame: .zero)
        backdrop.fill = Theme.thinned(Theme.background, .lit)
        lift.fill = Theme.mapWater
        lift.radius = Radius.xl
        lift.drop = (.xl, .shadowStronger)
        addSubview(backdrop)
        addSubview(lift)
        addSubview(stage)
        onPress = { _ in }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func takesPress(at point: CGPoint) -> Bool {
        bounds.contains(point) && !stage.frame.contains(point)
    }

    override func scrollWheel(with event: NSEvent) {}

    override func layoutNow() {
        backdrop.frame = bounds
        let inset = MapLayout.expandedInset(windowWidth: bounds.width)
        let top = max(inset, safeAreaInsets.top)
        stage.frame = CGRect(x: inset, y: top, width: max(0, bounds.width - 2 * inset), height: max(0, bounds.height - top - inset))
        lift.frame = stage.frame
    }
}

extension MapStage {
    /// The map as assistive technologies find it: a region "Map" of its controls and its pins.
    func describeMap(pins: [MapPinAccess]) {
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityRoleDescription("region")
        setAccessibilityLabel("Map")
        let controls = subviews.filter { !$0.isHidden && $0.isAccessibilityElement() }
        let elements = pins.map { MapPinElement($0, in: self) }
        setAccessibilityChildren(controls + elements)
    }
}

private final class MapPinElement: NSAccessibilityElement {
    private let press: () -> Void

    init(_ pin: MapPinAccess, in stage: NSView) {
        press = pin.press
        super.init()
        setAccessibilityRole(.button)
        setAccessibilityLabel(pin.label)
        setAccessibilitySelected(pin.selected)
        setAccessibilityParent(stage)
        let center = pin.center ?? CGPoint(x: stage.bounds.midX, y: stage.bounds.midY)
        let side = MapEmoji.side
        let flippedY = stage.isFlipped ? stage.bounds.height - center.y : center.y
        setAccessibilityFrameInParentSpace(CGRect(x: center.x - side / 2, y: flippedY - side / 2, width: side, height: side))
    }

    override func accessibilityPerformPress() -> Bool {
        press()
        return true
    }
}
#endif
