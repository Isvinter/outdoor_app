import XCTest

/// "Route hierher" from a tapped place, without the GPS.
final class RouteHereUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Without a position the route begins at the tapped place, and taps add the next points.
    func testWithoutPositionTheRouteBeginsThere() {
        let app = XCUIApplication.planning()
        app.place(0.3, 0.38).tap()
        let card = app.otherElements["map-point"]
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["plan-from-here"].label, "Route planen")
        app.buttons["plan-from-here"].tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 5))
        app.waitForStatus("first point") { $0.int("waypoints") == 1 && $0.int("picking") == 1 }

        app.place(0.7, 0.45).tap()
        app.waitForPlan(waypoints: 2)
        XCTAssertFalse(card.exists)

        // "Fertig": a tap offers the place again, now also as the next destination.
        app.buttons["plan-pick"].tap()
        app.place(0.5, 0.6).tap()
        XCTAssertTrue(app.buttons["route-here-append"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["plan-from-here"].label, "Neue Route ab hier")
        app.buttons["route-here-append"].tap()
        app.waitForPlan(waypoints: 3)
        XCTAssertEqual(app.mapStatus.int("picking"), 1)
    }
}
