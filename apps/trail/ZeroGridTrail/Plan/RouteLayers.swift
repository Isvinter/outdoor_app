import CoreLocation
import MapLibre
import UIKit

/// The planned route on the map: its legs as lines, its waypoints as views that
/// can be dragged and tapped, and the cursor of the elevation profile. Drawing
/// and input only; what a tap or a drag means decides the `PlanController`.
@MainActor
final class RouteOverlay: NSObject, UIGestureRecognizerDelegate {
    /// One leg as it is drawn: a failed one is a straight dashed line.
    struct Leg {
        let line: [CLLocationCoordinate2D]
        let color: String
        let failed: Bool
    }

    private static let source = "route"

    /// The dark look: set before its style loads, which adds the layers anew.
    var dark = false

    private let mapView: MLNMapView
    private var legs: [Leg] = []
    /// The leg drawn as chosen, an index into `legs`.
    private var selectedLeg: Int?
    private var waypoints: [WaypointAnnotation] = []
    private var cursor: CursorAnnotation?
    private let tap = UITapGestureRecognizer()
    /// Taps that may still turn out to be the first half of a double tap.
    private var pendingTaps: [DispatchWorkItem] = []
    /// How long a tap waits for a second one. Making the recognizer wait for
    /// the map's double tap instead would swallow every second of quick taps
    /// at different places.
    private static let doubleTapWait = 0.3
    /// When the map's double tap last ended. Its second tap may reach `tapped`
    /// only after that, too late to be cancelled.
    private var lastDoubleTap: CFTimeInterval = -.infinity

    /// A tap on the map, away from the waypoints.
    var onTap: ((CLLocationCoordinate2D) -> Void)?
    /// A tap on the line of a leg, with its index.
    var onLegTap: ((Int) -> Void)?
    var onWaypointTap: ((Int) -> Void)?
    /// A waypoint was dragged to a new place.
    var onWaypointMove: ((Int, CLLocationCoordinate2D) -> Void)?

    init(mapView: MLNMapView) {
        self.mapView = mapView
        super.init()
        tap.addTarget(self, action: #selector(tapped))
        tap.delegate = self
        for other in mapView.gestureRecognizers ?? [] {
            if let other = other as? UITapGestureRecognizer, other.numberOfTapsRequired > 1 {
                other.addTarget(self, action: #selector(tappedTwice))
            }
        }
        mapView.addGestureRecognizer(tap)
    }

    // MARK: Legs

    /// A style starts without the route: add its source and layers again.
    func styleLoaded(_ style: MLNStyle) {
        guard style.source(withIdentifier: Self.source) == nil else { return }
        let source = MLNShapeSource(identifier: Self.source, shape: shape(), options: nil)
        style.addSource(source)
        let drawn = NSPredicate(format: "failed == FALSE")
        // A casing in the colour of the ground sets the route apart from similar trails.
        let casing = MLNLineStyleLayer(identifier: "route-casing", source: source)
        casing.predicate = drawn
        casing.lineColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        casing.lineWidth = NSExpression(forConstantValue: 8)
        casing.lineOpacity = NSExpression(forConstantValue: 0.95)
        // The chosen leg, wide and see-through under its line; a dashed one as well.
        let selected = MLNLineStyleLayer(identifier: "route-selected", source: source)
        selected.predicate = NSPredicate(format: "selected == TRUE")
        selected.lineColor = NSExpression(mglJSONObject: ["to-color", ["get", "color"]])
        selected.lineWidth = NSExpression(forConstantValue: 10)
        selected.lineOpacity = NSExpression(forConstantValue: 0.45)
        let line = MLNLineStyleLayer(identifier: "route", source: source)
        line.predicate = drawn
        line.lineColor = NSExpression(mglJSONObject: ["to-color", ["get", "color"]])
        line.lineWidth = NSExpression(forConstantValue: 4.5)
        for layer in [casing, selected, line] {
            layer.lineCap = NSExpression(forConstantValue: "round")
            layer.lineJoin = NSExpression(forConstantValue: "round")
        }
        let failed = MLNLineStyleLayer(identifier: "route-failed", source: source)
        failed.predicate = NSPredicate(format: "failed == TRUE")
        failed.lineColor = NSExpression(mglJSONObject: ["to-color", ["get", "color"]])
        failed.lineWidth = NSExpression(forConstantValue: 3)
        failed.lineDashPattern = NSExpression(forConstantValue: [2, 2])
        for layer in [casing, selected, line, failed] {
            style.addLayer(layer)
        }
    }

    func show(_ legs: [Leg]) {
        self.legs = legs
        reshape()
    }

    /// Draws leg `leg` as chosen; nil for none.
    func select(leg: Int?) {
        guard leg != selectedLeg else { return }
        selectedLeg = leg
        reshape()
    }

    private func reshape() {
        (mapView.style?.source(withIdentifier: Self.source) as? MLNShapeSource)?.shape = shape()
    }

    /// WCAG contrast of "#rrggbb" against black.
    nonisolated static func contrastOnBlack(_ hex: String) -> Double {
        let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
        let linear = [16, 8, 0].map { shift -> Double in
            let channel = Double((value >> UInt32(shift)) & 0xff) / 255
            return channel <= 0.04045 ? channel / 12.92 : pow((channel + 0.055) / 1.055, 2.4)
        }
        return (0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2] + 0.05) / 0.05
    }

    /// The colour, mixed with white only as far as it takes to read at 4.5:1 on black,
    /// so that the hue that tells the legs apart stays.
    nonisolated static func readableOnBlack(_ hex: String) -> String {
        guard contrastOnBlack(hex) < 4.5, let value = UInt32(hex.dropFirst(), radix: 16) else { return hex }
        let channels = [16, 8, 0].map { Double((value >> UInt32($0)) & 0xff) }
        for step in 1...20 {
            let mix = Double(step) / 20
            let mixed = channels.map { Int(($0 + (255 - $0) * mix).rounded()) }
            let candidate = String(format: "#%02x%02x%02x", mixed[0], mixed[1], mixed[2])
            if contrastOnBlack(candidate) >= 4.5 { return candidate }
        }
        return "#ffffff"
    }

    private func shape() -> MLNShape {
        MLNShapeCollectionFeature(shapes: legs.enumerated().filter { $0.element.line.count >= 2 }.map { index, leg in
            let feature = MLNPolylineFeature(coordinates: leg.line, count: UInt(leg.line.count))
            feature.attributes = [
                "color": dark ? Self.readableOnBlack(leg.color) : leg.color, "failed": leg.failed,
                "leg": index, "selected": index == selectedLeg,
            ]
            return feature
        })
    }

    /// The legs the current style draws, for the self-test: their colours, and
    /// how many of them are dashed.
    var drawn: (colors: [String], dashed: Int) {
        guard let style = mapView.style, style.source(withIdentifier: Self.source) != nil,
              style.layer(withIdentifier: "route") != nil, style.layer(withIdentifier: "route-failed") != nil
        else { return ([], 0) }
        let shown = legs.filter { $0.line.count >= 2 }
        return (shown.map(\.color), shown.filter(\.failed).count)
    }

    // MARK: Waypoints

    func setWaypoints(_ points: [CLLocationCoordinate2D], stageEnds: Set<Int>) {
        if points.count != waypoints.count {
            mapView.removeAnnotations(waypoints)
            waypoints = points.indices.map { index in
                let waypoint = WaypointAnnotation()
                waypoint.index = index
                return waypoint
            }
        }
        for (waypoint, point) in zip(waypoints, points) {
            waypoint.coordinate = point
            waypoint.endsStage = stageEnds.contains(waypoint.index)
            (mapView.view(for: waypoint) as? WaypointView)?.show(waypoint)
        }
        // Adding one that is on the map already changes nothing.
        mapView.addAnnotations(waypoints)
    }

    /// Moves the map so that all of `points` show, clear of the bars and the sheet.
    func fit(_ points: [CLLocationCoordinate2D]) {
        guard let first = points.first else { return }
        var (south, north, west, east) = (first.latitude, first.latitude, first.longitude, first.longitude)
        for point in points {
            south = min(south, point.latitude)
            north = max(north, point.latitude)
            west = min(west, point.longitude)
            east = max(east, point.longitude)
        }
        let bounds = MLNCoordinateBounds(
            sw: CLLocationCoordinate2D(latitude: south, longitude: west),
            ne: CLLocationCoordinate2D(latitude: north, longitude: east))
        let camera = mapView.cameraThatFitsCoordinateBounds(
            bounds, edgePadding: UIEdgeInsets(top: 150, left: 40, bottom: 170, right: 40))
        mapView.setCamera(camera, animated: true)
    }

    func showCursor(_ at: CLLocationCoordinate2D?) {
        guard let at else {
            if let cursor { mapView.removeAnnotation(cursor) }
            cursor = nil
            return
        }
        if let cursor {
            cursor.coordinate = at
        } else {
            let cursor = CursorAnnotation()
            cursor.coordinate = at
            self.cursor = cursor
            mapView.addAnnotation(cursor)
        }
    }

    /// The view of one of the overlay's annotations; nil for any other.
    func view(for annotation: MLNAnnotation) -> MLNAnnotationView? {
        if annotation is CursorAnnotation {
            return mapView.dequeueReusableAnnotationView(withIdentifier: CursorView.reuseIdentifier)
                ?? CursorView(reuseIdentifier: CursorView.reuseIdentifier)
        }
        guard let waypoint = annotation as? WaypointAnnotation else { return nil }
        let view = mapView.dequeueReusableAnnotationView(withIdentifier: WaypointView.reuseIdentifier) as? WaypointView
            ?? newWaypointView()
        view.show(waypoint)
        return view
    }

    private func newWaypointView() -> WaypointView {
        let view = WaypointView(reuseIdentifier: WaypointView.reuseIdentifier)
        // A finger on a waypoint drags the waypoint, not the map.
        for other in mapView.gestureRecognizers ?? [] where other is UIPanGestureRecognizer {
            other.require(toFail: view.pan)
        }
        view.onTap = { [weak self] waypoint in self?.onWaypointTap?(waypoint.index) }
        view.onDrag = { [weak self] waypoint, centre in
            guard let self else { return }
            waypoint.coordinate = self.mapView.convert(centre, toCoordinateFrom: self.mapView)
        }
        view.onDrop = { [weak self] waypoint, centre in
            guard let self else { return }
            let to = self.mapView.convert(centre, toCoordinateFrom: self.mapView)
            waypoint.coordinate = to
            self.onWaypointMove?(waypoint.index, to)
        }
        return view
    }

    // MARK: Taps on the map

    @objc private func tapped() {
        guard tap.state == .ended else { return }
        let point = tap.location(in: mapView)
        let at = mapView.convert(point, toCoordinateFrom: mapView)
        // Where the map is now: it may move before the tap counts.
        let leg = leg(at: point)
        let time = CACurrentMediaTime()
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingTaps.removeFirst()
            guard abs(time - self.lastDoubleTap) >= Self.doubleTapWait else { return }
            if let leg {
                self.onLegTap?(leg)
            } else {
                self.onTap?(at)
            }
        }
        pendingTaps.append(item)
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.doubleTapWait, execute: item)
    }

    /// How far from a line a finger still hits it, in points.
    private static let legHitRadius: CGFloat = 22

    /// The leg whose line is nearest to `point`, if one is within reach.
    private func leg(at point: CGPoint) -> Int? {
        let reach = Self.legHitRadius
        let box = CGRect(x: point.x - reach, y: point.y - reach, width: 2 * reach, height: 2 * reach)
        let hits = mapView.visibleFeatures(in: box, styleLayerIdentifiers: ["route", "route-failed"])
        let nearest = hits.compactMap { feature -> (leg: Int, distance: CGFloat)? in
            guard let leg = (feature.attribute(forKey: "leg") as? NSNumber)?.intValue else { return nil }
            // What the map hands back may be cut into pieces at tile edges.
            let lines = (feature as? MLNMultiPolylineFeature)?.polylines ?? [feature as? MLNPolyline].compactMap { $0 }
            let distance = lines.map { line in
                let points = UnsafeBufferPointer(start: line.coordinates, count: Int(line.pointCount))
                    .map { mapView.convert($0, toPointTo: mapView) }
                return Self.distance(from: point, toLine: points)
            }.min() ?? reach
            return (leg, distance)
        }.min { $0.distance < $1.distance }
        return nearest?.leg
    }

    /// The distance from `point` to the polyline through `line`, in points.
    nonisolated static func distance(from point: CGPoint, toLine line: [CGPoint]) -> CGFloat {
        guard let first = line.first else { return .infinity }
        var best = hypot(point.x - first.x, point.y - first.y)
        for (a, b) in zip(line, line.dropFirst()) {
            let (dx, dy) = (b.x - a.x, b.y - a.y)
            let length = dx * dx + dy * dy
            let t = length > 0 ? max(0, min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length)) : 0
            best = min(best, hypot(point.x - (a.x + t * dx), point.y - (a.y + t * dy)))
        }
        return best
    }

    /// A double tap zooms the map and sets no points.
    @objc private func tappedTwice(_ recognizer: UITapGestureRecognizer) {
        guard recognizer.state == .ended else { return }
        lastDoubleTap = CACurrentMediaTime()
        pendingTaps.forEach { $0.cancel() }
        pendingTaps = []
    }

    /// A tap on a waypoint belongs to the waypoint.
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        var view = touch.view
        while let current = view, current !== mapView {
            if current is WaypointView { return false }
            view = current.superview
        }
        return true
    }

    /// The map has a tap recognizer of its own.
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        true
    }
}
