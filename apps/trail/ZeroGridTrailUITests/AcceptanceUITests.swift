import XCTest

/// The acceptance test of I3: the reference hike of the spike, planned by
/// tapping, then saved, loaded, changed and saved again.
///
/// In the simulator it needs `ISARWINKEL_PACKAGE`, the path of a package of
/// `isarwinkel-achensee` on the Mac (`scripts/test.sh` sets it if it can). On
/// a device the package cannot be copied from the Mac: with
/// `ISARWINKEL_INSTALLED=1` the test takes the region that is installed on the
/// device, keeps the data of the app and deletes the route it saved.
final class AcceptanceUITests: XCTestCase {
    /// The three points of the reference hike; 12 353 m on the desktop.
    private let reference = [(47.6803, 11.5741), (47.6625, 11.5233), (47.645, 11.556)]

    override func setUp() {
        continueAfterFailure = false
    }

    /// Screen positions from a status value "x,y;x,y".
    private func points(_ value: String?) -> [CGVector] {
        (value ?? "").split(separator: ";").compactMap { pair in
            let parts = pair.split(separator: ",").compactMap { Double($0) }
            return parts.count == 2 ? CGVector(dx: parts[0], dy: parts[1]) : nil
        }
    }

    /// Drags the map until the reference point `index` lies clear of the bars
    /// and the sheet, and returns where it is on the screen.
    private func bring(_ index: Int, onto app: XCUIApplication) -> CGVector {
        let screen = app.frame.size
        let middle = CGVector(dx: screen.width / 2, dy: screen.height * 0.45)
        let origin = app.coordinate(withNormalizedOffset: .zero)
        for _ in 0..<20 {
            // Wait until the map has come to rest after the drag.
            var place = points(app.mapStatus.values["project"])[index]
            var rests = 0
            for _ in 0..<30 where rests < 3 {
                Thread.sleep(forTimeInterval: 0.4)
                let now = points(app.mapStatus.values["project"])[index]
                rests = abs(now.dx - place.dx) < 0.2 && abs(now.dy - place.dy) < 0.2 ? rests + 1 : 0
                place = now
            }
            if abs(place.dx - middle.dx) < screen.width / 2 - 60, abs(place.dy - middle.dy) < screen.height * 0.22 {
                return place
            }
            // A drag that starts on a waypoint would move the waypoint, not the map.
            let waypoints = points(app.mapStatus.values["waypoint_xy"])
            let starts = [0, -90, 90, -180, 180].map { CGVector(dx: middle.dx, dy: middle.dy + $0) }
            let from = starts.first { start in
                waypoints.allSatisfy { abs($0.dx - start.dx) > 45 || abs($0.dy - start.dy) > 45 }
            } ?? middle
            let step = { (distance: Double) in max(-220, min(220, distance)) }
            let to = CGVector(dx: from.dx + step(middle.dx - place.dx), dy: from.dy + step(middle.dy - place.dy))
            origin.withOffset(from).press(
                forDuration: 0.05, thenDragTo: origin.withOffset(to), withVelocity: .slow, thenHoldForDuration: 0.3)
        }
        XCTFail("reference point \(index + 1) did not come onto the screen")
        return middle
    }

    private func launch() throws -> XCUIApplication {
        let environment = ProcessInfo.processInfo.environment
        let app = XCUIApplication()
        app.launchArguments = ["-selftest"]
        if environment["ISARWINKEL_INSTALLED"] != "1" {
            guard let package = environment["ISARWINKEL_PACKAGE"], !package.isEmpty else {
                throw XCTSkip("ISARWINKEL_PACKAGE is not set")
            }
            app.launchArguments += ["-resetData", "-importFixture", package]
        }
        // At zoom 14 a point of the screen is 3 m, fine enough for Valhalla to
        // take the same ways as from the coordinates. The points are 4 km apart,
        // so the test drags the map from one to the next.
        app.launchArguments += [
            "-startCenter", "\(reference[0].0),\(reference[0].1)", "-startZoom", "14",
            "-project", reference.map { "\($0.0),\($0.1)" }.joined(separator: ";"),
        ]
        app.launch()
        return app
    }

    func testReferenceHikeByTapping() throws {
        let app = try launch()
        let start = app.waitForStatus("map rendered", timeout: 60) { $0.rendered && $0.values["project"] != nil }
        guard !start.list("regions").isEmpty else { throw XCTSkip("no region is installed") }
        let routesBefore = start.int("routes")
        let origin = app.coordinate(withNormalizedOffset: .zero)

        // Hiking with its defaults is what the form starts with: T3, hills 80 %.
        XCTAssertEqual(app.buttons["plan-profile"].value as? String, "Wandern")
        for index in reference.indices {
            // A waypoint that did not land on the reference point (the map may
            // still have been gliding) is taken back and set again.
            var landed = false
            for _ in 0..<5 where !landed {
                origin.withOffset(bring(index, onto: app)).tap()
                app.waitForStatus("waypoint \(index + 1) set") {
                    $0.int("waypoints") == index + 1 && $0.int("planning") == 0
                }
                Thread.sleep(forTimeInterval: 1)
                let now = app.mapStatus
                let wanted = points(now.values["project"])[index]
                let set = points(now.values["waypoint_xy"])[index]
                landed = abs(wanted.dx - set.dx) <= 0.5 && abs(wanted.dy - set.dy) <= 0.5
                if !landed {
                    print("acceptance: waypoint \(index + 1) missed by \(set.dx - wanted.dx),\(set.dy - wanted.dy)")
                    app.buttons["plan-undo"].tap()
                    app.waitForStatus("waypoint taken back") { $0.int("waypoints") == index }
                }
            }
            XCTAssertTrue(landed, "waypoint \(index + 1) did not land on the reference point")
        }

        let planned = app.waitForPlan(waypoints: 3)
        XCTAssertEqual(planned.int("failed_legs"), 0)
        XCTAssertEqual(planned.list("route_colors"), ["#c62828", "#c62828"])
        XCTAssertEqual(Double(planned.int("distance_m")), 12353, accuracy: 124)
        print("acceptance: \(planned.values)")

        // Save, clear, load.
        app.pullSheet(by: -250)
        app.saveRoute()
        let saved = app.waitForStatus("route saved") {
            $0.int("routes") == routesBefore + 1 && $0.values["route_id"] != "-"
        }
        app.buttons["plan-clear"].tap()
        XCTAssertEqual(app.waitForPlan(waypoints: 0).values["route_id"], "-")
        app.pullSheet(by: -500)
        app.buttons["route-row-1"].tap()
        let loaded = app.waitForStatus("route loaded") {
            $0.int("waypoints") == 3 && $0.int("planning") == 0 && $0.list("route_colors").count == 2
        }
        XCTAssertEqual(loaded.values["route_id"], saved.values["route_id"])
        XCTAssertEqual(loaded.int("distance_m"), planned.int("distance_m"))

        // Move point 2 and save: the same route, once more.
        Thread.sleep(forTimeInterval: 1.5)  // the map glides to the route
        let second = points(app.mapStatus.values["waypoint_xy"])[1]
        origin.withOffset(second).press(
            forDuration: 0.1, thenDragTo: origin.withOffset(CGVector(dx: second.dx + 25, dy: second.dy + 15)))
        let moved = app.waitForStatus("route changed") {
            $0.int("planning") == 0 && $0.int("distance_m") != loaded.int("distance_m")
        }
        XCTAssertEqual(moved.int("failed_legs"), 0)
        app.pullSheet(by: -250)
        app.saveRoute()
        let again = app.waitForStatus("saved over the loaded route") {
            $0.int("route_rev") == saved.int("route_rev") + 1
        }
        XCTAssertEqual(again.int("routes"), routesBefore + 1)
        XCTAssertEqual(again.values["route_id"], saved.values["route_id"])
        print("acceptance: \(again.values)")

        // Leave no route of the test behind on a device.
        if ProcessInfo.processInfo.environment["ISARWINKEL_INSTALLED"] == "1" {
            app.pullSheet(by: -500)
            app.buttons["route-row-1"].swipeLeft()
            app.buttons["Löschen"].tap()
            app.waitForStatus("route of the test deleted") { $0.int("routes") == routesBefore }
        }
    }

    /// Measures how long a new leg takes, from the input to the drawn route:
    /// eleven points tapped around the first reference point, ten hiking legs.
    /// Prints `plan_ms`, the median of the last ten. Saves nothing.
    func testLegTimes() throws {
        let app = try launch()
        let start = app.waitForStatus("map rendered", timeout: 60) { $0.rendered }
        guard !start.list("regions").isEmpty else { throw XCTSkip("no region is installed") }

        let spots: [(Double, Double)] = [
            (0.5, 0.45), (0.25, 0.3), (0.75, 0.3), (0.3, 0.55), (0.7, 0.6), (0.5, 0.25),
            (0.2, 0.45), (0.8, 0.45), (0.4, 0.65), (0.6, 0.35), (0.5, 0.55),
        ]
        for (index, spot) in spots.enumerated() {
            app.place(spot.0, spot.1).tap()
            app.waitForStatus("leg \(index) computed") {
                $0.int("waypoints") == index + 1 && $0.int("planning") == 0 && $0.int("plans") == index
                    || index == 0 && $0.int("waypoints") == 1
            }
        }
        let status = app.mapStatus
        print("legtimes: plan_ms=\(status.int("plan_ms")) plans=\(status.int("plans")) "
            + "failed_legs=\(status.int("failed_legs")) distance_m=\(status.int("distance_m"))")
        XCTAssertEqual(status.int("plans"), 10)
        XCTAssertGreaterThan(status.int("distance_m"), 0)

        app.buttons["plan-clear"].tap()
        app.waitForPlan(waypoints: 0)
    }
}
