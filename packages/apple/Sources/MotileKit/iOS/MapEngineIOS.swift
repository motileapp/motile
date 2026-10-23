#if os(iOS)
import MapLibre
import UIKit

/// The iOS app's map: MapLibre Native with the bundled style. A finger pans it and two pinch it,
/// leaving the transcript and the drawer around it be. It never turns or tilts.
final class MapEngineView: UIView, MapEngine, MLNMapViewDelegate, MLNMapSnapshotterDelegate, UIGestureRecognizerDelegate {
    private static let empty: [String: Any] = ["type": "FeatureCollection", "features": [[String: Any]]()]
    private static let gestures: MLNCameraChangeReason = [
        .gesturePan, .gesturePinch, .gestureRotate, .gestureZoomIn, .gestureZoomOut, .gestureOneFingerZoom, .gestureTilt,
    ]

    var onUserMove: () -> Void = {}
    /// The camera came to rest; where each pin is drawn, by its key.
    var onRest: (MapCamera, [String: CGPoint]) -> Void = { _, _ in }
    /// A pin was pressed, with its label.
    var onPin: (String) -> Void = { _ in }
    var onTiles: (Bool) -> Void = { _ in }
    var dark: Bool {
        didSet {
            guard dark != oldValue else { return }
            style = nil
            map.styleURL = MapStyle.url(dark: dark)
        }
    }
    var narrow = false { didSet { showLabels() } }

    private let map: MLNMapView
    private var style: MLNStyle?
    private var pins: MapPins?
    private var line: [[Double]] = []
    private var lineSerial = 0
    private var failing = false
    private var snapshotter: MLNMapSnapshotter?

    init(camera: MapCamera, dark: Bool) {
        MapNetwork.start()
        self.dark = dark
        map = MLNMapView(frame: .zero, styleURL: MapStyle.url(dark: dark))
        super.init(frame: .zero)
        map.delegate = self
        map.logoView.isHidden = true
        map.attributionButton.isHidden = true
        map.compassView.isHidden = true
        map.scaleBar.isHidden = true
        map.isRotateEnabled = false
        map.isPitchEnabled = false
        map.alpha = 0
        map.setCenter(CLLocationCoordinate2D(latitude: camera.lat, longitude: camera.lng), zoomLevel: camera.zoom, animated: false)
        let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
        tap.delegate = self
        map.addGestureRecognizer(tap)
        addSubview(map)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        map.frame = bounds
    }

    func jump(to camera: MapCamera) {
        map.setCenter(CLLocationCoordinate2D(latitude: camera.lat, longitude: camera.lng), zoomLevel: camera.zoom, animated: false)
    }

    func fly(to camera: MapCamera, duration: TimeInterval) {
        map.fly(to: Self.camera(camera, size: map.bounds.size), withDuration: duration, completionHandler: nil)
    }

    func show(pins: MapPins) {
        self.pins = pins
        showPins()
    }

    /// Sets the route, its shape made off the main thread; the last one set wins.
    func show(line: [[Double]]) {
        self.line = line
        lineSerial &+= 1
        let serial = lineSerial
        MapMaths.revealing.async {
            let shape = Self.shape(MapPins.route(line))
            DispatchQueue.main.async { [weak self] in
                guard let self, self.lineSerial == serial else { return }
                (self.style?.source(withIdentifier: "route") as? MLNShapeSource)?.shape = shape
            }
        }
    }

    /// Draws the map again: MapLibre stops drawing while it is out of sight, and only starts
    /// again when its view comes into a window.
    func repaint() {
        map.removeFromSuperview()
        addSubview(map)
    }

    /// Zooms by `levels` about the map's centre.
    func zoom(by levels: Double, about point: CGPoint?) {
        map.setZoomLevel(map.zoomLevel + levels, animated: !Platform.reducesMotion)
    }

    /// A picture of the map as it is now: its style, camera, pins and route, drawn apart from it.
    func snapshot(_ done: @escaping (CGImage?) -> Void) {
        guard bounds.width > 0, bounds.height > 0, let data = MapStyle.filled(dark: dark, pins: pins, line: line) else { return done(nil) }
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("motile-map-\(UUID().uuidString).json")
        guard (try? data.write(to: file)) != nil else { return done(nil) }
        let center = map.centerCoordinate
        let camera = Self.camera(MapCamera(lat: center.latitude, lng: center.longitude, zoom: map.zoomLevel), size: bounds.size)
        let options = MLNMapSnapshotOptions(styleURL: file, camera: camera, size: bounds.size)
        options.zoomLevel = map.zoomLevel
        options.scale = traitCollection.displayScale
        let snapshotter = MLNMapSnapshotter(options: options)
        snapshotter.delegate = self
        self.snapshotter = snapshotter
        snapshotter.start { [weak self] snapshot, _ in
            try? FileManager.default.removeItem(at: file)
            self?.snapshotter = nil
            done(snapshot?.image.cgImage)
        }
    }

    func stop() {
        map.delegate = nil
    }

    private func showPins() {
        guard let style else { return }
        let scale = traitCollection.displayScale
        for emoji in pins?.icons ?? [] where style.image(forName: emoji) == nil {
            guard let image = MapEmoji.image(emoji, scale: scale) else { continue }
            style.setImage(UIImage(cgImage: image, scale: scale, orientation: .up), forName: emoji)
        }
        (style.source(withIdentifier: "pins") as? MLNShapeSource)?.shape = Self.shape(pins?.geoJSON ?? Self.empty)
    }

    private func showLabels() {
        guard let layer = style?.layer(withIdentifier: "pin-label") as? MLNSymbolStyleLayer else { return }
        layer.textFontSize = NSExpression(forConstantValue: narrow ? 10 : 12)
        layer.maximumTextWidth = NSExpression(forConstantValue: narrow ? 12 : 13)
    }

    private func tiles(_ failed: Bool) {
        guard failed != failing else { return }
        failing = failed
        if failed { map.alpha = 1 }
        onTiles(failed)
    }

    @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
        let point = recognizer.location(in: map)
        guard let pin = map.visibleFeatures(at: point, styleLayerIdentifiers: ["pin"]).first, let label = pin.attribute(forKey: "label") as? String else { return }
        onPin(label)
    }

    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }

    func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
        self.style = style
        showPins()
        show(line: line)
        showLabels()
    }

    func mapViewDidFinishLoadingMap(_ mapView: MLNMapView) {
        map.alpha = 1
    }

    func mapViewDidFailLoadingMap(_ mapView: MLNMapView, withError error: Error) {
        tiles(true)
    }

    func mapView(
        _ mapView: MLNMapView, tileDidTriggerAction operation: MLNTileOperation, x: Int, y: Int, z: Int, wrap: Int, overscaledZ: Int, sourceID: String
    ) {
        guard sourceID == "openmaptiles", operation == .error || operation == .loadFromNetwork else { return }
        DispatchQueue.main.async { [weak self] in self?.tiles(operation == .error) }
    }

    func mapView(_ mapView: MLNMapView, regionWillChangeWith reason: MLNCameraChangeReason, animated: Bool) {
        guard !reason.isDisjoint(with: Self.gestures) else { return }
        onUserMove()
    }

    func mapView(_ mapView: MLNMapView, regionDidChangeWith reason: MLNCameraChangeReason, animated: Bool) {
        let center = map.centerCoordinate
        let points = (pins?.pins ?? []).reduce(into: [String: CGPoint]()) { points, pin in
            points[pin.key] = map.convert(CLLocationCoordinate2D(latitude: pin.lat, longitude: pin.lng), toPointTo: self)
        }
        onRest(MapCamera(lat: center.latitude, lng: center.longitude, zoom: map.zoomLevel), points)
    }

    func mapSnapshotter(_ snapshotter: MLNMapSnapshotter, didFinishLoading style: MLNStyle) {
        let scale = traitCollection.displayScale
        for emoji in pins?.icons ?? [] {
            guard let image = MapEmoji.image(emoji, scale: scale) else { continue }
            style.setImage(UIImage(cgImage: image, scale: scale, orientation: .up), forName: emoji)
        }
    }

    private static func camera(_ camera: MapCamera, size: CGSize) -> MLNMapCamera {
        let center = CLLocationCoordinate2D(latitude: camera.lat, longitude: camera.lng)
        let altitude = MLNAltitudeForZoomLevel(camera.zoom, 0, camera.lat, size)
        return MLNMapCamera(lookingAtCenter: center, altitude: altitude, pitch: 0, heading: 0)
    }

    private static func shape(_ object: [String: Any]) -> MLNShape? {
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return nil }
        return try? MLNShape(data: data, encoding: String.Encoding.utf8.rawValue)
    }
}

/// Has the map's requests for tiles and glyphs say they come from Motile.
private final class MapNetwork: NSObject, MLNNetworkConfigurationDelegate {
    private static let shared = MapNetwork()
    private static var started = false

    static func start() {
        guard !started else { return }
        started = true
        MLNNetworkConfiguration.sharedManager.delegate = shared
    }

    func willSend(_ request: NSMutableURLRequest) -> NSMutableURLRequest {
        request.setValue(MapStyle.userAgent, forHTTPHeaderField: "User-Agent")
        return request
    }
}

/// The expanded map: a cover over the whole screen, with Collapse at its top right.
final class MapExpansion {
    private let cover: MapCoverController

    init?(stage: MapStage, from map: MapView, collapse: @escaping () -> Void) {
        guard let presenter = map.presenter else { return nil }
        cover = MapCoverController(stage: stage)
        presenter.present(cover, animated: !Platform.reducesMotion)
    }

    func dismiss(_ done: @escaping () -> Void) {
        cover.dismiss(animated: !Platform.reducesMotion, completion: done)
    }
}

private final class MapCoverController: UIViewController {
    private let stage: MapStage

    init(stage: MapStage) {
        self.stage = stage
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Theme.background
        view.addSubview(stage)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        stage.frame = view.bounds.inset(by: view.safeAreaInsets)
    }
}

extension MapStage {
    /// The map as assistive technologies find it: a region "Map" of its controls and its pins.
    func describeMap(pins: [MapPinAccess]) {
        isAccessibilityElement = false
        accessibilityLabel = "Map"
        accessibilityContainerType = .semanticGroup
        let controls: [Any] = subviews.filter { !$0.isHidden && $0.isAccessibilityElement }
        let elements: [Any] = pins.map { MapPinElement($0, in: self) }
        accessibilityElements = controls + elements
    }
}

private final class MapPinElement: UIAccessibilityElement {
    private let press: () -> Void

    init(_ pin: MapPinAccess, in stage: UIView) {
        press = pin.press
        super.init(accessibilityContainer: stage)
        accessibilityLabel = pin.label
        accessibilityTraits = pin.selected ? [.button, .selected] : .button
        let center = pin.center ?? CGPoint(x: stage.bounds.midX, y: stage.bounds.midY)
        let side = MapEmoji.side
        accessibilityFrameInContainerSpace = CGRect(x: center.x - side / 2, y: center.y - side / 2, width: side, height: side)
    }

    override func accessibilityActivate() -> Bool {
        press()
        return true
    }
}
#endif
