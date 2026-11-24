import XCTest

final class TracksUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testTrackListRenameShareDelete() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.recordWalk()

        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        let row = app.buttons["track-row-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("Wandern"), row.label)
        row.tap()

        let title = app.staticTexts["track-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        app.buttons["track-rename"].tap()
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        // It holds the name given when recording.
        XCTAssertTrue((field.value as? String ?? "").hasSuffix(" Wandern"))
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 30) + "Sylvenstein Runde")
        app.alerts.buttons["Umbenennen"].tap()
        XCTAssertTrue(app.staticTexts["Sylvenstein Runde"].waitForExistence(timeout: 5))

        // The share sheet of iOS opens with the GPX file.
        app.buttons["track-share"].tap()
        let share = app.otherElements["ActivityListView"]
        XCTAssertTrue(share.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Sylvenstein Runde"].exists)
        // Its buttons live in another process; a tap beside it closes it.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1)).tap()
        XCTAssertTrue(share.waitForNonExistence(timeout: 5))

        app.buttons["track-delete"].tap()
        // Asks first: "Abbrechen" keeps it.
        app.alerts.buttons["Abbrechen"].tap()
        XCTAssertTrue(title.exists)
        app.buttons["track-delete"].tap()
        app.alerts.buttons["Löschen"].tap()
        XCTAssertTrue(app.staticTexts["tracks-empty"].waitForExistence(timeout: 5))
    }
}
