import XCTest

@testable import MotileKit

/// The order of what a blank tab offers, as it is saved and as rows are dragged.
final class PanelLauncherOrderTests: XCTestCase {
    func testLoadsTheDefaultsWhenNothingIsSaved() {
        XCTAssertEqual(PanelLauncherOrder.loaded([]), PanelLauncherOrder.defaults)
    }

    func testDropsWhatIsUnknownAndAddsWhatIsNewInItsPlace() {
        let loaded = PanelLauncherOrder.loaded(["diff", "gone", "agents", "diff", "ricrac", "files"])
        XCTAssertEqual(loaded, [.diff, .pullRequest, .pullRequests, .linear, .agents, .ricrac, .files])
    }

    func testMovesARowAmongTheShownOnes() {
        let order: [PanelTab] = [.agents, .files, .diff, .pullRequest, .pullRequests, .linear, .ricrac]
        let shown: [PanelTab] = [.agents, .files, .diff, .pullRequest, .linear]
        XCTAssertEqual(
            PanelLauncherOrder.moved(order, .agents, to: 3, among: shown),
            [.files, .diff, .pullRequest, .pullRequests, .agents, .linear, .ricrac])
        XCTAssertEqual(
            PanelLauncherOrder.moved(order, .files, to: 4, among: shown),
            [.agents, .diff, .pullRequest, .pullRequests, .linear, .files, .ricrac])
        XCTAssertEqual(
            PanelLauncherOrder.moved(order, .linear, to: 0, among: shown),
            [.linear, .agents, .files, .diff, .pullRequest, .pullRequests, .ricrac])
    }
}
