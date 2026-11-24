import XCTest

final class GuideUITests: XCTestCase {
    /// The first two points of the reference hike in isarwinkel-achensee.
    private let route = "47.6803,11.5741;47.6625,11.5233"

    override func setUp() {
        continueAfterFailure = false
    }

    /// Needs routing data: the package of `isarwinkel-achensee` (`scripts/test.sh` names it).
    private func launch() throws -> XCUIApplication {
        guard let package = ProcessInfo.processInfo.environment["ISARWINKEL_PACKAGE"], !package.isEmpty else {
            throw XCTSkip("ISARWINKEL_PACKAGE is not set")
        }
        let app = XCUIApplication()
        app.launchArguments = [
            "-selftest", "-resetData", "-importFixture", package, "-route", route,
            "-startCenter", "47.6803,11.5741", "-startZoom", "14",
        ]
        let bundle = Bundle(for: GuideUITests.self)
        if let track = bundle.url(forResource: "isarwinkel-deviation", withExtension: "gpx", subdirectory: "replay") {
            app.launchArguments += ["-replayGPX", track.path]
        }
        app.launch()
        app.waitForStatus("route planned", timeout: 90) {
            $0.rendered && $0.int("waypoints") == 2 && $0.int("planning") == 0 && $0.int("distance_m") > 0
        }
        return app
    }

    func testGuideShowsWayBackAfterLeavingRoute() throws {
        let app = try launch()
        let toggle = app.buttons["guide-toggle"]
        XCTAssertEqual(toggle.value as? String, "aus")
        toggle.tap()
        XCTAssertTrue(app.otherElements["guide-bar"].waitForExistence(timeout: 10))
        XCTAssertEqual(toggle.value as? String, "an")

        let off = app.waitForStatus("off the route", timeout: 60) { $0.values["guide"] == "off_route" }
        // The walk goes on straight away; a second deviation may follow quickly.
        XCTAssertGreaterThanOrEqual(off.int("deviation"), 1)
        XCTAssertTrue(app.staticTexts["guide-way-back"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["guide-way-back"].label.hasPrefix("Weg zurück zum Ziel: "))
        app.waitForStatus("way back drawn") { $0.int("way_back") == 1 }

        // Moving the map ends following; the bar brings it back.
        XCTAssertEqual(app.mapStatus.int("following"), 1)
        app.swipeLeft()
        app.waitForStatus("not following") { $0.int("following") == 0 }
        app.buttons["guide-follow"].tap()
        app.waitForStatus("following again") { $0.int("following") == 1 }

        app.buttons["guide-stop"].tap()
        XCTAssertTrue(app.otherElements["guide-bar"].waitForNonExistence(timeout: 5))
        app.waitForStatus("guidance off") { $0.values["guide"] == "off" }
    }
}
