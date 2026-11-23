import CoreLocation
import XCTest
@testable import ZeroGridTrail

@MainActor
private final class FakeMap: SearchMap {
    var searchCentre = CLLocationCoordinate2D(latitude: -13.5, longitude: -72)
    var searchRadiusM = 5_000.0
    var searchCamera = SearchCamera(center: CLLocationCoordinate2D(latitude: -13.5, longitude: -72), zoom: 14)
    var dots: [CLLocationCoordinate2D] = []
    var selectedDot: Int?
    var fitted: [[CLLocationCoordinate2D]] = []
    var fitInsets: [Double] = []

    func showSearchHits(_ points: [CLLocationCoordinate2D], selected: Int?) {
        dots = points
        selectedDot = selected
    }

    func fitSearchHits(_ points: [CLLocationCoordinate2D], topInset: Double) -> SearchCamera? {
        guard let first = points.first else { return nil }
        fitted.append(points)
        fitInsets.append(topInset)
        searchCamera = SearchCamera(center: first, zoom: 13)
        return searchCamera
    }

    func showSearchResult(_ at: CLLocationCoordinate2D, topInset: Double) {
        searchCamera = SearchCamera(center: at, zoom: 15)
    }

    func moveSearchCamera(_ camera: SearchCamera) {
        searchCamera = camera
    }
}

private final class Calls: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [(String, LatLon)] = []

    func add(_ text: String, _ near: LatLon) {
        lock.withLock { values.append((text, near)) }
    }

    var all: [(String, LatLon)] { lock.withLock { values } }
}

private func hit(_ name: String, lat: Double = -13.51, distance: Double = 1_000, category: String? = "lodging") -> SearchHit {
    SearchHit(kind: "poi", category: category, name: name, town: nil, lat: lat, lon: -72, distanceM: distance)
}

private func answer(_ hits: [SearchHit], widened: Bool = false) -> SearchResults {
    SearchResults(hits: hits, unsearchable: [], widened: widened, radiusM: 5_000)
}

@MainActor
final class SearchControllerTests: XCTestCase {
    private func controller(
        map: FakeMap, calls: Calls = Calls(),
        text: @escaping @Sendable (String) -> SearchResults = { _ in answer([hit("A"), hit("B", lat: -13.52)]) }
    ) -> SearchController {
        let backend = SearchBackend(
            text: { query, near, _, _ in
                calls.add(query, near)
                return text(query)
            },
            category: { key, near, _, _ in
                calls.add(key, near)
                return answer([hit("Kategorie")])
            })
        return SearchController(backend: backend, map: map, typingDelay: 0.01, fitDelay: 0.01, layoutDelay: 0.08)
    }

    private func wait(_ what: String, _ condition: @MainActor () -> Bool) async {
        for _ in 0..<300 where !condition() {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition(), what)
    }

    func testOlderAnswerDoesNotReplaceNewer() async {
        let map = FakeMap()
        let search = controller(map: map) { query in
            if query == "alt" { Thread.sleep(forTimeInterval: 0.15) }
            return answer([hit(query)])
        }
        search.text = "alt"
        try? await Task.sleep(nanoseconds: 40_000_000)
        search.text = "neu"
        await wait("newer answer") { search.results?.hits.first?.name == "neu" }
        try? await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertEqual(search.results?.hits.first?.name, "neu")
    }

    func testShortTextDoesNotSearch() async {
        let calls = Calls()
        let search = controller(map: FakeMap(), calls: calls)
        search.text = "a"
        try? await Task.sleep(nanoseconds: 80_000_000)
        XCTAssertTrue(calls.all.isEmpty)
        XCTAssertNil(search.results)
    }

    func testUnnamedPoiShowsCategory() {
        XCTAssertEqual(SearchTexts.title(hit("", category: "fuel")), "Tankstelle")
    }

    func testHitsAppearAsDotsAndTheMapFitsThem() async {
        let map = FakeMap()
        let search = controller(map: map)
        search.text = "Hostel"
        await wait("dots") { map.dots.count == 2 }
        await wait("fit") { map.fitted.count == 1 }
    }

    func testFarPlaceDoesNotPullTheFitOut() async {
        let map = FakeMap()
        let search = controller(map: map) { _ in
            answer([hit("Nah"), hit("Fern", lat: -15, distance: 150_000)])
        }
        search.text = "Hostel"
        await wait("fit") { map.fitted.count == 1 }
        XCTAssertEqual(map.fitted[0].count, 1)
        XCTAssertEqual(map.dots.count, 2)
    }

    func testFittingKeepsTheSearchArea() async {
        let map = FakeMap()
        let calls = Calls()
        let search = controller(map: map, calls: calls)
        search.text = "Hostel"
        await wait("fit") { map.fitted.count == 1 }
        map.searchCentre = CLLocationCoordinate2D(latitude: -13.6, longitude: -72)
        search.text = "Hostels"
        await wait("second search") { calls.all.count == 2 }
        XCTAssertEqual(calls.all[1].1.lat, -13.5)
    }

    func testAMapMoveStartsANewSearchArea() async {
        let map = FakeMap()
        let calls = Calls()
        let search = controller(map: map, calls: calls)
        search.text = "Hostel"
        await wait("fit") { map.fitted.count == 1 }
        map.searchCentre = CLLocationCoordinate2D(latitude: -13.6, longitude: -72)
        search.mapMoved()
        search.text = "Hostels"
        await wait("second search") { calls.all.count == 2 }
        XCTAssertEqual(calls.all[1].1.lat, -13.6)
    }

    func testSlowAnswerLeavesAMovedMapAlone() async {
        let map = FakeMap()
        let search = controller(map: map) { _ in
            Thread.sleep(forTimeInterval: 0.1)
            return answer([hit("A")])
        }
        search.text = "Hostel"
        try? await Task.sleep(nanoseconds: 40_000_000)
        search.mapMoved()
        await wait("answer") { search.results != nil }
        try? await Task.sleep(nanoseconds: 60_000_000)
        XCTAssertTrue(map.fitted.isEmpty)
    }

    func testBackRestoresListAndOverview() async {
        let map = FakeMap()
        let calls = Calls()
        let search = controller(map: map, calls: calls)
        search.text = "Hostel"
        await wait("fit") { map.fitted.count == 1 }
        let overview = map.searchCamera
        search.tappedDot(0)
        XCTAssertEqual(search.selected?.name, "A")
        XCTAssertEqual(map.selectedDot, 0)
        XCTAssertFalse(search.showsList)
        search.tappedDot(1)
        XCTAssertEqual(search.selected?.name, "B")
        search.back()
        XCTAssertNil(search.selected)
        XCTAssertTrue(search.showsList)
        XCTAssertEqual(map.searchCamera, overview)
        XCTAssertNil(map.selectedDot)
        XCTAssertEqual(search.text, "Hostel")
        XCTAssertEqual(calls.all.count, 1)
    }

    func testBackBringsTheCategoriesBack() async {
        let map = FakeMap()
        let search = controller(map: map)
        search.open = true
        search.chooseCategory("lodging")
        await wait("dots") { map.dots.count == 1 }
        search.tappedDot(0)
        XCTAssertFalse(search.showsCategories)
        search.back()
        XCTAssertTrue(search.showsCategories)
        XCTAssertEqual(search.category, "lodging")
    }

    func testCategoryFitUsesThePanelAfterItsLayout() async {
        let map = FakeMap()
        let search = controller(map: map)
        search.topInset = 60
        search.chooseCategory("lodging")
        await wait("answer") { search.results != nil }
        search.topInset = 280
        await wait("fit") { map.fitted.count == 1 }
        XCTAssertEqual(map.fitInsets, [280])
    }

    func testRouteDoneRemovesTheDots() async {
        let map = FakeMap()
        let search = controller(map: map)
        search.chooseCategory("lodging")
        await wait("dots") { map.dots.count == 1 }
        search.tappedDot(0)
        search.routeDone()
        XCTAssertTrue(map.dots.isEmpty)
        XCTAssertNil(search.category)
        XCTAssertNil(search.selected)
    }

    func testCategoryWithoutHitsSaysToZoomOut() async {
        let map = FakeMap()
        let backend = SearchBackend(
            text: { _, _, _, _ in answer([]) },
            category: { _, _, _, _ in answer([]) })
        let search = SearchController(backend: backend, map: map, typingDelay: 0.01, fitDelay: 0.01)
        search.chooseCategory("fuel")
        await wait("answer") { search.results != nil }
        XCTAssertEqual(search.notes, ["Keine Treffer im Umkreis. Zoome heraus."])
    }

    func testFitKeepsPointsInTheFreePartOfTheScreen() throws {
        let points = [
            CLLocationCoordinate2D(latitude: -13.50, longitude: -72.00),
            CLLocationCoordinate2D(latitude: -13.55, longitude: -71.95),
        ]
        let camera = try XCTUnwrap(SearchFit.camera(
            for: points, width: 400, height: 800, top: 300, left: 40, bottom: 170, right: 40, currentZoom: 10))
        XCTAssertLessThanOrEqual(camera.zoom, SearchFit.maxZoom)
        // The free part lies below the middle of the screen, so the map looks further north.
        XCTAssertGreaterThan(camera.center.latitude, -13.525)
    }

    func testFitOfOnePointKeepsAStreetLevelZoom() throws {
        let point = CLLocationCoordinate2D(latitude: -13.5, longitude: -72)
        let camera = try XCTUnwrap(SearchFit.camera(
            for: [point], width: 400, height: 800, top: 0, left: 0, bottom: 0, right: 0, currentZoom: 16))
        XCTAssertEqual(camera.zoom, 16)
        XCTAssertEqual(camera.center.latitude, -13.5, accuracy: 1e-9)
    }
}
