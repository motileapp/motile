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
        let hittable = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == true AND hittable == true"), object: element)
        return XCTWaiter().wait(for: [hittable], timeout: 3) == .completed
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

    func testATapOnALinearIssueOpensIt() {
        app.swipeRight()
        guard let thread = row("Add API Rate Limiting") else { return XCTFail("no row") }
        thread.tap()
        app.buttons["Files and changes"].tap()
        XCTAssertTrue(shown(app.navigationBars.buttons["Back"]))
        let linear = app.buttons["Linear"].firstMatch
        XCTAssertTrue(linear.waitForExistence(timeout: 5))
        linear.tap()
        let issue = app.buttons["ENG-1 Greet by name"]
        XCTAssertTrue(issue.waitForExistence(timeout: 10))
        app.textFields["Search"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
        issue.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        XCTAssertTrue(app.buttons["Work on Issue"].waitForExistence(timeout: 10))
        app.navigationBars.buttons["Back"].tap()
    }

    func testASwipeToTheRightOnAThreadInTheSidebarMarksItDone() {
        app.swipeRight()
        guard let thread = row("Use an F-String in Greet") else { return XCTFail("no row") }
        thread.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: thread.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)))
        let undo = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Undo'")).firstMatch
        XCTAssertTrue(shown(undo))
        // The swipe was the row's, so the sidebar is where it was.
        XCTAssertTrue(newThread.isHittable)
        undo.tap()
        XCTAssertTrue(undo.waitForNonExistence(timeout: 3))
    }

    /// A hold lifts the row and a move drags it, before its menu comes up or out of the menu.
    func testAHoldAndAMoveOnAThreadDragsItAboveAnotherAndTheOrderStays() {
        dragOneThreadAboveTheOther(holding: 0.5)
    }

    func testAThreadDragsOutOfItsMenuAboveAnotherAndTheOrderStays() {
        dragOneThreadAboveTheOther(holding: 1.5)
    }

    private func dragOneThreadAboveTheOther(holding: TimeInterval) {
        openSidebar()
        guard let one = row("Use an F-String in Greet"), let other = row("Add API Rate Limiting") else { return XCTFail("no rows") }
        let (upper, lower) = one.frame.minY < other.frame.minY ? (one, other) : (other, one)
        let (upperTitle, lowerTitle) = upper == one ? ("Use an F-String in Greet", "Add API Rate Limiting") : ("Add API Rate Limiting", "Use an F-String in Greet")
        lower.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).press(
            forDuration: holding, thenDragTo: upper.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)), withVelocity: .slow,
            thenHoldForDuration: 0.4
        )
        XCTAssertTrue(ordered(lowerTitle, above: upperTitle))
        // The drag was the row's, so the sidebar is where it was.
        XCTAssertTrue(newThread.isHittable)
        app.terminate()
        app.launch()
        XCTAssertTrue(app.buttons["Threads"].waitForExistence(timeout: 30))
        app.swipeRight()
        XCTAssertTrue(ordered(lowerTitle, above: upperTitle))
    }

    func testAHoldOnAThreadBringsUpItsMenu() {
        openSidebar()
        guard let thread = row("Screenshot of the Landing Page") else { return XCTFail("no row") }
        thread.press(forDuration: 1.2)
        let rename = app.buttons["Rename"]
        XCTAssertTrue(shown(rename))
        rename.tap()
        XCTAssertTrue(app.alerts["Rename thread"].waitForExistence(timeout: 3))
        app.alerts["Rename thread"].buttons["Cancel"].tap()
        XCTAssertTrue(app.alerts.firstMatch.waitForNonExistence(timeout: 3))
    }

    /// Swipes the sidebar in, again if the client was still loading its threads.
    private func openSidebar() {
        for _ in 0..<3 {
            app.swipeRight()
            if shown(newThread) { return }
        }
    }

    /// Whether the sidebar lists the one thread's row over the other's, once both are drawn.
    private func ordered(_ title: String, above other: String) -> Bool {
        for _ in 0..<10 {
            if let one = row(title), let two = row(other), one.frame.minY < two.frame.minY { return true }
            sleep(1)
        }
        return false
    }

    func testSettingsOpenInASheetWhoseSectionsPushAndCloseTakesThemAway() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.buttons["Account"].tap()
        app.buttons["Settings"].tap()
        let title = app.navigationBars["Settings"]
        XCTAssertTrue(shown(title))
        app.buttons["Servers"].tap()
        XCTAssertTrue(shown(app.navigationBars["Servers"]))
        swipeFromTheEdge()
        XCTAssertTrue(shown(title))
        app.buttons["Close"].tap()
        XCTAssertTrue(title.waitForNonExistence(timeout: 3))
        XCTAssertTrue(shown(newThread))
    }

    func testCommandsOpenInASheetWhosePagesPushAndCloseTakesThemAway() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.buttons["Commands"].tap()
        let title = app.navigationBars["Commands"]
        XCTAssertTrue(shown(title))
        tapRow("Add a project")
        XCTAssertTrue(shown(app.navigationBars["Add a Project"]))
        tapRow("New project")
        XCTAssertTrue(shown(app.navigationBars["New Project"]))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
        app.navigationBars["New Project"].buttons.firstMatch.tap()
        XCTAssertTrue(shown(app.navigationBars["Add a Project"]))
        swipeFromTheEdge()
        XCTAssertTrue(shown(title))
        app.buttons["Close"].tap()
        XCTAssertTrue(title.waitForNonExistence(timeout: 3))
        XCTAssertTrue(shown(newThread))
    }

    private func tapRow(_ title: String) {
        let row = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", title)).firstMatch
        XCTAssertTrue(shown(row))
        row.tap()
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
        app.images["Motile"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        XCTAssertTrue(newThread.isHittable)
    }

    func testTheSidebarsSearchTakesItsLineUntilItIsClosed() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.textFields["Search"].tap()
        app.typeText("api")
        let close = app.buttons["Close search"]
        XCTAssertTrue(shown(close))
        XCTAssertFalse(newThread.exists)
        close.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        XCTAssertTrue(shown(newThread))
        XCTAssertEqual(app.textFields["Search"].value as? String ?? "", "")
    }

    func testTheSidebarsSearchFindsAThreadByItsMessagesAndOpensIt() {
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        app.textFields["Search"].tap()
        app.typeText("handlers")
        let found = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "reach the handlers"))
        XCTAssertTrue(found.firstMatch.waitForExistence(timeout: 5))
        guard let result = found.allElementsBoundByIndex.first(where: { $0.isHittable }) else { return XCTFail("no result") }
        XCTAssertTrue(result.label.contains("Add API Rate Limiting"))
        result.tap()
        XCTAssertTrue(newThread.waitForNonExistence(timeout: 3) || !newThread.isHittable)
        app.swipeRight()
        XCTAssertTrue(shown(newThread))
        XCTAssertEqual(app.textFields["Search"].value as? String ?? "", "")
    }

    /// A drag on what a program's page takes, its canvas here, is the page's: the transcript stays
    /// where it is and the sidebar doesn't come in. One beside the page scrolls the transcript. In a
    /// thread of its own, deleted after, so that the others don't find a canvas where they swipe.
    func testADragOnAProgramsCanvasIsThePagesAndOneBesideItScrollsTheTranscript() {
        addTeardownBlock { [self] in deleteThread("Catch the Dot") }
        app.buttons["Threads"].tap()
        XCTAssertTrue(shown(newThread))
        newThread.tap()
        send("show the ui html")
        send("show the ui html")
        guard let page = lastShown(app.webViews),
            let title = lastShown(app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Take a little break")))
        else { return XCTFail("no program") }
        let top = settled(title)
        let canvas = page.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: page.frame.width / 2, dy: 150))
        canvas.press(forDuration: 0.1, thenDragTo: canvas.withOffset(CGVector(dx: 0, dy: 160)), withVelocity: .slow, thenHoldForDuration: 0)
        XCTAssertEqual(title.frame.minY, top, accuracy: 1)
        canvas.press(forDuration: 0.1, thenDragTo: canvas.withOffset(CGVector(dx: 160, dy: 0)), withVelocity: .slow, thenHoldForDuration: 0)
        XCTAssertFalse(newThread.exists && newThread.isHittable)
        let beside = page.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 1)).withOffset(CGVector(dx: 0, dy: 40))
        beside.press(forDuration: 0.1, thenDragTo: beside.withOffset(CGVector(dx: 0, dy: 160)), withVelocity: .slow, thenHoldForDuration: 0)
        XCTAssertGreaterThan(title.frame.minY, top + 50)
    }

    /// A finger dragged on a map pans it: the transcript stays where it is and the sidebar doesn't
    /// come in. In a thread of its own, deleted after.
    func testAFingerDraggedOnAMapPansItAndLeavesTheTranscript() {
        addTeardownBlock { [self] in deleteThread("San Francisco, Your Way") }
        app.buttons["Threads"].tap()
        XCTAssertTrue(shown(newThread))
        newThread.tap()
        send("show the ui map")
        send("show the ui map")
        guard let map = lastShown(app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Map"))),
            let title = lastShown(app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "San Francisco, your way")))
        else { return XCTFail("no map") }
        let top = settled(title)
        let pin = { self.lastShown(self.app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Coit Tower")))?.frame.midY ?? 0 }
        let before = pin()
        let start = map.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4))
        start.press(forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 0, dy: 80)), withVelocity: .slow, thenHoldForDuration: 0)
        sleep(2)
        XCTAssertEqual(title.frame.minY, top, accuracy: 1)
        XCTAssertGreaterThan(pin(), before + 40)
        start.press(forDuration: 0.1, thenDragTo: start.withOffset(CGVector(dx: 160, dy: 0)), withVelocity: .slow, thenHoldForDuration: 0)
        XCTAssertFalse(newThread.exists && newThread.isHittable)
    }

    private func deleteThread(_ title: String) {
        app.buttons["Threads"].tap()
        guard let thread = row(title) else { return }
        thread.press(forDuration: 1.2)
        for _ in 0..<2 {
            let delete = app.buttons["Delete"].firstMatch
            XCTAssertTrue(shown(delete))
            delete.tap()
        }
        let rows = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", title))
        XCTAssertTrue(rows.firstMatch.waitForNonExistence(timeout: 5))
    }

    /// Where the element's top is once the rows around it stop moving.
    private func settled(_ element: XCUIElement) -> CGFloat {
        var top = element.frame.minY
        for _ in 0..<10 {
            sleep(1)
            guard element.frame.minY != top else { break }
            top = element.frame.minY
        }
        return top
    }

    /// Sends a message and waits for its reply, with the keyboard put away.
    private func send(_ message: String) {
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        text.tap()
        text.typeText(message)
        app.buttons["Send"].tap()
        _ = app.buttons["Stop"].waitForExistence(timeout: 5)
        XCTAssertTrue(app.buttons["Stop"].waitForNonExistence(timeout: 30))
        text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: -30)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        sleep(2)
    }

    /// The last of the elements that is on the screen, once one is.
    private func lastShown(_ query: XCUIElementQuery) -> XCUIElement? {
        for _ in 0..<30 {
            if let shown = query.allElementsBoundByIndex.last(where: { $0.isHittable }) { return shown }
            sleep(1)
        }
        return nil
    }

    func testATapBesideTheComposersTextBringsTheKeyboard() {
        let text = app.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: -5)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
    }
}
