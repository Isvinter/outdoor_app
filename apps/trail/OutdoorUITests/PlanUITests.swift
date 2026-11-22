import XCTest

extension MapStatus {
    /// A list value such as `route_colors=#6a1b9a,#d93025`; "-" stands for none.
    func list(_ key: String) -> [String] {
        guard let value = values[key], value != "-" else { return [] }
        return value.split(separator: ",").map(String.init)
    }
}

extension XCUIApplication {
    /// A place on the screen, in shares of its width and height.
    func place(_ x: Double, _ y: Double) -> XCUICoordinate {
        coordinate(withNormalizedOffset: CGVector(dx: x, dy: y))
    }

    /// Picks a profile in the menu, by its German name.
    func chooseProfile(_ name: String) {
        buttons["plan-profile"].tap()
        buttons[name].tap()
    }

    /// Starts with the fixture `mini` at zoom 14 and waits for the map.
    static func planning(profile: String? = "Skitour (manuell)") -> XCUIApplication {
        let app = XCUIApplication.selftest(withMini: true, zoom: 14)
        app.waitForStatus("map rendered") { $0.rendered }
        if let profile { app.chooseProfile(profile) }
        return app
    }

    /// Drags the sheet by its upper edge: -250 from the collapsed step to the
    /// middle one, -700 all the way up, positive values down again.
    func pullSheet(by distance: Double) {
        let totals = staticTexts["plan-totals"]
        let grip = totals.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: -14))
        grip.press(forDuration: 0.1, thenDragTo: grip.withOffset(CGVector(dx: 0, dy: distance)))
    }

    /// Switches plan mode on and taps the places one after the other.
    func setWaypoints(_ places: [(Double, Double)]) {
        buttons["plan-pick"].tap()
        for (x, y) in places {
            place(x, y).tap()
        }
    }

    /// Waits until the route through `count` waypoints is computed and drawn.
    @discardableResult
    func waitForPlan(waypoints count: Int, file: StaticString = #filePath, line: UInt = #line) -> MapStatus {
        waitForStatus("route through \(count) waypoints", file: file, line: line) {
            $0.int("waypoints") == count && $0.int("planning") == 0
                && $0.list("route_colors").count == max(count - 1, 0)
        }
    }
}

/// Places inside the fixture `mini` at zoom 14, away from the bars.
private let places: [(Double, Double)] = [(0.3, 0.38), (0.7, 0.45), (0.5, 0.6), (0.25, 0.55), (0.75, 0.62)]

final class PlanUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testTapsSetWaypoints() {
        let app = XCUIApplication.planning()

        app.setWaypoints(Array(places.prefix(3)))

        let status = app.waitForPlan(waypoints: 3)
        XCTAssertEqual(status.int("legs"), 2)
        XCTAssertGreaterThan(status.int("distance_m"), 0)
        XCTAssertEqual(status.list("route_colors"), ["#6a1b9a", "#6a1b9a"])
        XCTAssertEqual(status.int("dashed"), 0)
        XCTAssertEqual(status.int("failed_legs"), 0)
    }

    func testTapOutsidePlanModeSetsNothing() {
        let app = XCUIApplication.planning()

        app.place(0.3, 0.38).tap()
        app.place(0.7, 0.45).tap()
        // Plan mode on and off again: still nothing.
        app.buttons["plan-pick"].tap()
        app.buttons["plan-pick"].tap()
        app.place(0.5, 0.6).tap()

        Thread.sleep(forTimeInterval: 1)
        let status = app.mapStatus
        XCTAssertEqual(status.int("waypoints"), 0)
        XCTAssertEqual(status.int("picking"), 0)
    }

    func testDragMovesWaypoint() {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        let before = app.waitForPlan(waypoints: 3)

        let second = app.place(places[1].0, places[1].1)
        second.press(forDuration: 0.1, thenDragTo: second.withOffset(CGVector(dx: 0, dy: -80)))

        let after = app.waitForStatus("distance changed") {
            $0.int("planning") == 0 && $0.int("distance_m") != before.int("distance_m")
        }
        XCTAssertEqual(after.int("waypoints"), 3)
        XCTAssertEqual(after.int("legs"), 2)
        // Up the screen is away from the other two points: the route got longer.
        XCTAssertGreaterThan(after.int("distance_m"), before.int("distance_m") + 100)
    }

    func testWaypointMenu() {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        app.waitForPlan(waypoints: 3)

        app.place(places[1].0, places[1].1).tap()
        app.buttons["Etappenende hier"].tap()
        let staged = app.waitForStatus("stage ends at point 2") { $0.list("stage_ends") == ["1"] }
        XCTAssertEqual(staged.int("waypoints"), 3)

        // The first point cannot end a stage. Without it, point 2 is the first
        // and ends no stage any more.
        app.place(places[0].0, places[0].1).tap()
        XCTAssertTrue(app.buttons["Entfernen"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Etappenende hier"].exists)
        app.buttons["Entfernen"].tap()
        let removed = app.waitForPlan(waypoints: 2)
        XCTAssertEqual(removed.list("stage_ends"), [])
        XCTAssertEqual(removed.int("legs"), 1)
    }

    func testUndoAndClear() {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        app.waitForPlan(waypoints: 3)

        app.buttons["plan-undo"].tap()
        XCTAssertEqual(app.waitForPlan(waypoints: 2).int("legs"), 1)
        app.buttons["plan-undo"].tap()
        let one = app.waitForPlan(waypoints: 1)
        XCTAssertEqual(one.int("distance_m"), 0)
        XCTAssertEqual(one.int("picking"), 1)

        // Plan mode is still on.
        app.place(places[2].0, places[2].1).tap()
        app.waitForPlan(waypoints: 2)

        app.buttons["plan-clear"].tap()
        let cleared = app.waitForPlan(waypoints: 0)
        XCTAssertEqual(cleared.int("legs"), 0)
        XCTAssertEqual(cleared.int("picking"), 0)
        XCTAssertFalse(app.buttons["plan-undo"].isEnabled)
    }

    /// Review focus 1: answers that come late must not replace the newest plan.
    func testFastTapsKeepNewestPlan() {
        let app = XCUIApplication.planning()

        app.setWaypoints(places)

        let status = app.waitForPlan(waypoints: 5)
        XCTAssertEqual(status.int("legs"), 4)
        XCTAssertEqual(status.list("route_colors").count, 4)
    }

    /// Review focus 4: `mini` has no routing data, so a hiking leg fails. It is
    /// drawn dashed, with a message, and counts nothing.
    func testLegWithoutRoutingDataIsDashed() {
        let app = XCUIApplication.planning(profile: nil)

        app.setWaypoints(Array(places.prefix(2)))

        let status = app.waitForPlan(waypoints: 2)
        XCTAssertEqual(status.list("leg_profiles"), ["hike"])
        XCTAssertEqual(status.int("failed_legs"), 1)
        XCTAssertEqual(status.list("route_colors"), ["#d93025"])
        XCTAssertEqual(status.int("dashed"), 1)
        XCTAssertEqual(status.int("distance_m"), 0)
        let message = app.staticTexts["plan-message"]
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        XCTAssertTrue(message.label.contains("Teilstück 1: Keine Routingdaten"), message.label)
    }

    /// The route is drawn again after the style changed.
    func testRouteSurvivesStyleChange() {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        app.waitForPlan(waypoints: 3)

        app.buttons["base-winter"].tap()

        let winter = app.waitForStatus("winter map with the route") {
            $0.values["base"] == "topo-winter" && $0.rendered && $0.list("route_colors").count == 2
        }
        XCTAssertEqual(winter.int("waypoints"), 3)
    }

    // MARK: The sheet

    private func pullSheetUp(_ app: XCUIApplication) {
        app.pullSheet(by: -250)
    }

    /// The form applies to the last leg: the first one keeps its profile.
    func testProfileAppliesToLastLeg() {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        app.waitForPlan(waypoints: 3)

        app.chooseProfile("Wandern")

        // `mini` has no routing data: the hiking leg fails.
        let status = app.waitForStatus("last leg is a hike") {
            $0.int("planning") == 0 && $0.list("leg_profiles") == ["manual", "hike"] && $0.int("failed_legs") == 1
        }
        XCTAssertEqual(status.list("route_colors"), ["#6a1b9a", "#d93025"])
        XCTAssertEqual(status.int("dashed"), 1)
        XCTAssertGreaterThan(status.int("distance_m"), 0)

        // A new point takes the profile of the form as well.
        app.place(places[3].0, places[3].1).tap()
        let more = app.waitForPlan(waypoints: 4)
        XCTAssertEqual(more.list("leg_profiles"), ["manual", "hike", "hike"])
    }

    func testSheetShowsTotals() {
        let app = XCUIApplication.planning()
        let totals = app.staticTexts["plan-totals"]
        XCTAssertTrue(totals.label.contains("+ Punkt"), totals.label)

        app.setWaypoints(Array(places.prefix(3)))
        let status = app.waitForPlan(waypoints: 3)

        let metres = Double(status.int("distance_m"))
        XCTAssertGreaterThan(metres, 1000)
        // As `Format.distance` writes it; the status rounds to metres.
        let shown = totals.label.components(separatedBy: " km").first ?? ""
        let kilometres = try? XCTUnwrap(Double(shown.replacingOccurrences(of: ",", with: ".")))
        XCTAssertEqual(kilometres ?? -1, metres / 1000, accuracy: 0.0051, totals.label)
        XCTAssertTrue(totals.label.contains("↑"), totals.label)
    }

    func testChartCursor() throws {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(places.prefix(3)))
        app.waitForPlan(waypoints: 3)
        XCTAssertNil(app.mapStatus.values["cursor"])

        pullSheetUp(app)
        let chart = app.otherElements["plan-chart"]
        XCTAssertTrue(chart.waitForExistence(timeout: 5))
        let from = chart.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.4))
        from.press(forDuration: 0.1, thenDragTo: chart.coordinate(withNormalizedOffset: CGVector(dx: 0.7, dy: 0.4)))

        let status = app.waitForStatus("cursor on the map", timeout: 5) { $0.values["cursor"] != nil }
        let parts = try XCTUnwrap(status.values["cursor"]).split(separator: ",").compactMap { Double($0) }
        XCTAssertEqual(parts.count, 2)
        // Inside the fixture `mini`.
        XCTAssertTrue((47.57...47.58).contains(parts[0]), "\(parts)")
        XCTAssertTrue((11.54...11.56).contains(parts[1]), "\(parts)")

        // A changed route has no cursor.
        app.buttons["plan-undo"].tap()
        app.waitForStatus("cursor gone", timeout: 5) { $0.values["cursor"] == nil && $0.int("waypoints") == 2 }
    }
}
