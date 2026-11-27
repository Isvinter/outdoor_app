import CoreLocation
import MapLibre
import UIKit

/// A section of a recorded line: of the stored track being looked at, or of
/// the recording that runs. The index is its position among the segments.
enum SectionRef: Hashable {
    case track(Int)
    case recording(Int)

    var index: Int {
        switch self {
        case .track(let index), .recording(let index): index
        }
    }

    /// "track:2", for the self-test status.
    var status: String {
        switch self {
        case .track(let index): "track:\(index)"
        case .recording(let index): "recording:\(index)"
        }
    }
}

/// Recorded lines on the map: the recording that runs, and a stored track
/// being looked at. Points that come in while the app is in the background
/// are kept and drawn once it is back, so the map is not redrawn unseen.
/// In "Abschnitte bearbeiten" the borders of the sections and the rests show
/// as points, and the chosen section stands out.
@MainActor
final class TrackOverlay {
    private static let recordingSource = "recording"
    private static let trackSource = "track"
    private static let selectedSource = "section-selected"
    private static let marksSource = "section-marks"
    private static let restImage = "section-rest"
    /// Not a colour of the route legs, and readable on the light and the black map.
    static let recordingColor = UIColor(red: 0.85, green: 0.11, blue: 0.38, alpha: 1)
    static let trackColor = UIColor(red: 0.45, green: 0.23, blue: 0.85, alpha: 1)
    /// Around the chosen section, on the light and the black map.
    static let selectedColor = UIColor(red: 1, green: 0.76, blue: 0.03, alpha: 1)

    var dark = false
    private let mapView: MLNMapView
    private(set) var recording: [[CLLocationCoordinate2D]] = []
    private(set) var track: [[CLLocationCoordinate2D]] = []
    /// The sections that are rests, by index.
    private(set) var trackRests: Set<Int> = []
    private(set) var recordingRests: Set<Int> = []
    /// The borders and rests show, and taps on the lines choose sections.
    var showsSections = false {
        didSet {
            guard showsSections != oldValue else { return }
            drawMarks()
        }
    }
    private(set) var selected: SectionRef?
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
        addSelected(to: style)
        add(Self.trackSource, color: Self.trackColor, to: style, shape: shape(track))
        add(Self.recordingSource, color: Self.recordingColor, to: style, shape: shape(recording))
        addMarks(to: style)
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

    /// A broad band under the chosen section.
    private func addSelected(to style: MLNStyle) {
        guard style.source(withIdentifier: Self.selectedSource) == nil else { return }
        let source = MLNShapeSource(identifier: Self.selectedSource, shape: selectedShape(), options: nil)
        style.addSource(source)
        let band = MLNLineStyleLayer(identifier: Self.selectedSource, source: source)
        band.lineColor = NSExpression(forConstantValue: Self.selectedColor)
        band.lineWidth = NSExpression(forConstantValue: 14)
        band.lineCap = NSExpression(forConstantValue: "round")
        band.lineJoin = NSExpression(forConstantValue: "round")
        style.addLayer(band)
    }

    /// Circles at the borders of the sections, a cup at the start of each rest.
    private func addMarks(to style: MLNStyle) {
        guard style.source(withIdentifier: Self.marksSource) == nil else { return }
        let source = MLNShapeSource(identifier: Self.marksSource, shape: marksShape(), options: nil)
        style.addSource(source)
        let border = MLNCircleStyleLayer(identifier: "\(Self.marksSource)-border", source: source)
        border.predicate = NSPredicate(format: "rest == NO")
        border.circleRadius = NSExpression(forConstantValue: 6)
        border.circleColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        border.circleStrokeColor = NSExpression(forConstantValue: dark ? UIColor.white : UIColor.black)
        border.circleStrokeWidth = NSExpression(forConstantValue: 2)
        style.addLayer(border)
        let restBack = MLNCircleStyleLayer(identifier: "\(Self.marksSource)-rest-back", source: source)
        restBack.predicate = NSPredicate(format: "rest == YES")
        restBack.circleRadius = NSExpression(forConstantValue: 12)
        restBack.circleColor = NSExpression(forConstantValue: dark ? UIColor.black : UIColor.white)
        restBack.circleStrokeColor = NSExpression(forConstantValue: dark ? UIColor.white : UIColor.black)
        restBack.circleStrokeWidth = NSExpression(forConstantValue: 2)
        style.addLayer(restBack)
        if let symbol = Self.restSymbol(color: dark ? .white : .black) {
            style.setImage(symbol, forName: Self.restImage)
            let rest = MLNSymbolStyleLayer(identifier: "\(Self.marksSource)-rest", source: source)
            rest.predicate = NSPredicate(format: "rest == YES")
            rest.iconImageName = NSExpression(forConstantValue: Self.restImage)
                        rest.iconAllowsOverlap = NSExpression(forConstantValue: true)
            style.addLayer(rest)
        }
    }

    /// The cup as a plain bitmap in `color`: a tinted symbol image keeps its own colour on the map.
    private static func restSymbol(color: UIColor) -> UIImage? {
        let configuration = UIImage.SymbolConfiguration(pointSize: 14, weight: .semibold)
        guard let symbol = UIImage(systemName: "cup.and.saucer.fill", withConfiguration: configuration) else {
            return nil
        }
        let format = UIGraphicsImageRendererFormat()
        format.scale = UIScreen.main.scale
        return UIGraphicsImageRenderer(size: symbol.size, format: format).image { _ in
            symbol.withTintColor(color, renderingMode: .alwaysOriginal).draw(at: .zero)
        }
    }

    // MARK: Recording

    /// The whole line of the recording, as the core has it, with its rests.
    func setRecording(_ segments: [[LatLon]], rests: Set<Int> = []) {
        recording = segments.map { $0.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) } }
        recordingRests = rests
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

    /// The section of the recording that runs now is a rest.
    func markRecordingRest() {
        guard !recording.isEmpty else { return }
        recordingRests.insert(recording.count - 1)
        drawMarks()
    }

    func clearRecording() {
        recording = []
        recordingRests = []
        if case .recording = selected { selected = nil }
        drawRecording()
    }

    private func drawRecording() {
        stale = false
        (mapView.style?.source(withIdentifier: Self.recordingSource) as? MLNShapeSource)?.shape = shape(recording)
        drawSelected()
        drawMarks()
    }

    // MARK: Stored track

    /// Shows a stored track and fits the map to it, above the sheet at its
    /// middle step; nil takes it away.
    func show(track segments: [[LatLon]]?, rests: Set<Int> = []) {
        track = (segments ?? []).map { $0.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) } }
        trackRests = rests
        if case .track = selected { selected = nil }
        (mapView.style?.source(withIdentifier: Self.trackSource) as? MLNShapeSource)?.shape = shape(track)
        drawSelected()
        drawMarks()
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
        // Below the search field and the toolbar.
        let padding = UIEdgeInsets(top: 210, left: 50, bottom: mapView.bounds.height * 0.5 + 30, right: 50)
        mapView.setVisibleCoordinateBounds(bounds, edgePadding: padding, animated: true, completionHandler: nil)
    }

    /// The rests of the stored track changed (not its points).
    func setTrackRests(_ rests: Set<Int>) {
        trackRests = rests
        drawMarks()
    }

    // MARK: Sections

    func select(_ section: SectionRef?) {
        selected = section.flatMap { line(of: $0) == nil ? nil : $0 }
        drawSelected()
    }

    /// The points of a section, if it is on the map.
    func line(of section: SectionRef) -> [CLLocationCoordinate2D]? {
        let lines = lines(of: section)
        guard lines.indices.contains(section.index), !lines[section.index].isEmpty else { return nil }
        return lines[section.index]
    }

    /// A point in the middle of each section of the stored track; for the self-test.
    var trackMiddles: [CLLocationCoordinate2D] {
        track.compactMap { $0.isEmpty ? nil : $0[$0.count / 2] }
    }

    private func lines(of section: SectionRef) -> [[CLLocationCoordinate2D]] {
        switch section {
        case .track: track
        case .recording: recording
        }
    }

    /// The section whose line is nearest to `point`, if one is within reach;
    /// only while the sections show.
    func hit(at point: CGPoint) -> (section: SectionRef, distance: CGFloat)? {
        guard showsSections else { return nil }
        let reach = RouteOverlay.legHitRadius
        var best: (section: SectionRef, distance: CGFloat)?
        let all = track.enumerated().map { (SectionRef.track($0.offset), $0.element) }
            + recording.enumerated().map { (SectionRef.recording($0.offset), $0.element) }
        for (section, line) in all where !line.isEmpty {
            let points = line.map { mapView.convert($0, toPointTo: mapView) }
            let distance = RouteOverlay.distance(from: point, toLine: points)
            if distance <= reach, distance < (best?.distance ?? .infinity) {
                best = (section, distance)
            }
        }
        return best
    }

    private func drawSelected() {
        (mapView.style?.source(withIdentifier: Self.selectedSource) as? MLNShapeSource)?.shape = selectedShape()
    }

    private func drawMarks() {
        (mapView.style?.source(withIdentifier: Self.marksSource) as? MLNShapeSource)?.shape = marksShape()
    }

    private func selectedShape() -> MLNShape {
        guard let selected, let line = line(of: selected) else { return MLNShapeCollectionFeature(shapes: []) }
        if line.count == 1 {
            let point = MLNPointFeature()
            point.coordinate = line[0]
            return MLNShapeCollectionFeature(shapes: [point])
        }
        return MLNShapeCollectionFeature(shapes: [MLNPolylineFeature(coordinates: line, count: UInt(line.count))])
    }

    /// The start of each section after the first, and of each rest.
    private func marksShape() -> MLNShape {
        guard showsSections else { return MLNShapeCollectionFeature(shapes: []) }
        let marks = Self.marks(track, rests: trackRests) + Self.marks(recording, rests: recordingRests)
        return MLNShapeCollectionFeature(shapes: marks.map { mark in
            let feature = MLNPointFeature()
            feature.coordinate = mark.at
            feature.attributes = ["rest": mark.rest]
            return feature
        })
    }

    nonisolated static func marks(
        _ segments: [[CLLocationCoordinate2D]], rests: Set<Int>
    ) -> [(at: CLLocationCoordinate2D, rest: Bool)] {
        segments.enumerated().compactMap { index, line in
            guard let first = line.first, index > 0 || rests.contains(index) else { return nil }
            return (first, rests.contains(index))
        }
    }

    private func shape(_ segments: [[CLLocationCoordinate2D]]) -> MLNShape {
        MLNShapeCollectionFeature(shapes: segments.filter { $0.count >= 2 }.map { line in
            MLNPolylineFeature(coordinates: line, count: UInt(line.count))
        })
    }
}
