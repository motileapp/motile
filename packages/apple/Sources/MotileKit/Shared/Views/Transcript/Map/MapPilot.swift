import CoreGraphics
import Foundation
import QuartzCore

/// Where a map looks: its centre and MapLibre's zoom level, which every engine takes as it is.
struct MapCamera: Equatable {
    var lat: Double
    var lng: Double
    var zoom: Double

    /// Where a map with no pin yet looks.
    static let world = MapCamera(lat: 20, lng: 0, zoom: 1)
}

/// The core's camera maths for the map (`crates/core/src/render/openui/map.rs`).
enum MapMaths {
    enum Framing: String {
        /// The first pin while the program streams.
        case first
        /// Every pin, once the program ends or the filter changes.
        case fit
        /// Every pin, from the recenter button.
        case recenter
    }

    static let flyDuration: TimeInterval = 0.92
    static let drawDuration: TimeInterval = 0.76
    /// How long after a fly starts the route begins to draw.
    static let framedDelay: TimeInterval = 0.96

    /// Where the camera goes for the points (`[lat, lng]`) in a map of the size.
    static func frame(_ points: [[Double]], size: CGSize, _ framing: Framing) -> MapCamera? {
        let question: [String: Any] = ["op": "frame", "points": points, "width": size.width, "height": size.height, "framing": framing.rawValue]
        guard let camera = CoreBridge.uiMap(question) as? [String: Any] else { return nil }
        guard let lat = camera["lat"] as? Double, let lng = camera["lng"] as? Double, let zoom = camera["zoom"] as? Double else { return nil }
        return MapCamera(lat: lat, lng: lng, zoom: zoom)
    }

    /// The route as drawn `t` (0–1, linear) of the way through its reveal.
    static func reveal(_ line: [[Double]], _ t: Double) -> [[Double]] {
        guard let points = CoreBridge.uiMap(["op": "reveal", "line": line, "t": t]) as? [[Double]] else { return line }
        return points
    }
}

/// What draws a map: MapLibre Native on iOS, MapLibre GL JS on the Mac. The pilot moves it.
protocol MapEngine: AnyObject {
    func jump(to camera: MapCamera)
    func fly(to camera: MapCamera, duration: TimeInterval)
    func show(pins: MapPins)
    /// `[lat, lng]` points; fewer than two draw nothing.
    func show(line: [[Double]])
}

/// Something under way that can be stopped.
final class MapTask {
    private var stop: (() -> Void)?

    init(stop: @escaping () -> Void) {
        self.stop = stop
    }

    func cancel() {
        stop?()
        stop = nil
    }
}

/// How the pilot waits: a moment, or every frame of an animation, called with how far along it is
/// (0–1, linear) and last with 1.
protocol MapClock {
    func after(_ delay: TimeInterval, _ run: @escaping () -> Void) -> MapTask
    func frames(over duration: TimeInterval, _ frame: @escaping (Double) -> Void) -> MapTask
}

/// The main run loop's time.
struct MainMapClock: MapClock {
    func after(_ delay: TimeInterval, _ run: @escaping () -> Void) -> MapTask {
        let work = DispatchWorkItem(block: run)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
        return MapTask { work.cancel() }
    }

    func frames(over duration: TimeInterval, _ frame: @escaping (Double) -> Void) -> MapTask {
        let start = CACurrentMediaTime()
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { timer in
            let t = min(1, (CACurrentMediaTime() - start) / duration)
            frame(t)
            if t >= 1 { timer.invalidate() }
        }
        RunLoop.main.add(timer, forMode: .common)
        return MapTask { timer.invalidate() }
    }
}

/// What a map remembers of how it was moved, so that a view made again for it carries on.
struct MapMemory {
    var anchored = false
    var userMoved = false
    var fitSignature: String?
    var framedKey: String?
    var lineDrawn = false
    /// Where the camera is, or is going.
    var camera: MapCamera?
    /// The route as drawn now.
    var line: [[Double]] = []
}

/// Moves a map's camera and draws its route as open-intelligent-ui's map does: the first pin while
/// the program streams, a fly to every pin once it ends, then the route drawn from its start. It
/// never moves the camera again once the user did. Without an engine it keeps where the camera
/// would be and what would be drawn, for the engine it gets later.
final class MapPilot {
    private(set) var memory: MapMemory
    private(set) var engine: MapEngine?
    /// The map's size in points, which the camera is framed for.
    private(set) var size = CGSize.zero
    private let clock: MapClock
    private let reducesMotion: () -> Bool
    private var map: UiKind.Map?
    private var streaming = false
    private var pins: MapPins?
    private var framing: MapTask?
    private var drawing: MapTask?
    /// The route and pins the line was last set for.
    private var drawnFor: String?

    init(memory: MapMemory = MapMemory(), clock: MapClock = MainMapClock(), reducesMotion: @escaping () -> Bool = { Platform.reducesMotion }) {
        self.memory = memory
        self.clock = clock
        self.reducesMotion = reducesMotion
    }

    deinit {
        framing?.cancel()
        drawing?.cancel()
    }

    /// Takes the map as core resolved it, while its program streams or once it ended.
    func show(_ map: UiKind.Map, streaming: Bool) {
        self.map = map
        self.streaming = streaming
        update()
    }

    /// Hands the map to an engine, or takes it away. A new engine starts where the last one was.
    func attach(_ engine: MapEngine?) {
        self.engine = engine
        guard let engine else { return }
        engine.jump(to: memory.camera ?? .world)
        if let pins { engine.show(pins: pins) }
        engine.show(line: memory.line)
        update()
    }

    /// The map's size changed. Only the first size frames anything: a resize never moves the camera.
    func resize(to size: CGSize) {
        let first = self.size.width <= 0 || self.size.height <= 0
        self.size = size
        guard first else { return }
        update()
    }

    /// The user dragged, pinched or zoomed the map.
    func userMoved() {
        memory.userMoved = true
    }

    /// The engine's camera came to rest here.
    func rested(at camera: MapCamera) {
        memory.camera = camera
    }

    /// Shows every place again.
    func recenter() {
        guard let target = MapMaths.frame(visiblePoints, size: size, .recenter) else { return }
        guard reducesMotion() || engine == nil else { return fly(target) }
        jump(target)
    }

    private var visible: [UiMapPin] { map?.pins.filter(\.visible) ?? [] }
    private var visiblePoints: [[Double]] { visible.map { [$0.lat, $0.lng] } }

    private func update() {
        guard let map, size.width > 0, size.height > 0 else { return }
        let visible = self.visible
        let points = visible.map { [$0.lat, $0.lng] }
        let routingKey = visible.map { "\($0.lng),\($0.lat)" }.joined(separator: ";")
        let pins = MapPins(visible)
        if pins != self.pins {
            self.pins = pins
            engine?.show(pins: pins)
        }
        if !points.isEmpty, !memory.anchored, let first = MapMaths.frame(points, size: size, .first) {
            jump(first)
            memory.anchored = true
        }
        guard !streaming, !points.isEmpty else { return }
        fit(points, signature: (map.filter ?? "") + "\n" + routingKey, routingKey: routingKey)
        guard memory.framedKey == routingKey else { return }
        drawRoute(map.route == .off ? [] : map.line, key: routingKey)
    }

    private func fit(_ points: [[Double]], signature: String, routingKey: String) {
        guard signature != memory.fitSignature else { return }
        memory.fitSignature = signature
        framing?.cancel()
        drawing?.cancel()
        drawnFor = nil
        setLine([])
        let target = MapMaths.frame(points, size: size, .fit)
        guard !memory.userMoved, !reducesMotion(), engine != nil, let target else {
            if !memory.userMoved, let target { jump(target) }
            memory.framedKey = routingKey
            return
        }
        fly(target)
        framing = clock.after(MapMaths.framedDelay) { [weak self] in
            self?.memory.framedKey = routingKey
            self?.update()
        }
    }

    private func drawRoute(_ positions: [[Double]], key routingKey: String) {
        let key = "\(routingKey)|\(positions)"
        guard key != drawnFor else { return }
        drawnFor = key
        drawing?.cancel()
        guard positions.count > 1, !memory.lineDrawn, !reducesMotion(), engine != nil else {
            memory.lineDrawn = positions.count > 1
            return setLine(positions)
        }
        drawing = clock.frames(over: MapMaths.drawDuration) { [weak self] t in
            self?.setLine(MapMaths.reveal(positions, t))
            guard t >= 1 else { return }
            self?.memory.lineDrawn = true
        }
    }

    private func jump(_ camera: MapCamera) {
        memory.camera = camera
        engine?.jump(to: camera)
    }

    private func fly(_ camera: MapCamera) {
        memory.camera = camera
        engine?.fly(to: camera, duration: MapMaths.flyDuration)
    }

    private func setLine(_ line: [[Double]]) {
        let line = line.count > 1 ? line : []
        memory.line = line
        engine?.show(line: line)
    }
}
