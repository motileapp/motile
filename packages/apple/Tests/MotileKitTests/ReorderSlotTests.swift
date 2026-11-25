import XCTest

@testable import MotileKit

/// Where a dragged item lands: in the slot nearest it, so a neighbour makes way once the item is
/// halfway across it.
final class ReorderSlotTests: XCTestCase {
    private let even: [ClosedRange<CGFloat>] = [0...100, 100...200, 200...300]

    private func slot(_ middle: CGFloat, from: Int, spans: [ClosedRange<CGFloat>], current: Int? = nil) -> Int {
        ReorderSlot.nearest(to: middle, from: from, spans: spans, current: current ?? from, among: 0...(spans.count - 1))
    }

    func testANeighbourOfTheSameSizeMakesWayHalfwayAcrossIt() {
        XCTAssertEqual(slot(50 + 45, from: 0, spans: even), 0)
        XCTAssertEqual(slot(50 + 55, from: 0, spans: even), 1)
        XCTAssertEqual(slot(50 + 155, from: 0, spans: even), 2)
        XCTAssertEqual(slot(250 - 55, from: 2, spans: even), 1)
    }

    func testASmallItemPassesABigNeighbourHalfwayAcrossItAndDoesntFlipBack() {
        let spans: [ClosedRange<CGFloat>] = [0...100, 100...240]
        XCTAssertEqual(slot(50 + 65, from: 0, spans: spans), 0)
        XCTAssertEqual(slot(50 + 75, from: 0, spans: spans), 1)
        // Just past the halfway point, in the new slot, it stays there.
        XCTAssertEqual(slot(50 + 75, from: 0, spans: spans, current: 1), 1)
    }

    func testATremblingPointerOnTheEdgeDoesntMoveTheOthers() {
        XCTAssertEqual(slot(50 + 52, from: 0, spans: even, current: 0), 0)
        XCTAssertEqual(slot(50 + 48, from: 0, spans: even, current: 1), 1)
        XCTAssertEqual(slot(50 + 40, from: 0, spans: even, current: 1), 0)
    }

    func testItLandsOnlyAmongTheSlotsItMay() {
        XCTAssertEqual(ReorderSlot.nearest(to: 280, from: 0, spans: even, current: 0, among: 0...1), 1)
    }

    func testItStartsWhereTheSlotIs() {
        XCTAssertEqual(ReorderSlot.start(of: 0, from: 1, spans: even), 0)
        XCTAssertEqual(ReorderSlot.start(of: 2, from: 0, spans: even), 200)
    }
}
