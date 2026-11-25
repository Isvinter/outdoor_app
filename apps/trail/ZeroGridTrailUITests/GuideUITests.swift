import XCTest

final class GuideUITests: XCTestCase {
    /// The first two points of the reference hike in isarwinkel-achensee.
    private let route = "47.6803,11.5741;47.6625,11.5233"

    override func setUp() {
        continueAfterFailure = false
    }

    /// Needs routing data: the package of `isarwinkel-achensee` (`scripts/test.sh` names it).
    private func launch(replay: String = "isarwinkel-deviation") throws -> XCUIApplication {
        guard let package = ProcessInfo.processInfo.environment["ISARWINKEL_PACKAGE"], !package.isEmpty else {
            throw XCTSkip("ISARWINKEL_PACKAGE is not set")
        }
        let app = XCUIApplication()
        app.launchArguments = [
            "-selftest", "-resetData", "-importFixture", package, "-route", route,
            "-startCenter", "47.6803,11.5741", "-startZoom", "14",
        ]
        let bundle = Bundle(for: GuideUITests.self)
        if let track = bundle.url(forResource: replay, withExtension: "gpx", subdirectory: "replay") {
            app.launchArguments += ["-replayGPX", track.path]
        }
        app.launch()
        app.waitForStatus("route planned", timeout: 90) {
            $0.rendered && $0.int("waypoints") == 2 && $0.int("planning") == 0 && $0.int("distance_m") > 0
        }
        return app
    }

    func testLeavingTheRouteReroutesFromThere() throws {
        let app = try launch()
        let toggle = app.buttons["guide-toggle"]
        XCTAssertEqual(toggle.value as? String, "aus")
        toggle.tap()
        XCTAssertTrue(app.otherElements["guide-bar"].waitForExistence(timeout: 10))
        XCTAssertEqual(toggle.value as? String, "an")

        let before = app.mapStatus
        // Off the route the route is planned anew from there, without asking; the guidance goes on.
        let rerouted = app.waitForStatus("rerouted", timeout: 90) {
            $0.int("plans") > before.int("plans") && $0.int("planning") == 0
                && $0.values["start"] != before.values["start"]
        }
        XCTAssertEqual(rerouted.int("waypoints"), 2)
        XCTAssertNotEqual(rerouted.values["guide"], "off")
        XCTAssertEqual(toggle.value as? String, "an")
        XCTAssertFalse(app.alerts.firstMatch.exists)

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

    /// A walk along the route through its first turn, a left one: the field
    /// above the bar comes before the turn and goes after it.
    func testTurnBannerAppearsAndGoes() throws {
        let app = try launch(replay: "isarwinkel-turn")
        app.buttons["guide-toggle"].tap()
        XCTAssertTrue(app.otherElements["guide-bar"].waitForExistence(timeout: 10))
        let banner = app.descendants(matching: .any)["turn-banner"]
        XCTAssertTrue(banner.waitForExistence(timeout: 60))
        XCTAssertTrue(banner.label.contains("links"), banner.label)
        XCTAssertTrue(banner.waitForNonExistence(timeout: 30))
        XCTAssertNotEqual(app.mapStatus.values["guide"], "off")
        XCTAssertEqual(app.mapStatus.values["guide"], "on_route")
    }

    /// The Isarwinkel package with the GPS replaced by a walk, without a route.
    private func launchWithoutRoute() throws -> XCUIApplication {
        guard let package = ProcessInfo.processInfo.environment["ISARWINKEL_PACKAGE"], !package.isEmpty else {
            throw XCTSkip("ISARWINKEL_PACKAGE is not set")
        }
        let app = XCUIApplication()
        app.launchArguments = [
            "-selftest", "-resetData", "-importFixture", package,
            "-startCenter", "47.6803,11.5741", "-startZoom", "15",
        ]
        let bundle = Bundle(for: GuideUITests.self)
        if let track = bundle.url(forResource: "isarwinkel-deviation", withExtension: "gpx", subdirectory: "replay") {
            app.launchArguments += ["-replayGPX", track.path]
        }
        app.launch()
        app.waitForStatus("map rendered", timeout: 90) { $0.rendered }
        return app
    }

    private func locate(_ app: XCUIApplication) {
        app.buttons["center-location"].tap()
        XCTAssertTrue(app.staticTexts["location-accuracy"].waitForExistence(timeout: 20))
    }

    /// A tap on the map offers what a search hit offers; with the position "Hierher führen" also guides.
    func testTapOnMapGuidesThere() throws {
        let app = try launchWithoutRoute()
        let card = app.otherElements["map-point"]

        // Without a position only a route that begins there.
        app.place(0.3, 0.45).tap()
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["plan-from-here"].exists)
        XCTAssertFalse(app.buttons["guide-here"].exists)
        app.buttons["point-close"].tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 5))

        locate(app)
        app.place(0.3, 0.45).tap()
        XCTAssertTrue(app.buttons["guide-here"].waitForExistence(timeout: 5))
        app.buttons["guide-here"].tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 10))
        app.waitForStatus("route planned", timeout: 90) {
            $0.int("waypoints") == 2 && $0.int("planning") == 0 && $0.int("distance_m") > 0
        }
        XCTAssertTrue(app.otherElements["guide-bar"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.buttons["guide-toggle"].value as? String, "an")
        XCTAssertEqual(app.mapStatus.int("picking"), 0)
    }

    /// The first point set with "+ Punkt" offers the way from the own position;
    /// afterwards taps add points until "Fertig".
    func testFirstPointOffersTheWayFromThePosition() throws {
        let app = try launchWithoutRoute()
        locate(app)
        app.buttons["plan-pick"].tap()
        app.place(0.3, 0.45).tap()
        XCTAssertTrue(app.staticTexts["Von meinem Standort hierher?"].waitForExistence(timeout: 5))
        app.buttons["route-here"].tap()
        app.waitForStatus("route from the position", timeout: 90) {
            $0.int("waypoints") == 2 && $0.int("planning") == 0 && $0.int("distance_m") > 0
        }
        XCTAssertEqual(app.mapStatus.int("picking"), 1)
        XCTAssertEqual(app.mapStatus.values["guide"], "off")

        app.place(0.7, 0.4).tap()
        app.waitForStatus("third point", timeout: 90) { $0.int("waypoints") == 3 && $0.int("planning") == 0 }
        XCTAssertFalse(app.otherElements["map-point"].exists)
    }
}
