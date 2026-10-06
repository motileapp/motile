import XCTest

/// What fingers do, against the dev account (scripts/ui-test.sh starts it): the swipes that show
/// the sidebar and the panel, and the ones that must not.
final class GestureTests: XCTestCase {
    private let app = XCUIApplication()

    override func setUp() {
        continueAfterFailure = false
        for key in ["MOTILE_AUTH_URL", "MOTILE_LOCAL", "MOTILE_SERVER_ADDR", "MOTILE_DEMO_SIGN_IN"] {
            app.launchEnvironment[key] = ProcessInfo.processInfo.environment[key]
        }
        app.launch()
        XCTAssertTrue(app.buttons["Threads"].waitForExistence(timeout: 30))
    }

    private var newThread: XCUIElement { app.buttons["New thread"] }

    private func shown(_ element: XCUIElement) -> Bool {
        element.waitForExistence(timeout: 3) && element.isHittable
    }

    /// The thread's row in the sidebar, not its title on the thread that is off the screen.
    private func row(_ title: String) -> XCUIElement? {
        XCTAssertTrue(shown(newThread))
        return app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", title))
            .allElementsBoundByIndex.first { $0.isHittable }
    }

    func testASwipeToTheRightShowsTheSidebarAndOneBackHidesIt() {
        XCTAssertFalse(newThread.exists && newThread.isHittable)
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.swipeLeft()
        XCTAssertTrue(newThread.waitForNonExistence(timeout: 3) || !newThread.isHittable)
    }

    func testAThreadOpensFromTheSidebarAndScrolls() {
        app.swipeRight()
        guard let thread = row("Add API Rate Limiting") else { return XCTFail("no row") }
        thread.tap()
        XCTAssertTrue(newThread.waitForNonExistence(timeout: 3) || !newThread.isHittable)
        // Up and down is the transcript's, and leaves the sidebar where it is.
        app.swipeDown()
        app.swipeUp()
        XCTAssertFalse(newThread.exists && newThread.isHittable)
    }

    func testASwipeToTheLeftShowsThePanelAndOneBackHidesItWithoutTheSidebar() {
        let back = app.navigationBars.buttons["Back"]
        app.swipeLeft()
        XCTAssertTrue(shown(back))
        app.swipeRight()
        XCTAssertTrue(back.waitForNonExistence(timeout: 3))
        XCTAssertTrue(app.buttons["Threads"].isHittable)
        XCTAssertFalse(newThread.exists && newThread.isHittable)
    }

    func testThePanelsBackButtonHidesIt() {
        app.buttons["Files and changes"].tap()
        let back = app.navigationBars.buttons["Back"]
        XCTAssertTrue(shown(back))
        back.tap()
        XCTAssertTrue(back.waitForNonExistence(timeout: 3))
        XCTAssertTrue(app.buttons["Threads"].isHittable)
    }

    func testASwipeToTheRightOnAThreadInTheSidebarMarksItDone() {
        app.swipeRight()
        guard let thread = row("Use an F-String in Greet") else { return XCTFail("no row") }
        thread.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: thread.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)))
        let undo = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Marked done'")).firstMatch
        XCTAssertTrue(shown(undo))
        // The swipe was the row's, so the sidebar is where it was.
        XCTAssertTrue(newThread.isHittable)
        undo.tap()
        XCTAssertTrue(undo.waitForNonExistence(timeout: 3))
    }

    func testSettingsArePushedAndASwipeFromTheEdgeTakesThemAway() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.buttons["Account"].tap()
        app.buttons["Settings"].tap()
        let title = app.navigationBars["Settings"]
        XCTAssertTrue(shown(title))
        app.buttons["Usage"].tap()
        XCTAssertTrue(shown(app.navigationBars["Usage"]))
        swipeFromTheEdge()
        XCTAssertTrue(shown(title))
        swipeFromTheEdge()
        XCTAssertTrue(title.waitForNonExistence(timeout: 3))
        XCTAssertTrue(shown(newThread))
    }

    private func swipeFromTheEdge() {
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.01, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)), withVelocity: .slow, thenHoldForDuration: 0)
    }

    func testATapBesideTheSidebarsSearchPutsTheKeyboardAway() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.textFields["Search"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
        app.staticTexts["Motile"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        XCTAssertTrue(newThread.isHittable)
    }

    func testATapBesideTheComposersTextBringsTheKeyboard() {
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: -5)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
    }
}
