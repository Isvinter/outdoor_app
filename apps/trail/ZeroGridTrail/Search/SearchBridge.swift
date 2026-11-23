import CoreLocation
import Foundation

extension SearchBackend {
    /// The search of the core; called off the main thread.
    init(core: CoreApp) {
        self.init(
            text: { text, near, radius, limit in
                core.search(text: text, near: near, radiusM: radius, limit: limit)
            },
            category: { category, near, radius, limit in
                try core.searchCategory(category: category, near: near, radiusM: radius, limit: limit)
            })
    }
}

/// Asks for the position while "Route hierher" waits for one, and only then,
/// so that it costs no battery the rest of the time.
@MainActor
final class PositionRequest: NSObject, @preconcurrency CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private var onUpdate: ((CLLocation) -> Void)?
    private var denied: (() -> Void)?

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
    }

    func start(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void) {
        self.onUpdate = onUpdate
        self.denied = denied
        apply(manager.authorizationStatus)
    }

    func stop() {
        onUpdate = nil
        denied = nil
        manager.stopUpdatingLocation()
    }

    private func apply(_ status: CLAuthorizationStatus) {
        guard onUpdate != nil else { return }
        switch status {
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .authorizedAlways, .authorizedWhenInUse:
            manager.startUpdatingLocation()
        default:
            let denied = denied
            stop()
            denied?()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        apply(manager.authorizationStatus)
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        if let location = locations.last { onUpdate?(location) }
    }

    /// No position yet is no reason to stop: the flow has its own time limit.
    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {}
}

/// "Route hierher" on the real planning, core and position.
@MainActor
final class LiveRouteHereHost: RouteHereHost {
    private let core: CoreApp
    private let plan: PlanController
    private let map: MapController
    private let position = PositionRequest()

    init(core: CoreApp, plan: PlanController, map: MapController) {
        self.core = core
        self.plan = plan
        self.map = map
    }

    var hasRoute: Bool { !plan.request.waypoints.isEmpty }
    var isReplacingRoute: Bool { plan.isReplacingRoute }
    var lastPosition: CLLocation? { map.mapView.userLocation?.location }

    func routeIsUnsaved() async -> Bool {
        let (core, request) = (core, plan.request)
        return await Task.detached(priority: .userInitiated) { core.routeIsUnsaved(request: request) }.value
    }

    func routeStart(position: KnownPosition?, target: CLLocationCoordinate2D) async -> RouteStart {
        let core = core
        let to = LatLon(lat: target.latitude, lon: target.longitude)
        return await Task.detached(priority: .userInitiated) { core.routeStart(position: position, target: to) }.value
    }

    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> Bool {
        plan.startRoute(from: from, to: to)
    }

    func appendTarget(_ to: CLLocationCoordinate2D) -> Bool {
        plan.appendTarget(to)
    }

    func requestPosition(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void) {
        position.start(onUpdate, denied: denied)
    }

    func stopRequestingPosition() {
        position.stop()
    }

    func stopPicking() {
        plan.picking = false
    }
}
