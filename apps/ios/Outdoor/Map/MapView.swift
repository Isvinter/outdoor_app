import CoreLocation
import MapLibre
import SwiftUI

/// The map and what the screen needs to know about it.
@MainActor
final class MapController: NSObject, ObservableObject, @preconcurrency MLNMapViewDelegate,
    @preconcurrency CLLocationManagerDelegate
{
    let mapView: MLNMapView
    private let core: CoreApp

    @Published private(set) var base: MapBase = .summer
    /// True once the current style is drawn completely.
    @Published private(set) var rendered = false
    /// How strongly the slope shading of the winter map shows, 0…1. Kept across
    /// style changes and set again after each, because a style starts with its own.
    @Published var slopeOpacity = 0.6 {
        didSet { applySlopeOpacity() }
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
    /// Only asks for the permission and tells when it changes; the map locates by itself.
    private let locationManager = CLLocationManager()
    /// The user asked for their position and it is not known yet.
    private var centreWhenLocated = false
    /// Counts the waits for a position, so that an old wait cannot end a newer one.
    private var fixRequest = 0

    static let noPermissionHint = "Position nicht verfügbar – Standortzugriff in den Einstellungen erlauben."
    static let noFixHint = "Position noch nicht verfügbar."

    /// Where the map starts while no region is installed (as on the desktop).
    private static let fallbackCentre = CLLocationCoordinate2D(latitude: 47.68, longitude: 11.57)

    init(core: CoreApp, bounds: [Double]?) {
        self.core = core
        mapView = MLNMapView(frame: .zero)
        super.init()
        mapView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        mapView.maximumZoomLevel = 19
        mapView.delegate = self
        var centre = Self.fallbackCentre
        if let bounds, bounds.count == 4 {
            centre = CLLocationCoordinate2D(latitude: (bounds[1] + bounds[3]) / 2, longitude: (bounds[0] + bounds[2]) / 2)
        }
        mapView.setCenter(centre, zoomLevel: 11, animated: false)
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

    func show(_ base: MapBase) {
        self.base = base
        rendered = false
        styleLoaded = false
        do {
            mapView.styleURL = try MapStyle.url(for: base, core: core)
        } catch {
            fail(error)
        }
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

    private func fail(_ error: Error) {
        loadFailures += 1
        message = "Die Karte konnte nicht geladen werden: \(describe(error))"
    }

    // MARK: MLNMapViewDelegate

    func mapView(_ mapView: MLNMapView, didFinishLoading style: MLNStyle) {
        styleLoaded = true
        applySlopeOpacity()
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
