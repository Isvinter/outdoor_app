import CoreLocation
import SwiftUI
import UserNotifications

@main
struct ZeroGridTrailApp: App {
    init() {
        _ = AppStart.launched
        // Packages left over when the app ended while sharing one.
        TracksController.removeLeftoverPackages()
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
            RecordingController, TracksController, GuideController, MediaController)
        case failed(String)
    }

    @State private var phase = Phase.starting

    var body: some View {
        switch phase {
        case .starting:
            ProgressView("Karte wird vorbereitet …")
                .task { await start() }
        case .ready(let start, let controller, let plan, let regions, let search, let routeHere, let recording,
            let tracks, let guide, let media):
            MapScreen(
                controller: controller, plan: plan, regions: regions, search: search, routeHere: routeHere,
                recording: recording, tracks: tracks, guide: guide, media: media, start: start)
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
                replay = try? GPXReplay(contentsOf: URL(fileURLWithPath: path), speedUp: start.options.replaySpeed ?? 10)
            }
            let location = LocationService(core: start.host.core, manager: replay)
            let map = MapController(
                core: start.host.core, location: location, bounds: start.bounds, zoom: start.options.startZoom ?? 11,
                centre: start.options.startCenter)
            let plan = PlanController(core: start.host.core, map: map)
            plan.failPlansAfter = start.options.failPlansAfter
            let regions = RegionsController(
                host: start.host, engine: start.engine, map: map, plan: plan, installed: start.regions)
            let search = SearchController(
                backend: SearchBackend(core: start.host.core), map: map.searchOverlay,
                routeFrom: { [weak location, weak plan] in
                    guard let location, let plan else { return nil }
                    return RouteFrom.live(location: location, plan: plan)
                })
            let routeHere = RouteHereFlow(
                host: LiveRouteHereHost(core: start.host.core, plan: plan, location: location))
            // A hit chosen or left: the card of a tapped place goes.
            search.onSelectionCleared = { [weak routeHere] in routeHere?.dismiss() }
            routeHere.onDone = { [weak search] in search?.routeDone() }
            map.searchOverlay.onOtherMove = { [weak search] in search?.mapMoved() }
            // A tap on a search hit opens it, in both modes; elsewhere the mode decides.
            plan.onMapTap = { [weak search, weak map] at in
                guard let index = map?.searchOverlay.dot(at: at) else { return false }
                search?.tappedDot(index)
                return true
            }
            // The first waypoint of a route: the way there from the own position is offered.
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
            // "Abschnitte bearbeiten": taps on the lines choose legs of the route and sections of the tracks.
            map.route.sectionHit = { [weak map] point in map?.tracks.hit(at: point) }
            map.route.onSectionTap = { [weak tracks] section in tracks?.select(section) }
            map.route.onAmbiguousTap = { [weak plan] leg, section in
                plan?.ambiguousTap = .init(leg: leg, section: section)
            }
            plan.onModeChanged = { [weak tracks] mode in tracks?.setEditing(mode == .sections) }
            plan.onFreeTap = { [weak tracks] in tracks?.select(nil) }
            plan.onLegChosen = { [weak tracks] in
                if tracks?.selectedSection != nil { tracks?.select(nil) }
            }
            tracks.onWantsEditing = { [weak plan] in plan?.mode = .sections }
            tracks.onSectionChosen = { [weak plan] in plan?.selectLeg(nil) }
            // The cards offer "anhängen" only with a route: they follow it.
            plan.onRouteChanged = { [weak routeHere] in routeHere?.objectWillChange.send() }
            // Off the route: the route is planned anew from there, the old one goes.
            guide.onReroute = { [weak plan] from, next in plan?.reroute(from: from, toWaypoint: next) }
            routeHere.onGuide = { [weak plan, weak guide] in
                plan?.whenPlanned { guide?.startIfIdle() }
            }
            recording.onTracksChanged = { [weak tracks] in tracks?.refresh() }
            let fakes = start.options.mediaFakes
            let media = MediaController(
                core: start.host.core, camera: fakes ? FakeCamera() : SystemCamera(),
                audio: fakes ? FakeAudio() : SystemAudio(), library: fakes ? FakeLibrary() : SystemLibrary(),
                picker: fakes ? FakeImagePicker() : SystemImagePicker(),
                permissions: fakes ? AllowingPermissions() : SystemPermissions(),
                lastFix: { [weak location] in location?.last },
                isRecording: { [weak recording] in recording?.active == true })
            // A voice note still spoken belongs to the recording that ends.
            recording.onStopping = { [weak media] in media?.endVoice() }
            // The card of a chosen section of the recording shows a new photo or note at once.
            media.onAdded = { [weak tracks, weak recording] _ in
                if let id = recording?.info.trackId { tracks?.mediaChanged(track: id) }
            }
            if fakes {
                tracks.transcriber = FakeTranscriber()
                tracks.permissions = AllowingPermissions()
            }
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
            phase = .ready(start, map, plan, regions, search, routeHere, recording, tracks, guide, media)
        case .failure(let error):
            phase = .failed(describe(error))
        }
    }
}
