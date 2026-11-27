import XCTest

/// Taking routes over from the desktop: `share-serve` on the Mac offers the
/// route "Sylvenstein Test" with two waypoints (`scripts/test.sh`).
final class RemoteRoutesUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Starts with `mini` loaded from the desktop and the sheet pulled up all the way.
    private func appWithRegion() throws -> XCUIApplication {
        let app = try XCUIApplication.sharing(zoom: 13)
        app.openRegionsFromHint()
        app.loadMini()
        app.closeRegions()
        app.waitForStatus("map rendered") { $0.rendered }
        app.pullSheet(by: -700)
        return app
    }

    /// Opens the list of the desktop's routes and waits until it is read.
    @discardableResult
    private func openRemote(_ app: XCUIApplication, expecting states: [String]) -> MapStatus {
        app.buttons["routes-from-desktop"].tap()
        return app.waitForStatus("routes of the desktop: \(states)") { $0.list("remote_states") == states }
    }

    /// Takes the first route over and waits for the outcome and the new state.
    @discardableResult
    private func importFirst(_ app: XCUIApplication, outcome: String, then states: [String]) -> MapStatus {
        app.buttons["remote-import-1"].tap()
        return app.waitForStatus("route taken over: \(outcome)") {
            $0.values["import_outcome"] == outcome && $0.list("remote_states") == states
        }
    }

    private func closeRemote(_ app: XCUIApplication) {
        app.buttons["remote-done"].tap()
        XCTAssertTrue(app.buttons["remote-done"].waitForNonExistence(timeout: 5))
    }

    func testImportNewRoute() throws {
        let app = try appWithRegion()
        XCTAssertEqual(app.mapStatus.int("routes"), 0)

        let offered = openRemote(app, expecting: ["new"])
        XCTAssertEqual(offered.int("remote_routes"), 1)
        XCTAssertTrue(app.staticTexts["Sylvenstein Test"].exists)
        XCTAssertEqual(app.staticTexts["remote-state-1"].label, "Neu")

        let added = importFirst(app, outcome: "added", then: ["up_to_date"])
        XCTAssertEqual(added.int("routes"), 1)
        XCTAssertEqual(app.staticTexts["remote-notice"].label, "Übernommen")
        XCTAssertEqual(app.staticTexts["remote-state-1"].label, "Aktuell")
        XCTAssertFalse(app.buttons["remote-import-1"].exists)
        closeRemote(app)

        let row = app.buttons["route-row-1"]
        XCTAssertTrue(row.label.contains("Sylvenstein Test"), row.label)
        row.tap()
        let loaded = app.waitForStatus("route loaded") { $0.int("waypoints") == 2 && $0.int("planning") == 0 }
        XCTAssertEqual(loaded.list("leg_profiles"), ["manual"])
        XCTAssertEqual(loaded.int("failed_legs"), 0)
        XCTAssertGreaterThan(loaded.int("distance_m"), 0)
    }

    func testChangedHereMakesCopy() throws {
        let app = try appWithRegion()
        openRemote(app, expecting: ["new"])
        importFirst(app, outcome: "added", then: ["up_to_date"])
        closeRemote(app)

        // Load it, add a point, save: the iPhone's version is its own now.
        app.buttons["route-row-1"].tap()
        let loaded = app.waitForStatus("route loaded") { $0.int("waypoints") == 2 && $0.int("planning") == 0 }
        app.place(0.5, 0.3).tap()
        app.waitForPlan(waypoints: 3)
        app.pullSheet(by: -250)
        app.saveRoute()
        app.waitForStatus("changed route saved") { $0.int("route_rev") == loaded.int("route_rev") + 1 }

        app.pullSheet(by: -500)
        openRemote(app, expecting: ["changed_here"])
        XCTAssertTrue(app.staticTexts["remote-state-1"].label.contains("wird als Kopie übernommen"))
        let copied = importFirst(app, outcome: "copied", then: ["changed_here"])
        XCTAssertEqual(copied.int("routes"), 2)
        XCTAssertTrue(app.staticTexts["remote-notice"].label.contains("Sylvenstein Test (Desktop)"))
        // The changed route stays the loaded one, with its three points.
        XCTAssertEqual(copied.values["route_id"], loaded.values["route_id"])
        XCTAssertEqual(copied.int("waypoints"), 3)
        closeRemote(app)

        app.showSavedRoutes()
        let copy = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Sylvenstein Test (Desktop)"))
        XCTAssertEqual(copy.count, 1)
    }

    /// The loaded route was planned on and not saved: that counts as changed
    /// here. The desktop's version comes as a copy and the planning stays.
    func testUnsavedChangeMakesCopy() throws {
        let app = try appWithRegion()
        openRemote(app, expecting: ["new"])
        importFirst(app, outcome: "added", then: ["up_to_date"])
        closeRemote(app)

        app.buttons["route-row-1"].tap()
        let loaded = app.waitForStatus("route loaded") { $0.int("waypoints") == 2 && $0.int("planning") == 0 }
        app.place(0.5, 0.3).tap()
        let edited = app.waitForPlan(waypoints: 3)
        XCTAssertEqual(edited.int("route_rev"), loaded.int("route_rev"), "nothing was saved")

        app.pullSheet(by: -700)
        openRemote(app, expecting: ["changed_here"])
        let copied = importFirst(app, outcome: "copied", then: ["changed_here"])
        XCTAssertEqual(copied.int("routes"), 2)
        XCTAssertEqual(copied.values["route_id"], loaded.values["route_id"])
        XCTAssertEqual(copied.int("waypoints"), 3)
        XCTAssertEqual(copied.int("distance_m"), edited.int("distance_m"))
    }

    func testDeletedHereIsRestored() throws {
        let app = try appWithRegion()
        openRemote(app, expecting: ["new"])
        let added = importFirst(app, outcome: "added", then: ["up_to_date"])
        XCTAssertEqual(added.int("routes"), 1)
        closeRemote(app)

        app.buttons["route-row-1"].swipeLeft()
        app.buttons["Löschen"].tap()
        app.waitForStatus("route deleted") { $0.int("routes") == 0 }

        openRemote(app, expecting: ["deleted_here"])
        XCTAssertEqual(app.staticTexts["remote-state-1"].label, "Auf dem iPhone gelöscht")
        let restored = importFirst(app, outcome: "restored", then: ["up_to_date"])
        XCTAssertEqual(restored.int("routes"), 1)
        XCTAssertEqual(app.staticTexts["remote-notice"].label, "Wiederhergestellt")
    }
}
