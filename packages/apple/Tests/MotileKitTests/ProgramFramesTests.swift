import CoreGraphics
import XCTest

@testable import MotileKit

/// Every program of the corpus laid out as the transcript lays it out: each node has a frame, the
/// program fills the column, and nothing is drawn at a size that isn't one. The frames are written to
/// build/frames/<mac|ios>/<program>-<width>.json for `scripts/compare-frames.sh`.
final class ProgramFramesTests: XCTestCase {
    #if os(macOS)
    private static let output = Fixtures.repository.appendingPathComponent("build/frames/mac")
    #else
    private static let output = Fixtures.repository.appendingPathComponent("build/frames/ios")
    #endif

    func testEveryNodeOfTheCorpusHasAFrameAtBothWidths() throws {
        try FileManager.default.createDirectory(at: Self.output, withIntermediateDirectories: true)
        for (name, tree) in try Fixtures.programs() {
            for width: CGFloat in [390, 720] {
                let frames = ProgramFrames.frames(tree: tree, width: width)
                let dump = Self.output.appendingPathComponent("\(name.dropLast(5))-\(Int(width)).json")
                try ProgramFrames.json(tree: tree, width: width).write(to: dump, atomically: true, encoding: .utf8)
                XCTAssertEqual(frames[""]?[2], Double(width), "\(name) at \(width) fills the column")
                for id in Self.shownIDs(tree) {
                    let frame = try XCTUnwrap(frames[id], "\(name) at \(width) has no frame for \(id)")
                    XCTAssertTrue(frame.allSatisfy { $0.isFinite }, "\(name) \(id)")
                    XCTAssertGreaterThanOrEqual(frame[2], 0, "\(name) \(id)")
                    XCTAssertGreaterThanOrEqual(frame[3], 0, "\(name) \(id)")
                }
            }
        }
    }

    func testANarrowColumnPadsCardsLess() throws {
        let tree: JSON = [
            "id": "", "component": "Card", "soft": false, "nested": false, "spacing": [0],
            "children": [["id": "children/0", "component": "Spacer"]],
        ]
        XCTAssertEqual(ProgramFrames.frames(tree: tree, width: 720)["children/0"]?[0], 20)
        XCTAssertEqual(ProgramFrames.frames(tree: tree, width: 390)["children/0"]?[0], 16)
    }

    func testAGridFitsAsManyColumnsAsTheirLeastWidthAllows() {
        let grid = { (min: Double) in
            UiKind.Grid(json: ["children": [], "columns": 4, "min_child_width": min, "gap": 12])
        }
        XCTAssertEqual(ProgramLayout.columns(grid(200), width: 720), 3)
        XCTAssertEqual(ProgramLayout.columns(grid(200), width: 390), 1)
        XCTAssertEqual(ProgramLayout.columns(grid(100), width: 720), 4)
        let justFits = UiKind.Grid(json: ["children": [], "columns": 3, "min_child_width": 100, "gap": 8])
        XCTAssertEqual(ProgramLayout.columns(justFits, width: 390), 3)
    }

    /// The ids of the nodes a program shows in its row: not those of a popover's panel, which shows
    /// over it, nor an Svg's shapes, which have none.
    private static func shownIDs(_ value: Any, inPanel: Bool = false) -> [String] {
        switch value {
        case let node as JSON:
            var ids: [String] = []
            if !inPanel, let id = node["id"] as? String, node["component"] is String { ids.append(id) }
            for (key, child) in node {
                let panel = inPanel || node["component"] as? String == "Popover" && key == "content"
                ids += shownIDs(child, inPanel: panel)
            }
            return ids
        case let list as [Any]:
            return list.flatMap { shownIDs($0, inPanel: inPanel) }
        default:
            return []
        }
    }
}
