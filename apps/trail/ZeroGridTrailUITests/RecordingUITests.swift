import XCTest

extension XCUIApplication {
    /// The app on the mini region with the GPS replaced by a replayed walk across it.
    static func replaying(_ gpx: String = "sylvenstein", arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-selftest", "-resetData"]
        let bundle = Bundle(for: RecordingUITests.self)
        if let package = bundle.url(forResource: "mini", withExtension: nil) {
            app.launchArguments += ["-importFixture", package.path]
        }
        if let track = bundle.url(forResource: gpx, withExtension: "gpx", subdirectory: "replay") {
            app.launchArguments += ["-replayGPX", track.path]
        }
        app.launchArguments += arguments
        app.launch()
        return app
    }

    /// Records a walk and saves it as "Wandern".
    func recordWalk(seconds: TimeInterval = 8, file: StaticString = #filePath, line: UInt = #line) {
        buttons["record"].tap()
        let distance = staticTexts["recording-distance"]
        XCTAssertTrue(distance.waitForExistence(timeout: 10), file: file, line: line)
        Thread.sleep(forTimeInterval: seconds)
        // Without a point the core would discard the track on stopping.
        XCTAssertNotEqual(distance.label, "0 m", "nothing recorded", file: file, line: line)
        buttons["recording-stop"].tap()
        XCTAssertTrue(buttons["track-save"].waitForExistence(timeout: 5), file: file, line: line)
        buttons["activity-hike"].tap()
        buttons["track-save"].tap()
        XCTAssertTrue(buttons["track-save"].waitForNonExistence(timeout: 5), file: file, line: line)
    }
}

final class RecordingUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// One tap records; nothing is asked before.
    func testRecordPauseResumeStopSave() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }

        app.buttons["record"].tap()
        let distance = app.staticTexts["recording-distance"]
        XCTAssertTrue(distance.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["record"].exists)
        expectation(for: NSPredicate(format: "label != %@", "0 m"), evaluatedWith: distance)
        waitForExpectations(timeout: 20)

        app.buttons["recording-pause"].tap()
        XCTAssertEqual(app.buttons["recording-pause"].label, "Weiter")
        app.buttons["recording-pause"].tap()
        XCTAssertEqual(app.buttons["recording-pause"].label, "Pause")

        app.buttons["recording-stop"].tap()
        XCTAssertTrue(distance.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Track speichern?"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["activity-other"].value as? String, "gewählt")
        app.buttons["activity-bike"].tap()
        XCTAssertEqual(app.buttons["activity-bike"].value as? String, "gewählt")
        XCTAssertTrue(app.staticTexts["save-track-name"].label.contains(" Rad · "))
        app.buttons["track-save"].tap()
        XCTAssertTrue(app.buttons["record"].waitForExistence(timeout: 5))

        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        let row = app.buttons["track-row-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("Rad"), row.label)
    }

    /// "Verwerfen" asks first, then the track is gone.
    func testStopAndDiscard() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.buttons["record"].tap()
        let distance = app.staticTexts["recording-distance"]
        expectation(for: NSPredicate(format: "label != %@", "0 m"), evaluatedWith: distance)
        waitForExpectations(timeout: 20)
        app.buttons["recording-stop"].tap()

        app.buttons["track-discard"].tap()
        XCTAssertTrue(app.staticTexts["Track verwerfen?"].exists)
        app.buttons["discard-cancel"].tap()
        XCTAssertTrue(app.staticTexts["Track speichern?"].exists)
        app.buttons["track-discard"].tap()
        app.buttons["discard-confirm"].tap()
        XCTAssertTrue(app.buttons["record"].waitForExistence(timeout: 5))

        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        XCTAssertTrue(app.staticTexts["tracks-empty"].waitForExistence(timeout: 5))
    }

    /// Ended during a recording, the app saves the piece at the next start and says so.
    func testInterruptedRecordingIsSaved() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.buttons["record"].tap()
        let distance = app.staticTexts["recording-distance"]
        expectation(for: NSPredicate(format: "label != %@", "0 m"), evaluatedWith: distance)
        waitForExpectations(timeout: 20)
        app.terminate()

        let again = XCUIApplication()
        again.launchArguments = ["-selftest"]
        again.launch()
        let notice = again.staticTexts["Eine Aufzeichnung wurde unterbrochen und als Track gespeichert."]
        XCTAssertTrue(notice.waitForExistence(timeout: 20))
        XCTAssertFalse(again.staticTexts["recording-distance"].exists)
        again.pullSheet(by: -700)
        again.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        XCTAssertTrue(again.buttons["track-row-1"].waitForExistence(timeout: 5))
    }
}
