import Combine
import CoreLocation
import Foundation

/// What "Route hierher" needs from the planning, the core and the GPS.
@MainActor
protocol RouteHereHost: AnyObject {
    var hasRoute: Bool { get }
    /// A saved route is being loaded or taken over; nothing may change it now.
    var isReplacingRoute: Bool { get }
    /// The own position, if the GPS runs and knows it; from the one GPS of the app.
    var position: CLLocation? { get }
    func routeStart(position: KnownPosition, target: CLLocationCoordinate2D) async -> RouteStart
    /// A new route; false if the planning cannot take it now.
    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> Bool
    func appendTarget(_ to: CLLocationCoordinate2D) -> Bool
    /// A new route that begins at `at`; the user sets the next points.
    func beginRoute(at: CLLocationCoordinate2D) -> Bool
}

/// A place tapped on the map, waiting for an answer on its card.
struct MapPoint: Equatable {
    let lat: Double
    let lon: Double
    /// Set as the first waypoint with "+ Punkt": the card offers the way from the own position.
    let placed: Bool

    var coordinate: CLLocationCoordinate2D { CLLocationCoordinate2D(latitude: lat, longitude: lon) }
}

/// From a chosen place, a search hit or a tap on the map, to a route. With the
/// position known: "Route hierher" (from there) and "Hierher führen" (and the
/// guidance). Without: "Route planen", which begins the route at the place.
/// With a route on the map the place may also become its next destination.
/// Afterwards further taps add points until "Fertig".
@MainActor
final class RouteHereFlow: ObservableObject {
    /// The tapped place whose card shows; search hits have their own card.
    @Published private(set) var point: MapPoint?
    /// Planning from the position is under way.
    @Published private(set) var busy = false
    /// Why the route could not be made, in German.
    @Published var message: String?
    /// A route to the place is on the map.
    var onDone: (() -> Void)?
    /// "Hierher führen": the guidance should start once the route is computed.
    var onGuide: (() -> Void)?
    /// The tapped place to mark on the map, or nil to take the mark away.
    var onPoint: ((CLLocationCoordinate2D?) -> Void)?

    static let noPosition = "Dein Standort ist nicht bekannt."
    static let otherMap = "Dein Standort liegt nicht auf derselben Karte wie das Ziel."

    private let host: RouteHereHost
    /// Counts the places, so that a late answer about an earlier one does nothing.
    private var run = 0

    init(host: RouteHereHost) {
        self.host = host
    }

    var hasRoute: Bool { host.hasRoute }

    /// A tap on the map away from search hits; `placed` if it set the first waypoint.
    func offer(_ at: CLLocationCoordinate2D, placed: Bool = false) {
        guard !host.isReplacingRoute else { return }
        run += 1
        busy = false
        message = nil
        point = MapPoint(lat: at.latitude, lon: at.longitude, placed: placed)
        // A placed point is a waypoint already and shows as one.
        onPoint?(placed ? nil : at)
    }

    func dismiss() {
        run += 1
        busy = false
        message = nil
        guard point != nil else { return }
        point = nil
        onPoint?(nil)
    }

    /// "Route hierher": a new route from the own position.
    func routeHere(to at: CLLocationCoordinate2D) {
        fromPosition(to: at, guide: false)
    }

    /// "Hierher führen": a new route from the own position, then the guidance.
    func guideHere(to at: CLLocationCoordinate2D) {
        fromPosition(to: at, guide: true)
    }

    /// "Als nächstes Ziel anhängen".
    func append(_ at: CLLocationCoordinate2D) {
        finish(guide: false) { host.appendTarget(at) }
    }

    /// "Route planen" without a position: the route begins here.
    func planFrom(_ at: CLLocationCoordinate2D) {
        finish(guide: false) { host.beginRoute(at: at) }
    }

    private func fromPosition(to at: CLLocationCoordinate2D, guide: Bool) {
        guard !busy else { return }
        guard let position = host.position else {
            message = Self.noPosition
            return
        }
        busy = true
        message = nil
        let number = run
        let known = KnownPosition(
            lat: position.coordinate.latitude, lon: position.coordinate.longitude,
            ageS: max(0, -position.timestamp.timeIntervalSinceNow))
        Task { [weak self] in
            guard let self else { return }
            let start = await self.host.routeStart(position: known, target: at)
            guard number == self.run else { return }
            self.busy = false
            switch start {
            case .ready:
                self.finish(guide: guide) { self.host.startRoute(from: position.coordinate, to: at) }
            case .otherMap:
                self.message = Self.otherMap
            case .noPosition:
                self.message = Self.noPosition
            }
        }
    }

    /// `make` puts the route on the map; then the card goes, and the guidance
    /// may follow. The map mode stays as the switch has it.
    private func finish(guide: Bool, _ make: () -> Bool) {
        guard !host.isReplacingRoute else { return }
        dismiss()
        guard make() else { return }
        onDone?()
        if guide { onGuide?() }
    }
}
