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

    /// Starts a recording with `activity`.
    func startRecording(_ activity: String = "hike", file: StaticString = #filePath, line: UInt = #line) {
        buttons["record"].tap()
        XCTAssertTrue(staticTexts["Aufnahme starten"].waitForExistence(timeout: 5), file: file, line: line)
        buttons["activity-\(activity)"].tap()
    }

    /// Records a walk as "Wandern" and saves it.
    func recordWalk(seconds: TimeInterval = 8, file: StaticString = #filePath, line: UInt = #line) {
        startRecording(file: file, line: line)
        let distance = staticTexts["recording-distance"]
        XCTAssertTrue(distance.waitForExistence(timeout: 10), file: file, line: line)
        Thread.sleep(forTimeInterval: seconds)
        // Without a point the core would discard the track on stopping.
        XCTAssertNotEqual(distance.label, "0 m", "nothing recorded", file: file, line: line)
        buttons["recording-stop"].tap()
        XCTAssertTrue(buttons["track-save"].waitForExistence(timeout: 5), file: file, line: line)
        buttons["track-save"].tap()
        XCTAssertTrue(buttons["track-save"].waitForNonExistence(timeout: 5), file: file, line: line)
    }
}

final class RecordingUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// The activity is chosen at the start; ended, only "Speichern" or "Verwerfen" is asked.
    func testRecordPauseResumeStopSave() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }

        app.startRecording("bike")
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
        app.startRecording()
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
        app.startRecording()
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

    /// "Aufnahme starten" offers the activities with the last one chosen; "Abbrechen" records nothing.
    func testStartAsksForActivity() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.buttons["record"].tap()
        XCTAssertTrue(app.staticTexts["Aufnahme starten"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["activity-other"].value as? String, "gewählt")
        for activity in ["bike", "mtb", "hike", "ski_tour"] {
            XCTAssertTrue(app.buttons["activity-\(activity)"].exists, activity)
        }
        app.buttons["start-cancel"].tap()
        XCTAssertFalse(app.staticTexts["Aufnahme starten"].exists)
        XCTAssertFalse(app.staticTexts["recording-distance"].exists)

        app.buttons["record"].tap()
        app.buttons["activity-mtb"].tap()
        XCTAssertTrue(app.staticTexts["recording-distance"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.buttons["recording-activity"].label, "MTB ▾")
        let distance = app.staticTexts["recording-distance"]
        expectation(for: NSPredicate(format: "label != %@", "0 m"), evaluatedWith: distance)
        waitForExpectations(timeout: 20)
        app.buttons["recording-stop"].tap()
        app.buttons["track-save"].tap()

        app.buttons["record"].tap()
        XCTAssertTrue(app.staticTexts["Aufnahme starten"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["activity-mtb"].value as? String, "gewählt")
    }

    /// The menu in the bar starts a section with another activity; the name keeps the first.
    func testSwitchActivityInBar() {
        // Slower than usual, so that the walk goes on after the switch.
        let app = XCUIApplication.replaying(arguments: ["-replaySpeed", "4"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording("mtb")
        app.waitForStatus("first section") { $0.int("recording_sections") == 1 }
        app.buttons["recording-activity"].tap()
        app.buttons["switch-hike"].tap()
        expectation(for: NSPredicate(format: "label == %@", "Wandern ▾"), evaluatedWith: app.buttons["recording-activity"])
        waitForExpectations(timeout: 5)
        app.waitForStatus("second section", timeout: 40) { $0.int("recording_sections") == 2 }

        app.buttons["recording-stop"].tap()
        XCTAssertTrue(app.buttons["track-save"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["activity-hike"].exists, "no activity chips when saving")
        XCTAssertTrue(app.staticTexts["save-track-name"].label.contains(" MTB · "))
        app.buttons["track-save"].tap()
    }

    /// Standing still for six minutes is a rest: told in the bar, gone once on the way again.
    func testRestHintShows() {
        let app = XCUIApplication.replaying("rest", arguments: ["-replaySpeed", "20"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording()
        let hint = app.staticTexts["rest-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: 40))
        XCTAssertEqual(hint.label, "Rast – GPS gedrosselt")
        XCTAssertTrue(hint.waitForNonExistence(timeout: 30))
        app.waitForStatus("rest over") { $0.values["resting"] == "0" }
    }

    /// With "Aus" in the bar the rest is told without throttling.
    func testThrottleLevelInBar() {
        let app = XCUIApplication.replaying("rest", arguments: ["-replaySpeed", "20"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording()
        app.buttons["recording-throttle"].tap()
        app.buttons["Aus"].tap()
        XCTAssertEqual(app.buttons["recording-throttle"].value as? String, "Aus")
        let hint = app.staticTexts["rest-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: 40))
        XCTAssertEqual(hint.label, "Rast")
    }
}
