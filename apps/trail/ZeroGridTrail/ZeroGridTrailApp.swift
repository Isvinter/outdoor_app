import CoreLocation
import SwiftUI
import UserNotifications

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
        case ready(
            AppStart.Ready, MapController, PlanController, RegionsController, SearchController, RouteHereFlow,
            RecordingController, TracksController, GuideController)
        case failed(String)
    }

    @State private var phase = Phase.starting

    var body: some View {
        switch phase {
        case .starting:
            ProgressView("Karte wird vorbereitet …")
                .task { await start() }
        case .ready(let start, let controller, let plan, let regions, let search, let routeHere, let recording,
            let tracks, let guide):
            MapScreen(
                controller: controller, plan: plan, regions: regions, search: search, routeHere: routeHere,
                recording: recording, tracks: tracks, guide: guide, start: start)
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
            var replay: GPXReplay?
            if let path = start.options.replayGPX {
                replay = try? GPXReplay(contentsOf: URL(fileURLWithPath: path))
            }
            let location = LocationService(core: start.host.core, manager: replay)
            let map = MapController(
                core: start.host.core, location: location, bounds: start.bounds, zoom: start.options.startZoom ?? 11,
                centre: start.options.startCenter)
            let plan = PlanController(core: start.host.core, map: map)
            plan.failPlansAfter = start.options.failPlansAfter
            let regions = RegionsController(
                host: start.host, engine: start.engine, map: map, plan: plan, installed: start.regions)
            let search = SearchController(backend: SearchBackend(core: start.host.core), map: map.searchOverlay)
            let routeHere = RouteHereFlow(
                host: LiveRouteHereHost(core: start.host.core, plan: plan, location: location))
            // A hit chosen or left: the card of a tapped place goes.
            search.onSelectionCleared = { [weak routeHere] in routeHere?.dismiss() }
            routeHere.onDone = { [weak search] in search?.routeDone() }
            map.searchOverlay.onOtherMove = { [weak search] in search?.mapMoved() }
            // A tap first answers "where does the route start", then sets a waypoint, then opens a dot.
            // A tap on a search hit opens it; elsewhere it offers a route to the place, as a hit does.
            plan.onFreeTap = { [weak search, weak map, weak routeHere] at in
                if let index = map?.searchOverlay.dot(at: at) {
                    search?.tappedDot(index)
                } else if search?.selected == nil {
                    routeHere?.offer(at)
                }
            }
            // The first point of a route set with "+ Punkt": the way there from the own position is offered.
            plan.onPickedTap = { [weak routeHere, weak location] at, first in
                if first && location?.knowsPosition == true {
                    routeHere?.offer(at, placed: true)
                } else {
                    routeHere?.dismiss()
                }
            }
            routeHere.onPoint = { [weak map] at in map?.route.showCursor(at) }
            let recording = RecordingController(
                core: start.host.core, location: location, changes: start.host.recordingChanges, overlay: map.tracks)
            let tracks = TracksController(
                core: start.host.core, overlay: map.tracks, recordingChanges: start.host.recordingChanges)
            let guide = GuideController(
                core: start.host.core, location: location, changes: start.host.guidanceChanges, overlay: map.guide,
                camera: map)
            tracks.onOpen = { [weak guide] in guide?.following = false }
            // The cards offer "anhängen" only with a route: they follow it.
            plan.onRouteChanged = { [weak routeHere] in routeHere?.objectWillChange.send() }
            // Off the route: the route is planned anew from there, the old one goes.
            guide.onReroute = { [weak plan] from, next in plan?.reroute(from: from, toWaypoint: next) }
            routeHere.onGuide = { [weak plan, weak guide] in
                plan?.whenPlanned { guide?.startIfIdle() }
            }
            recording.onTracksChanged = { [weak tracks] in tracks?.refresh() }
            let haptics = SystemHaptics()
            let alert = DeviationAlert(center: UNUserNotificationCenter.current(), haptics: haptics)
            guide.haptics = haptics
            guide.announcer = TurnAnnouncer(center: UNUserNotificationCenter.current(), speaker: SystemSpeaker())
            guide.onFirstStart = { alert.requestPermission() }
            guide.alert = alert
            plan.onRouteDrawn = { [weak guide, weak map] legs, waypoints in
                map?.guide.setRoute(legs)
                guide?.waypointCount = waypoints
            }
            if start.options.route.count == 2 {
                let ends = start.options.route.map { CLLocationCoordinate2D(latitude: $0[0], longitude: $0[1]) }
                _ = plan.startRoute(from: ends[0], to: ends[1])
            }
            phase = .ready(start, map, plan, regions, search, routeHere, recording, tracks, guide)
        case .failure(let error):
            phase = .failed(describe(error))
        }
    }
}
