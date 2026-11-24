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

    private let choice = RouteHereStep.pickStart(otherMap: false, offersPosition: true)

    /// Starts the flow on an empty route and takes the own position as the start.
    private func beginFromPosition(_ flow: RouteHereFlow) {
        flow.begin(target: target)
        XCTAssertEqual(flow.step, choice)
        flow.chooseMyPosition()
    }

    func testEmptyRouteAsksForTheStartFirst() async {
        let host = FakeHost()
        host.start = .ready
        host.lastPosition = fix
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await settle()
        XCTAssertEqual(flow.step, choice)
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertEqual(host.requests, 0)
    }

    func testKnownPositionStartsTheRoute() async {
        let host = FakeHost()
        host.start = .ready
        host.lastPosition = fix
        var done = 0
        let flow = RouteHereFlow(host: host)
        flow.onDone = { done += 1 }
        beginFromPosition(flow)
        await wait("route") { host.started.count == 1 }
        XCTAssertEqual(host.started[0].0.latitude, fix.coordinate.latitude)
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(done, 1)
    }

    func testNewRouteAsksForTheStart() async {
        let host = FakeHost()
        host.hasRoute = true
        host.lastPosition = fix
        host.start = .ready
        let flow = RouteHereFlow(host: host)
        flow.begin(target: target)
        await wait("choice") { flow.step == .choosing(unsaved: true) }
        flow.chooseNewRoute()
        XCTAssertEqual(flow.step, choice)
        XCTAssertTrue(host.started.isEmpty)
    }

    func testASearchHitSetsTheStart() {
        let host = FakeHost()
        var done = 0
        let flow = RouteHereFlow(host: host)
        flow.onDone = { done += 1 }
        flow.hitChosen(target)
        XCTAssertEqual(flow.step, choice)
        // Searching for the start drops the chosen target hit; the flow goes on.
        flow.selectionCleared()
        XCTAssertEqual(flow.step, choice)
        // The target itself is no start.
        flow.hitChosen(target)
        XCTAssertEqual(flow.step, choice)
        flow.hitChosen(tap)
        XCTAssertEqual(host.started.count, 1)
        XCTAssertEqual(host.started[0].0.latitude, tap.latitude)
        XCTAssertEqual(host.started[0].1.latitude, target.latitude)
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(done, 1)
    }

    func testClearedSelectionEndsTheFlowWhileWaiting() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
        await wait("waiting") { flow.step == .waiting }
        flow.selectionCleared()
        XCTAssertEqual(flow.step, .idle)
        XCTAssertEqual(host.stops, 1)
    }

    func testATapBeforeThePositionIsCheckedWins() async {
        let host = FakeHost()
        host.start = .ready
        host.lastPosition = fix
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
        XCTAssertTrue(flow.tapped(tap))
        await settle()
        XCTAssertEqual(host.started.count, 1)
        XCTAssertEqual(host.started[0].0.latitude, tap.latitude)
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
        beginFromPosition(flow)
        await wait("waiting") { flow.step == .waiting }
        flow.chooseStartMyself()
        XCTAssertEqual(flow.step, choice)
        XCTAssertEqual(host.stops, 1)
        host.start = .ready
        flow.received(fix)
        await settle()
        XCTAssertTrue(host.started.isEmpty)
        XCTAssertEqual(flow.step, choice)
    }

    func testLateFixIgnoredAfterCancel() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
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
        beginFromPosition(flow)
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
        beginFromPosition(flow)
        await wait("waiting") { flow.step == .waiting }
        await wait("tap") { flow.step == self.choice }
    }

    func testDeniedPermissionGoesStraightToTap() async {
        let host = FakeHost()
        host.deniedAtOnce = true
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
        await wait("tap") { flow.step == .pickStart(otherMap: false, offersPosition: false) }
    }

    func testPositionOnAnotherMapAsksForStart() async {
        let host = FakeHost()
        host.start = .otherMap
        host.lastPosition = fix
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
        await wait("tap") { flow.step == .pickStart(otherMap: true, offersPosition: false) }
    }

    func testTapOutsideFlowIsNotConsumed() {
        let flow = RouteHereFlow(host: FakeHost())
        XCTAssertFalse(flow.tapped(tap))
    }

    func testTapSetsTheStart() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host, positionWait: 0.01)
        flow.begin(target: target)
        await wait("tap") { flow.step == self.choice }
        XCTAssertTrue(flow.tapped(tap))
        XCTAssertEqual(host.started[0].0.latitude, tap.latitude)
        XCTAssertEqual(host.started[0].1.latitude, target.latitude)
    }

    func testRouteChangedWhileWaitingEndsTheFlow() async {
        let host = FakeHost()
        let flow = RouteHereFlow(host: host)
        beginFromPosition(flow)
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
        await wait("tap") { flow.step == self.choice }
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
