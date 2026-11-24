import CoreLocation
import Foundation

/// Plays the points of a GPX file in place of the GPS, ten times as fast as
/// they were recorded; for the UI tests (start argument `-replayGPX <path>`).
/// The positions keep the time spans of the file, counted from now, so the
/// core sees the speed of the file and keeps the points as it would outdoors;
/// their times therefore run ahead of the clock.
@MainActor
final class GPXReplay: LocationManaging {
    struct Point: Equatable {
        let lat: Double
        let lon: Double
        let ele: Double?
        /// Seconds after the first point.
        let offset: TimeInterval
    }

    weak var sink: LocationSink?
    let authorizationStatus = CLAuthorizationStatus.authorizedWhenInUse
    var desiredAccuracy = kCLLocationAccuracyBest
    var distanceFilter = kCLDistanceFilterNone
    var activityType = CLActivityType.other
    var pausesLocationUpdatesAutomatically = true
    var allowsBackgroundLocationUpdates = false
    var showsBackgroundLocationIndicator = false

    let points: [Point]
    let speedUp: Double
    private var task: Task<Void, Never>?
    /// Where the replay goes on after a stop.
    private var next = 0

    init(points: [Point], speedUp: Double = 10) {
        self.points = points
        self.speedUp = speedUp
    }

    convenience init(contentsOf url: URL, speedUp: Double = 10) throws {
        self.init(points: try Self.parse(Data(contentsOf: url)), speedUp: speedUp)
    }

    func requestWhenInUseAuthorization() {}

    func startUpdatingLocation() {
        guard task == nil else { return }
        let started = Date()
        let from = next
        let base = from < points.count ? points[from].offset : 0
        task = Task { [weak self] in
            guard let self else { return }
            for index in from..<self.points.count {
                let point = self.points[index]
                let due = (point.offset - base) / self.speedUp
                let wait = due - Date().timeIntervalSince(started)
                if wait > 0 { try? await Task.sleep(nanoseconds: UInt64(wait * 1_000_000_000)) }
                guard !Task.isCancelled else { return }
                self.next = index + 1
                let location = CLLocation(
                    coordinate: CLLocationCoordinate2D(latitude: point.lat, longitude: point.lon),
                    altitude: point.ele ?? 0, horizontalAccuracy: 5, verticalAccuracy: point.ele == nil ? -1 : 5,
                    course: -1, speed: -1,
                    timestamp: started.addingTimeInterval(point.offset - base))
                self.sink?.received([location])
            }
        }
    }

    func stopUpdatingLocation() {
        task?.cancel()
        task = nil
    }

    /// The track points of all tracks, in order. Points without a time are one second apart.
    nonisolated static func parse(_ data: Data) throws -> [Point] {
        let reader = Reader()
        let parser = XMLParser(data: data)
        parser.delegate = reader
        guard parser.parse() else {
            throw parser.parserError ?? CocoaError(.fileReadCorruptFile)
        }
        return reader.points
    }

    private final class Reader: NSObject, XMLParserDelegate {
        var points: [Point] = []
        private var lat: Double?
        private var lon: Double?
        private var ele: Double?
        private var time: Date?
        private var first: Date?
        private var text = ""
        private let dates: ISO8601DateFormatter = {
            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            return formatter
        }()
        private let plainDates = ISO8601DateFormatter()

        func parser(
            _ parser: XMLParser, didStartElement name: String, namespaceURI: String?, qualifiedName: String?,
            attributes: [String: String] = [:]
        ) {
            text = ""
            if name == "trkpt" {
                lat = attributes["lat"].flatMap(Double.init)
                lon = attributes["lon"].flatMap(Double.init)
                ele = nil
                time = nil
            }
        }

        func parser(_ parser: XMLParser, foundCharacters string: String) {
            text += string
        }

        func parser(_ parser: XMLParser, didEndElement name: String, namespaceURI: String?, qualifiedName: String?) {
            let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
            switch name {
            case "ele": ele = Double(value)
            case "time": time = dates.date(from: value) ?? plainDates.date(from: value)
            case "trkpt":
                guard let lat, let lon else { return }
                let offset: TimeInterval
                if let time {
                    if first == nil { first = time }
                    offset = time.timeIntervalSince(first!)
                } else {
                    offset = (points.last?.offset ?? -1) + 1
                }
                points.append(Point(lat: lat, lon: lon, ele: ele, offset: offset))
            default: break
            }
        }
    }
}
