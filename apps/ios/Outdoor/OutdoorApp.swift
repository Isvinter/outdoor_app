import SwiftUI

@main
struct OutdoorApp: App {
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
        case ready(AppStart.Ready, MapController, PlanController, RegionsController)
        case failed(String)
    }

    @State private var phase = Phase.starting

    var body: some View {
        switch phase {
        case .starting:
            ProgressView("Karte wird vorbereitet …")
                .task { await start() }
        case .ready(let start, let controller, let plan, let regions):
            MapScreen(controller: controller, plan: plan, regions: regions, start: start)
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
            phase = .ready(start, map, plan, regions)
        case .failure(let error):
            phase = .failed(describe(error))
        }
    }
}
