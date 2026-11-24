import CoreLocation
import MapLibre
import UIKit

/// Recorded lines on the map: the recording that runs, and a stored track
/// being looked at. Points that come in while the app is in the background
/// are kept and drawn once it is back, so the map is not redrawn unseen.
@MainActor
final class TrackOverlay {
    private static let recordingSource = "recording"
    private static let trackSource = "track"
    /// Not a colour of the route legs, and readable on the light and the black map.
    static let recordingColor = UIColor(red: 0.85, green: 0.11, blue: 0.38, alpha: 1)
    static let trackColor = UIColor(red: 0.45, green: 0.23, blue: 0.85, alpha: 1)

    var dark = false
    private let mapView: MLNMapView
    private(set) var recording: [[CLLocationCoordinate2D]] = []
    private(set) var track: [[CLLocationCoordinate2D]] = []
    /// Points arrived that the map does not show yet.
    private var stale = false
    private var foreground: NSObjectProtocol?

    init(mapView: MLNMapView) {
        self.mapView = mapView
        foreground = NotificationCenter.default.addObserver(
            forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                if self?.stale == true { self?.drawRecording() }
            }
        }
    }

    func styleLoaded(_ style: MLNStyle) {
        add(Self.trackSource, color: Self.trackColor, to: style, shape: shape(track))
        add(Self.recordingSource, color: Self.recordingColor, to: style, shape: shape(recording))
        stale = false
    }

    private func add(_ id: String, color: UIColor, to style: MLNStyle, shape: MLNShape) {
        guard style.source(withIdentifier: id) == nil else { return }
        let source = MLNShapeSource(identifier: id, shape: shape, options: nil)
        style.addSource(source)
        let casing = MLNLineStyleLayer(identifier: "\(id)-casing", source: source)
        casing.lineColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        casing.lineWidth = NSExpression(forConstantValue: 7)
        let line = MLNLineStyleLayer(identifier: id, source: source)
        line.lineColor = NSExpression(forConstantValue: color)
        line.lineWidth = NSExpression(forConstantValue: 4)
        for layer in [casing, line] {
            layer.lineCap = NSExpression(forConstantValue: "round")
            layer.lineJoin = NSExpression(forConstantValue: "round")
            style.addLayer(layer)
        }
    }

    // MARK: Recording

    /// The whole line of the recording, as the core has it.
    func setRecording(_ segments: [[LatLon]]) {
        recording = segments.map { $0.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) } }
        drawRecording()
    }

    /// Points recorded since the last change.
    func append(_ vertices: [LineVertex]) {
        guard !vertices.isEmpty else { return }
        for vertex in vertices {
            let segment = Int(vertex.segment)
            while recording.count <= segment { recording.append([]) }
            recording[segment].append(CLLocationCoordinate2D(latitude: vertex.lat, longitude: vertex.lon))
        }
        stale = true
        if UIApplication.shared.applicationState != .background {
            drawRecording()
        }
    }

    func clearRecording() {
        recording = []
        drawRecording()
    }

    private func drawRecording() {
        stale = false
        (mapView.style?.source(withIdentifier: Self.recordingSource) as? MLNShapeSource)?.shape = shape(recording)
    }

    // MARK: Stored track

    /// Shows a stored track and fits the map to it; nil takes it away.
    func show(track segments: [[LatLon]]?, padding: UIEdgeInsets = .init(top: 120, left: 40, bottom: 320, right: 40)) {
        track = (segments ?? []).map { $0.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) } }
        (mapView.style?.source(withIdentifier: Self.trackSource) as? MLNShapeSource)?.shape = shape(track)
        let points = track.flatMap { $0 }
        guard points.count >= 2 else {
            if let only = points.first { mapView.setCenter(only, zoomLevel: max(mapView.zoomLevel, 14), animated: true) }
            return
        }
        var bounds = MLNCoordinateBounds(sw: points[0], ne: points[0])
        for point in points {
            bounds.sw.latitude = min(bounds.sw.latitude, point.latitude)
            bounds.sw.longitude = min(bounds.sw.longitude, point.longitude)
            bounds.ne.latitude = max(bounds.ne.latitude, point.latitude)
            bounds.ne.longitude = max(bounds.ne.longitude, point.longitude)
        }
        mapView.setVisibleCoordinateBounds(bounds, edgePadding: padding, animated: true, completionHandler: nil)
    }

    private func shape(_ segments: [[CLLocationCoordinate2D]]) -> MLNShape {
        MLNShapeCollectionFeature(shapes: segments.filter { $0.count >= 2 }.map { line in
            MLNPolylineFeature(coordinates: line, count: UInt(line.count))
        })
    }
}
