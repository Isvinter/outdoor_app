import Combine
import CoreLocation
import XCTest
@testable import ZeroGridTrail

@MainActor
final class RecordingControllerTests: XCTestCase {
    private var sandbox: Sandbox!
    private var manager: FakeLocationManager!

    override func setUp() async throws {
        sandbox = try Sandbox()
        manager = FakeLocationManager()
    }

    override func tearDown() async throws {
        sandbox.remove()
    }

    private func controller(host: CoreHost? = nil) -> (RecordingController, LocationService) {
        let host = host ?? sandbox.host
        let location = LocationService(core: host.core, manager: manager)
        let recording = RecordingController(
            core: host.core, location: location, changes: host.recordingChanges)
        return (recording, location)
    }

    func testStartRequiresLocationAndStopReleasesIt() throws {
        let (recording, location) = controller()
        XCTAssertFalse(manager.updating)

        recording.start()
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
        recording.start()
        recording.stop()
        XCTAssertTrue(manager.updating)
    }

    func testPauseAndResume() {
        let (recording, _) = controller()
        recording.start()
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
            core: sandbox.host.core, location: location, changes: changes.eraseToAnyPublisher(), overlay: overlay)
        recording.start()
        let id = recording.info.trackId
        recording.stop()
        changes.send(RecordingInfo(
            phase: "recording", trackId: id, activity: "bike", distanceM: 10, movingS: 1, elapsedS: 1, ascentM: nil,
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

    private func recordWalk(_ recording: RecordingController, _ location: LocationService) throws -> TrackSummary {
        recording.start()
        manager.deliver(FakeLocationManager.walk(count: 60, step: 0.00001))
        location.drain()
        recording.stop()
        return try XCTUnwrap(recording.ended)
    }

    /// Ended, the track is stored and named; "Speichern" sets the activity.
    func testSaveWithActivity() throws {
        let (recording, location) = controller()
        var changed = 0
        recording.onTracksChanged = { changed += 1 }
        let track = try recordWalk(recording, location)
        XCTAssertEqual(track.activity, "other")
        let stored = try XCTUnwrap(sandbox.host.core.listTracks().first)
        XCTAssertEqual(stored.name, RecordingController.name(startedMs: track.startedMs, activity: "other"))

        recording.save(activity: "hike")
        XCTAssertNil(recording.ended)
        let saved = try XCTUnwrap(sandbox.host.core.listTracks().first)
        XCTAssertEqual(saved.activity, "hike")
        XCTAssertEqual(saved.name, RecordingController.name(startedMs: track.startedMs, activity: "hike"))
        XCTAssertEqual(changed, 2)
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
        recording.start()
        recording.stop()
        XCTAssertNil(recording.ended)
        XCTAssertEqual(recording.notice, "Es wurde kein Punkt aufgezeichnet.")
    }

    /// More than two minutes without a fix is told once a fix comes again.
    func testGapIsTold() {
        let (recording, location) = controller()
        recording.start()
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
            core: sandbox.host.core, location: location, changes: sandbox.host.recordingChanges, overlay: overlay)
        recording.start()
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
