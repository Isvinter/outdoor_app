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

    /// Records "Wandern", switches to "Rad" during the rest and opens the saved track:
    /// three sections, the rest in the middle.
    private func recordSections() -> XCUIApplication {
        let app = XCUIApplication.replaying("rest")
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording("hike")
        XCTAssertTrue(app.staticTexts["rest-hint"].waitForExistence(timeout: 60))
        app.buttons["recording-activity"].tap()
        app.buttons["switch-bike"].tap()
        XCTAssertTrue(app.staticTexts["rest-hint"].waitForNonExistence(timeout: 5))
        // Two minutes more standing, then on the way (at ten times the speed).
        Thread.sleep(forTimeInterval: 25)
        app.buttons["recording-stop"].tap()
        XCTAssertTrue(app.buttons["track-save"].waitForExistence(timeout: 5))
        app.buttons["track-save"].tap()

        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        let row = app.buttons["track-row-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        XCTAssertTrue(app.staticTexts["track-title"].waitForExistence(timeout: 5))
        return app
    }

    func testDetailListsSections() {
        let app = recordSections()
        XCTAssertTrue(app.buttons["section-0-activity"].exists)
        XCTAssertEqual(app.buttons["section-0-activity"].label, "Wandern ▾")
        let rest = app.staticTexts["section-1-title"]
        XCTAssertTrue(rest.exists)
        XCTAssertNotNil(rest.label.range(of: #"^Rast \d\d:\d\d–\d\d:\d\d$"#, options: .regularExpression), rest.label)
        XCTAssertEqual(app.buttons["section-2-activity"].label, "Rad ▾")
        XCTAssertFalse(app.otherElements["section-3"].exists)
    }

    func testChangeSectionActivity() {
        let app = recordSections()
        XCTAssertTrue(app.staticTexts["Wandern"].exists, "the activity of the track is the first one's")
        app.buttons["section-0-activity"].tap()
        app.buttons["section-0-mtb"].tap()
        let menu = app.buttons["section-0-activity"]
        expectation(for: NSPredicate(format: "label == %@", "MTB ▾"), evaluatedWith: menu)
        waitForExpectations(timeout: 5)
        XCTAssertTrue(app.staticTexts["MTB"].exists, "the activity of the track follows")
        XCTAssertEqual(app.buttons["section-2-activity"].label, "Rad ▾")
    }

    func testRestSectionHasNoActivityMenu() {
        let app = recordSections()
        XCTAssertTrue(app.staticTexts["section-1-title"].exists)
        XCTAssertFalse(app.buttons["section-1-activity"].exists)
    }
}
