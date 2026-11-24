import Combine
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

        recording.start(activity: "hike")
        XCTAssertEqual(recording.info.phase, "recording")
        XCTAssertTrue(manager.updating)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)
        XCTAssertEqual(manager.activityType, .fitness)

        // At a walking pace: the core drops jumps too fast for the activity.
        manager.deliver(FakeLocationManager.walk(count: 200, step: 0.00001))
        location.drain()
        let track = try XCTUnwrap(recording.stop())
        XCTAssertGreaterThan(track.distanceM, 100)
        XCTAssertEqual(recording.info.phase, "idle")
        XCTAssertFalse(manager.updating)
        XCTAssertFalse(manager.allowsBackgroundLocationUpdates)
    }

    /// The map's GPS stays on when the recording ends.
    func testStopKeepsTheMapsLocation() {
        let (recording, location) = controller()
        location.require(.map)
        recording.start(activity: "bike")
        _ = recording.stop()
        XCTAssertTrue(manager.updating)
    }

    func testPauseAndResume() {
        let (recording, _) = controller()
        recording.start(activity: "bike")
        recording.pause()
        XCTAssertTrue(recording.paused)
        recording.resume()
        XCTAssertEqual(recording.info.phase, "recording")
        _ = recording.stop()
    }

    /// News of an ended recording that arrives late changes nothing.
    func testLateNewsOfAnEndedRecordingIsDropped() {
        let changes = PassthroughSubject<RecordingInfo, Never>()
        let overlay = TrackOverlay(mapView: .init(frame: .zero))
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let recording = RecordingController(
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher(), overlay: overlay,
            defaults: defaults)
        recording.start(activity: "bike")
        let id = recording.info.trackId
        _ = recording.stop()
        changes.send(RecordingInfo(
            phase: "recording", trackId: id, activity: "bike", distanceM: 10, movingS: 1, elapsedS: 1, ascentM: nil,
            appended: [LineVertex(segment: 0, lat: 47.5, lon: 11.5), LineVertex(segment: 0, lat: 47.6, lon: 11.5)]))
        XCTAssertEqual(recording.info.phase, "idle")
        XCTAssertTrue(overlay.recording.isEmpty)
    }

    func testLastActivityIsPreselected() {
        XCTAssertEqual(controller().0.lastActivity, "bike")
        let (recording, _) = controller()
        recording.start(activity: "ski_tour")
        _ = recording.stop()
        XCTAssertEqual(controller().0.lastActivity, "ski_tour")
    }

    /// The listener's points reach the line, each once.
    func testAppendedPointsReachTheLine() async throws {
        let overlay = TrackOverlay(mapView: .init(frame: .zero))
        let location = LocationService(core: sandbox.host.core, manager: manager)
        let recording = RecordingController(
            core: sandbox.host.core, location: location, changes: sandbox.host.recordingChanges, overlay: overlay,
            defaults: defaults)
        recording.start(activity: "bike")
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
        _ = recording.stop()
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

    func testInterruptedOffersContinue() throws {
        let host = try interruptedHost()
        let (recording, _) = controller(host: host)
        XCTAssertNotNil(recording.interrupted)
        XCTAssertFalse(recording.active)

        recording.continueInterrupted()
        XCTAssertNil(recording.interrupted)
        XCTAssertEqual(recording.info.phase, "recording")
        XCTAssertTrue(manager.updating)
        _ = recording.stop()
    }

    func testInterruptedOffersSave() throws {
        let host = try interruptedHost()
        let (recording, _) = controller(host: host)
        recording.finishInterrupted()
        XCTAssertNil(recording.interrupted)
        XCTAssertFalse(recording.active)
        XCTAssertFalse(manager.updating)
        XCTAssertEqual(try host.core.listTracks().count, 1)
    }
}
