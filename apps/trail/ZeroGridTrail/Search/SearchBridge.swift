import CoreLocation
import Foundation

extension SearchBackend {
    /// The search of the core; called off the main thread.
    init(core: CoreApp) {
        self.init(
            text: { text, near, radius, limit, from in
                core.search(text: text, near: near, radiusM: radius, limit: limit, from: from)
            },
            category: { category, near, radius, limit, from in
                try core.searchCategory(category: category, near: near, radiusM: radius, limit: limit, from: from)
            })
    }
}

extension RouteFrom {
    /// The own position, if it is known, with the settings a new leg would get.
    @MainActor
    static func live(location: LocationService, plan: PlanController) -> RouteFrom? {
        guard location.knowsPosition, let last = location.last else { return nil }
        return RouteFrom(
            position: KnownPosition(
                lat: last.coordinate.latitude, lon: last.coordinate.longitude,
                ageS: max(0, -last.timestamp.timeIntervalSinceNow)),
            leg: plan.newLeg)
    }
}

/// "Route hierher" on the real planning, core and GPS.
@MainActor
final class LiveRouteHereHost: RouteHereHost {
    private let core: CoreApp
    private let plan: PlanController
    private let location: LocationService

    init(core: CoreApp, plan: PlanController, location: LocationService) {
        self.core = core
        self.plan = plan
        self.location = location
    }

    var hasRoute: Bool { !plan.request.waypoints.isEmpty }
    var isReplacingRoute: Bool { plan.isReplacingRoute }
    var position: CLLocation? { location.knowsPosition ? location.last : nil }

    func routeStart(position: KnownPosition, target: CLLocationCoordinate2D) async -> RouteStart {
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

    func beginRoute(at: CLLocationCoordinate2D) -> Bool {
        plan.beginRoute(at: at)
    }
}
