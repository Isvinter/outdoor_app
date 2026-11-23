import CoreLocation
import XCTest
@testable import ZeroGridTrail

@MainActor
private final class FakeHost: RouteHereHost {
    var hasRoute = false
    var isReplacingRoute = false
    var lastPosition: CLLocation?
    var unsaved = true
    var start = RouteStart.noPosition
    var deniedAtOnce = false
    var started: [(CLLocationCoordinate2D, CLLocationCoordinate2D)] = []
    var appended: [CLLocationCoordinate2D] = []
    var requests = 0
    var stops = 0
    var pickingStopped = 0
    var acceptsRoute = true

    func routeIsUnsaved() async -> Bool { unsaved }

    func routeStart(position: KnownPosition?, target: CLLocationCoordinate2D) async -> RouteStart {
        position == nil && start == .ready ? .noPosition : start
    }

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

    func requestPosition(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void) {
        requests += 1
        if deniedAtOnce { denied() }
    }

    func stopRequestingPosition() {
        stops += 1
    }

    func stopPicking() {
        pickingStopped += 1
    }
}

@MainActor
final class RouteHereFlowTests: XCTestCase {
    private let target = CLLocationCoordinate2D(latitude: -13.52, longitude: -71.98)
    private let tap = CLLocationCoordinate2D(latitude: -13.51, longitude: -71.97)
    private let fix = CLLocation(latitude: -13.5, longitude: -71.96)

    private func wait(_ what: String, _ condition: @MainActor () -> Bool) async {
        for _ in 0..<300 where !condition() {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), what)
    }

    private func settle() async {
        try? await Task.sleep(nanoseconds: 40_000_000)
    }

    func testEmptyRouteSkipsChoice() async {
        let host = FakeHost()
        host.start = .ready
        host.lastPosition = fix
        var done = 0
        let flow = RouteHereFlow(host: host)
        flow.onDone = { done += 1 }
        flow.begin(target: target)
        await wait("route") { host.started.count == 1 }
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(done, 1)
    }

    func testOpenRouteAsks() async {
        let host = FakeHost()
        host.hasRoute = true
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("choice") { flow.step == .choosing(unsaved: true) }
        flow.chooseAppend()
        XCTAssertEqual(host.appended.count, 1)
        XCTAssertEqual(flow.step, .idle)
    }

    func testLateFixIgnoredAfterChooseStart() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("waiting") { flow.step == .waiting }
        flow.chooseStartMyself()
        XCTAssertEqual(flow.step, .tapStart(otherMap: false))
        XCTAssertEqual(host.stops, 1)
        host.start = .ready
        flow.received(fix)
        await settle()
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertEqual(flow.step, .tapStart(otherMap: false))
    }

    func testLateFixIgnoredAfterCancel() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("waiting") { flow.step == .waiting }
        flow.cancel()
        host.start = .ready
        flow.received(fix)
        await settle()
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertEqual(flow.step, .idle)
    }

    func testAFixInTimeStartsTheRoute() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("waiting") { flow.step == .waiting }
        host.start = .ready
        flow.received(fix)
        await wait("route") { host.started.count == 1 }
        XCTAssertEqual(host.started[0].0.latitude, -13.5)
        XCTAssertEqual(host.stops, 1)
    }

    func testThirtySecondsLeadToTapStart() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host, positionWait: 0.02)
        flow.begin(target: target)
        await wait("tap") { flow.step == .tapStart(otherMap: false) }
    }

    func testDeniedPermissionGoesStraightToTap() async {
        let host = FakeHost()
        host.deniedAtOnce = true
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("tap") { flow.step == .tapStart(otherMap: false) }
    }

    func testPositionOnAnotherMapAsksForStart() async {
        let host = FakeHost()
        host.start = .otherMap
        host.lastPosition = fix
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("tap") { flow.step == .tapStart(otherMap: true) }
    }

    func testTapOutsideFlowIsNotConsumed() {
        let flow = RouteHereFlow(host: FakeHost())
        XCTAssertFalse(flow.tapped(tap))
    }

    func testTapSetsTheStart() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host, positionWait: 0.01)
        flow.begin(target: target)
        await wait("tap") { flow.step == .tapStart(otherMap: false) }
        XCTAssertTrue(flow.tapped(tap))
        XCTAssertEqual(host.started[0].0.latitude, tap.latitude)
        XCTAssertEqual(host.started[0].1.latitude, target.latitude)
    }

    func testRouteChangedWhileWaitingEndsTheFlow() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("waiting") { flow.step == .waiting }
        flow.routeChanged()
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(host.stops, 1)
        host.start = .ready
        flow.received(fix)
        await settle()
        XCTAssertTrue(host.started.isEmpty)
    }

    func testRefusedRouteDoesNotReportDone() async {
        let host = FakeHost()
        host.acceptsRoute = false
        var done = 0
        let flow = RouteHereFlow(host: host, positionWait: 0.01)
        flow.onDone = { done += 1 }
        flow.begin(target: target)
        await wait("tap") { flow.step == .tapStart(otherMap: false) }
        XCTAssertTrue(flow.tapped(tap))
        XCTAssertEqual(done, 0)
        XCTAssertEqual(flow.step, .idle)
    }

    func testNothingHappensWhileARouteIsBeingReplaced() async {
        let host = FakeHost()
        host.isReplacingRoute = true
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await settle()
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(host.requests, 0)
    }
}
