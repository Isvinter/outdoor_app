import XCTest

extension XCUIApplication {
    /// Starts without regions, with the share that `scripts/test.sh` runs on
    /// the Mac (`share-serve` with the fixture `mini` and the route
    /// "Sylvenstein Test") as the desktop, and waits for the map.
    /// `slow` takes the share that sends 200 KiB/s.
    static func sharing(slow: Bool = false, zoom: Double? = nil) throws -> XCUIApplication {
        let name = slow ? "SHARE_SLOW_URL" : "SHARE_URL"
        guard let source = ProcessInfo.processInfo.environment[name], !source.isEmpty else {
            throw XCTSkip("\(name) is not set (scripts/test.sh sets it)")
        }
        return sharing(source: source, zoom: zoom)
    }

    static func sharing(source: String, zoom: Double? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-selftest", "-resetData", "-catalogSource", source]
        if let zoom {
            app.launchArguments += ["-startZoom", String(zoom)]
        }
        app.launch()
        app.waitForStatus("map rendered") { $0.rendered }
        return app
    }

    /// Opens "Regionen" from the hint on the map and waits for the catalog.
    func openRegionsFromHint() {
        buttons["regions-hint-open"].tap()
        waitForStatus("catalog read") { $0.values["catalog"] != nil }
    }

    /// Loads `mini` in the open screen "Regionen" and waits until it is installed.
    @discardableResult
    func loadMini() -> MapStatus {
        buttons["region-load-mini"].tap()
        return waitForStatus("mini installed", timeout: 30) { $0.list("regions") == ["mini:1"] }
    }

    func closeRegions() {
        buttons["regions-done"].tap()
        XCTAssertTrue(buttons["regions-done"].waitForNonExistence(timeout: 5))
    }
}

/// The region manager, with a real download from `share-serve` on the Mac.
final class RegionsUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testInstallFromDesktop() throws {
        let app = try XCUIApplication.sharing()
        let start = app.mapStatus
        XCTAssertEqual(start.list("regions"), [])
        XCTAssertEqual(start.int("served"), 0)
        // The start does not read the catalog.
        XCTAssertNil(start.values["catalog"])

        app.openRegionsFromHint()
        let catalog = app.mapStatus
        XCTAssertEqual(catalog.int("catalog"), 1)
        XCTAssertEqual(catalog.int("catalog_error"), 0)
        XCTAssertTrue(app.staticTexts["Sylvenstein (Test)"].exists)

        app.loadMini()
        XCTAssertTrue(app.switches["region-enabled-mini"].exists)
        XCTAssertFalse(app.buttons["region-load-mini"].exists)
        XCTAssertFalse(app.buttons["region-update-mini"].exists)
        app.closeRegions()

        // The map draws the new region without a restart.
        let drawn = app.waitForStatus("tiles of the new region") { $0.rendered && $0.int("served") > 0 }
        XCTAssertEqual(drawn.int("failed"), 0)
        XCTAssertEqual(drawn.int("bounds"), 1)
        XCTAssertEqual(drawn.values["downloading"], "-")
        XCTAssertFalse(app.staticTexts["no-region-hint"].exists)
    }

    func testDisableHidesRegion() throws {
        let app = try XCUIApplication.sharing(zoom: 14)
        app.openRegionsFromHint()
        app.loadMini()

        app.switches["region-enabled-mini"].firstMatch.tap()
        let off = app.waitForStatus("region switched off") { $0.list("regions") == ["mini:0"] && $0.int("bounds") == 0 }
        XCTAssertEqual(off.int("failed"), 0)
        app.closeRegions()

        // The map asks for tiles and gets none.
        let before = app.waitForStatus("map drawn without the region") { $0.rendered }
        XCTAssertFalse(app.staticTexts["no-region-hint"].exists)
        // Planning by hand needs no region.
        app.chooseProfile("Skitour (manuell)")
        app.setWaypoints([(0.3, 0.38), (0.7, 0.45)])
        let planned = app.waitForPlan(waypoints: 2)
        XCTAssertEqual(planned.list("leg_profiles"), ["manual"])
        XCTAssertEqual(planned.int("failed_legs"), 0)
        XCTAssertEqual(planned.int("served"), before.int("served"), "a tile of the switched-off region was drawn")

        app.pullSheet(by: -700)
        app.buttons["regions-open"].tap()
        app.switches["region-enabled-mini"].firstMatch.tap()
        app.waitForStatus("region switched on") { $0.list("regions") == ["mini:1"] && $0.int("bounds") == 1 }
        app.closeRegions()
        // The route is still there, computed again, and the tiles are back.
        let on = app.waitForPlan(waypoints: 2)
        XCTAssertGreaterThan(app.waitForStatus("tiles are back") { $0.rendered }.int("served"), before.int("served"))
        XCTAssertEqual(on.int("failed_legs"), 0)
    }

    func testDeleteRegion() throws {
        let app = try XCUIApplication.sharing()
        app.openRegionsFromHint()
        app.loadMini()

        app.staticTexts["Sylvenstein (Test)"].swipeLeft()
        app.buttons["Löschen"].tap()
        let alert = app.alerts["Region löschen?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        // Nothing is deleted before the question is answered.
        XCTAssertEqual(app.mapStatus.list("regions"), ["mini:1"])
        alert.buttons["Löschen"].tap()

        let deleted = app.waitForStatus("region deleted") { $0.list("regions").isEmpty }
        XCTAssertEqual(deleted.int("bounds"), 0)
        // The desktop still offers it.
        XCTAssertTrue(app.buttons["region-load-mini"].waitForExistence(timeout: 5))
        app.closeRegions()
        XCTAssertTrue(app.buttons["regions-hint-open"].waitForExistence(timeout: 5))
    }

    /// Review focus 1: a source that does not answer holds up neither the
    /// start nor the map.
    func testDesktopUnreachable() {
        let app = XCUIApplication.sharing(source: "http://127.0.0.1:9/")
        let start = app.mapStatus
        XCTAssertLessThan(start.int("rendered_ms"), 5000)
        XCTAssertNil(start.values["catalog"])

        app.buttons["regions-hint-open"].tap()
        app.waitForStatus("catalog failed") { $0.int("catalog_error") == 1 }

        let message = app.staticTexts["catalog-error"]
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        XCTAssertTrue(message.label.contains("Desktop nicht erreichbar"), message.label)
        XCTAssertTrue(app.buttons["catalog-retry"].exists)
        XCTAssertEqual(app.staticTexts["catalog-source"].label, "http://127.0.0.1:9/")
    }

    /// Review focus 3: a download that was stopped goes on where it was.
    func testInterruptedDownloadResumes() throws {
        let app = try XCUIApplication.sharing(slow: true)
        app.openRegionsFromHint()

        app.buttons["region-load-mini"].tap()
        let running = app.waitForStatus("a fifth is loaded") {
            $0.values["downloading"] == "mini" && $0.int("download_pct") >= 20
        }
        XCTAssertEqual(running.int("download_start_pct"), 0)
        app.buttons["region-cancel-mini"].tap()
        let stopped = app.waitForStatus("download stopped") { $0.values["downloading"] == "-" }
        XCTAssertEqual(stopped.list("regions"), [])
        XCTAssertFalse(app.staticTexts["regions-error"].exists, "stopping is not a failure")

        app.buttons["region-load-mini"].tap()
        let resumed = app.waitForStatus("download goes on") {
            $0.values["downloading"] == "mini" && $0.values["download_start_pct"] != nil
                || $0.list("regions") == ["mini:1"]
        }
        // Continued, not started over.
        XCTAssertGreaterThanOrEqual(resumed.int("download_start_pct"), 15)
        app.waitForStatus("mini installed", timeout: 30) { $0.list("regions") == ["mini:1"] }
    }
}
