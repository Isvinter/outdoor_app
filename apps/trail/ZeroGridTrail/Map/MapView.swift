import CoreLocation
import MapLibre
import SwiftUI

/// The map and what the screen needs to know about it.
@MainActor
final class MapController: NSObject, ObservableObject, @preconcurrency MLNMapViewDelegate,
    @preconcurrency CLLocationManagerDelegate
{
    let mapView: MLNMapView
    /// The planned route on this map.
    let route: RouteOverlay
    private let core: CoreApp

    @Published private(set) var base: MapBase = .summer
    /// True once the current style is drawn completely.
    @Published private(set) var rendered = false
    /// How strongly the slope shading of the winter map shows, 0…1. Kept across
    /// style changes and set again after each, because a style starts with its own.
    @Published var slopeOpacity = 0.6 {
        didSet { applySlopeOpacity() }
    }
    /// The black OLED look of map and controls; remembered across starts.
    @Published private(set) var dark: Bool
    /// How strongly the dark map shades the terrain, 0…1; at 0 it is not drawn at all.
    /// Contour lines do not depend on it.
    @Published var reliefStrength: Double {
        didSet {
            UserDefaults.standard.set(reliefStrength, forKey: Self.reliefKey)
            applyRelief()
        }
    }
    /// Seconds from the start of the app until the first map was drawn completely.
    private(set) var firstRenderedAfter: TimeInterval?
    /// A failure worth telling the user, in German.
    @Published var message: String?
    /// Why the map could not be centred on the own position, if it could not.
    @Published var locationHint: String?
    /// How uncertain the own position is, in metres; nil while it is unknown.
    @Published private(set) var locationAccuracy: Double?
    /// Styles that could not be loaded; for the self-test.
    private(set) var loadFailures = 0
    private var styleLoaded = false
    /// West, south, east, north of the active regions; nil if there is none.
    private(set) var bounds: [Double]?
    /// Only asks for the permission and tells when it changes; the map locates by itself.
    private let locationManager = CLLocationManager()
    /// The user asked for their position and it is not known yet.
    private var centreWhenLocated = false
    /// Counts the waits for a position, so that an old wait cannot end a newer one.
    private var fixRequest = 0

    nonisolated private static let darkKey = "map.dark"
    nonisolated private static let reliefKey = "map.reliefStrength"
    /// The layers of the relief, as `RELIEF_LAYERS` in core-maps. Contour lines stay
    /// visible in the dark map whatever the slider says.
    private static let reliefLayers = ["hillshade"]

    /// Back to the look of a first start; for the UI tests.
    nonisolated static func resetLook() {
        UserDefaults.standard.removeObject(forKey: darkKey)
        UserDefaults.standard.removeObject(forKey: reliefKey)
    }

    static let noPermissionHint = "Position nicht verfügbar – Standortzugriff in den Einstellungen erlauben."
    static let noFixHint = "Position noch nicht verfügbar."

    /// Where the map starts while no region is installed (as on the desktop).
    private static let fallbackCentre = CLLocationCoordinate2D(latitude: 47.68, longitude: 11.57)

    init(core: CoreApp, bounds: [Double]?, zoom: Double = 11, centre start: [Double]? = nil) {
        self.core = core
        mapView = MLNMapView(frame: .zero)
        route = RouteOverlay(mapView: mapView)
        dark = UserDefaults.standard.bool(forKey: Self.darkKey)
        reliefStrength = UserDefaults.standard.double(forKey: Self.reliefKey)
        super.init()
        route.dark = dark
        mapView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        mapView.maximumZoomLevel = 19
        mapView.delegate = self
        self.bounds = bounds
        var centre = Self.centre(of: bounds) ?? Self.fallbackCentre
        if let start, start.count == 2 {
            centre = CLLocationCoordinate2D(latitude: start[0], longitude: start[1])
        }
        mapView.setCenter(centre, zoomLevel: zoom, animated: false)
        show(.summer)
        locationManager.delegate = self
        showLocationIfAllowed()
    }

    // MARK: Own position

    private var mayLocate: Bool {
        let status = locationManager.authorizationStatus
        return status == .authorizedWhenInUse || status == .authorizedAlways
    }

    /// The map draws the dot itself. Without the permission it would ask for it
    /// at once, so it is only switched on when the permission is there.
    private func showLocationIfAllowed() {
        if mayLocate { mapView.showsUserLocation = true }
    }

    /// Centres on the last known position, at zoom 14 or closer. Asks for the
    /// permission the first time; says so if there is no permission or no position.
    func centreOnLocation() {
        locationHint = nil
        if locationManager.authorizationStatus == .notDetermined {
            centreWhenLocated = true
            locationManager.requestWhenInUseAuthorization()
        } else if !mayLocate {
            locationHint = Self.noPermissionHint
        } else if let location = mapView.userLocation?.location {
            centre(on: location)
        } else {
            mapView.showsUserLocation = true
            waitForFix()
        }
    }

    /// Centres as soon as a position arrives; says so if none comes within 10 s.
    /// After that the map stays where the user has moved it.
    private func waitForFix() {
        centreWhenLocated = true
        fixRequest += 1
        let request = fixRequest
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: 10_000_000_000)
            guard let self, self.centreWhenLocated, self.fixRequest == request else { return }
            self.centreWhenLocated = false
            self.locationHint = Self.noFixHint
        }
    }

    private func centre(on location: CLLocation) {
        centreWhenLocated = false
        locationHint = nil
        mapView.setCenter(location.coordinate, zoomLevel: max(mapView.zoomLevel, 14), animated: true)
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        showLocationIfAllowed()
        if centreWhenLocated && mayLocate {
            waitForFix()
        } else if centreWhenLocated && manager.authorizationStatus != .notDetermined {
            centreWhenLocated = false
            locationHint = Self.noPermissionHint
        }
    }

    func mapView(_ mapView: MLNMapView, didUpdate userLocation: MLNUserLocation?) {
        let accuracy = userLocation?.location?.horizontalAccuracy ?? -1
        locationAccuracy = accuracy >= 0 ? accuracy : nil
        if centreWhenLocated, let location = userLocation?.location {
            centre(on: location)
        }
    }

    func mapView(_ mapView: MLNMapView, didFailToLocateUserWithError error: Error) {
        if centreWhenLocated {
            centreWhenLocated = false
            locationHint = mayLocate ? Self.noFixHint : Self.noPermissionHint
        }
    }

    private static func centre(of bounds: [Double]?) -> CLLocationCoordinate2D? {
        guard let bounds, bounds.count == 4 else { return nil }
        return CLLocationCoordinate2D(latitude: (bounds[1] + bounds[3]) / 2, longitude: (bounds[0] + bounds[2]) / 2)
    }

    /// The regions changed (installed, replaced, deleted, switched on or
    /// off): the core fills the style anew, and the map draws all tiles again.
    /// The first active region brings the map to it, unless it shows it already.
    func reloadRegions(bounds: [Double]?) {
        let hadNone = self.bounds == nil
        self.bounds = bounds
        if hadNone, let bounds, let centre = Self.centre(of: bounds) {
            let shown = mapView.centerCoordinate
            let inside = (bounds[0]...bounds[2]).contains(shown.longitude)
                && (bounds[1]...bounds[3]).contains(shown.latitude)
            if !inside { mapView.setCenter(centre, animated: false) }
        }
        show(base)
    }

    func show(_ base: MapBase) {
        self.base = base
        rendered = false
        styleLoaded = false
        do {
            mapView.styleURL = try MapStyle.url(for: base, dark: dark, core: core)
        } catch {
            fail(error)
        }
    }

    /// Switches between light and dark. The camera and the route stay.
    func setDark(_ dark: Bool) {
        guard dark != self.dark else { return }
        self.dark = dark
        UserDefaults.standard.set(dark, forKey: Self.darkKey)
        route.dark = dark
        show(base)
    }

    /// Follows the slider live, without loading the style again.
    private func applyRelief() {
        guard dark, let style = mapView.style else { return }
        let visible = reliefStrength > 0
        for id in Self.reliefLayers {
            style.layer(withIdentifier: id)?.isVisible = visible
        }
        guard visible else { return }
        let hillshade = style.layer(withIdentifier: "hillshade") as? MLNHillshadeStyleLayer
        hillshade?.hillshadeExaggeration = NSExpression(forConstantValue: 0.15 + 0.65 * reliefStrength)
    }

    /// Whether the map draws the relief at the moment; for the self-test.
    var reliefVisibleOnMap: Bool {
        mapView.style?.layer(withIdentifier: "hillshade")?.isVisible ?? false
    }

    private var slopeLayer: MLNRasterStyleLayer? {
        mapView.style?.layer(withIdentifier: "slope") as? MLNRasterStyleLayer
    }

    private func applySlopeOpacity() {
        slopeLayer?.rasterOpacity = NSExpression(forConstantValue: slopeOpacity)
    }

    /// The opacity the map really draws the slope shading with; for the self-test.
    var slopeOpacityOnMap: Double? {
        (slopeLayer?.rasterOpacity.constantValue as? NSNumber)?.doubleValue
    }

    /// How many contour lines the map shows at the moment; for the self-test.
    /// Zero as well when the installed regions bring no contour lines.
    var contoursOnMap: Int {
        let layers = Set(["contour-minor", "contour-major"].filter { mapView.style?.layer(withIdentifier: $0) != nil })
        guard !layers.isEmpty else { return 0 }
        return mapView.visibleFeatures(in: mapView.bounds, styleLayerIdentifiers: layers).count
    }

    /// Where places are on the screen, as "x,y;x,y"; for the self-test.
    func screenPoints(of places: [CLLocationCoordinate2D]) -> String {
        places.map { place in
            let point = mapView.convert(place, toPointTo: mapView)
            return String(format: "%.1f,%.1f", point.x, point.y)
        }.joined(separator: ";")
    }

    private func fail(_ error: Error) {
        loadFailures += 1
        message = "Die Karte konnte nicht geladen werden: \(describe(error))"
    }

    // MARK: MLNMapViewDelegate

    func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
        styleLoaded = true
        applySlopeOpacity()
        applyRelief()
        route.styleLoaded(style)
    }

    func mapView(_ mapView: MLNMapView, viewFor annotation: MLNAnnotation) -> MLNAnnotationView? {
        route.view(for: annotation)
    }

    func mapView(_ mapView: MLNMapView, didFailToLoadImage imageName: String) -> UIImage? {
        Icons.image(named: imageName)
    }

    func mapViewDidFinishRenderingMap(_ mapView: MLNMapView, fullyRendered: Bool) {
        if styleLoaded && fullyRendered && !rendered {
            rendered = true
            if firstRenderedAfter == nil {
                firstRenderedAfter = Date().timeIntervalSince(AppStart.launched)
            }
        }
    }

    func mapViewDidFailLoadingMap(_ mapView: MLNMapView, withError error: Error) {
        fail(error)
    }
}

struct MapView: UIViewRepresentable {
    let controller: MapController

    func makeUIView(context: Context) -> MLNMapView {
        controller.mapView
    }

    func updateUIView(_ view: MLNMapView, context: Context) {}
}
