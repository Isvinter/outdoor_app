import CoreLocation
import MapLibre
import UIKit

/// The guidance on the map: the part of the route already behind the user
/// drawn paler, and the way back in a colour of its own. The planned route
/// itself stays as it is.
@MainActor
final class GuideOverlay {
    private static let doneSource = "guide-done"
    private static let wayBackSource = "guide-way-back"
    /// Not a colour of a profile and not the one of failed legs.
    static let wayBackColor = UIColor(red: 0, green: 0.78, blue: 0.33, alpha: 1)

    var dark = false
    private let mapView: MLNMapView
    /// The route as the core measures progress along it: the legs in order,
    /// a failed one as a straight line between its waypoints.
    private var route: [CLLocationCoordinate2D] = []
    private var cumulative: [Double] = []
    private var info: GuidanceInfo?
    private var foreground: NSObjectProtocol?

    init(mapView: MLNMapView) {
        self.mapView = mapView
        foreground = NotificationCenter.default.addObserver(
            forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.redraw() }
        }
    }

    /// After the route layers, so that the pale part lies on top of the route.
    func styleLoaded(_ style: MLNStyle) {
        guard style.source(withIdentifier: Self.doneSource) == nil else { return }
        let done = MLNShapeSource(identifier: Self.doneSource, shape: doneShape(), options: nil)
        let wayBack = MLNShapeSource(identifier: Self.wayBackSource, shape: wayBackShape(), options: nil)
        style.addSource(done)
        style.addSource(wayBack)
        let veil = MLNLineStyleLayer(identifier: Self.doneSource, source: done)
        veil.lineColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        veil.lineOpacity = NSExpression(forConstantValue: 0.6)
        veil.lineWidth = NSExpression(forConstantValue: 5)
        let casing = MLNLineStyleLayer(identifier: "\(Self.wayBackSource)-casing", source: wayBack)
        casing.lineColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        casing.lineWidth = NSExpression(forConstantValue: 8)
        let line = MLNLineStyleLayer(identifier: Self.wayBackSource, source: wayBack)
        line.lineColor = NSExpression(forConstantValue: Self.wayBackColor)
        line.lineWidth = NSExpression(forConstantValue: 5)
        for layer in [veil, casing, line] {
            layer.lineCap = NSExpression(forConstantValue: "round")
            layer.lineJoin = NSExpression(forConstantValue: "round")
            style.addLayer(layer)
        }
    }

    /// The planned route, after each computation.
    func setRoute(_ legs: [[CLLocationCoordinate2D]]) {
        // As the core's `RouteLine`: a point equal to the one before is left out
        // (the legs meet at their waypoints), distances on the same sphere.
        route = []
        cumulative = []
        for point in legs.joined() {
            guard let last = route.last else {
                route.append(point)
                cumulative.append(0)
                continue
            }
            if Self.same(point, last) { continue }
            cumulative.append(cumulative[cumulative.count - 1] + Self.haversine(last, point))
            route.append(point)
        }
        redraw()
    }

    func show(_ info: GuidanceInfo?) {
        guard info != self.info else { return }
        self.info = info
        // Unseen in the background; drawn once the app is back.
        if UIApplication.shared.applicationState != .background { redraw() }
    }

    func redraw() {
        (mapView.style?.source(withIdentifier: Self.doneSource) as? MLNShapeSource)?.shape = doneShape()
        (mapView.style?.source(withIdentifier: Self.wayBackSource) as? MLNShapeSource)?.shape = wayBackShape()
    }

    private static func same(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Bool {
        a.latitude == b.latitude && a.longitude == b.longitude
    }

    /// `EARTH_RADIUS_M` of core-track.
    static let earthRadius = 6_371_008.8

    /// Great-circle distance in metres, the formula of `core_track::geo::haversine_m`.
    nonisolated static func haversine(_ a: CLLocationCoordinate2D, _ b: CLLocationCoordinate2D) -> Double {
        let phi1 = a.latitude * .pi / 180
        let phi2 = b.latitude * .pi / 180
        let dPhi = phi2 - phi1
        let dLambda = (b.longitude - a.longitude) * .pi / 180
        let h = pow(sin(dPhi / 2), 2) + cos(phi1) * cos(phi2) * pow(sin(dLambda / 2), 2)
        return 2 * earthRadius * asin(min(sqrt(h), 1))
    }

    /// The route from its start to `along` metres.
    func line(upTo along: Double) -> [CLLocationCoordinate2D] {
        guard along > 0, route.count >= 2 else { return [] }
        var line = [route[0]]
        for index in route.indices.dropFirst() {
            if cumulative[index] <= along {
                line.append(route[index])
                continue
            }
            let span = cumulative[index] - cumulative[index - 1]
            let part = span > 0 ? (along - cumulative[index - 1]) / span : 0
            let a = route[index - 1]
            let b = route[index]
            line.append(CLLocationCoordinate2D(
                latitude: a.latitude + (b.latitude - a.latitude) * part,
                longitude: a.longitude + (b.longitude - a.longitude) * part))
            break
        }
        return line
    }

    private func doneShape() -> MLNShape {
        let line = line(upTo: info?.alongM ?? 0)
        guard line.count >= 2 else { return MLNShapeCollectionFeature(shapes: []) }
        return MLNShapeCollectionFeature(shapes: [MLNPolylineFeature(coordinates: line, count: UInt(line.count))])
    }

    private func wayBackShape() -> MLNShape {
        let line = (info?.wayBack ?? []).map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        guard line.count >= 2 else { return MLNShapeCollectionFeature(shapes: []) }
        return MLNShapeCollectionFeature(shapes: [MLNPolylineFeature(coordinates: line, count: UInt(line.count))])
    }

    /// Whether a way back is drawn; for the self-test.
    var wayBackDrawn: Bool { (info?.wayBack.count ?? 0) >= 2 }
}
