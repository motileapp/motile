import CoreGraphics
import XCTest

@testable import MotileKit

/// A program's texts are selected as one and copied in the order they read.
final class ProgramSelectionTests: XCTestCase {
    private static func text(_ string: String) -> JSON {
        ["kind": "text", "text": ["text": string, "runs": [], "links": []]]
    }

    private static func badge(_ id: String, _ string: String) -> JSON {
        ["kind": "node", "node": ["id": id, "component": "Badge", "text": string, "tone": "success", "icon": NSNull()]]
    }

    private static let card: JSON = [
        "id": "", "component": "Card",
        "children": [
            ["id": "children/0", "component": "Title", "text": ["text": "Who earns", "runs": [], "links": []], "style": "card_title", "color": "foreground"],
            [
                "id": "children/1", "component": "Table",
                "columns": [["title": "Player", "numeric": false], ["title": "Margin", "numeric": true], ["title": "Difficulty", "numeric": false]],
                "rows": [
                    [text("Pilots"), text("55%"), badge("children/1/rows/0/2", "Easy")],
                    [text("Makers"), text("38%"), badge("children/1/rows/1/2", "Hard")],
                ],
            ],
        ],
    ]

    private func selection(of tree: JSON) throws -> ProgramSelection {
        let node = try XCTUnwrap(UiNode(json: tree))
        let box = ProgramLayout.layout(node, width: 720, env: ProgramEnv(column: 720, window: CGSize(width: 1200, height: 800)))
        let view = ProgramViews.make(for: box.node)
        view.frame = CGRect(origin: .zero, size: box.size)
        view.show(box, host: nil)
        #if os(macOS)
        view.layoutSubtreeIfNeeded()
        #else
        view.layoutIfNeeded()
        #endif
        let selection = ProgramSelection()
        selection.collect(view.selectables, in: view)
        return selection
    }

    func testATableCopiesAsRowsOfCellsSeparatedByTabs() throws {
        let selection = try selection(of: Self.card)
        selection.selectAll()
        XCTAssertEqual(selection.copied(), "Who earns\nPlayer\tMargin\tDifficulty\nPilots\t55%\tEasy\nMakers\t38%\tHard")
    }

    func testASelectionFromInsideOneCellToInsideAnotherTakesWhatLiesBetween() throws {
        let selection = try selection(of: Self.card)
        let start = selection.text.range(of: "lots").location
        let end = NSMaxRange(selection.text.range(of: "Mak"))
        selection.select(NSRange(location: start, length: end - start))
        XCTAssertEqual(selection.copied(), "lots\t55%\tEasy\nMak")
    }
}
