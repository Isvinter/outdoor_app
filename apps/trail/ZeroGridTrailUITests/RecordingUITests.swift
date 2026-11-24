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

    /// Records a walk as "Wandern" and ends it.
    func recordWalk(seconds: TimeInterval = 8, file: StaticString = #filePath, line: UInt = #line) {
        buttons["record"].tap()
        buttons["activity-hike"].tap()
        let distance = staticTexts["recording-distance"]
        XCTAssertTrue(distance.waitForExistence(timeout: 10), file: file, line: line)
        Thread.sleep(forTimeInterval: seconds)
        // Without a point the core would discard the track on stopping.
        XCTAssertNotEqual(distance.label, "0 m", "nothing recorded", file: file, line: line)
        buttons["recording-stop"].tap()
        buttons["recording-stop-confirm"].tap()
        XCTAssertTrue(distance.waitForNonExistence(timeout: 5), file: file, line: line)
    }
}

final class RecordingUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testRecordPauseResumeStop() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }

        app.buttons["record"].tap()
        // The first recording offers "Rad" first.
        XCTAssertEqual(app.buttons["activity-bike"].value as? String, "zuletzt gewählt")
        app.buttons["activity-hike"].tap()

        let distance = app.staticTexts["recording-distance"]
        XCTAssertTrue(distance.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["record"].exists)
        let moved = NSPredicate(format: "label != %@", "0 m")
        expectation(for: moved, evaluatedWith: distance)
        waitForExpectations(timeout: 20)

        app.buttons["recording-pause"].tap()
        XCTAssertEqual(app.buttons["recording-pause"].label, "Weiter")
        app.buttons["recording-pause"].tap()
        XCTAssertEqual(app.buttons["recording-pause"].label, "Pause")

        app.buttons["recording-stop"].tap()
        XCTAssertTrue(app.staticTexts["Aufzeichnung beenden?"].exists)
        app.buttons["recording-stop-cancel"].tap()
        XCTAssertTrue(distance.exists)
        app.buttons["recording-stop"].tap()
        app.buttons["recording-stop-confirm"].tap()
        XCTAssertTrue(distance.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["record"].exists)

        // The next recording offers the activity of the last one.
        app.buttons["record"].tap()
        XCTAssertEqual(app.buttons["activity-hike"].value as? String, "zuletzt gewählt")
        app.buttons["activity-cancel"].tap()
    }

    /// Ended during a recording, the app asks at the next start.
    func testInterruptedRecordingIsOffered() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.buttons["record"].tap()
        app.buttons["activity-hike"].tap()
        let distance = app.staticTexts["recording-distance"]
        expectation(for: NSPredicate(format: "label != %@", "0 m"), evaluatedWith: distance)
        waitForExpectations(timeout: 20)
        app.terminate()

        let again = XCUIApplication()
        again.launchArguments = ["-selftest"]
        again.launch()
        XCTAssertTrue(again.staticTexts["Eine Aufzeichnung wurde unterbrochen."].waitForExistence(timeout: 20))
        again.buttons["recording-finish"].tap()
        XCTAssertFalse(again.staticTexts["Eine Aufzeichnung wurde unterbrochen."].exists)
        XCTAssertFalse(again.staticTexts["recording-distance"].exists)
    }
}
