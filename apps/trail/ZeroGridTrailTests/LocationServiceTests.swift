import CoreLocation
import XCTest
@testable import ZeroGridTrail

/// Stands in for `CLLocationManager`: records what the service sets.
@MainActor
final class FakeLocationManager: LocationManaging {
    weak var sink: LocationSink?
    var authorizationStatus = CLAuthorizationStatus.authorizedWhenInUse
    var desiredAccuracy: CLLocationAccuracy = 0
    var distanceFilter: CLLocationDistance = 0
    var activityType = CLActivityType.automotiveNavigation
    var pausesLocationUpdatesAutomatically = true
    var allowsBackgroundLocationUpdates = false
    var showsBackgroundLocationIndicator = false
    private(set) var updating = false
    private(set) var authorizationRequests = 0

    func requestWhenInUseAuthorization() { authorizationRequests += 1 }
    func startUpdatingLocation() { updating = true }
    func stopUpdatingLocation() { updating = false }

    /// Delivers positions as the GPS would.
    func deliver(_ locations: [CLLocation]) {
        sink?.received(locations)
    }

    func setAuthorization(_ status: CLAuthorizationStatus) {
        authorizationStatus = status
        sink?.authorizationChanged()
    }

    /// `count` positions one second apart, `step` degrees (0.0001: about 11 m)
    /// to the north each, ending now.
    static func walk(
        count: Int, step: Double = 0.0001, from start: CLLocationCoordinate2D = .init(latitude: 47.575, longitude: 11.55)
    ) -> [CLLocation] {
        let now = Date()
        return (0..<count).map { index in
            CLLocation(
                coordinate: CLLocationCoordinate2D(
                    latitude: start.latitude + Double(index) * step, longitude: start.longitude),
                altitude: 800 + Double(index), horizontalAccuracy: 5, verticalAccuracy: 5, course: 0, speed: step * 111_000,
                timestamp: now.addingTimeInterval(Double(index - count)))
        }
    }
}

@MainActor
final class LocationServiceTests: XCTestCase {
    private var sandbox: Sandbox!
    private var manager: FakeLocationManager!
    private var service: LocationService!

    override func setUp() async throws {
        sandbox = try Sandbox()
        manager = FakeLocationManager()
        service = LocationService(core: sandbox.host.core, manager: manager)
    }

    override func tearDown() async throws {
        service = nil
        sandbox.remove()
    }

    func testDemandsUnionAndRelease() {
        service.require(.map)
        service.require(.recording)
        XCTAssertTrue(manager.updating)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)

        service.release(.recording)
        XCTAssertTrue(manager.updating, "the map still needs the GPS")
        XCTAssertTrue(service.running)
        XCTAssertFalse(manager.allowsBackgroundLocationUpdates)
        XCTAssertFalse(manager.showsBackgroundLocationIndicator)

        service.release(.map)
        XCTAssertFalse(manager.updating)
        XCTAssertFalse(service.running)
    }

    func testBackgroundOnlyForRecordingOrGuidance() {
        service.require(.map)
        XCTAssertFalse(manager.allowsBackgroundLocationUpdates)
        service.require(.guidance)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)
        XCTAssertTrue(manager.showsBackgroundLocationIndicator)
        service.release(.guidance)
        service.require(.recording)
        XCTAssertTrue(manager.allowsBackgroundLocationUpdates)
        service.release(.recording)
        XCTAssertFalse(manager.allowsBackgroundLocationUpdates)
    }

    func testStandardProfileValues() {
        XCTAssertEqual(LocationProfile.standard.accuracy, kCLLocationAccuracyBest)
        XCTAssertEqual(LocationProfile.standard.distanceFilter, kCLDistanceFilterNone)
        XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyBest)
        XCTAssertEqual(manager.distanceFilter, kCLDistanceFilterNone)
        XCTAssertFalse(manager.pausesLocationUpdatesAutomatically)
        service.activity = "bike"
        XCTAssertEqual(manager.activityType, .fitness)
        service.activity = ""
        XCTAssertEqual(manager.activityType, .other)
    }

    /// Without the permission the service asks once it is needed, and starts once given.
    func testAsksForPermissionWhenNeeded() {
        manager.authorizationStatus = .notDetermined
        service = LocationService(core: sandbox.host.core, manager: manager)
        XCTAssertEqual(manager.authorizationRequests, 0)
        service.require(.recording)
        XCTAssertEqual(manager.authorizationRequests, 1)
        XCTAssertFalse(manager.updating)
        manager.setAuthorization(.authorizedWhenInUse)
        XCTAssertTrue(manager.updating)
    }

    func testFixesReachTheCore() throws {
        let core = sandbox.host.core
        _ = try core.startRecording(activity: "bike")
        service.require(.recording)
        manager.deliver(FakeLocationManager.walk(count: 20))
        service.drain()

        XCTAssertGreaterThan(core.recordingStatus().distanceM, 100)
        XCTAssertEqual(service.last?.coordinate.latitude ?? 0, 47.575 + 19 * 0.0001, accuracy: 1e-9)
        _ = try core.stopRecording()
    }

    /// Nothing reaches the core while no one needs the GPS.
    func testNoFixesWhileStopped() throws {
        let core = sandbox.host.core
        _ = try core.startRecording(activity: "bike")
        manager.deliver(FakeLocationManager.walk(count: 20))
        service.drain()
        XCTAssertEqual(core.recordingStatus().distanceM, 0)
        XCTAssertNil(service.last)
        _ = try core.stopRecording()
    }

    func testGPXReplayReadsTrackPoints() throws {
        let gpx = """
            <?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>
            <trkpt lat="47.5" lon="11.5"><ele>800</ele><time>2026-11-24T10:00:00Z</time></trkpt>
            <trkpt lat="47.6" lon="11.6"><time>2026-11-24T10:00:03.5Z</time></trkpt>
            </trkseg></trk></gpx>
            """
        let points = try GPXReplay.parse(Data(gpx.utf8))
        XCTAssertEqual(points, [
            .init(lat: 47.5, lon: 11.5, ele: 800, offset: 0), .init(lat: 47.6, lon: 11.6, ele: nil, offset: 3.5),
        ])
    }
}
