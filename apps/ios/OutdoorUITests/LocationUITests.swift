import CoreLocation
import XCTest

final class LocationUITests: XCTestCase {
    private let place = CLLocation(latitude: 47.6625, longitude: 11.5233)

    override func setUp() {
        continueAfterFailure = false
    }

    /// Starts the app as if it had never asked for the location.
    private func launch() -> XCUIApplication {
        let app = XCUIApplication.selftest(withMini: false) { $0.resetAuthorizationStatus(for: .location) }
        app.waitForStatus("map rendered") { $0.rendered }
        return app
    }

    /// Answers the system's question about the location; the simulator may speak English or German.
    private func answerPermission(allow: Bool, file: StaticString = #filePath, line: UInt = #line) {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let labels = allow
            ? ["Allow While Using App", "Beim Verwenden der App erlauben"]
            : ["Don’t Allow", "Don't Allow", "Nicht erlauben"]
        let alert = springboard.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 10), "no permission dialog", file: file, line: line)
        guard let button = labels.map({ alert.buttons[$0] }).first(where: \.exists) else {
            return XCTFail("no button of \(labels) in \(alert.buttons.allElementsBoundByIndex.map(\.label))", file: file, line: line)
        }
        button.tap()
    }

    func testCenterOnSimulatedLocation() throws {
        XCUIDevice.shared.location = XCUILocation(location: place)
        let app = launch()

        app.buttons["center-location"].tap()
        answerPermission(allow: true)

        let status = app.waitForStatus("map centred on the position") { status in
            guard let centre = status.values["center"]?.split(separator: ",").compactMap({ Double($0) }), centre.count == 2
            else { return false }
            return CLLocation(latitude: centre[0], longitude: centre[1]).distance(from: self.place) < 50
        }
        XCTAssertEqual(status.int("failed"), 0)
        XCTAssertFalse(app.staticTexts["location-hint"].exists)
    }

    func testCenterWithoutPermissionShowsHint() {
        let app = launch()
        let before = app.mapStatus.values["center"]

        app.buttons["center-location"].tap()
        answerPermission(allow: false)

        let hint = app.staticTexts["location-hint"]
        XCTAssertTrue(hint.waitForExistence(timeout: 10))
        XCTAssertTrue(hint.label.contains("Position nicht verfügbar"))
        XCTAssertEqual(app.mapStatus.values["center"], before)

        // Asking again does not bring the dialog back, the hint stays.
        app.buttons["center-location"].tap()
        XCTAssertTrue(hint.waitForExistence(timeout: 5))
    }
}
