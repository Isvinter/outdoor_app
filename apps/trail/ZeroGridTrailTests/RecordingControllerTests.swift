import Combine
import CoreLocation
import XCTest
@testable import ZeroGridTrail

@MainActor
final class RecordingControllerTests: XCTestCase {
    private var sandbox: Sandbox!
    private var manager: FakeLocationManager!
    private var defaults: UserDefaults!
    private let suite = "RecordingControllerTests"

    override func setUp() async throws {
        sandbox = try Sandbox()
        manager = FakeLocationManager()
        UserDefaults().removePersistentDomain(forName: suite)
        defaults = UserDefaults(suiteName: suite)
    }

    override func tearDown() async throws {
        UserDefaults().removePersistentDomain(forName: suite)
        sandbox.remove()
    }

    private func controller(host: CoreHost? = nil) -> (RecordingController, LocationService) {
        let host = host ?? sandbox.host
        let location = LocationService(core: host.core, manager: manager)
        let recording = RecordingController(
            core: host.core, location: location, changes: host.recordingChanges, defaults: defaults)
        return (recording, location)
    }

    func testStartRequiresLocationAndStopReleasesIt() throws {
        let (recording, location) = controller()
        XCTAssertFalse(manager.updating)

        recording.start(activity: "other")
        XCTAssertEqual(recording.info.phase, "recording")
        XCTAssertTrue(manager.updating)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)

        // At a walking pace: the core drops jumps too fast for the activity.
        manager.deliver(FakeLocationManager.walk(count: 200, step: 0.00001))
        location.drain()
        recording.stop()
        let track = try XCTUnwrap(recording.ended)
        XCTAssertGreaterThan(track.distanceM, 100)
        XCTAssertEqual(recording.info.phase, "idle")
        XCTAssertFalse(manager.updating)
        XCTAssertFalse(manager.allowsBackgroundLocationUpdates)
    }

    /// The map's GPS stays on when the recording ends.
    func testStopKeepsTheMapsLocation() {
        let (recording, location) = controller()
        location.require(.map)
        recording.start(activity: "other")
        recording.stop()
        XCTAssertTrue(manager.updating)
    }

    func testPauseAndResume() {
        let (recording, _) = controller()
        recording.start(activity: "other")
        recording.pause()
        XCTAssertTrue(recording.paused)
        recording.resume()
        XCTAssertEqual(recording.info.phase, "recording")
        recording.stop()
    }

    /// News of an ended recording that arrives late changes nothing.
    func testLateNewsOfAnEndedRecordingIsDropped() {
        let changes = PassthroughSubject<RecordingInfo, Never>()
        let overlay = TrackOverlay(mapView: .init(frame: .zero))
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let recording = RecordingController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher(), overlay: overlay,
            defaults: defaults)
        recording.start(activity: "other")
        let id = recording.info.trackId
        recording.stop()
        changes.send(RecordingInfo(
            phase: "recording", trackId: id, activity: "bike", resting: false, distanceM: 10, movingS: 1, elapsedS: 1, ascentM: nil,
            appended: [LineVertex(segment: 0, lat: 47.5, lon: 11.5), LineVertex(segment: 0, lat: 47.6, lon: 11.5)]))
        XCTAssertEqual(recording.info.phase, "idle")
        XCTAssertTrue(overlay.recording.isEmpty)
    }

    func testNames() {
        // 2026-11-24 10:05 in Berlin.
        let started: Int64 = 1_795_511_100_000
        let time = DateFormatter()
        time.dateFormat = "yyyy-MM-dd HH:mm"
        let start = time.string(from: Date(timeIntervalSince1970: 1_795_511_100))
        XCTAssertEqual(RecordingController.name(startedMs: started, activity: "other"), start)
        XCTAssertEqual(RecordingController.name(startedMs: started, activity: "bike"), "\(start) Rad")
        XCTAssertEqual(RecordingController.name(startedMs: started, activity: "ski_tour"), "\(start) Skitour")
    }

    private func recordWalk(
        _ recording: RecordingController, _ location: LocationService, activity: String = "other"
    ) throws -> TrackSummary {
        recording.start(activity: activity)
        manager.deliver(FakeLocationManager.walk(count: 60, step: 0.00001))
        location.drain()
        recording.stop()
        return try XCTUnwrap(recording.ended)
    }

    /// Ended, the track is stored and named after the activity it started with.
    func testSaveKeepsTheStartActivity() throws {
        let (recording, location) = controller()
        var changed = 0
        recording.onTracksChanged = { changed += 1 }
        let track = try recordWalk(recording, location, activity: "hike")
        XCTAssertEqual(track.activity, "hike")
        let stored = try XCTUnwrap(sandbox.host.core.listTracks().first)
        XCTAssertEqual(stored.name, RecordingController.name(startedMs: track.startedMs, activity: "hike"))

        recording.save()
        XCTAssertNil(recording.ended)
        XCTAssertEqual(try sandbox.host.core.listTracks().first?.activity, "hike")
        XCTAssertEqual(changed, 2)
    }

    /// The record button asks first; the activity started with is chosen next time.
    func testStartAsksAndRemembersTheActivity() {
        let (recording, location) = controller()
        XCTAssertEqual(recording.lastActivity, "other")
        recording.requestStart()
        XCTAssertTrue(recording.choosingActivity)
        XCTAssertFalse(recording.active)
        recording.cancelStart()
        XCTAssertFalse(recording.choosingActivity)

        recording.requestStart()
        recording.start(activity: "mtb")
        XCTAssertFalse(recording.choosingActivity)
        XCTAssertEqual(recording.activity, "mtb")
        XCTAssertEqual(location.activity, "mtb")
        recording.stop()
        let (again, _) = controller()
        XCTAssertEqual(again.lastActivity, "mtb")
    }

    /// A switch makes a section of its own; the GPS follows the activity.
    func testSwitchActivity() throws {
        let (recording, location) = controller()
        recording.start(activity: "bike")
        manager.deliver(FakeLocationManager.walk(count: 30, step: 0.00001))
        location.drain()
        recording.switchActivity("hike")
        XCTAssertEqual(recording.activity, "hike")
        XCTAssertEqual(location.activity, "hike")
        // Later than the first walk, which ends now.
        let on = FakeLocationManager.walk(count: 30, step: 0.00001, from: .init(latitude: 47.5751, longitude: 11.55))
            .map { fix in
                CLLocation(
                    coordinate: fix.coordinate, altitude: fix.altitude, horizontalAccuracy: 5, verticalAccuracy: 5,
                    course: 0, speed: fix.speed, timestamp: fix.timestamp.addingTimeInterval(31))
            }
        manager.deliver(on)
        location.drain()
        XCTAssertEqual(recording.sectionCount, 2)
        recording.stop()
        let track = try XCTUnwrap(recording.ended)
        let detail = try sandbox.host.core.trackDetail(id: track.id)
        XCTAssertEqual(detail.sections.map(\.activity), ["bike", "hike"])
    }

    /// `count` fixes `seconds` apart at one spot, ending `before` seconds ago.
    private static func standing(count: Int, seconds: TimeInterval = 10, before: TimeInterval = 0) -> [CLLocation] {
        let spot = CLLocationCoordinate2D(latitude: 47.575, longitude: 11.55)
        let end = Date().addingTimeInterval(-before)
        return (0..<count).map { index in
            CLLocation(
                coordinate: spot, altitude: 800, horizontalAccuracy: 5, verticalAccuracy: 5, course: -1, speed: 0,
                timestamp: end.addingTimeInterval(Double(index - count + 1) * seconds))
        }
    }

    /// During a rest the GPS is throttled after the chosen level, and no longer after it.
    func testThrottleFollowsRestingAndLevel() async throws {
        let (recording, location) = controller()
        XCTAssertEqual(recording.throttle, .strong)
        recording.start(activity: "hike")
        // Six minutes at one spot, ending a minute ago.
        manager.deliver(Self.standing(count: 37, before: 60))
        location.drain()
        // The core tells the rest at once, through the listener on the main queue.
        try await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertTrue(recording.info.resting)
        XCTAssertEqual(location.restProfile, .restStrong)
        XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyHundredMeters)

        recording.throttle = .moderate
        XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyNearestTenMeters)
        recording.throttle = .off
        XCTAssertNil(location.restProfile)
        XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyBest)
        XCTAssertEqual(defaults.string(forKey: RecordingController.throttleKey), "off")
        recording.throttle = .strong

        // On the way again: two fixes well away from the spot end the rest.
        manager.deliver(FakeLocationManager.walk(count: 20, step: 0.0001, from: .init(latitude: 47.576, longitude: 11.55)))
        location.drain()
        try await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertFalse(recording.info.resting)
        XCTAssertNil(location.restProfile)
        XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyBest)
        recording.stop()
    }

    /// The settings of the rest reach the core: with 15 min, six minutes are no rest.
    func testRestSettingsReachTheCore() async throws {
        let (recording, location) = controller()
        recording.restDwellMin = 15
        XCTAssertEqual(defaults.integer(forKey: RecordingController.restDwellKey), 15)
        recording.start(activity: "hike")
        manager.deliver(Self.standing(count: 37, before: 60))
        location.drain()
        try await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertFalse(recording.info.resting)
        recording.stop()
    }

    func testDiscardDeletesTheTrack() throws {
        let (recording, location) = controller()
        _ = try recordWalk(recording, location)
        recording.discard()
        XCTAssertNil(recording.ended)
        XCTAssertEqual(try sandbox.host.core.listTracks().count, 0)
    }

    /// Without a point there is nothing to ask about.
    func testEmptyRecordingIsGone() {
        let (recording, _) = controller()
        recording.start(activity: "other")
        recording.stop()
        XCTAssertNil(recording.ended)
        XCTAssertEqual(recording.notice, "Es wurde kein Punkt aufgezeichnet.")
    }

    /// More than two minutes without a fix is told once a fix comes again.
    func testGapIsTold() {
        let (recording, location) = controller()
        recording.start(activity: "other")
        let start = CLLocationCoordinate2D(latitude: 47.6, longitude: 11.5)
        manager.deliver([CLLocation(coordinate: start, altitude: 700, horizontalAccuracy: 5, verticalAccuracy: 5,
                                    timestamp: Date())])
        XCTAssertNil(recording.notice)
        manager.deliver([CLLocation(coordinate: start, altitude: 700, horizontalAccuracy: 5, verticalAccuracy: 5,
                                    timestamp: Date().addingTimeInterval(300))])
        location.drain()
        XCTAssertEqual(recording.notice, "Kein GPS für 5 min. Die Aufzeichnung läuft weiter.")
        recording.stop()
    }

    /// The listener's points reach the line, each once.
    func testAppendedPointsReachTheLine() async throws {
        let overlay = TrackOverlay(mapView: .init(frame: .zero))
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let recording = RecordingController(
            core: sandbox.host.core, location: location, changes: sandbox.host.recordingChanges, overlay: overlay,
            defaults: defaults)
        recording.start(activity: "other")
        manager.deliver(FakeLocationManager.walk(count: 10))
        location.drain()
        // The core tells about new points at most four times a second; a pause tells at once.
        recording.pause()
        // The listener hands over on the main queue.
        try await Task.sleep(nanoseconds: 200_000_000)
        let core = sandbox.host.core.recordingLine().flatMap { $0 }.count
        XCTAssertGreaterThan(core, 1)
        XCTAssertEqual(overlay.recording.flatMap { $0 }.count, core)
        recording.resume()
        manager.deliver(FakeLocationManager.walk(count: 5, from: .init(latitude: 47.58, longitude: 11.55)))
        location.drain()
        // The last points come with the end; the line goes away all the same.
        recording.stop()
        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertTrue(overlay.recording.isEmpty)
    }

    /// A recording left running by an app that was ended.
    private func interruptedHost() throws -> CoreHost {
        let core = sandbox.host.core
        _ = try core.startRecording(activity: "hike")
        for location in FakeLocationManager.walk(count: 60, step: 0.00001) {
            core.handleFix(fix: LocationService.fix(location))
        }
        return try CoreHost(
            dataDirectory: sandbox.root.appendingPathComponent("data", isDirectory: true),
            documentsDirectory: sandbox.root.appendingPathComponent("Documents", isDirectory: true))
    }

    /// A recording the app was ended during is saved at the next start and told.
    func testInterruptedIsSaved() throws {
        let host = try interruptedHost()
        let (recording, _) = controller(host: host)
        XCTAssertFalse(recording.active)
        XCTAssertFalse(manager.updating)
        XCTAssertEqual(recording.notice, RecordingController.interruptedNotice)
        let tracks = try host.core.listTracks()
        XCTAssertEqual(tracks.count, 1)
        XCTAssertEqual(tracks[0].name, RecordingController.name(startedMs: tracks[0].startedMs, activity: "hike"))
        XCTAssertNil(try host.core.interruptedRecording())
    }
}
