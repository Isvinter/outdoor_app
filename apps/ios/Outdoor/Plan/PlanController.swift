import CoreLocation
import Foundation

/// German texts for what the core names by a code.
enum PlanTexts {
    /// Profiles in the order they are offered, as on the desktop.
    static let profileOrder = ["hike", "mtb", "trekking", "road_bike", "car", "manual"]

    static func profile(_ profile: String) -> String {
        switch profile {
        case "hike": return "Wandern"
        case "mtb": return "MTB"
        case "trekking": return "Trekkingrad"
        case "road_bike": return "Rennrad"
        case "car": return "Auto"
        case "manual": return "Skitour (manuell)"
        default: return profile
        }
    }

    static func option(_ key: String) -> String {
        switch key {
        case "use_hills": return "Steigungen (meiden ↔ egal)"
        case "use_roads": return "Straßen (meiden ↔ egal)"
        case "avoid_bad_surfaces": return "Schlechten Belag meiden"
        case "max_hiking_difficulty": return "Schwierigkeit bis (SAC)"
        default: return key
        }
    }

    /// The text for an error code of the core; `fallback` (the core's own
    /// English message) for a code without a translation.
    static func error(code: String, fallback: String) -> String {
        switch code {
        case "route_too_few_waypoints": return "Eine Route braucht Start und Ziel"
        case "route_not_found":
            return "Keine Route gefunden – für dieses Profil gibt es keinen Weg zwischen den Wegpunkten"
        case "routing_unavailable":
            return "Keine Routingdaten: Alle Wegpunkte müssen in einer installierten Region mit Routing liegen"
        case "routing_incompatible":
            return "Die Routingdaten dieser Region passen nicht zu dieser App-Version – bitte die Region neu laden"
        case "routing": return "Die Route kann nicht berechnet werden"
        case "route_plan": return "Die gespeicherte Route kann nicht gelesen werden"
        case "no_route_planned": return "Es ist keine Route geplant"
        case "storage": return "Datenbankfehler"
        default: return fallback
        }
    }

    static func error(_ error: Error) -> String {
        if case CoreError.Failed(let code, let message) = error {
            return self.error(code: code, fallback: message)
        }
        return error.localizedDescription
    }
}

/// The number of the newest computation, readable from the planning queue.
private final class Newest: @unchecked Sendable {
    private let lock = NSLock()
    private var number = 0

    func next() -> Int {
        lock.lock()
        defer { lock.unlock() }
        number += 1
        return number
    }

    var current: Int {
        lock.lock()
        defer { lock.unlock() }
        return number
    }
}

/// The route being planned, as the screen sees it. It holds the request (the
/// waypoints, the legs with their profiles, the stage ends) and the answer of
/// the core. Every change of the request is made by the core (`plan…`
/// functions); this only says which change the user asked for.
@MainActor
final class PlanController: ObservableObject {
    private let core: CoreApp
    private let overlay: RouteOverlay

    /// The profiles of the core, in the order they are offered.
    let profiles: [ProfileChoice]
    @Published private(set) var request = planEmpty()
    /// The answer to `request`, or to an older one while `planning`.
    @Published private(set) var plan: PlanResult?
    /// What the form shows: the profile and settings of the last leg and of new legs.
    @Published private(set) var profile: String
    @Published private(set) var options: [String: Double] = [:]
    /// In plan mode a tap on the map sets a waypoint.
    @Published var picking = false
    /// A computation is running.
    @Published private(set) var planning = false
    /// Why the route or a leg of it could not be computed.
    @Published var message: String?
    /// Where the finger was last on the elevation profile.
    @Published private(set) var cursor: ElevationSample?
    /// The waypoint whose menu is open.
    @Published var menuWaypoint: Int?

    /// Computations run here one after the other, off the main thread; so do
    /// the other calls that change what the core holds as the planned route.
    private let queue = DispatchQueue(label: "de.zeibig.outdoor.plan", qos: .userInitiated)
    /// Counts the computations, so that a slow answer cannot replace a newer one.
    private let newest = Newest()
    /// From the user's input to the drawn route, per computed plan; for the self-test.
    private(set) var planTimes: [TimeInterval] = []

    init(core: CoreApp, map: MapController) {
        self.core = core
        overlay = map.route
        let known = core.routeProfiles()
        profiles = PlanTexts.profileOrder.compactMap { name in known.first { $0.profile == name } }
        profile = profiles.first { $0.profile == "hike" }?.available == true ? "hike" : "manual"
        overlay.onTap = { [weak self] at in
            guard let self, self.picking else { return }
            self.addWaypoint(at)
        }
        overlay.onWaypointTap = { [weak self] index in self?.menuWaypoint = index }
        overlay.onWaypointMove = { [weak self] index, to in self?.moveWaypoint(index, to: to) }
    }

    // MARK: The form

    /// The settings the chosen profile offers.
    var optionSpecs: [OptionInfo] {
        profiles.first { $0.profile == profile }?.options ?? []
    }

    func value(of spec: OptionInfo) -> Double {
        options[spec.key] ?? spec.default
    }

    /// What a new leg gets: the profile and the settings that were changed.
    private var formLeg: LegInput {
        LegInput(
            profile: profile,
            options: optionSpecs.compactMap { spec in options[spec.key].map { OptionValue(key: spec.key, value: $0) } })
    }

    func choose(profile: String) {
        guard profile != self.profile else { return }
        self.profile = profile
        options = [:]
        formChanged()
    }

    func set(option key: String, to value: Double) {
        options[key] = value
        formChanged()
    }

    /// The last leg follows the form.
    private func formChanged() {
        guard !request.legs.isEmpty else { return }
        request = planSetLastLeg(request: request, leg: formLeg)
        compute()
    }

    // MARK: Waypoints

    var canMarkStageEnd: Bool {
        guard let index = menuWaypoint else { return false }
        return index > 0 && index < request.waypoints.count - 1
    }

    func endsStage(_ index: Int) -> Bool {
        request.stageEnds.contains(UInt32(index))
    }

    private func addWaypoint(_ at: CLLocationCoordinate2D) {
        request = planAddWaypoint(
            request: request, at: LatLon(lat: at.latitude, lon: at.longitude), leg: formLeg)
        compute()
    }

    private func moveWaypoint(_ index: Int, to: CLLocationCoordinate2D) {
        request = planMoveWaypoint(
            request: request, index: UInt32(index), to: LatLon(lat: to.latitude, lon: to.longitude))
        compute()
    }

    func removeWaypoint(_ index: Int) {
        request = planRemoveWaypoint(request: request, index: UInt32(index))
        compute()
    }

    func toggleStageEnd(_ index: Int) {
        request = planToggleStageEnd(request: request, index: UInt32(index))
        compute()
    }

    /// Takes the last waypoint away.
    func undo() {
        guard !request.waypoints.isEmpty else { return }
        removeWaypoint(request.waypoints.count - 1)
    }

    /// Starts afresh; the next "save" stores a new route.
    func clear() {
        picking = false
        request = planEmpty()
        let core = core
        queue.async { core.startNewRoute() }
        compute()
    }

    // MARK: Computing

    /// Asks the core for the route through the waypoints and shows it.
    private func compute() {
        let number = newest.next()
        let started = Date()
        drawWaypoints()
        let core = core
        guard request.waypoints.count >= 2 else {
            plan = nil
            planning = false
            message = nil
            drawLegs()
            queue.async { core.clearPlannedRoute() }
            return
        }
        planning = true
        let request = request
        let newest = newest
        queue.async { [weak self] in
            // The user went on meanwhile: only the newest request is worth computing.
            guard newest.current == number else { return }
            let result = Result { try core.planRoute(request: request) }
            DispatchQueue.main.async {
                guard let self, newest.current == number else { return }
                self.computed(result, started: started)
            }
        }
    }

    private func computed(_ result: Result<PlanResult, Error>, started: Date) {
        planning = false
        switch result {
        case .success(let plan):
            self.plan = plan
            if let failed = plan.legs.firstIndex(where: { $0.errorCode != nil }) {
                let leg = plan.legs[failed]
                let reason = PlanTexts.error(code: leg.errorCode ?? "", fallback: leg.errorMessage ?? "")
                message = "Teilstück \(failed + 1): \(reason)"
            } else {
                message = nil
            }
        case .failure(let error):
            plan = nil
            message = PlanTexts.error(error)
        }
        drawLegs()
        planTimes.append(Date().timeIntervalSince(started))
    }

    // MARK: Drawing

    /// Marks a place of the elevation profile on the map.
    func showCursor(_ sample: ElevationSample?) {
        cursor = sample
        overlay.showCursor(sample.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) })
    }

    private func drawWaypoints() {
        overlay.setWaypoints(
            request.waypoints.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) },
            stageEnds: Set(request.stageEnds.map(Int.init)))
    }

    /// A failed leg is a straight line between its waypoints, in the colour for failures.
    private func drawLegs() {
        // The cursor belonged to the route before.
        showCursor(nil)
        guard let plan else { return overlay.show([]) }
        let points = plan.request.waypoints.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        let failedColor = failedLegColor()
        overlay.show(plan.legs.enumerated().map { index, leg in
            if leg.errorCode != nil {
                let ends = index + 1 < points.count ? [points[index], points[index + 1]] : []
                return RouteOverlay.Leg(line: ends, color: failedColor, failed: true)
            }
            return RouteOverlay.Leg(
                line: leg.line.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) },
                color: leg.color, failed: false)
        })
    }

    // MARK: Self-test

    /// What the UI tests read about the planning, as `key=value` pairs.
    var status: [String] {
        let list = { (values: [String]) in values.isEmpty ? "-" : values.joined(separator: ",") }
        let drawn = overlay.drawn
        var parts = [
            "waypoints=\(request.waypoints.count)",
            "legs=\(request.legs.count)",
            "leg_profiles=\(list(request.legs.map(\.profile)))",
            "stage_ends=\(list(request.stageEnds.map(String.init)))",
            "planning=\(planning ? 1 : 0)",
            "picking=\(picking ? 1 : 0)",
            "failed_legs=\(plan?.total.failedLegs ?? 0)",
            String(format: "distance_m=%.0f", plan?.total.distanceM ?? 0),
            "route_colors=\(list(drawn.colors))",
            "dashed=\(drawn.dashed)",
        ]
        if let cursor {
            parts.append(String(format: "cursor=%.5f,%.5f", cursor.lat, cursor.lon))
        }
        if !planTimes.isEmpty {
            let recent = planTimes.suffix(10).sorted()
            parts.append(String(format: "plan_ms=%.0f", recent[recent.count / 2] * 1000))
            parts.append("plans=\(planTimes.count)")
        }
        return parts
    }
}
