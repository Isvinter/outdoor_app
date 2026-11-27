import XCTest

/// Places in the upper half of the map, below the bars and clear of the sheet at its middle step.
private let upper: [(Double, Double)] = [(0.3, 0.31), (0.7, 0.34), (0.5, 0.44), (0.2, 0.42)]

extension XCUIApplication {
    /// Presses "Speichern" or "Als neue Route speichern" in the sheet, which
    /// must be pulled up, and confirms the name the dialog proposes.
    func saveRoute(asNew: Bool = false) {
        buttons[asNew ? "route-save-new" : "route-save"].tap()
        alerts.buttons["Speichern"].tap()
    }

    /// Scrolls the sheet until the saved routes show: a route with an elevation
    /// profile pushes them below the screen.
    func showSavedRoutes() {
        for _ in 0..<3 where !buttons["route-row-1"].isHittable {
            collectionViews.firstMatch.swipeUp()
        }
    }
}

final class RoutesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Two waypoints, the sheet at its middle step, the route saved once.
    private func appWithSavedRoute(points: Int = 2) -> (XCUIApplication, MapStatus) {
        let app = XCUIApplication.planning()
        app.setWaypoints(Array(upper.prefix(points)))
        app.waitForPlan(waypoints: points)
        app.pullSheet(by: -250)
        app.saveRoute()
        let saved = app.waitForStatus("route saved") { $0.int("routes") == 1 && $0.values["route_id"] != "-" }
        return (app, saved)
    }

    func testSaveTwiceKeepsOneRoute() {
        let (app, first) = appWithSavedRoute()
        XCTAssertTrue(app.staticTexts["plan-notice"].label.contains("Route gespeichert"))

        app.place(upper[2].0, upper[2].1).tap()
        app.waitForPlan(waypoints: 3)
        app.saveRoute()

        let second = app.waitForStatus("saved again") { $0.int("route_rev") == first.int("route_rev") + 1 }
        XCTAssertEqual(second.int("routes"), 1)
        XCTAssertEqual(second.values["route_id"], first.values["route_id"])
    }

    func testSaveAsNew() {
        let (app, first) = appWithSavedRoute()

        app.saveRoute(asNew: true)

        let second = app.waitForStatus("second route") { $0.int("routes") == 2 }
        XCTAssertNotEqual(second.values["route_id"], first.values["route_id"])
        // "Save" overwrites the new one from now on.
        app.saveRoute()
        let third = app.waitForStatus("new route saved again") { $0.int("route_rev") == second.int("route_rev") + 1 }
        XCTAssertEqual(third.int("routes"), 2)
        XCTAssertEqual(third.values["route_id"], second.values["route_id"])
    }

    /// Undo below two points keeps the saved route; "Leeren" lets go of it.
    func testUndoKeepsRouteAndClearStartsNew() {
        let (app, first) = appWithSavedRoute()

        app.buttons["plan-undo"].tap()
        app.waitForPlan(waypoints: 1)
        app.place(upper[2].0, upper[2].1).tap()
        app.waitForPlan(waypoints: 2)
        app.saveRoute()
        let again = app.waitForStatus("same route saved again") { $0.int("route_rev") == first.int("route_rev") + 1 }
        XCTAssertEqual(again.int("routes"), 1)
        XCTAssertEqual(again.values["route_id"], first.values["route_id"])

        app.buttons["plan-clear"].tap()
        XCTAssertEqual(app.waitForPlan(waypoints: 0).values["route_id"], "-")
        app.setWaypoints(Array(upper.prefix(2)))
        app.waitForPlan(waypoints: 2)
        app.saveRoute()
        let new = app.waitForStatus("a new route") { $0.int("routes") == 2 }
        XCTAssertNotEqual(new.values["route_id"], first.values["route_id"])
    }

    func testLoadReplanSave() {
        let (app, first) = appWithSavedRoute(points: 3)
        app.buttons["plan-clear"].tap()
        app.waitForPlan(waypoints: 0)

        app.pullSheet(by: -500)
        app.buttons["route-row-1"].tap()

        let loaded = app.waitForStatus("route loaded") {
            $0.int("waypoints") == 3 && $0.list("route_colors").count == 2 && $0.int("planning") == 0
        }
        XCTAssertEqual(loaded.values["route_id"], first.values["route_id"])
        XCTAssertEqual(loaded.int("distance_m"), first.int("distance_m"))
        XCTAssertEqual(loaded.list("leg_profiles"), ["manual", "manual"])
        XCTAssertEqual(loaded.int("picking"), 1, "loading leaves the mode as it is")

        // The sheet went down and the map shows the route: add a point and save.
        app.place(0.55, 0.29).tap()
        let longer = app.waitForPlan(waypoints: 4)
        XCTAssertGreaterThan(longer.int("distance_m"), loaded.int("distance_m"))
        app.pullSheet(by: -250)
        app.saveRoute()

        let saved = app.waitForStatus("loaded route overwritten") { $0.int("route_rev") == first.int("route_rev") + 1 }
        XCTAssertEqual(saved.int("routes"), 1)
        XCTAssertEqual(saved.values["route_id"], first.values["route_id"])
    }

    /// Review focus 3: deleting the loaded route and saving again stores a new one.
    func testRenameAndDelete() {
        let (app, first) = appWithSavedRoute()
        app.pullSheet(by: -500)
        app.showSavedRoutes()
        let row = app.buttons["route-row-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("Skitour (manuell)"), row.label)

        row.swipeLeft()
        app.buttons["Umbenennen"].tap()
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText(" Abend")
        app.alerts.buttons["Umbenennen"].tap()
        let renamed = NSPredicate(format: "label CONTAINS %@", "Abend")
        expectation(for: renamed, evaluatedWith: row)
        waitForExpectations(timeout: 5)
        XCTAssertEqual(app.mapStatus.values["route_id"], first.values["route_id"])

        row.swipeLeft()
        app.buttons["Löschen"].tap()
        let deleted = app.waitForStatus("route deleted") { $0.int("routes") == 0 }
        XCTAssertEqual(deleted.values["route_id"], "-")
        XCTAssertEqual(deleted.int("waypoints"), 2)

        app.saveRoute()
        let new = app.waitForStatus("saved as a new route") { $0.int("routes") == 1 }
        XCTAssertNotEqual(new.values["route_id"], first.values["route_id"])
        XCTAssertNotEqual(new.values["route_id"], "-")
    }
}
