import CoreLocation
import SwiftUI

struct MapScreen: View {
    @ObservedObject var controller: MapController
    @ObservedObject var plan: PlanController
    @ObservedObject var regions: RegionsController
    @ObservedObject var search: SearchController
    @ObservedObject var routeHere: RouteHereFlow
    @ObservedObject var recording: RecordingController
    @ObservedObject var tracks: TracksController
    @ObservedObject var guide: GuideController
    let start: AppStart.Ready
    @State private var detent = PlanSheet.collapsed

    /// The search takes the room of the toolbar while it shows more than its field.
    private var searchIsOpen: Bool {
        search.open || search.results != nil || search.selected != nil
    }

    var body: some View {
        ZStack(alignment: .top) {
            MapView(controller: controller)
                .ignoresSafeArea()
            VStack(spacing: 8) {
                SearchBar(search: search, flow: routeHere, location: controller.location)
                    .background(GeometryReader { geometry in
                        let bottom = geometry.frame(in: .global).maxY
                        Color.clear
                            .onAppear { search.topInset = Double(bottom) }
                            .onChange(of: bottom) { search.topInset = Double($0) }
                    })
                if !searchIsOpen {
                    MapToolbar(controller: controller)
                }
                // Always there: the sheet may cover the lower half of the map.
                MapModeSwitch(plan: plan)
                if let point = routeHere.point {
                    MapPointCard(flow: routeHere, location: controller.location, point: point)
                }
                LocationBlockedNotice(location: controller.location)
                if let turn = guide.info?.turn {
                    TurnBanner(info: turn)
                }
                GuideBar(guide: guide)
                if let message = guide.message {
                    Notice(text: message, warning: true)
                        .onTapGesture { guide.message = nil }
                }
                if recording.active {
                    RecordingBar(recording: recording)
                    if case .recording = tracks.selectedSection, let section = tracks.recordingSection {
                        RecordingSectionCard(tracks: tracks, section: section)
                    }
                } else if recording.choosingActivity {
                    StartRecordingPanel(recording: recording)
                } else if let track = recording.ended {
                    SaveTrackPanel(recording: recording, track: track)
                        .id(track.id)
                }
                if let notice = recording.notice {
                    Notice(text: notice)
                        .accessibilityIdentifier("recording-notice")
                        .onTapGesture { recording.notice = nil }
                }
                if let message = recording.message {
                    Notice(text: message, warning: true)
                        .onTapGesture { recording.message = nil }
                }
                if let message = tracks.message {
                    Notice(text: message, warning: true)
                        .onTapGesture { tracks.message = nil }
                }
                if let leg = plan.straightLineLeg {
                    StraightLineOffer(plan: plan, leg: leg)
                }
                if regions.installed.isEmpty {
                    NoRegionHint { regions.showing = true }
                }
                ForEach(start.failedImports, id: \.self) { failure in
                    Notice(text: failure, warning: true)
                }
                if let hint = controller.locationHint {
                    Notice(text: hint)
                        .accessibilityIdentifier("location-hint")
                        .onTapGesture { controller.locationHint = nil }
                }
                if let message = controller.message {
                    Notice(text: message, warning: true)
                        .onTapGesture { controller.message = nil }
                }
                if let message = regions.error, !regions.showing {
                    Notice(text: message, warning: true)
                        .onTapGesture { regions.error = nil }
                }
                if let message = plan.message {
                    Notice(text: message, warning: true)
                        .accessibilityIdentifier("plan-message")
                        .onTapGesture { plan.message = nil }
                }
            }
            .padding()
            VStack(alignment: .trailing, spacing: 12) {
                MapSideButtons(
                    recording: recording,
                    record: recording.active || recording.ended != nil || recording.choosingActivity
                        ? nil : { recording.requestStart() })
                LocationButton(accuracy: controller.locationAccuracy, on: controller.locating) {
                    controller.toggleLocation()
                }
            }
                .padding(.trailing, 12)
                // Above the collapsed sheet.
                .padding(.bottom, 108)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
            if start.options.selftest {
                SelfTestStatus(
                    controller: controller, plan: plan, regions: regions, guide: guide, recording: recording,
                    tracks: tracks, start: start)
            }
        }
        // The hits and the chosen one must not end up behind a raised sheet.
        .onChange(of: searchIsOpen) { open in
            if open { detent = PlanSheet.collapsed }
        }
        .onChange(of: search.selected) { selected in
            if selected != nil { detent = PlanSheet.collapsed }
        }
        // Always there, like the sheet of Apple Maps; the map stays usable beside it.
        .sheet(isPresented: .constant(true)) {
            PlanSheet(plan: plan, regions: regions, tracks: tracks, guide: guide, detent: $detent)
                .presentationDetents(PlanSheet.detents, selection: $detent)
                .presentationBackgroundInteraction(.enabled(upThrough: .medium))
                .presentationDragIndicator(.visible)
                .interactiveDismissDisabled()
                .preferredColorScheme(controller.dark ? .dark : .light)
        }
        .preferredColorScheme(controller.dark ? .dark : .light)
    }
}

/// A leg without a route can be joined by a straight line, but only on request.
private struct StraightLineOffer: View {
    @ObservedObject var plan: PlanController
    let leg: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(plan.straightLineText(leg: leg))
                .font(.callout)
                .accessibilityIdentifier("route-straight-text")
            Button("Als gerade Linie verbinden") { plan.connectStraight(leg: leg) }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("route-straight")
        }
        .padding(12)
        .frame(maxWidth: 480, alignment: .leading)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
    }
}

/// Shown while no region is on the iPhone: where to get one.
private struct NoRegionHint: View {
    let open: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            Text("Noch keine Region installiert. Lade eine vom Desktop im Heimnetz.")
                .font(.callout)
                .accessibilityIdentifier("no-region-hint")
            Button("Regionen laden", action: open)
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("regions-hint-open")
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
    }
}

/// Recording or guiding without the permission to locate: nothing would come in.
private struct LocationBlockedNotice: View {
    @ObservedObject var location: LocationService

    var body: some View {
        if location.blocked {
            Notice(text: LocationService.blockedHint, warning: true)
                .accessibilityIdentifier("location-blocked")
        }
    }
}

/// A short text over the map.
struct Notice: View {
    let text: String
    var warning = false
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let label = Text(text)
            .font(.callout)
            .foregroundStyle(warning ? Color.white : Color.primary)
            .padding(12)
            .frame(maxWidth: 480)
        if warning {
            // A deeper red in the dark look keeps white text at 7:1.
            label.background(
                colorScheme == .dark ? Color(red: 0.66, green: 0.06, blue: 0.07) : Color.red.opacity(0.85),
                in: RoundedRectangle(cornerRadius: 12))
        } else {
            label.panelBackground(in: RoundedRectangle(cornerRadius: 12))
        }
    }
}

/// What the UI tests read: an invisible element whose accessibility value is
/// `rendered=<0|1> served=<n> failed=<n> …`, refreshed while the app runs.
private struct SelfTestStatus: View {
    @ObservedObject var controller: MapController
    @ObservedObject var plan: PlanController
    @ObservedObject var regions: RegionsController
    @ObservedObject var guide: GuideController
    @ObservedObject var recording: RecordingController
    @ObservedObject var tracks: TracksController
    let start: AppStart.Ready

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.25)) { _ in
            Color.clear
                .frame(width: 1, height: 1)
                .accessibilityElement()
                .accessibilityIdentifier("map-status")
                .accessibilityLabel("Kartenstatus")
                .accessibilityValue(status)
        }
        .allowsHitTesting(false)
        // Also on the console, to read the measurements from a device:
        // xcrun devicectl device process launch --console --device <id> de.zeibig.zerogrid.trail -- -selftest
        .onChange(of: controller.rendered) { rendered in
            if rendered { print("selftest: \(status)") }
        }
        // And after each computed route, for `plan_ms` when planning by hand.
        .onChange(of: plan.planning) { planning in
            if !planning { print("selftest: \(status)") }
        }
    }

    private var status: String {
        let stats = TileURLProtocol.stats
        var parts = [
            "rendered=\(controller.rendered ? 1 : 0)",
            "served=\(stats.served)",
            "empty=\(stats.empty)",
            "failed=\(stats.failed + controller.loadFailures)",
            "base=\(controller.base.rawValue)",
            "dark=\(controller.dark ? 1 : 0)",
            "relief=\(controller.reliefVisibleOnMap ? 1 : 0)",
            "bounds=\(controller.bounds == nil ? 0 : 1)",
        ]
        parts += regions.status
        let centre = controller.mapView.centerCoordinate
        parts.append(String(format: "zoom=%.2f", controller.mapView.zoomLevel))
        parts.append(String(format: "center=%.5f,%.5f", centre.latitude, centre.longitude))
        if let slope = controller.slopeOpacityOnMap {
            parts.append(String(format: "slope=%.2f", slope))
        }
        if controller.rendered {
            parts.append("contours=\(controller.contoursOnMap)")
        }
        parts.append("import_failed=\(start.failedImports.count)")
        parts.append(String(format: "import_ms=%.0f", start.importSeconds * 1000))
        if let rendered = controller.firstRenderedAfter {
            parts.append(String(format: "rendered_ms=%.0f", rendered * 1000))
        }
        parts += plan.status
        if let info = guide.info {
            parts.append("guide=\(info.phase)")
            parts.append("deviation=\(info.deviation)")
            parts.append("way_back=\(controller.guide.wayBackDrawn ? 1 : 0)")
        } else {
            parts.append("guide=off")
        }
        parts.append("following=\(guide.following ? 1 : 0)")
        parts.append("mode=\(plan.mode.rawValue)")
        parts.append("section=\(tracks.selectedSection?.status ?? "-")")
        let middles = controller.tracks.trackMiddles
        if !middles.isEmpty {
            parts.append("track_section_xy=\(controller.screenPoints(of: middles))")
        }
        if let last = controller.tracks.recording.last(where: { !$0.isEmpty }) {
            parts.append("recording_xy=\(controller.screenPoints(of: [last[last.count / 2]]))")
        }
        if recording.active {
            parts.append("recording_sections=\(recording.sectionCount)")
            parts.append("resting=\(recording.info.resting ? 1 : 0)")
        }
        let waypoints = plan.request.waypoints.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        if let first = waypoints.first {
            parts.append("waypoint_xy=\(controller.screenPoints(of: waypoints))")
            parts.append(String(format: "start=%.5f,%.5f", first.latitude, first.longitude))
        }
        if !start.options.project.isEmpty {
            let places = start.options.project.map { CLLocationCoordinate2D(latitude: $0[0], longitude: $0[1]) }
            parts.append("project=\(controller.screenPoints(of: places))")
        }
        return parts.joined(separator: " ")
    }
}

/// On the map, always: what a tap does. "Wegpunkte setzen" sets waypoints,
/// "Abschnitte bearbeiten" chooses legs of the route and sections of a track.
struct MapModeSwitch: View {
    @ObservedObject var plan: PlanController

    var body: some View {
        HStack(spacing: 0) {
            segment(.waypoints, title: "Wegpunkte", symbol: "mappin.and.ellipse", label: "Wegpunkte setzen")
            segment(.sections, title: "Abschnitte", symbol: "scissors", label: "Abschnitte bearbeiten")
        }
        .frame(maxWidth: 360)
        .panelBackground(in: RoundedRectangle(cornerRadius: 10))
    }

    private func segment(_ mode: MapMode, title: String, symbol: String, label: String) -> some View {
        let selected = plan.mode == mode
        return Button {
            plan.mode = mode
        } label: {
            Label(title, systemImage: symbol)
                .font(.callout.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? Color.white : Color.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .padding(.horizontal, 10)
                .frame(maxWidth: .infinity, minHeight: 40)
                .background(selected ? Color.accentColor : Color.clear, in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier("mode-\(mode.rawValue)")
        .accessibilityLabel(label)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// The chosen section of the recording that runs; it changes in step 2 (notes, photos).
private struct RecordingSectionCard: View {
    @ObservedObject var tracks: TracksController
    let section: TrackSection

    var body: some View {
        HStack(spacing: 8) {
            Text(text)
                .font(.callout)
                .lineLimit(2)
                .minimumScaleFactor(0.8)
                .accessibilityIdentifier("recording-section-text")
            Spacer(minLength: 0)
            Button {
                tracks.select(nil)
            } label: {
                Image(systemName: "xmark")
                    .frame(width: 32, height: 32)
            }
            .accessibilityIdentifier("recording-section-close")
            .accessibilityLabel("Auswahl aufheben")
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("recording-section")
    }

    private var text: String {
        let time = "\(TracksController.time(section.startMs))–\(TracksController.time(section.endMs))"
        if section.rest { return "Rast \(time)" }
        var parts = ["Abschnitt \(section.index + 1)", Activity.name(section.activity), time,
                     Format.distance(section.distanceM), Format.hours(section.movingS)]
        if let ascent = section.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
