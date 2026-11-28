import CoreLocation
import MapLibre
import UIKit

/// The search hits on the map: a dot for each, the chosen one larger. Drawing
/// and camera only; what a tap means decides the `SearchController`.
@MainActor
final class SearchOverlay: SearchMap {
    private static let source = "search-hits"
    /// How far from a dot, in points, a finger still counts as on it.
    private static let tapTolerance: CGFloat = 24
    /// A camera move of the search takes less than this.
    private static let moveDuration: TimeInterval = 0.8

    /// The map started moving for another reason than the search.
    var onOtherMove: (() -> Void)?

    private let mapView: MLNMapView
    private var points: [CLLocationCoordinate2D] = []
    private var selected: Int?
    private var searchMoveEnds = Date.distantPast

    init(mapView: MLNMapView) {
        self.mapView = mapView
    }

    /// A style starts without the dots: add their source and layers again.
    func styleLoaded(_ style: MLNStyle) {
        guard style.source(withIdentifier: Self.source) == nil else { return }
        let source = MLNShapeSource(identifier: Self.source, shape: shape(), options: nil)
        style.addSource(source)
        let dots = MLNCircleStyleLayer(identifier: "search-hits", source: source)
        dots.circleRadius = NSExpression(forConstantValue: 7)
        dots.circleColor = NSExpression(forConstantValue: UIColor(hex: "#b3261e"))
        dots.circleStrokeColor = NSExpression(forConstantValue: UIColor.white)
        dots.circleStrokeWidth = NSExpression(forConstantValue: 2)
        let chosen = MLNCircleStyleLayer(identifier: "search-selected", source: source)
        chosen.predicate = NSPredicate(format: "selected == TRUE")
        chosen.circleRadius = NSExpression(forConstantValue: 11)
        chosen.circleColor = NSExpression(forConstantValue: UIColor(hex: "#b3261e"))
        chosen.circleStrokeColor = NSExpression(forConstantValue: UIColor.white)
        chosen.circleStrokeWidth = NSExpression(forConstantValue: 3)
        style.addLayer(dots)
        style.addLayer(chosen)
    }

    private func shape() -> MLNShape {
        MLNShapeCollectionFeature(shapes: points.enumerated().map { index, point in
            let feature = MLNPointFeature()
            feature.coordinate = point
            feature.attributes = ["index": index, "selected": index == selected]
            return feature
        })
    }

    /// The dot nearest to a tap, if the finger was on one.
    func dot(at place: CLLocationCoordinate2D) -> Int? {
        let tap = mapView.convert(place, toPointTo: mapView)
        var best: (index: Int, distance: CGFloat)?
        for (index, point) in points.enumerated() {
            let at = mapView.convert(point, toPointTo: mapView)
            let distance = hypot(at.x - tap.x, at.y - tap.y)
            if distance <= Self.tapTolerance, distance < (best?.distance ?? .infinity) {
                best = (index, distance)
            }
        }
        return best?.index
    }

    /// Called by the map before its camera changes.
    func mapWillMove(reason: MLNCameraChangeReason) {
        let bySearch = reason == .programmatic && Date() < searchMoveEnds
        if !bySearch { onOtherMove?() }
    }

    // MARK: SearchMap

    var searchCentre: CLLocationCoordinate2D { mapView.centerCoordinate }

    var searchRadiusM: Double {
        let centre = mapView.centerCoordinate
        let corner = mapView.visibleCoordinateBounds.ne
        return CLLocation(latitude: centre.latitude, longitude: centre.longitude)
            .distance(from: CLLocation(latitude: corner.latitude, longitude: corner.longitude))
    }

    var searchCamera: SearchCamera {
        SearchCamera(center: mapView.centerCoordinate, zoom: mapView.zoomLevel)
    }

    func showSearchHits(_ points: [CLLocationCoordinate2D], selected: Int?) {
        self.points = points
        self.selected = selected
        (mapView.style?.source(withIdentifier: Self.source) as? MLNShapeSource)?.shape = shape()
    }

    func showSearchResult(_ at: CLLocationCoordinate2D, topInset: Double) {
        if let camera = camera(for: [at], topInset: topInset) { moveSearchCamera(camera) }
    }

    func moveSearchCamera(_ camera: SearchCamera) {
        searchMoveEnds = Date().addingTimeInterval(Self.moveDuration)
        mapView.setCenter(camera.center, zoomLevel: camera.zoom, animated: true)
    }

    /// Clear of the panel above and the sheet below.
    private func camera(for points: [CLLocationCoordinate2D], topInset: Double) -> SearchCamera? {
        SearchFit.camera(
            for: points, width: Double(mapView.bounds.width), height: Double(mapView.bounds.height),
            top: topInset + 24, left: 40, bottom: 170, right: 40, currentZoom: mapView.zoomLevel)
    }
}
