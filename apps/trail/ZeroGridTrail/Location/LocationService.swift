import CoreLocation
import Foundation

/// Who needs the GPS. It runs while anyone does; in the background only for
/// a recording or the guidance.
enum LocationDemand: Hashable {
    case map, recording, guidance
}

/// How the GPS is set. Only "Standard" for now; the profiles that save
/// battery come once the measurements are there.
struct LocationProfile: Equatable {
    let accuracy: CLLocationAccuracy
    let distanceFilter: CLLocationDistance

    /// The usual setting of such apps; how often a point is kept is up to
    /// the core (by activity).
    static let standard = LocationProfile(accuracy: kCLLocationAccuracyBest, distanceFilter: kCLDistanceFilterNone)
}

/// Told about positions and permission changes by a `LocationManaging`.
@MainActor
protocol LocationSink: AnyObject {
    func received(_ locations: [CLLocation])
    func authorizationChanged()
    func failed(_ error: Error)
}

/// What `LocationService` uses of `CLLocationManager`; replaced in the tests
/// and by `GPXReplay`.
@MainActor
protocol LocationManaging: AnyObject {
    var sink: LocationSink? { get set }
    var authorizationStatus: CLAuthorizationStatus { get }
    var desiredAccuracy: CLLocationAccuracy { get set }
    var distanceFilter: CLLocationDistance { get set }
    var activityType: CLActivityType { get set }
    var pausesLocationUpdatesAutomatically: Bool { get set }
    var allowsBackgroundLocationUpdates: Bool { get set }
    var showsBackgroundLocationIndicator: Bool { get set }
    func requestWhenInUseAuthorization()
    func startUpdatingLocation()
    func stopUpdatingLocation()
}

/// The real GPS.
@MainActor
final class SystemLocationManager: NSObject, LocationManaging, @preconcurrency CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    weak var sink: LocationSink?

    override init() {
        super.init()
        manager.delegate = self
    }

    var authorizationStatus: CLAuthorizationStatus { manager.authorizationStatus }
    var desiredAccuracy: CLLocationAccuracy {
        get { manager.desiredAccuracy }
        set { manager.desiredAccuracy = newValue }
    }
    var distanceFilter: CLLocationDistance {
        get { manager.distanceFilter }
        set { manager.distanceFilter = newValue }
    }
    var activityType: CLActivityType {
        get { manager.activityType }
        set { manager.activityType = newValue }
    }
    var pausesLocationUpdatesAutomatically: Bool {
        get { manager.pausesLocationUpdatesAutomatically }
        set { manager.pausesLocationUpdatesAutomatically = newValue }
    }
    var allowsBackgroundLocationUpdates: Bool {
        get { manager.allowsBackgroundLocationUpdates }
        set { manager.allowsBackgroundLocationUpdates = newValue }
    }
    var showsBackgroundLocationIndicator: Bool {
        get { manager.showsBackgroundLocationIndicator }
        set { manager.showsBackgroundLocationIndicator = newValue }
    }
    func requestWhenInUseAuthorization() { manager.requestWhenInUseAuthorization() }
    func startUpdatingLocation() { manager.startUpdatingLocation() }
    func stopUpdatingLocation() { manager.stopUpdatingLocation() }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        sink?.received(locations)
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        sink?.authorizationChanged()
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        sink?.failed(error)
    }
}

/// The one GPS of the app, shared by the map, the recording and the
/// guidance. Every position goes to the core (`handleFix`); the map shows
/// `last`. Asks for the permission when first needed.
@MainActor
final class LocationService: NSObject, ObservableObject, LocationSink {
    @Published private(set) var last: CLLocation?
    @Published private(set) var authorization: CLAuthorizationStatus
    /// The GPS runs.
    @Published private(set) var running = false
    /// The last position could not be found (no fix, or the permission is gone).
    @Published private(set) var failure: Error?
    /// A recording or the guidance needs the GPS, but the permission is refused.
    @Published private(set) var blocked = false

    /// "bike", "mtb", "hike", "ski_tour" or "other" while recording; empty otherwise.
    var activity: String = "" {
        didSet { manager.activityType = Self.activityType(for: activity) }
    }
    let profile: LocationProfile

    private let core: CoreApp
    private let manager: LocationManaging
    private var demands = Set<LocationDemand>()
    /// The core may compute a way back on a fix; that stays off the main thread,
    /// in the order the fixes came.
    private let fixes = DispatchQueue(label: "de.zeibig.zerogrid.trail.fixes")

    init(core: CoreApp, manager: LocationManaging? = nil, profile: LocationProfile = .standard) {
        let manager = manager ?? SystemLocationManager()
        self.core = core
        self.manager = manager
        self.profile = profile
        authorization = manager.authorizationStatus
        super.init()
        manager.sink = self
        manager.desiredAccuracy = profile.accuracy
        manager.distanceFilter = profile.distanceFilter
        manager.pausesLocationUpdatesAutomatically = false
        manager.activityType = .other
        manager.allowsBackgroundLocationUpdates = false
    }

    static let blockedHint =
        "Ohne Standortzugriff zeichnet Trail nichts auf und kann nicht führen. Bitte in den Einstellungen erlauben."

    static func activityType(for activity: String) -> CLActivityType {
        ["bike", "mtb", "hike", "ski_tour", "other"].contains(activity) ? .fitness : .other
    }

    var mayLocate: Bool {
        authorization == .authorizedWhenInUse || authorization == .authorizedAlways
    }

    func isRequired(_ demand: LocationDemand) -> Bool { demands.contains(demand) }

    func require(_ demand: LocationDemand) {
        demands.insert(demand)
        update()
    }

    func release(_ demand: LocationDemand) {
        demands.remove(demand)
        update()
    }

    /// Starts or stops the GPS and sets the background mode after the demands.
    private func update() {
        blocked = (demands.contains(.recording) || demands.contains(.guidance))
            && (authorization == .denied || authorization == .restricted)
        let background = demands.contains(.recording) || demands.contains(.guidance)
        // Before starting: a recording begun in the foreground goes on when the screen locks.
        if manager.allowsBackgroundLocationUpdates != background {
            manager.allowsBackgroundLocationUpdates = background
            manager.showsBackgroundLocationIndicator = background
        }
        if demands.isEmpty {
            if running {
                manager.stopUpdatingLocation()
                running = false
                last = nil
            }
        } else if mayLocate {
            if !running {
                failure = nil
                manager.startUpdatingLocation()
                running = true
            }
        } else if authorization == .notDetermined {
            manager.requestWhenInUseAuthorization()
        }
    }

    // MARK: LocationSink

    func received(_ locations: [CLLocation]) {
        guard running else { return }
        for location in locations {
            let fix = Self.fix(location)
            fixes.async { [core] in core.handleFix(fix: fix) }
        }
        if let newest = locations.last {
            failure = nil
            last = newest
        }
    }

    func authorizationChanged() {
        authorization = manager.authorizationStatus
        if !mayLocate && running {
            manager.stopUpdatingLocation()
            running = false
            last = nil
        }
        update()
    }

    func failed(_ error: Error) {
        failure = error
    }

    /// Waits until the core has taken every position received so far; for the tests.
    nonisolated func drain() {
        fixes.sync {}
    }

    nonisolated static func fix(_ location: CLLocation) -> FixInput {
        FixInput(
            lat: location.coordinate.latitude,
            lon: location.coordinate.longitude,
            ele: location.verticalAccuracy >= 0 ? location.altitude : nil,
            timeMs: Int64((location.timestamp.timeIntervalSince1970 * 1000).rounded()),
            hAccuracyM: location.horizontalAccuracy >= 0 ? Float(location.horizontalAccuracy) : nil,
            speedMps: location.speed >= 0 ? Float(location.speed) : nil,
            courseDeg: location.course >= 0 ? Float(location.course) : nil)
    }
}
