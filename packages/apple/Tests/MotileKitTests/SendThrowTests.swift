import XCTest

@testable import MotileKit

/// A sent message flies on the springs and the curve Messages throws with, worked out as Core
/// Animation runs them.
final class SendThrowTests: XCTestCase {
    private let travel = Spring(mass: 1, stiffness: 141.759, damping: 17.3503)

    func testTheTravelSpringLeavesAtRestOvershootsALittleAndSettles() {
        XCTAssertEqual(travel.progress(-0.1), 0)
        XCTAssertEqual(travel.progress(0), 0)
        XCTAssertEqual(travel.progress(0.1), 0.3868, accuracy: 0.0005)
        let highest = stride(from: 0.0, to: 1, by: 0.001).map(travel.progress).max() ?? 0
        XCTAssertEqual(highest, 1.0354, accuracy: 0.0005)
        XCTAssertEqual(travel.progress(0.88), 1, accuracy: 0.001)
    }

    func testTheShrinkSpringIsMostlyThereAFifthOfASecondIn() {
        XCTAssertEqual(Spring(mass: 2, stiffness: 310, damping: 38).progress(0.2), 0.8293, accuracy: 0.0005)
    }

    func testTheReshapeCurveEasesInAndOut() {
        let curve = CubicCurve(0.542, 0, 0.58, 1)
        XCTAssertEqual(curve.value(-1), 0)
        XCTAssertEqual(curve.value(0.25), 0.0864, accuracy: 0.0005)
        XCTAssertEqual(curve.value(0.5), 0.4137, accuracy: 0.0005)
        XCTAssertEqual(curve.value(2), 1)
    }
}
