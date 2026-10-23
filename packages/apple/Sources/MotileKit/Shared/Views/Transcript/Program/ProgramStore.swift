import Foundation
import os

/// The trees of the program rows as the core last sent them, by row, so that a `ui_patch` can be
/// applied to the row's latest tree. It is used on the queue that decodes the core's events.
enum ProgramStore {
    private struct Entry {
        var tree: JSON?
        var errors: [String]
        let cache: ProgramCache
    }

    private static let entries = OSAllocatedUnfairLock<[String: Entry]>(initialState: [:])

    /// Keeps the tree a row came with, and hands over the measurements its nodes share.
    static func remember(rowID: String, tree: JSON?, errors: [String]) -> ProgramCache {
        entries.withLock { entries in
            let cache = entries[rowID]?.cache ?? ProgramCache()
            cache.clear()
            entries[rowID] = Entry(tree: tree, errors: errors, cache: cache)
            return cache
        }
    }

    /// The row's tree as the core last sent it.
    static func tree(rowID: String) -> JSON? {
        entries.withLock { $0[rowID]?.tree }
    }

    /// Applies the ops to the row's tree: its tree and errors after them, with the cache's
    /// generation they are laid out in, or nothing when the row isn't known.
    static func patch(rowID: String, ops: [JSON]) -> (tree: JSON?, errors: [String], cache: ProgramCache, generation: Int)? {
        entries.withLock { entries in
            guard var entry = entries[rowID] else { return nil }
            for op in ops {
                apply(op, to: &entry)
            }
            entries[rowID] = entry
            return (entry.tree, entry.errors, entry.cache, entry.cache.current)
        }
    }

    private static func apply(_ op: JSON, to entry: inout Entry) {
        switch op.string("op") {
        case "root":
            entry.tree = op.object("node")
            entry.cache.clear()
        case "replace":
            guard let node = op.object("node") else { return }
            let id = op.string("id")
            entry.tree = entry.tree.map { ProgramPatch.change(id, in: $0) { _ in node } }
            entry.cache.forget(id)
        case "props":
            let id = op.string("id")
            let set = op.object("set") ?? [:]
            let unset = op.strings("unset")
            entry.tree = entry.tree.map {
                ProgramPatch.change(id, in: $0) { node in
                    var node = node
                    for (key, value) in set { node[key] = value }
                    for key in unset { node.removeValue(forKey: key) }
                    return node
                }
            }
            entry.cache.forget(id)
        case "insert":
            guard let child = op.object("node") else { return }
            let (parent, key, index) = (op.string("parent"), op.string("key"), op.int("index"))
            entry.tree = entry.tree.map {
                ProgramPatch.change(parent, in: $0) { node in
                    var node = node
                    var list = node[key] as? [Any] ?? []
                    list.insert(child, at: min(max(0, index), list.count))
                    node[key] = list
                    return node
                }
            }
            entry.cache.forget(parent)
        case "remove":
            let id = op.string("id")
            entry.tree = entry.tree.map { ProgramPatch.removing(id, from: $0) }
            entry.cache.forget(id)
        case "errors":
            entry.errors = op.strings("errors")
        default:
            break
        }
    }
}

/// Changes a node of a tree's JSON, found by its id. A node's id is its path from the root, so
/// only the nodes whose id leads to it are looked into.
enum ProgramPatch {
    static func change(_ id: String, in tree: JSON, _ transform: (JSON) -> JSON) -> JSON {
        changed(tree, id: id, transform) as? JSON ?? tree
    }

    static func removing(_ id: String, from tree: JSON) -> JSON {
        removed(tree, id: id) as? JSON ?? tree
    }

    /// Whether `id` is `ancestor` or lies under it.
    static func leads(_ ancestor: String, to id: String) -> Bool {
        ancestor.isEmpty || id == ancestor || id.hasPrefix(ancestor + "/")
    }

    private static func isNode(_ object: JSON) -> Bool {
        object["id"] is String && object["component"] is String
    }

    private static func changed(_ value: Any, id: String, _ transform: (JSON) -> JSON) -> Any {
        switch value {
        case let object as JSON:
            if isNode(object) {
                let own = object.string("id")
                if own == id { return transform(object) }
                guard leads(own, to: id) else { return object }
            }
            return object.mapValues { changed($0, id: id, transform) }
        case let list as [Any]:
            return list.map { changed($0, id: id, transform) }
        default:
            return value
        }
    }

    private static func removed(_ value: Any, id: String) -> Any {
        switch value {
        case let object as JSON:
            if isNode(object), !leads(object.string("id"), to: id) { return object }
            return object.mapValues { removed($0, id: id) }
        case let list as [Any]:
            return list.filter { item in
                guard let object = item as? JSON, isNode(object) else { return true }
                return object.string("id") != id
            }.map { removed($0, id: id) }
        default:
            return value
        }
    }
}

/// The boxes a program's nodes were laid out in, kept while their nodes stay the same, so that a
/// patch only lays out again what it changed and what holds it.
final class ProgramCache: @unchecked Sendable {
    private let lock = NSLock()
    private var boxes: [String: [ProgramCacheKey: ProgramBox]] = [:]
    private var widths: [String: [ProgramCacheKey: CGFloat]] = [:]
    private var generation = 0

    /// Goes up with every change, so that what a tree before it lays out is never kept.
    var current: Int { lock.withLock { generation } }

    /// What a tree of the generation `generation` kept; nothing once a change came after it.
    func box(_ key: ProgramCacheKey, generation: Int) -> ProgramBox? {
        lock.withLock { generation == self.generation ? boxes[key.id]?[key] : nil }
    }

    func keep(_ box: ProgramBox, for key: ProgramCacheKey, generation: Int) {
        lock.withLock {
            guard generation == self.generation else { return }
            boxes[key.id, default: [:]][key] = box
        }
    }

    func width(_ key: ProgramCacheKey, generation: Int) -> CGFloat? {
        lock.withLock { generation == self.generation ? widths[key.id]?[key] : nil }
    }

    func keep(width: CGFloat, for key: ProgramCacheKey, generation: Int) {
        lock.withLock {
            guard generation == self.generation else { return }
            widths[key.id, default: [:]][key] = width
        }
    }

    func clear() {
        lock.withLock {
            generation += 1
            boxes.removeAll()
            widths.removeAll()
        }
    }

    /// Lets go of the node, what lies under it and what holds it.
    func forget(_ id: String) {
        lock.withLock {
            generation += 1
            for key in Array(boxes.keys) where ProgramPatch.leads(key, to: id) || ProgramPatch.leads(id, to: key) {
                boxes.removeValue(forKey: key)
            }
            for key in Array(widths.keys) where ProgramPatch.leads(key, to: id) || ProgramPatch.leads(id, to: key) {
                widths.removeValue(forKey: key)
            }
        }
    }
}

struct ProgramCacheKey: Hashable {
    let id: String
    let width: CGFloat
    let fill: Bool
    let height: CGFloat
    let narrow: Bool
    let surface: Surface
    let align: Int
    let sized: Bool
    let window: CGSize
}
