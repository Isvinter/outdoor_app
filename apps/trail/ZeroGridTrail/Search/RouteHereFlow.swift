import Combine
import CoreLocation
import Foundation

/// Where "Route hierher" stands.
enum RouteHereStep: Equatable {
    case idle
    case choosing(unsaved: Bool)
    case waiting
    /// The user chooses the start: a tap on the map, a search hit or, if
    /// offered, the own position.
    case pickStart(otherMap: Bool, offersPosition: Bool)

    var picksStart: Bool {
        if case .pickStart = self { return true }
        return false
    }
}

/// What "Route hierher" needs from the planning, the core and the position.
@MainActor
protocol RouteHereHost: AnyObject {
    var hasRoute: Bool { get }
    /// A saved route is being loaded or taken over; nothing may change it now.
    var isReplacingRoute: Bool { get }
    var lastPosition: CLLocation? { get }
    func routeIsUnsaved() async -> Bool
    func routeStart(position: KnownPosition?, target: CLLocationCoordinate2D) async -> RouteStart
    /// False if the planning cannot take the route now.
    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D) -> Bool
    func appendTarget(_ to: CLLocationCoordinate2D) -> Bool
    func requestPosition(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void)
    func stopRequestingPosition()
    func stopPicking()
}

/// From a search hit to a route: asks if a route is open, then lets the user
/// choose the start: the own position (waiting for it if needed), a tap on the
/// map or another search hit.
@MainActor
final class RouteHereFlow: ObservableObject {
    @Published private(set) var step = RouteHereStep.idle
    /// A place tapped on the map, offered as a target while the position is known.
    @Published private(set) var point: CLLocationCoordinate2D?
    /// A route to the target is on the map.
    var onDone: (() -> Void)?
    /// The route was asked for by "Hierher führen": the guidance should start once it is computed.
    var onGuide: (() -> Void)?
    /// The tapped place to mark on the map, or nil to take the mark away.
    var onPoint: ((CLLocationCoordinate2D?) -> Void)?

    private let host: RouteHereHost
    private let positionWait: Double
    private var target: CLLocationCoordinate2D?
    /// Counts the runs, so that an answer of an ended run does nothing.
    private var run = 0
    private var waitTask: Task<Void, Never>?
    private var requesting = false
    /// "Hierher führen": the route starts at the own position without asking, and the guidance follows.
    private var guiding = false

    init(host: RouteHereHost, positionWait: Double = 30) {
        self.host = host
        self.positionWait = positionWait
    }

    func begin(target: CLLocationCoordinate2D, guide: Bool = false) {
        guard !host.isReplacingRoute else { return }
        dismissPoint()
        cancel()
        self.target = target
        guiding = guide
        let number = run
        guard host.hasRoute else {
            if guide {
                startFromPosition(number, location: host.lastPosition)
            } else {
                askForStart(otherMap: false, offersPosition: true)
            }
            return
        }
        Task { [weak self] in
            guard let self else { return }
            let unsaved = await self.host.routeIsUnsaved()
            guard number == self.run else { return }
            self.step = .choosing(unsaved: unsaved)
        }
    }

    func chooseNewRoute() {
        guard case .choosing = step else { return }
        if guiding {
            startFromPosition(run, location: host.lastPosition)
        } else {
            askForStart(otherMap: false, offersPosition: true)
        }
    }

    /// A tap on the map, away from search hits, while the own position is known.
    func offerPoint(_ at: CLLocationCoordinate2D) {
        guard step == .idle else { return }
        point = at
        onPoint?(at)
    }

    func dismissPoint() {
        guard point != nil else { return }
        point = nil
        onPoint?(nil)
    }

    /// "Route hierher" on the tapped place: as for a search hit.
    func routeToPoint() {
        guard let point else { return }
        begin(target: point)
    }

    /// "Hierher führen": a route from the own position, then the guidance.
    func guideToPoint() {
        guard let point else { return }
        begin(target: point, guide: true)
    }

    /// The own position as the start; waits for it if it is not known yet.
    func chooseMyPosition() {
        guard case .pickStart(_, true) = step else { return }
        startFromPosition(run, location: host.lastPosition)
    }

    /// Is this the target of the route being set up?
    func isTarget(_ at: CLLocationCoordinate2D) -> Bool {
        guard let target else { return false }
        return target.latitude == at.latitude && target.longitude == at.longitude
    }

    /// "Route hierher" or, while the start is chosen, "Hier starten" on a search hit.
    func hitChosen(_ at: CLLocationCoordinate2D) {
        if step.picksStart {
            if let target, !isTarget(at) { finish { host.startRoute(from: at, to: target) } }
        } else {
            begin(target: at)
        }
    }

    /// The chosen search hit is gone. While the start is chosen the search
    /// looks for the start, so the flow goes on.
    func selectionCleared() {
        if !step.picksStart { cancel() }
    }

    func chooseAppend() {
        guard case .choosing = step, let target else { return }
        finish { host.appendTarget(target) }
    }

    func chooseStartMyself() {
        guard step == .waiting else { return }
        askForStart(otherMap: false, offersPosition: true)
    }

    /// Ends the flow where it stands; later answers and positions do nothing.
    func cancel() {
        run += 1
        stopWaiting()
        step = .idle
        target = nil
        guiding = false
    }

    /// The route on the map changed: what was asked or waited for no longer holds.
    func routeChanged() {
        if target != nil { cancel() }
    }

    /// A new position while waiting for one.
    func received(_ location: CLLocation) {
        guard step == .waiting else { return }
        startFromPosition(run, location: location)
    }

    /// A tap on the map; true if it was the start of the route.
    func tapped(_ at: CLLocationCoordinate2D) -> Bool {
        guard step.picksStart, let target else { return false }
        finish { host.startRoute(from: at, to: target) }
        return true
    }

    private func startFromPosition(_ number: Int, location: CLLocation?) {
        guard let target else { return }
        let known = location.map {
            KnownPosition(
                lat: $0.coordinate.latitude, lon: $0.coordinate.longitude,
                ageS: max(0, -$0.timestamp.timeIntervalSinceNow))
        }
        Task { [weak self] in
            guard let self else { return }
            let start = await self.host.routeStart(position: known, target: target)
            // The user chose the start meanwhile: a position no longer decides it.
            guard number == self.run else { return }
            switch start {
            case .ready:
                if let location {
                    self.finish { self.host.startRoute(from: location.coordinate, to: target) }
                } else {
                    self.askForStart(otherMap: false, offersPosition: false)
                }
            case .otherMap:
                self.askForStart(otherMap: true, offersPosition: false)
            case .noPosition:
                self.waitForPosition(number)
            }
        }
    }

    private func waitForPosition(_ number: Int) {
        guard step != .waiting else { return }
        step = .waiting
        let wait = positionWait
        waitTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(wait * 1_000_000_000))
            guard !Task.isCancelled, let self, number == self.run, self.step == .waiting else { return }
            self.askForStart(otherMap: false, offersPosition: true)
        }
        requesting = true
        host.requestPosition({ [weak self] location in
            self?.received(location)
        }, denied: { [weak self] in
            guard let self, number == self.run, self.step == .waiting else { return }
            self.askForStart(otherMap: false, offersPosition: false)
        })
    }

    /// A new run: an answer about the position asked for before does nothing.
    private func askForStart(otherMap: Bool, offersPosition: Bool) {
        run += 1
        stopWaiting()
        host.stopPicking()
        step = .pickStart(otherMap: otherMap, offersPosition: offersPosition)
    }

    private func stopWaiting() {
        waitTask?.cancel()
        waitTask = nil
        if requesting {
            requesting = false
            host.stopRequestingPosition()
        }
    }

    /// `make` puts the route on the map. Changing the route ends the flow
    /// (`routeChanged`), so what the flow was for is read before.
    private func finish(_ make: () -> Bool) {
        let guide = guiding
        let made = make()
        cancel()
        guard made else { return }
        onDone?()
        if guide { onGuide?() }
    }
}
