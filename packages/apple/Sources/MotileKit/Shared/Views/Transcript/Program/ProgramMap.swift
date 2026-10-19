import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A Map: the program's places on MapLibre (`MapView`), live while its row is on screen. The map
/// of a row and node is kept a while once its row scrolls away, so that it comes back as it was.
final class ProgramMapView: ProgramNodeView {
    private var map: MapView?

    override func configure(_ box: ProgramBox) {
        guard case .map(let node) = box.node.kind else { return }
        let map = MapViews.view(for: "\(host?.rowID ?? "")\n\(box.id)")
        if map !== ownMap { adopt(map) }
        let id = box.id
        map.onSelect = { [weak self] label in self?.host?.send(id, ["type": "select", "value": label]) }
        map.onFilter = { [weak self] category in self?.host?.send(id, ["type": "filter", "category": category ?? NSNull()]) }
        map.onExpand = { [weak self] open in self?.host?.send(id, ["type": "expand", "open": open]) }
        map.set(node, streaming: host?.streaming ?? false)
    }

    override func didShow(_ box: ProgramBox) {
        guard let map = ownMap else { return }
        map.onScreen = window != nil
    }

    override func layoutNow() {
        ownMap?.frame = bounds
    }

    override func leave() {
        super.leave()
        ownMap?.onScreen = false
    }

    #if os(macOS)
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil, box != nil else { return }
        ownMap?.onScreen = true
    }
    #else
    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard window != nil, box != nil else { return }
        ownMap?.onScreen = true
    }
    #endif

    /// Its map, while another view of the same row and node hasn't taken it.
    private var ownMap: MapView? {
        guard let map, map.superview === self else { return nil }
        return map
    }

    private func adopt(_ map: MapView) {
        ownMap?.removeFromSuperview()
        self.map = map
        map.frame = bounds
        addSubview(map)
    }
}

/// The maps of the rows shown lately, by row and node, and what the ones let go remember.
enum MapViews {
    static let kept = 6

    private static var views: [(place: String, view: MapView)] = []
    private static var memories: [String: MapMemory] = [:]

    static func view(for place: String) -> MapView {
        if let index = views.firstIndex(where: { $0.place == place }) {
            let entry = views.remove(at: index)
            views.append(entry)
            return entry.view
        }
        let view = MapView(memory: memories.removeValue(forKey: place) ?? MapMemory())
        views.append((place, view))
        while views.count > kept {
            let old = views.removeFirst()
            memories[old.place] = old.view.memory
            old.view.retire()
        }
        return view
    }
}
