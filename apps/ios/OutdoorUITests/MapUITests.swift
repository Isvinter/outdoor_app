import XCTest

/// The self-test status of the app: `key=value` pairs.
struct MapStatus {
    let values: [String: String]

    init(_ text: String) {
        var values: [String: String] = [:]
        for pair in text.split(separator: " ") {
            let parts = pair.split(separator: "=", maxSplits: 1)
            if parts.count == 2 { values[String(parts[0])] = String(parts[1]) }
        }
        self.values = values
    }

    func int(_ key: String) -> Int { values[key].flatMap(Int.init) ?? -1 }
    func double(_ key: String) -> Double? { values[key].flatMap(Double.init) }
    var rendered: Bool { values["rendered"] == "1" }
}

extension XCUIApplication {
    /// Starts the app in self-test mode without regions, or with the fixture `mini`.
    static func selftest(withMini: Bool, beforeLaunch: (XCUIApplication) -> Void = { _ in }) -> XCUIApplication {
        let app = XCUIApplication()
        beforeLaunch(app)
        app.launchArguments = ["-selftest", "-resetData"]
        if withMini, let mini = Bundle(for: MapUITests.self).url(forResource: "mini", withExtension: nil) {
            // The simulator shares the Mac's file system, so the app can copy from here.
            app.launchArguments += ["-importFixture", mini.path]
        }
        app.launch()
        return app
    }

    var mapStatus: MapStatus {
        MapStatus(otherElements["map-status"].value as? String ?? "")
    }

    /// Waits until the status fulfils `condition` and returns it.
    @discardableResult
    func waitForStatus(
        _ what: String, timeout: TimeInterval = 20, file: StaticString = #filePath, line: UInt = #line,
        _ condition: (MapStatus) -> Bool
    ) -> MapStatus {
        let end = Date().addingTimeInterval(timeout)
        var status = mapStatus
        while !condition(status) && Date() < end {
            Thread.sleep(forTimeInterval: 0.3)
            status = mapStatus
        }
        XCTAssertTrue(condition(status), "\(what): \(status.values)", file: file, line: line)
        return status
    }
}

final class MapUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testMapRendersFromCore() {
        let app = XCUIApplication.selftest(withMini: true)

        let status = app.waitForStatus("map rendered") { $0.rendered }

        XCTAssertEqual(status.int("regions"), 1)
        XCTAssertGreaterThan(status.int("served"), 0, "MapLibre did not ask the URL protocol")
        XCTAssertEqual(status.int("failed"), 0)
        XCTAssertFalse(app.staticTexts["no-region-hint"].exists)
    }

    func testEmptyStartShowsHint() {
        let app = XCUIApplication.selftest(withMini: false)

        let status = app.waitForStatus("map rendered") { $0.rendered }

        XCTAssertEqual(status.int("regions"), 0)
        XCTAssertEqual(status.int("failed"), 0)
        // Nothing is installed, so nothing is asked for.
        XCTAssertEqual(status.int("served"), 0)
        XCTAssertEqual(status.int("empty"), 0)
        let hint = app.staticTexts["no-region-hint"]
        XCTAssertTrue(hint.exists)
        XCTAssertTrue(hint.label.contains("Noch keine Region installiert"))
    }

    private func switchTo(_ base: String, in app: XCUIApplication) -> MapStatus {
        app.buttons["base-\(base)"].tap()
        return app.waitForStatus("\(base) map rendered") { $0.values["base"] == "topo-\(base)" && $0.rendered }
    }

    func testSwitchToWinterShowsSlopeSlider() {
        let app = XCUIApplication.selftest(withMini: true)
        app.waitForStatus("map rendered") { $0.rendered }
        let slider = app.sliders["slope-opacity"]
        XCTAssertFalse(slider.exists)

        let winter = switchTo("winter", in: app)
        XCTAssertEqual(winter.int("failed"), 0)
        XCTAssertTrue(slider.exists)

        let summer = switchTo("summer", in: app)
        XCTAssertEqual(summer.int("failed"), 0)
        XCTAssertFalse(slider.exists)
    }

    /// The fixture has no slope tiles: the winter map still loads, its slope layer stays empty.
    func testWinterWithoutSlopeTiles() {
        let app = XCUIApplication.selftest(withMini: true)
        app.waitForStatus("map rendered") { $0.rendered }

        let winter = switchTo("winter", in: app)

        XCTAssertEqual(winter.int("failed"), 0)
        XCTAssertEqual(try XCTUnwrap(winter.double("slope")), 0.6, accuracy: 0.001)
    }

    func testSlopeOpacityDrag() throws {
        let app = XCUIApplication.selftest(withMini: true)
        app.waitForStatus("map rendered") { $0.rendered }
        _ = switchTo("winter", in: app)

        let slider = app.sliders["slope-opacity"]
        slider.adjust(toNormalizedSliderPosition: 0.2)

        // XCUITest drags only roughly to the position; the layer has to follow the slider exactly.
        let position = Double(slider.normalizedSliderPosition)
        XCTAssertEqual(position, 0.2, accuracy: 0.1)
        let dragged = app.waitForStatus("slope opacity follows the slider", timeout: 5) {
            abs(($0.double("slope") ?? -1) - position) <= 0.02
        }
        XCTAssertEqual(dragged.int("failed"), 0)

        // The opacity survives a change of style.
        _ = switchTo("summer", in: app)
        let back = switchTo("winter", in: app)
        XCTAssertEqual(try XCTUnwrap(back.double("slope")), try XCTUnwrap(dragged.double("slope")), accuracy: 0.001)
    }
}
