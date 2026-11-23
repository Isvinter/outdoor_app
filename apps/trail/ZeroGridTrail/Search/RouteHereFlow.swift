import Combine
import CoreLocation
import Foundation

/// Where "Route hierher" stands.
enum RouteHereStep: Equatable {
    case idle
    case choosing(unsaved: Bool)
    case waiting
    case tapStart(otherMap: Bool)
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
    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D)
    func appendTarget(_ to: CLLocationCoordinate2D)
    func requestPosition(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void)
    func stopRequestingPosition()
    func stopPicking()
}

/// From a search hit to a route: asks if a route is open, waits for the
/// position if needed, or lets the user tap the start.
@MainActor
final class RouteHereFlow: ObservableObject {
    @Published private(set) var step = RouteHereStep.idle
    /// A route to the target is on the map.
    var onDone: (() -> Void)?

    private let host: RouteHereHost
    private let positionWait: Double
    private var target: CLLocationCoordinate2D?
    /// Counts the runs, so that an answer of an ended run does nothing.
    private var run = 0
    private var waitTask: Task<Void, Never>?
    private var requesting = false

    init(host: RouteHereHost, positionWait: Double = 30) {
        self.host = host
        self.positionWait = positionWait
    }

    func begin(target: CLLocationCoordinate2D) {
        guard !host.isReplacingRoute else { return }
        cancel()
        self.target = target
        let number = run
        guard host.hasRoute else {
            startFromPosition(number, location: host.lastPosition)
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
        startFromPosition(run, location: host.lastPosition)
    }

    func chooseAppend() {
        guard case .choosing = step, let target else { return }
        host.appendTarget(target)
        finish()
    }

    func chooseStartMyself() {
        guard step == .waiting else { return }
        askForStart(otherMap: false)
    }

    /// Ends the flow where it stands; later answers and positions do nothing.
    func cancel() {
        run += 1
        stopWaiting()
        step = .idle
        target = nil
    }

    /// A new position while waiting for one.
    func received(_ location: CLLocation) {
        guard step == .waiting else { return }
        startFromPosition(run, location: location)
    }

    /// A tap on the map; true if it was the start of the route.
    func tapped(_ at: CLLocationCoordinate2D) -> Bool {
        guard case .tapStart = step, let target else { return false }
        host.startRoute(from: at, to: target)
        finish()
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
            if case .tapStart = self.step { return }
            switch start {
            case .ready:
                if let location {
                    self.host.startRoute(from: location.coordinate, to: target)
                    self.finish()
                } else {
                    self.askForStart(otherMap: false)
                }
            case .otherMap:
                self.askForStart(otherMap: true)
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
            self.askForStart(otherMap: false)
        }
        requesting = true
        host.requestPosition({ [weak self] location in
            self?.received(location)
        }, denied: { [weak self] in
            guard let self, number == self.run, self.step == .waiting else { return }
            self.askForStart(otherMap: false)
        })
    }

    private func askForStart(otherMap: Bool) {
        stopWaiting()
        host.stopPicking()
        step = .tapStart(otherMap: otherMap)
    }

    private func stopWaiting() {
        waitTask?.cancel()
        waitTask = nil
        if requesting {
            requesting = false
            host.stopRequestingPosition()
        }
    }

    private func finish() {
        cancel()
        onDone?()
    }
}
