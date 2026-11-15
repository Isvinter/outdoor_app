import MapLibre
import SwiftUI

/// The map and what the screen needs to know about it.
@MainActor
final class MapController: NSObject, ObservableObject, @preconcurrency MLNMapViewDelegate {
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
    /// A failure worth telling the user, in German.
    @Published var message: String?
    /// Styles that could not be loaded; for the self-test.
    private(set) var loadFailures = 0
    private var styleLoaded = false

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
        message = "Die Karte konnte nicht geladen werden: \(error.localizedDescription)"
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
