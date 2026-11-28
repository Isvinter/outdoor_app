import Combine
import CoreLocation
import Foundation

/// What the search asks the core; a seam for the tests.
struct SearchBackend {
    var text: @Sendable (String, LatLon, Double, UInt32, RouteFrom?) -> SearchResults
    var category: @Sendable (String, LatLon, Double, UInt32, RouteFrom?) throws -> SearchResults
}

/// Where the map looks.
struct SearchCamera: Equatable {
    var center: CLLocationCoordinate2D
    var zoom: Double

    static func == (a: SearchCamera, b: SearchCamera) -> Bool {
        a.center.latitude == b.center.latitude && a.center.longitude == b.center.longitude && a.zoom == b.zoom
    }
}

/// What the search needs from the map.
@MainActor
protocol SearchMap: AnyObject {
    var searchCentre: CLLocationCoordinate2D { get }
    /// Half the diagonal of what the map shows, in metres.
    var searchRadiusM: Double { get }
    var searchCamera: SearchCamera { get }
    func showSearchHits(_ points: [CLLocationCoordinate2D], selected: Int?)
    /// Moves the map so that all points show below the panel; returns where it is heading.
    func fitSearchHits(_ points: [CLLocationCoordinate2D], topInset: Double) -> SearchCamera?
    func showSearchResult(_ at: CLLocationCoordinate2D, topInset: Double)
    func moveSearchCamera(_ camera: SearchCamera)
}

/// Where the map has to look to show points inside the free part of the screen.
enum SearchFit {
    static let maxZoom = 15.0
    private static let tileSize = 512.0

    static func camera(
        for points: [CLLocationCoordinate2D], width: Double, height: Double,
        top: Double, left: Double, bottom: Double, right: Double, currentZoom: Double
    ) -> SearchCamera? {
        guard !points.isEmpty else { return nil }
        var (minX, maxX, minY, maxY) = (Double.infinity, -Double.infinity, Double.infinity, -Double.infinity)
        for point in points {
            let (x, y) = mercator(point)
            minX = min(minX, x)
            maxX = max(maxX, x)
            minY = min(minY, y)
            maxY = max(maxY, y)
        }
        let (spanX, spanY) = (maxX - minX, maxY - minY)
        let freeWidth = max(width - left - right, 1)
        let freeHeight = max(height - top - bottom, 1)
        let zoom: Double
        if spanX <= 0 && spanY <= 0 {
            zoom = max(currentZoom, maxZoom)
        } else {
            let fitX = spanX > 0 ? log2(freeWidth / (spanX * tileSize)) : maxZoom
            let fitY = spanY > 0 ? log2(freeHeight / (spanY * tileSize)) : maxZoom
            zoom = max(min(fitX, fitY, maxZoom), 0)
        }
        let scale = tileSize * pow(2, zoom)
        let x = (minX + maxX) / 2 - (left - right) / 2 / scale
        let y = min(max((minY + maxY) / 2 - (top - bottom) / 2 / scale, 0), 1)
        return SearchCamera(
            center: CLLocationCoordinate2D(
                latitude: atan(sinh(.pi * (1 - 2 * y))) * 180 / .pi, longitude: x * 360 - 180),
            zoom: zoom)
    }

    private static func mercator(_ point: CLLocationCoordinate2D) -> (Double, Double) {
        let sine = sin(point.latitude * .pi / 180)
        return ((point.longitude + 180) / 360, 0.5 - log((1 + sine) / (1 - sine)) / (4 * .pi))
    }
}

/// German texts of the search.
enum SearchTexts {
    static let categories = ["lodging", "food", "groceries", "pharmacy", "fuel", "camping", "parking", "sights"]

    static func category(_ key: String) -> String {
        switch key {
        case "lodging": return "Unterkunft"
        case "food": return "Essen"
        case "groceries": return "Lebensmittel"
        case "pharmacy": return "Apotheke"
        case "fuel": return "Tankstelle"
        case "camping": return "Camping"
        case "parking": return "Parken"
        case "retail": return "Laden"
        case "sights": return "Sehenswertes"
        default: return key
        }
    }

    static func kind(_ key: String) -> String {
        switch key {
        case "place": return "Ort"
        case "street": return "Straße"
        default: return "Ort von Interesse"
        }
    }

    static func title(_ hit: SearchHit) -> String {
        if !hit.name.isEmpty { return hit.name }
        return hit.category.map(category) ?? kind(hit.kind)
    }

    static let noRoute = "Umweg oder kein Weg"
    static let ascentIgnored = "Zeiten ohne Höhenmeter."

    /// Kind, town and distance, as far as there are any; with a way from the
    /// own position its length and time, then the distance as the crow flies.
    static func details(_ hit: SearchHit) -> String {
        let what = hit.category.map(category) ?? kind(hit.kind)
        let distances: [String?]
        if let metres = hit.routeM {
            distances = [Format.distance(metres), hit.routeS.map(Format.hours), "Luftl. \(Format.distance(hit.distanceM))"]
        } else if hit.noRoute {
            distances = [Format.distance(hit.distanceM), noRoute]
        } else {
            distances = [Format.distance(hit.distanceM)]
        }
        return ([what, hit.town] + distances)
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }
}

/// The search above the map: text or a category, a list of hits and their dots
/// on the map, a card for the chosen one. What is found comes from the core.
@MainActor
final class SearchController: ObservableObject {
    static let minChars = 2
    static let limit: UInt32 = 30

    @Published var text = "" {
        didSet {
            if text != oldValue { typed() }
        }
    }
    @Published private(set) var results: SearchResults?
    @Published private(set) var selected: SearchHit?
    @Published private(set) var category: String?
    /// The field is in use: the panel shows the quick choices.
    @Published var open = false
    /// The hits are measured and ordered by the way from the own position.
    @Published private(set) var routes: Bool
    static let routesKey = "search.routes"
    /// The list is hidden behind the card of the chosen hit.
    var showsList: Bool { selected == nil && !(results?.hits.isEmpty ?? true) }
    /// Where the panel ends on the screen, so that the map fits the hits below it.
    var topInset = 0.0

    /// A hit was chosen or the chosen one is gone.
    var onSelectionCleared: (() -> Void)?

    private struct Anchor {
        let centre: LatLon
        let radiusM: Double
    }

    private let backend: SearchBackend
    /// The position and the leg settings, asked only while `routes` is on.
    private let routeFrom: @MainActor () -> RouteFrom?
    private let defaults: UserDefaults
    private let map: SearchMap
    private let typingDelay: Double
    private let fitDelay: Double
    /// Lets the panel take its new height before the map fits the hits below it.
    private let layoutDelay: Double
    private let queue = DispatchQueue(label: "de.zeibig.zerogrid.trail.search", qos: .userInitiated)
    /// Counts the searches, so that a slow answer cannot replace a newer one.
    private var request = 0
    private var typingTask: Task<Void, Never>?
    private var fitTask: Task<Void, Never>?
    /// Kept while the search moves the map, so that fitting it does not shift the search.
    private var anchor: Anchor?
    /// Where the map is heading, or has arrived, to show all hits.
    private var overview: SearchCamera?
    /// Where "Zurück" takes the map: the view before the first hit was opened.
    private var returnTo: SearchCamera?
    /// Counts map moves not made by the search, so that a late answer leaves them alone.
    private var otherMoves = 0

    init(backend: SearchBackend, map: SearchMap, typingDelay: Double = 0.2, fitDelay: Double = 0.5,
        layoutDelay: Double = 0.1, routeFrom: @escaping @MainActor () -> RouteFrom? = { nil },
        defaults: UserDefaults = .standard
    ) {
        self.backend = backend
        self.routeFrom = routeFrom
        self.defaults = defaults
        routes = defaults.bool(forKey: Self.routesKey)
        self.map = map
        self.typingDelay = typingDelay
        self.fitDelay = fitDelay
        self.layoutDelay = layoutDelay
    }

    /// Hints below the list.
    var notes: [String] {
        guard let results else { return [] }
        var notes = results.unsearchable.map { "\($0) ist nicht durchsuchbar. Baue die Region neu." }
        if results.ascentIgnored { notes.append(SearchTexts.ascentIgnored) }
        if results.widened { notes.insert("Nichts im Umkreis, nächste Treffer weiter entfernt.", at: 0) }
        if results.hits.isEmpty {
            notes.insert(category == nil ? "Nichts gefunden." : "Keine Treffer im Umkreis. Zoome heraus.", at: 0)
        }
        return notes
    }

    /// The quick choices show while nothing is typed and no hit is open.
    var showsCategories: Bool {
        open && selected == nil && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func typed() {
        typingTask?.cancel()
        fitTask?.cancel()
        clearSelection()
        category = nil
        let query = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard query.count >= Self.minChars else {
            request += 1
            anchor = nil
            overview = nil
            show(nil)
            return
        }
        let backend = backend
        let delay = typingDelay
        typingTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard !Task.isCancelled, let self else { return }
            self.runText(query)
        }
    }

    private func runText(_ query: String) {
        let backend = backend
        run(fitDelay: fitDelay) { near, radius, from in
            backend.text(query, near, radius, Self.limit, from)
        }
    }

    private func runCategory(_ key: String) {
        let backend = backend
        run(fitDelay: layoutDelay) { near, radius, from in
            try backend.category(key, near, radius, Self.limit, from)
        }
    }

    /// Switches measuring by the way on or off and searches again what is shown.
    func toggleRoutes() {
        routes.toggle()
        defaults.set(routes, forKey: Self.routesKey)
        typingTask?.cancel()
        fitTask?.cancel()
        if let category {
            runCategory(category)
        } else {
            let query = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if query.count >= Self.minChars { runText(query) }
        }
    }

    func chooseCategory(_ key: String) {
        typingTask?.cancel()
        fitTask?.cancel()
        clearSelection()
        category = key
        runCategory(key)
    }

    private func run(
        fitDelay: Double, _ work: @escaping @Sendable (LatLon, Double, RouteFrom?) throws -> SearchResults
    ) {
        request += 1
        let number = request
        let current = anchor ?? Anchor(
            centre: LatLon(lat: map.searchCentre.latitude, lon: map.searchCentre.longitude),
            radiusM: map.searchRadiusM)
        anchor = current
        let movesBefore = otherMoves
        let queue = queue
        let (centre, radius) = (current.centre, current.radiusM)
        let from = routes ? routeFrom() : nil
        Task { [weak self] in
            let result: Result<SearchResults, Error> = await withCheckedContinuation { done in
                queue.async { done.resume(returning: Result { try work(centre, radius, from) }) }
            }
            guard let self, number == self.request else { return }
            guard case .success(let results) = result else {
                self.show(nil)
                return
            }
            if let chosen = self.selected {
                // Searched again behind the card: it stays, with the new numbers if the hit is still there.
                self.selected = results.hits.first { Self.same($0, chosen) } ?? chosen
                self.show(results)
                return
            }
            self.show(results)
            self.overview = nil
            guard self.otherMoves == movesBefore else { return }
            self.fitTask = Task { [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(fitDelay * 1_000_000_000))
                guard !Task.isCancelled, let self else { return }
                self.fit(results)
            }
        }
    }

    /// Brings the hits around the search into view; a far place alone does not pull the map out.
    private func fit(_ results: SearchResults) {
        let inside = results.hits.filter { $0.centreDistanceM <= results.radiusM }
        overview = map.fitSearchHits((inside.isEmpty ? results.hits : inside).map(Self.coordinate), topInset: topInset)
    }

    private func show(_ results: SearchResults?) {
        self.results = results
        drawDots()
    }

    private func drawDots() {
        let hits = results?.hits ?? []
        map.showSearchHits(
            hits.map(Self.coordinate), selected: selected.flatMap { chosen in hits.firstIndex { Self.same($0, chosen) } })
    }

    /// The same thing found, whatever it was measured from.
    private static func same(_ a: SearchHit, _ b: SearchHit) -> Bool {
        a.kind == b.kind && a.name == b.name && a.lat == b.lat && a.lon == b.lon
    }

    private static func coordinate(_ hit: SearchHit) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: hit.lat, longitude: hit.lon)
    }

    /// A dot on the map was tapped.
    func tappedDot(_ index: Int) {
        guard let hits = results?.hits, hits.indices.contains(index) else { return }
        select(hits[index])
    }

    func select(_ hit: SearchHit) {
        // A search still under way must not open its list over the card.
        typingTask?.cancel()
        fitTask?.cancel()
        request += 1
        onSelectionCleared?()
        if returnTo == nil { returnTo = overview ?? map.searchCamera }
        selected = hit
        open = false
        drawDots()
        map.showSearchResult(Self.coordinate(hit), topInset: topInset)
    }

    /// From the chosen hit back to the list and the view it was opened from, without searching.
    func back() {
        let camera = returnTo
        clearSelection()
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { open = true }
        drawDots()
        if let camera {
            overview = camera
            map.moveSearchCamera(camera)
        }
    }

    /// The map was moved by something other than the search.
    func mapMoved() {
        otherMoves += 1
        fitTask?.cancel()
        anchor = nil
        overview = nil
    }

    /// A route to the hit is on the map; it takes the place of the pin and the dots.
    func routeDone() {
        fitTask?.cancel()
        request += 1
        selected = nil
        category = nil
        anchor = nil
        overview = nil
        returnTo = nil
        show(nil)
    }

    /// Empties the search and closes the panel.
    func close() {
        open = false
        if text.isEmpty {
            typed()
        } else {
            text = ""
        }
    }

    private func clearSelection() {
        onSelectionCleared?()
        returnTo = nil
        selected = nil
    }
}
