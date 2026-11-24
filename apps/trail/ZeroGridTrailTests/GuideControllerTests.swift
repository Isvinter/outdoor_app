import Combine
import XCTest
@testable import ZeroGridTrail

@MainActor
final class GuideControllerTests: XCTestCase {
    private var sandbox: Sandbox!
    private var manager: FakeLocationManager!

    override func setUp() async throws {
        sandbox = try Sandbox()
        manager = FakeLocationManager()
    }

    override func tearDown() async throws {
        sandbox.remove()
    }

    /// A straight route across the mini region; needs no routing data.
    private func planStraightRoute() throws {
        let leg = LegInput(profile: "manual", options: [])
        let request = [LatLon(lat: 47.572, lon: 11.545), LatLon(lat: 47.578, lon: 11.555)]
            .reduce(planEmpty()) { planAddWaypoint(request: $0, at: $1, leg: leg) }
        _ = try sandbox.host.core.planRoute(request: request)
    }

    static func info(phase: String = "on_route", deviation: UInt32, next: UInt32? = 1) -> GuidanceInfo {
        GuidanceInfo(
            phase: phase, alongM: 0, nextWaypoint: next, deviation: deviation, wayBack: [], wayBackM: 650,
            toStageEndM: nil, toStageEndS: nil, toDestinationM: 1000, toDestinationS: nil)
    }

    func testToggleRequiresAndReleasesGuidanceLocation() throws {
        try planStraightRoute()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: sandbox.host.guidanceChanges)
        var asked = 0
        guide.onFirstStart = { asked += 1 }

        guide.toggle()
        XCTAssertTrue(guide.active)
        XCTAssertTrue(guide.following)
        XCTAssertTrue(manager.updating)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)
        XCTAssertNotNil(sandbox.host.core.guidanceStatus())

        guide.toggle()
        XCTAssertFalse(guide.active)
        XCTAssertFalse(manager.updating)
        XCTAssertNil(sandbox.host.core.guidanceStatus())

        guide.toggle()
        guide.toggle()
        XCTAssertEqual(asked, 1, "the permission is asked for once")
    }

    func testWithoutRouteSaysSo() {
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: sandbox.host.guidanceChanges)
        guide.toggle()
        XCTAssertFalse(guide.active)
        XCTAssertNotNil(guide.message)
        XCTAssertFalse(manager.updating)
    }

    func testDeviationPublisherFiresOncePerDeviation() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        var told: [UInt32] = []
        let watch = guide.newDeviation.sink { told.append($0.deviation) }

        changes.send(Self.info(deviation: 0))
        changes.send(Self.info(phase: "off_route", deviation: 1))
        changes.send(Self.info(phase: "off_route", deviation: 1))
        changes.send(Self.info(deviation: 1))
        changes.send(Self.info(phase: "off_route", deviation: 2))
        XCTAssertEqual(told, [1, 2])

        // A guide set anew counts from zero.
        changes.send(Self.info(deviation: 0))
        changes.send(Self.info(phase: "off_route", deviation: 1))
        XCTAssertEqual(told, [1, 2, 1])

        // Ended and started again.
        changes.send(nil)
        XCTAssertFalse(manager.updating)
        changes.send(Self.info(phase: "off_route", deviation: 1))
        XCTAssertEqual(told, [1, 2, 1, 1])
        watch.cancel()
    }

    func testArrivedIsShownAfterTheEnd() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        changes.send(Self.info(deviation: 0))
        changes.send(Self.info(phase: "arrived", deviation: 0))
        changes.send(nil)
        XCTAssertNil(guide.info)
        XCTAssertTrue(guide.arrived)
        XCTAssertFalse(manager.updating)
    }

    func testBarTexts() {
        var info = Self.info(phase: "off_route", deviation: 1, next: 3)
        XCTAssertEqual(GuideBar.wayBackText(info, waypoints: 6), "Weg zurück zu Wegpunkt 4: 650 m")
        XCTAssertEqual(GuideBar.wayBackText(info, waypoints: 4), "Weg zurück zum Ziel: 650 m")
        XCTAssertEqual(GuideBar.lines(info), ["Ziel: 1,00 km"])
        info.toStageEndM = 12_300
        info.toStageEndS = 48 * 60
        info.toDestinationS = 310 * 60
        XCTAssertEqual(GuideBar.lines(info), ["Etappe: 12,30 km · 0:48 h", "Ziel: 1,00 km · 5:10 h"])
        info.phase = "on_route"
        XCTAssertNil(GuideBar.wayBackText(info, waypoints: 6))
    }

    /// The pale part of the route ends where the user is.
    func testDoneLineEndsAtProgress() {
        let overlay = GuideOverlay(mapView: .init(frame: .zero))
        let a = CLLocationCoordinate2D(latitude: 47.0, longitude: 11.0)
        let b = CLLocationCoordinate2D(latitude: 47.001, longitude: 11.0)
        let c = CLLocationCoordinate2D(latitude: 47.002, longitude: 11.0)
        overlay.setRoute([[a, b], [b, c]])
        let half = overlay.line(upTo: 150)
        XCTAssertEqual(half.count, 3)
        XCTAssertEqual(half.last!.latitude, 47.0 + 0.001 * 150 / CLLocation(latitude: 47, longitude: 11)
            .distance(from: CLLocation(latitude: 47.001, longitude: 11)), accuracy: 1e-6)
        XCTAssertTrue(overlay.line(upTo: 0).isEmpty)
        XCTAssertEqual(overlay.line(upTo: 10_000).count, 3)
    }

    /// The same distances as the core (`core_track::geo`): one degree of latitude.
    func testHaversineAsTheCore() {
        let d = GuideOverlay.haversine(.init(latitude: 47, longitude: 11), .init(latitude: 48, longitude: 11))
        XCTAssertEqual(d, 111_195.08, accuracy: 0.01)
    }

    /// A failed leg is a straight line between its waypoints, measured like the others.
    func testFailedLegCountsStraight() {
        let overlay = GuideOverlay(mapView: .init(frame: .zero))
        let a = CLLocationCoordinate2D(latitude: 47.0, longitude: 11.0)
        let b = CLLocationCoordinate2D(latitude: 47.01, longitude: 11.0)
        let c = CLLocationCoordinate2D(latitude: 47.02, longitude: 11.0)
        overlay.setRoute([[a, b], [b, c]])
        let leg = GuideOverlay.haversine(a, b)
        let mid = overlay.line(upTo: leg * 1.5)
        XCTAssertEqual(mid.last!.latitude, 47.015, accuracy: 1e-9)
    }
}
