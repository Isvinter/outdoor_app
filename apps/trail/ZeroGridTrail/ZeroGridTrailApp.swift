import CoreLocation
import SwiftUI

@main
struct ZeroGridTrailApp: App {
    init() {
        _ = AppStart.launched
    }

    var body: some Scene {
        WindowGroup {
            // The unit tests run inside the app and bring their own core.
            if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
                RootView()
            }
        }
    }
}

private struct RootView: View {
    private enum Phase {
        case starting
        case ready(AppStart.Ready, MapController, PlanController, RegionsController, SearchController, RouteHereFlow)
        case failed(String)
    }

    @State private var phase = Phase.starting

    var body: some View {
        switch phase {
        case .starting:
            ProgressView("Karte wird vorbereitet …")
                .task { await start() }
        case .ready(let start, let controller, let plan, let regions, let search, let routeHere):
            MapScreen(
                controller: controller, plan: plan, regions: regions, search: search, routeHere: routeHere,
                start: start)
        case .failed(let message):
            Text("Die App kann nicht starten: \(message)")
                .padding()
        }
    }

    private func start() async {
        // Importing a region reads all of its files; keep that off the main thread.
        let result = await Task.detached(priority: .userInitiated) {
            Result { try AppStart.run() }
        }.value
        switch result {
        case .success(let start):
            let map = MapController(core: start.host.core, bounds: start.bounds, zoom: start.options.startZoom ?? 11,
                centre: start.options.startCenter)
            let plan = PlanController(core: start.host.core, map: map)
            let regions = RegionsController(
                host: start.host, engine: start.engine, map: map, plan: plan, installed: start.regions)
            let search = SearchController(backend: SearchBackend(core: start.host.core), map: map.searchOverlay)
            let routeHere = RouteHereFlow(host: LiveRouteHereHost(core: start.host.core, plan: plan, map: map))
            search.onRouteHere = { [weak routeHere] hit in
                routeHere?.hitChosen(CLLocationCoordinate2D(latitude: hit.lat, longitude: hit.lon))
            }
            search.onSelectionCleared = { [weak routeHere] in routeHere?.selectionCleared() }
            routeHere.onDone = { [weak search] in search?.routeDone() }
            map.searchOverlay.onOtherMove = { [weak search] in search?.mapMoved() }
            // A tap first answers "where does the route start", then sets a waypoint, then opens a dot.
            plan.interceptTap = { [weak routeHere] at in routeHere?.tapped(at) ?? false }
            plan.onRouteChanged = { [weak routeHere] in routeHere?.routeChanged() }
            plan.onFreeTap = { [weak search, weak map] at in
                if let index = map?.searchOverlay.dot(at: at) { search?.tappedDot(index) }
            }
            phase = .ready(start, map, plan, regions, search, routeHere)
        case .failure(let error):
            phase = .failed(describe(error))
        }
    }
}
