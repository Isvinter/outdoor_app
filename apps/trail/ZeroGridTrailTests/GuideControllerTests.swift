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

    static func info(
        phase: String = "on_route", deviation: UInt32, next: UInt32? = 1, turn: TurnInfo? = nil,
        turnSeq: UInt32 = 0, hasTurns: Bool = true, offNetwork: OffNetworkInfo? = nil, offNetworkSeq: UInt32 = 0
    ) -> GuidanceInfo {
        GuidanceInfo(
            phase: phase, alongM: 0, nextWaypoint: next, deviation: deviation, wayBack: [], wayBackM: 650,
            toStageEndM: nil, toStageEndS: nil, toDestinationM: 1000, toDestinationS: nil, turn: turn,
            turnSeq: turnSeq, hasTurns: hasTurns, offNetwork: offNetwork, offNetworkSeq: offNetworkSeq)
    }

    static let forest = OffNetworkInfo(
        distanceM: 118, bearingDeg: 147, text: "Die Route liegt 100 m bei 150°",
        spoken: "Abseits der Route. Die Route liegt 100 m bei 150 Grad.")

    /// Off the route and off the network of ways, for the `seq`th time.
    static func lost(_ deviation: UInt32, seq: UInt32, phase: String = "off_route") -> GuidanceInfo {
        info(phase: phase, deviation: deviation, offNetwork: forest, offNetworkSeq: seq)
    }

    static func turn(_ kind: String = "right", stage: String = "ahead", metres: Double = 200) -> TurnInfo {
        let label = kind == "right" ? "rechts" : kind
        let text = stage == "now" ? "Jetzt \(label)" : "In \(Int(metres)) m \(label)"
        return TurnInfo(
            kind: kind, stage: stage, distanceM: metres, distanceText: "\(Int(metres)) m", label: label,
            spoken: text)
    }

    private func wait(_ what: String, _ condition: @MainActor () -> Bool) async {
        for _ in 0..<400 where !condition() {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), what)
    }

    /// The controller follows the listener, which tells about start and stop a moment later.
    func testToggleRequiresAndReleasesGuidanceLocation() async throws {
        try planStraightRoute()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: sandbox.host.guidanceChanges)
        var asked = 0
        guide.onFirstStart = { asked += 1 }

        guide.toggle()
        XCTAssertNotNil(sandbox.host.core.guidanceStatus())
        await wait("started") { guide.active }
        XCTAssertTrue(guide.following)
        XCTAssertTrue(manager.updating)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)

        guide.toggle()
        XCTAssertNil(sandbox.host.core.guidanceStatus())
        await wait("stopped") { !guide.active }
        XCTAssertFalse(manager.updating)

        guide.toggle()
        await wait("started again") { guide.active }
        guide.toggle()
        await wait("stopped again") { !guide.active }
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

    /// Told once each time the user gets off the network of ways, never for a
    /// deviation on it (the route is planned anew without a word).
    func testOffNetworkPublisherFiresOncePerTime() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        var told: [UInt32] = []
        let watch = guide.newOffNetwork.sink { told.append($0.offNetworkSeq) }

        changes.send(Self.info(deviation: 0))
        changes.send(Self.info(phase: "off_route", deviation: 1))
        XCTAssertEqual(told, [], "on the network: nothing to tell")
        changes.send(Self.lost(2, seq: 1))
        changes.send(Self.lost(3, seq: 1))
        changes.send(Self.info(deviation: 3, offNetworkSeq: 1))
        changes.send(Self.lost(4, seq: 2, phase: "no_way_back"))
        XCTAssertEqual(told, [1, 2])

        // A guide set anew counts from zero.
        changes.send(Self.info(deviation: 0))
        changes.send(Self.lost(1, seq: 1))
        XCTAssertEqual(told, [1, 2, 1])

        // Ended and started again.
        changes.send(nil)
        XCTAssertFalse(manager.updating)
        changes.send(Self.lost(1, seq: 1))
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

    /// Each deviation with a way back asks once to plan the route anew from there.
    func testRerouteOncePerDeviationWithAWayBack() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        var asked: [(LatLon, Int)] = []
        guide.onReroute = { asked.append(($0, $1)) }
        let off = LatLon(lat: 47.575, lon: 11.56)
        func status(_ phase: String, _ deviation: UInt32, back: Bool, next: UInt32 = 2) -> GuidanceInfo {
            var info = Self.info(phase: phase, deviation: deviation, next: next)
            info.wayBack = back ? [off, LatLon(lat: 47.576, lon: 11.56)] : []
            return info
        }

        changes.send(status("on_route", 0, back: false))
        changes.send(status("no_way_back", 1, back: false))
        XCTAssertTrue(asked.isEmpty, "no way back, nothing to plan")
        // Found on a retry, for the same deviation.
        changes.send(status("off_route", 1, back: true))
        changes.send(status("off_route", 1, back: true))
        XCTAssertEqual(asked.count, 1)
        XCTAssertEqual(asked[0].0, off)
        XCTAssertEqual(asked[0].1, 2)

        // Off the network of ways the route stays; reaching it plans anew.
        changes.send(status("on_route", 0, back: false, next: 1))
        var forest = status("off_route", 1, back: true, next: 1)
        forest.offNetwork = Self.forest
        forest.offNetworkSeq = 1
        changes.send(forest)
        XCTAssertEqual(asked.count, 1, "not off the network")
        forest.offNetwork = nil
        changes.send(forest)
        XCTAssertEqual(asked.count, 2)
        asked.removeLast()

        // The guide set anew on the new route counts from zero.
        changes.send(status("on_route", 0, back: false, next: 1))
        changes.send(status("off_route", 1, back: true, next: 1))
        XCTAssertEqual(asked.count, 2)
        XCTAssertEqual(asked[1].1, 1)
    }

    /// Each new hint vibrates once and is passed on once; a new guide counts from zero.
    func testTurnsAreToldOncePerHint() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        let haptics = CountingHaptics()
        guide.haptics = haptics
        var told: [String] = []
        let watch = guide.newTurn.sink { told.append($0.spoken) }

        changes.send(Self.info(deviation: 0))
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 200), turnSeq: 1))
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 150), turnSeq: 1))
        changes.send(Self.info(deviation: 0, turn: Self.turn(stage: "now", metres: 10), turnSeq: 2))
        changes.send(Self.info(deviation: 0, turnSeq: 2))
        XCTAssertEqual(told, ["In 200 m rechts", "Jetzt rechts"])
        XCTAssertEqual(haptics.turns, 2)
        XCTAssertEqual(haptics.deviations, 0)

        // A guide set anew (rerouted) counts from zero.
        changes.send(Self.info(deviation: 0, turnSeq: 0))
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 100), turnSeq: 1))
        XCTAssertEqual(told.count, 3)

        // Ended and started again.
        changes.send(nil)
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 100), turnSeq: 1))
        XCTAssertEqual(told.count, 4)
        XCTAssertEqual(haptics.turns, 4)
        watch.cancel()
    }

    /// The announcer hears of each hint, of its end, of deviations and of the arrival.
    func testAnnouncerIsTold() {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        let center = FakeCenter()
        let speaker = RecordingSpeaker()
        let suite = "GuideControllerTests.announcer"
        UserDefaults().removePersistentDomain(forName: suite)
        defer { UserDefaults().removePersistentDomain(forName: suite) }
        let announcer = TurnAnnouncer(center: center, speaker: speaker, defaults: UserDefaults(suiteName: suite)!)
        announcer.speech = true
        announcer.notifications = true
        guide.announcer = announcer

        changes.send(Self.info(deviation: 0))
        XCTAssertTrue(center.removed.isEmpty, "nothing to clear yet")
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 200), turnSeq: 1))
        changes.send(Self.info(deviation: 0, turn: Self.turn(metres: 150), turnSeq: 1))
        changes.send(Self.info(deviation: 0, turnSeq: 1))
        XCTAssertEqual(speaker.spoken, ["In 200 m rechts"])
        XCTAssertEqual(center.removed, ["turn"])
        changes.send(Self.info(phase: "off_route", deviation: 1, turnSeq: 1))
        changes.send(Self.info(phase: "off_route", deviation: 1, turnSeq: 1, offNetwork: Self.forest, offNetworkSeq: 1))
        changes.send(Self.info(phase: "off_route", deviation: 1, turnSeq: 1, offNetwork: Self.forest, offNetworkSeq: 1))
        changes.send(Self.info(deviation: 1, turn: Self.turn(metres: 100), turnSeq: 2))
        changes.send(Self.info(phase: "arrived", deviation: 1, turnSeq: 2))
        changes.send(Self.info(phase: "arrived", deviation: 1, turnSeq: 2))
        XCTAssertEqual(
            speaker.spoken,
            ["In 200 m rechts", Self.forest.spoken, "In 100 m rechts", "Ziel erreicht"])
        XCTAssertEqual(center.removed, ["turn", "turn"])
        changes.send(nil)
        XCTAssertEqual(center.removed, ["turn", "turn", "turn"], "the end clears too")
        XCTAssertEqual(speaker.spoken.count, 4)
    }

    /// A route from before the turns: the bar says so once, for a while.
    func testRouteWithoutTurnsIsToldOnce() async {
        let changes = PassthroughSubject<GuidanceInfo?, Never>()
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let guide = GuideController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher())
        guide.noTurnsSeconds = 0.05
        changes.send(Self.info(deviation: 0, hasTurns: true))
        XCTAssertFalse(guide.noTurnsNotice)
        changes.send(nil)
        changes.send(Self.info(deviation: 0, hasTurns: false))
        XCTAssertTrue(guide.noTurnsNotice)
        await wait("gone after a while") { !guide.noTurnsNotice }
        changes.send(Self.info(deviation: 0, hasTurns: false))
        XCTAssertFalse(guide.noTurnsNotice, "told once per guidance")
    }

    /// The way from the position is routed as the core's way back, never straight.
    func testRerouteLegIsRouted() {
        let hike = LegInput(profile: "hike", options: [])
        let mtb = LegInput(profile: "mtb", options: [])
        let manual = LegInput(profile: "manual", options: [])
        XCTAssertEqual(PlanController.rerouteLeg([hike, manual, mtb], replacing: 2).profile, "mtb")
        XCTAssertEqual(PlanController.rerouteLeg([hike, manual, mtb], replacing: 1).profile, "hike")
        XCTAssertEqual(PlanController.rerouteLeg([manual, manual, mtb], replacing: 0).profile, "mtb")
        XCTAssertEqual(PlanController.rerouteLeg([manual], replacing: 0).profile, "manual")
    }
}

final class CountingHaptics: Haptics {
    var deviations = 0
    var turns = 0
    func deviation() { deviations += 1 }
    func turn() { turns += 1 }
}

final class RecordingSpeaker: Speaking {
    var spoken: [String] = []
    func speak(_ text: String) { spoken.append(text) }
}
