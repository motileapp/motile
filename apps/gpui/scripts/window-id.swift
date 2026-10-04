// Prints the id of a window of the process whose id is given, for `screencapture -l`: the one
// whose title has the text given, or the largest.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count >= 2, let pid = Int(CommandLine.arguments[1]) else {
    FileHandle.standardError.write(Data("usage: window-id <pid> [title]\n".utf8))
    exit(2)
}
let title = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil
let windows = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
let mine = windows.filter { window in
    window[kCGWindowOwnerPID as String] as? Int == pid && window[kCGWindowLayer as String] as? Int == 0
}
func area(_ window: [String: Any]) -> Double {
    let bounds = window[kCGWindowBounds as String] as? [String: Double] ?? [:]
    return (bounds["Width"] ?? 0) * (bounds["Height"] ?? 0)
}
let named = mine.filter { window in
    guard let title else { return true }
    return (window[kCGWindowName as String] as? String ?? "").contains(title)
}
guard let largest = named.max(by: { area($0) < area($1) }), let id = largest[kCGWindowNumber as String] as? Int else {
    exit(1)
}
print(id)
