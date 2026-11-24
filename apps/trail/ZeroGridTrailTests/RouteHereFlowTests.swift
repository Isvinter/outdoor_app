import CoreLocation
import XCTest
@testable import ZeroGridTrail

@MainActor
private final class FakeHost: RouteHereHost {
    var hasRoute = false
    var isReplacingRoute = false
    var position: CLLocation?
    var unsaved = true
    var start = RouteStart.ready
    var started: [(CLLocationCoordinate2D, CLLocationCoordinate2D)] = []
    var appended: [CLLocationCoordinate2D] = []
    var begun: [CLLocationCoordinate2D] = []
    var picking = 0
    var acceptsRoute = true

    func routeIsUnsaved() async -> Bool { unsaved }

    func routeStart(position: KnownPosition, target: CLLocationCoordinate2D) async -> RouteStart { start }

    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> Bool {
        guard acceptsRoute else { return false }
        started.append((from, to))
        return true
    }

    func appendTarget(_ to: CLLocationCoordinate2D) -> Bool {
        guard acceptsRoute else { return false }
        appended.append(to)
        return true
    }

    func beginRoute(at: CLLocationCoordinate2D) -> Bool {
        guard acceptsRoute else { return false }
        begun.append(at)
        return true
    }

    func keepPicking() {
        picking += 1
    }
}

@MainActor
final class RouteHereFlowTests: XCTestCase {
    private let target = CLLocationCoordinate2D(latitude: 47.52, longitude: 11.58)
    private let fix = CLLocation(latitude: 47.5, longitude: 11.56)

    private func wait(_ what: String, _ condition: @MainActor () -> Bool) async {
        for _ in 0..<300 where !condition() {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), what)
    }

    private func flow(_ host: FakeHost) -> (RouteHereFlow, Counts) {
        let flow = RouteHereFlow(host: host)
        let counts = Counts()
        flow.onDone = { counts.done += 1 }
        flow.onGuide = { counts.guided += 1 }
        flow.onPoint = { counts.marks.append($0) }
        return (flow, counts)
    }

    private final class Counts {
        var done = 0
        var guided = 0
        var marks: [CLLocationCoordinate2D?] = []
    }

    func testTappedPointIsOfferedMarkedAndDismissed() {
        let (flow, counts) = flow(FakeHost())
        flow.offer(target)
        XCTAssertEqual(flow.point, MapPoint(lat: target.latitude, lon: target.longitude, placed: false))
        XCTAssertEqual(counts.marks.last??.latitude, target.latitude)
        flow.dismiss()
        XCTAssertNil(flow.point)
        XCTAssertNil(counts.marks.last ?? nil)
    }

    /// A placed point is a waypoint already: no mark of its own.
    func testPlacedPointIsNotMarked() {
        let (flow, counts) = flow(FakeHost())
        flow.offer(target, placed: true)
        XCTAssertEqual(flow.point?.placed, true)
        XCTAssertEqual(counts.marks.count, 1)
        XCTAssertNil(counts.marks[0])
    }

    /// "Route hierher" plans from the position at once; then taps add points.
    func testRouteHereStartsFromThePosition() async {
        let host = FakeHost()
        host.position = fix
        let (flow, counts) = flow(host)
        flow.offer(target)
        flow.routeHere(to: target)
        await wait("route started") { host.started.count == 1 }
        XCTAssertEqual(host.started[0].0.latitude, fix.coordinate.latitude)
        XCTAssertEqual(host.started[0].1.latitude, target.latitude)
        XCTAssertNil(flow.point)
        XCTAssertFalse(flow.busy)
        XCTAssertEqual(counts.done, 1)
        XCTAssertEqual(counts.guided, 0)
        XCTAssertEqual(host.picking, 1)
    }

    /// "Hierher führen" plans from the position and asks for the guidance, not for more points.
    func testGuideHereAsksForTheGuidance() async {
        let host = FakeHost()
        host.position = fix
        host.hasRoute = true
        let (flow, counts) = flow(host)
        flow.guideHere(to: target)
        await wait("route started") { host.started.count == 1 }
        XCTAssertEqual(counts.guided, 1)
        XCTAssertEqual(host.picking, 0)
    }

    func testWithoutPositionNothingIsPlannedFromIt() {
        let host = FakeHost()
        let (flow, counts) = flow(host)
        flow.routeHere(to: target)
        XCTAssertEqual(flow.message, RouteHereFlow.noPosition)
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertEqual(counts.done, 0)
    }

    func testPositionOnAnotherMapIsTold() async {
        let host = FakeHost()
        host.position = fix
        host.start = .otherMap
        let (flow, _) = flow(host)
        flow.offer(target)
        flow.routeHere(to: target)
        await wait("told") { flow.message == RouteHereFlow.otherMap }
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertNotNil(flow.point)
    }

    /// Without a position the route begins at the place; taps add the next points.
    func testPlanFromBeginsTheRouteThere() {
        let host = FakeHost()
        let (flow, counts) = flow(host)
        flow.offer(target)
        flow.planFrom(target)
        XCTAssertEqual(host.begun.first?.latitude, target.latitude)
        XCTAssertNil(flow.point)
        XCTAssertEqual(counts.done, 1)
        XCTAssertEqual(host.picking, 1)
    }

    func testAppendKeepsPicking() {
        let host = FakeHost()
        host.hasRoute = true
        let (flow, counts) = flow(host)
        flow.append(target)
        XCTAssertEqual(host.appended.count, 1)
        XCTAssertEqual(counts.done, 1)
        XCTAssertEqual(host.picking, 1)
    }

    /// An answer about an earlier place does nothing once another is chosen.
    func testLateAnswerIsDropped() async {
        let host = FakeHost()
        host.position = fix
        let (flow, _) = flow(host)
        flow.offer(target)
        flow.routeHere(to: target)
        flow.dismiss()
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertFalse(flow.busy)
    }

    func testRouteBeingReplacedTakesNothing() {
        let host = FakeHost()
        host.isReplacingRoute = true
        let (flow, counts) = flow(host)
        flow.offer(target)
        XCTAssertNil(flow.point)
        flow.planFrom(target)
        XCTAssertTrue(host.begun.isEmpty)
        XCTAssertEqual(counts.done, 0)
    }

    func testUnsavedIsAskedOnlyWithARoute() async {
        let host = FakeHost()
        let (flow, _) = flow(host)
        await flow.checkUnsaved()
        XCTAssertFalse(flow.unsaved)
        host.hasRoute = true
        await flow.checkUnsaved()
        XCTAssertTrue(flow.unsaved)
    }
}
