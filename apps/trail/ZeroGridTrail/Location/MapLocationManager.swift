import Combine
import CoreLocation
import MapLibre

/// The map's source of the own position: the positions of `LocationService`,
/// so that the dot costs no GPS of its own. Whether the GPS runs is up to the
/// service, not to the map.
@MainActor
final class MapLocationManager: NSObject, @preconcurrency MLNLocationManager {
    weak var delegate: MLNLocationManagerDelegate?
    private let service: LocationService
    private var updating = false
    private var watch: AnyCancellable?
    private var permission: AnyCancellable?

    init(service: LocationService) {
        self.service = service
        super.init()
        watch = service.$last.compactMap { $0 }.sink { [weak self] location in
            guard let self, self.updating else { return }
            self.delegate?.locationManager(self, didUpdate: [location])
        }
        permission = service.$authorization.dropFirst().sink { [weak self] _ in
            guard let self else { return }
            self.delegate?.locationManagerDidChangeAuthorization(self)
        }
    }

    var authorizationStatus: CLAuthorizationStatus { service.authorization }
    var headingOrientation: CLDeviceOrientation = .portrait

    /// The map does not ask; the service does when it needs the GPS.
    func requestAlwaysAuthorization() {}
    func requestWhenInUseAuthorization() {}

    func startUpdatingLocation() {
        updating = true
        if let last = service.last {
            delegate?.locationManager(self, didUpdate: [last])
        }
    }

    func stopUpdatingLocation() {
        updating = false
    }

    func startUpdatingHeading() {}
    func stopUpdatingHeading() {}
    func dismissHeadingCalibrationDisplay() {}
}
