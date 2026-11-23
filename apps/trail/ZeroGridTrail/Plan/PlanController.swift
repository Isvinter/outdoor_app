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
    @Published private(set) var profiles: [ProfileChoice]
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
    /// The saved routes, newest first.
    @Published private(set) var routes: [SavedRoute] = []
    /// The saved route that "save" overwrites: the one loaded or saved last.
    @Published private(set) var loaded: SavedRoute?
    /// A short confirmation, e.g. after saving.
    @Published var notice: String?

    /// The routes the desktop offers, as read last; nil before the first reading.
    @Published private(set) var remoteRoutes: [RemoteRouteEntry]?
    /// Why the desktop's routes could not be read.
    @Published private(set) var remoteFailure: String?
    /// No address of the desktop is set yet.
    @Published private(set) var remoteNoSource = false
    /// A route is being taken over.
    @Published private(set) var remoteImporting = false
    /// What became of the route taken over last, as a text.
    @Published var remoteNotice: String?
    /// The same as the core's code; for the self-test.
    private var importOutcome: String?
    /// Counts the readings of the desktop's routes, so that a slow one cannot replace a newer one.
    private var remoteReads = 0

    /// Computations run here one after the other, off the main thread; so do
    /// the other calls that change what the core holds as the planned route.
    private let queue = DispatchQueue(label: "de.zeibig.zerogrid.trail.plan", qos: .userInitiated)
    /// A saved route is being loaded or one of the desktop taken over. Until
    /// that is done the planning is not changed: the core is about to replace
    /// the planned route and the route "save" overwrites, and a change made
    /// meanwhile would end up saved over the wrong route.
    private var replacing = false
    /// Counts the computations, so that a slow answer cannot replace a newer one.
    private let newest = Newest()
    /// From the user's input to the drawn route, per computed plan; for the self-test.
    private(set) var planTimes: [TimeInterval] = []
    /// Asked first about a tap on the map; true means it has been dealt with.
    var interceptTap: ((CLLocationCoordinate2D) -> Bool)?
    /// A tap on the map that sets no waypoint.
    var onFreeTap: ((CLLocationCoordinate2D) -> Void)?

    private static let lastProfileKey = "lastRouteProfile"

    init(core: CoreApp, map: MapController) {
        self.core = core
        overlay = map.route
        let profiles = Self.profiles(of: core)
        self.profiles = profiles
        profile = profiles.first { $0.profile == "hike" }?.available == true ? "hike" : "manual"
        overlay.onTap = { [weak self] at in
            guard let self, self.interceptTap?(at) != true else { return }
            if self.picking {
                self.addWaypoint(at)
            } else {
                self.onFreeTap?(at)
            }
        }
        overlay.onWaypointTap = { [weak self] index in self?.menuWaypoint = index }
        overlay.onWaypointMove = { [weak self] index, to in self?.moveWaypoint(index, to: to) }
        routes = (try? core.savedRoutes()) ?? []
    }

    private static func profiles(of core: CoreApp) -> [ProfileChoice] {
        let known = core.routeProfiles()
        return PlanTexts.profileOrder.compactMap { name in known.first { $0.profile == name } }
    }

    /// The regions changed: other profiles may be computable now, and the
    /// planned route is computed again with the data that is there.
    func regionsChanged() {
        profiles = Self.profiles(of: core)
        // A route that is being loaded is computed with the new data anyway.
        if request.waypoints.count >= 2, !replacing { compute() }
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
        guard !replacing, profile != self.profile else { return }
        self.profile = profile
        options = [:]
        if profile != "manual" { UserDefaults.standard.set(profile, forKey: Self.lastProfileKey) }
        formChanged()
    }

    func set(option key: String, to value: Double) {
        guard !replacing else { return }
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
        guard !replacing else { return }
        request = planAddWaypoint(
            request: request, at: LatLon(lat: at.latitude, lon: at.longitude), leg: formLeg)
        compute()
    }

    private func moveWaypoint(_ index: Int, to: CLLocationCoordinate2D) {
        // The pin goes back to where the route has it.
        guard !replacing else { return drawWaypoints() }
        request = planMoveWaypoint(
            request: request, index: UInt32(index), to: LatLon(lat: to.latitude, lon: to.longitude))
        compute()
    }

    func removeWaypoint(_ index: Int) {
        guard !replacing else { return }
        request = planRemoveWaypoint(request: request, index: UInt32(index))
        compute()
    }

    func toggleStageEnd(_ index: Int) {
        guard !replacing else { return }
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
        guard !replacing else { return }
        picking = false
        request = planEmpty()
        loaded = nil
        let core = core
        queue.async { core.startNewRoute() }
        compute()
    }

    // MARK: Route to a search result

    var isReplacingRoute: Bool { replacing }

    /// A new route from `from` to `to`, in place of the one on the map.
    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) {
        guard !replacing else { return }
        useRouteHereProfile()
        picking = false
        loaded = nil
        let leg = formLeg
        let first = planAddWaypoint(request: planEmpty(), at: LatLon(lat: from.latitude, lon: from.longitude), leg: leg)
        request = planAddWaypoint(request: first, at: LatLon(lat: to.latitude, lon: to.longitude), leg: leg)
        let core = core
        queue.async { core.startNewRoute() }
        compute()
    }

    /// `to` becomes the next destination of the route on the map.
    func appendTarget(_ to: CLLocationCoordinate2D) {
        guard !replacing else { return }
        useRouteHereProfile()
        picking = false
        request = planAddWaypoint(request: request, at: LatLon(lat: to.latitude, lon: to.longitude), leg: formLeg)
        compute()
    }

    /// The form shows the last profile. The open route stays as it is.
    private func useRouteHereProfile() {
        let available = { [profiles] (name: String?) -> Bool in
            name != nil && name != "manual" && profiles.contains { $0.profile == name && $0.available }
        }
        let last = UserDefaults.standard.string(forKey: Self.lastProfileKey)
        guard let chosen = available(last) ? last : available("hike") ? "hike" : nil, chosen != profile else { return }
        profile = chosen
        options = [:]
    }

    /// The first leg that failed in a way a straight line can stand in for.
    var straightLineLeg: Int? {
        guard !planning else { return nil }
        return plan?.legs.firstIndex { leg in leg.errorCode.map { legCanConnectStraight(code: $0) } ?? false }
    }

    func straightLineText(leg: Int) -> String {
        plan?.legs[leg].errorCode == "routing_unavailable"
            ? "Für dieses Stück gibt es keine Routing-Daten."
            : "Keine Route gefunden. Versuche ein anderes Profil, verschiebe den Punkt oder verbinde gerade."
    }

    func connectStraight(leg: Int) {
        guard !replacing, request.legs.indices.contains(leg) else { return }
        request = planSetLeg(request: request, index: UInt32(leg), leg: LegInput(profile: "manual", options: []))
        compute()
    }

    // MARK: Saved routes

    /// What a route is called if the user gives no name, as on the desktop.
    var defaultName: String {
        guard let plan else { return "" }
        return "\(profileNames(plan.legs.map(\.profile))) \(Format.distance(plan.total.distanceM))"
    }

    /// Runs `work` after the computations that are under way, and `done` with
    /// its result and the saved routes on the main thread.
    private func store<T: Sendable>(
        _ work: @escaping @Sendable (CoreApp) throws -> T, done: @escaping @MainActor (T) -> Void,
        failed: @escaping @MainActor () -> Void = {}
    ) {
        let core = core
        queue.async { [weak self] in
            let result = Result { (try work(core), try core.savedRoutes(), core.loadedRouteId()) }
            DispatchQueue.main.async {
                guard let self else { return }
                switch result {
                case .success(let (value, routes, loadedId)):
                    self.routes = routes
                    self.loaded = routes.first { $0.id == loadedId }
                    done(value)
                case .failure(let error):
                    failed()
                    self.message = PlanTexts.error(error)
                }
            }
        }
    }

    /// Stores the planned route: over the route loaded or saved last, or as a
    /// new one (always, with `asNew`).
    func save(name: String, asNew: Bool) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let name = trimmed.isEmpty ? defaultName : trimmed
        store({ core in
            _ = asNew ? try core.saveRouteAsNew(name: name) : try core.saveRoute(name: name)
        }, done: { [weak self] in self?.notice = "Route gespeichert" })
    }

    /// Shows a saved route with its waypoints and settings, ready to be changed.
    func load(_ route: SavedRoute) {
        guard !replacing else { return }
        replacing = true
        planning = true
        // Whatever is still being computed belongs to the route before.
        _ = newest.next()
        store({ core in try core.loadRoute(id: route.id) }, done: { [weak self] plan in
            guard let self else { return }
            self.replacing = false
            self.show(loaded: plan)
            self.onLoaded?()
        }, failed: { [weak self] in
            guard let self else { return }
            self.replacing = false
            // Nothing was loaded: the planning on the screen stays, and its
            // route is computed again, because the answer under way was dropped.
            self.compute()
        })
    }

    /// Puts a route the core has loaded on the screen and the map.
    private func show(loaded plan: PlanResult) {
        planning = false
        picking = false
        message = nil
        self.plan = plan
        request = plan.request
        if let last = plan.request.legs.last {
            profile = last.profile
            options = Dictionary(uniqueKeysWithValues: last.options.map { ($0.key, $0.value) })
        }
        drawWaypoints()
        drawLegs()
        overlay.fit(plan.request.waypoints.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
            + plan.legs.flatMap { $0.line.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) } })
    }

    /// Called when a saved route was loaded, to make room for the map.
    var onLoaded: (() -> Void)?

    func rename(_ route: SavedRoute, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        store({ core in try core.renameRoute(id: route.id, name: name) }, done: { _ in })
    }

    func delete(_ route: SavedRoute) {
        store({ core in try core.deleteRoute(id: route.id) }, done: { _ in })
    }

    // MARK: Routes of the desktop

    /// Reads the routes the desktop offers, off the main thread.
    func readRemoteRoutes() {
        Task { await loadRemoteRoutes() }
    }

    func loadRemoteRoutes() async {
        remoteReads += 1
        let number = remoteReads
        let core = core
        let result = await Task.detached(priority: .userInitiated) { Result { try core.remoteRoutes() } }.value
        guard number == remoteReads else { return }
        remoteNoSource = false
        remoteFailure = nil
        switch result {
        case .success(let routes):
            remoteRoutes = routes
        case .failure(CoreError.Failed("no_catalog", _)):
            remoteRoutes = nil
            remoteNoSource = true
        case .failure(let error):
            remoteRoutes = nil
            remoteFailure = describe(error)
        }
    }

    /// Takes a route of the desktop over. The core decides what that means
    /// (add, replace, restore or copy; a copy also if the loaded route was
    /// planned on and not saved). If it replaced the route that is loaded
    /// here, the core hands the desktop's version back and the screen shows
    /// it, so that "save" cannot write the old planning over it.
    func importRemote(_ route: RemoteRouteEntry) {
        guard !remoteImporting, !replacing else { return }
        remoteImporting = true
        replacing = true
        remoteNotice = nil
        let core = core
        // After the computations under way: the core compares the planned
        // route with the saved one to tell whether it was changed here.
        queue.async { [weak self] in
            let result = Result { (try core.importRoute(id: route.id), try core.savedRoutes(), core.loadedRouteId()) }
            DispatchQueue.main.async {
                guard let self else { return }
                self.remoteImporting = false
                self.replacing = false
                switch result {
                case .success(let (imported, routes, loadedId)):
                    self.routes = routes
                    self.loaded = routes.first { $0.id == loadedId }
                    if let plan = imported.reloaded {
                        // An answer still on its way belongs to the version before.
                        _ = self.newest.next()
                        self.show(loaded: plan)
                    }
                    self.importOutcome = imported.outcome
                    let name = routes.first { $0.id == imported.id }?.name ?? route.name
                    self.remoteNotice = RemoteRouteTexts.outcome(imported.outcome, name: name)
                    self.readRemoteRoutes()
                case .failure(let error):
                    self.remoteNotice = RegionTexts.error(error)
                }
            }
        }
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
        parts += ["routes=\(routes.count)", "route_id=\(loaded?.id ?? "-")", "route_rev=\(loaded?.rev ?? 0)"]
        if let remoteRoutes {
            parts.append("remote_routes=\(remoteRoutes.count)")
            parts.append("remote_states=\(list(remoteRoutes.map(\.state)))")
        }
        if let importOutcome { parts.append("import_outcome=\(importOutcome)") }
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
