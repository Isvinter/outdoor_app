import XCTest

/// Taps on the map without the GPS.
final class RouteHereUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Without a position a tap sets the first waypoint at once; no card asks.
    func testWithoutPositionTapsSetWaypoints() {
        let app = XCUIApplication.planning()
        app.place(0.3, 0.38).tap()
        app.waitForStatus("first point") { $0.int("waypoints") == 1 && $0.int("picking") == 1 }
        XCTAssertFalse(app.otherElements["map-point"].exists)

        app.place(0.7, 0.45).tap()
        app.waitForPlan(waypoints: 2)
        XCTAssertFalse(app.otherElements["map-point"].exists)
    }
}
