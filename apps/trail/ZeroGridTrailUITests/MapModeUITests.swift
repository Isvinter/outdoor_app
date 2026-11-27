import XCTest

/// The switch between "Wegpunkte setzen" and "Abschnitte bearbeiten", and sections chosen on the map.
final class MapModeUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// On the map after the start, during a recording and with a track open.
    func testSwitchIsAlwaysVisible() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        XCTAssertTrue(app.buttons["mode-waypoints"].isSelected)
        XCTAssertTrue(app.buttons["mode-sections"].isHittable)

        app.recordWalk()
        XCTAssertTrue(app.buttons["mode-sections"].isHittable)
        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        app.buttons["track-row-1"].tap()
        XCTAssertTrue(app.staticTexts["track-title"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["mode-sections"].isHittable)
        app.buttons["mode-sections"].tap()
        app.waitForStatus("sections mode") { $0.values["mode"] == "sections" }
    }

    /// In "Wegpunkte setzen" a tap on a track sets a waypoint (a way back on the same path).
    func testTapOnTrackSetsWaypoint() {
        let app = XCUIApplication.replaying()
        app.waitForStatus("map rendered") { $0.rendered }
        app.recordWalk()
        app.pullSheet(by: -700)
        app.segmentedControls["saved-lists"].buttons["Tracks"].tap()
        app.buttons["track-row-1"].tap()
        XCTAssertTrue(app.staticTexts["track-title"].waitForExistence(timeout: 5))
        app.waitForStatus("track on the map") { $0.values["track_section_xy"] != nil }
        // The map glides to the track first.
        Thread.sleep(forTimeInterval: 2)

        app.tapStatusPoint("track_section_xy", 0)
        let status = app.waitForStatus("waypoint on the track") { $0.int("waypoints") == 1 }
        XCTAssertEqual(status.values["section"], "-")
    }

    /// A tap on the line after the rest chooses that section; its row is marked, and a row chooses on the map.
    func testChooseSectionOnMap() {
        let app = XCUIApplication.recordingSections()
        app.waitForStatus("track on the map") { ($0.values["track_section_xy"] ?? "").split(separator: ";").count == 3 }
        Thread.sleep(forTimeInterval: 2)
        app.buttons["mode-sections"].tap()

        app.tapStatusPoint("track_section_xy", 2)
        app.waitForStatus("last section chosen") { $0.values["section"] == "track:2" }
        XCTAssertEqual(app.mapStatus.int("waypoints"), 0)
        XCTAssertEqual(app.descendants(matching: .any)["section-2"].value as? String, "gewählt")

        app.descendants(matching: .any)["section-0"].tap()
        app.waitForStatus("first section chosen") { $0.values["section"] == "track:0" }
        XCTAssertEqual(app.descendants(matching: .any)["section-0"].value as? String, "gewählt")

        // Back to "Wegpunkte setzen": the choice goes.
        app.buttons["mode-waypoints"].tap()
        app.waitForStatus("nothing chosen") { $0.values["section"] == "-" }
    }

    /// A section of the recording that runs shows on a card over the map.
    func testRecordingSectionCard() {
        let app = XCUIApplication.replaying(arguments: ["-replaySpeed", "4"])
        app.waitForStatus("map rendered") { $0.rendered }
        app.startRecording("hike")
        app.waitForStatus("recorded line") { $0.values["recording_xy"] != nil }
        app.buttons["mode-sections"].tap()

        app.tapStatusPoint("recording_xy", 0)
        app.waitForStatus("section of the recording chosen") { $0.values["section"] == "recording:0" }
        let text = app.staticTexts["recording-section-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        XCTAssertTrue(text.label.hasPrefix("Abschnitt 1 · Wandern"), text.label)
        app.buttons["recording-section-close"].tap()
        app.waitForStatus("nothing chosen") { $0.values["section"] == "-" }
        XCTAssertFalse(text.exists)
    }
}
