import Combine
import CoreLocation
import UIKit

/// Where the map is moved while guiding; the map itself in the app.
@MainActor
protocol GuideCamera: AnyObject {
    /// Centres on the position, keeping the zoom (at least 15).
    func follow(_ location: CLLocation)
    /// Called when the user moves the map by hand.
    var onUserMove: (() -> Void)? { get set }
}

/// The guidance along the planned route: switches it in the core, holds the
/// GPS while it runs, lets the map follow the position, plans the route anew
/// without a word when the user leaves it on the network of ways, and tells
/// once each time they get off the network.
@MainActor
final class GuideController: ObservableObject {
    /// Where the guidance stands; nil while it is off.
    @Published private(set) var info: GuidanceInfo?
    /// The map follows the position. On when the guidance starts; moving the
    /// map ends it, the button in the bar brings it back.
    @Published var following = false {
        didSet {
            if following, let last = location.last { camera?.follow(last) }
        }
    }
    /// Shown for a while after the destination was reached.
    @Published private(set) var arrived = false
    @Published var message: String?
    /// "Diese Route hat keine Abbiegehinweise", for a while after the start
    /// when the route is from before the turns.
    @Published private(set) var noTurnsNotice = false
    /// How long that notice stays.
    var noTurnsSeconds = 8.0
    /// Vibrates at each new turn hint.
    var haptics: Haptics?
    /// Tells the hints as notification and by voice, after its switches.
    var announcer: TurnAnnouncer?
    /// How many waypoints the planned route has; to tell the destination from the others.
    var waypointCount = 0
    /// Before the guidance first runs; to ask for the permission of notifications.
    var onFirstStart: (() -> Void)?
    /// Off the route with a way back found: the route should go on from
    /// `from` to waypoint `toWaypoint` (from 0), in place of the old one.
    var onReroute: ((_ from: LatLon, _ toWaypoint: Int) -> Void)?
    /// Tells the user each time they get off the network of ways.
    var alert: DeviationAlert? {
        didSet {
            alerting = newOffNetwork.sink { [weak self] info in
                guard let self else { return }
                self.alert?.handle(
                    info, appActive: UIApplication.shared.applicationState == .active, waypoints: self.waypointCount)
            }
        }
    }
    private var alerting: AnyCancellable?

    private let core: CoreApp
    private let location: LocationService
    let overlay: GuideOverlay?
    private weak var camera: GuideCamera?
    private let offNetworks = PassthroughSubject<GuidanceInfo, Never>()
    private let turns = PassthroughSubject<TurnInfo, Never>()
    /// The time off the network of ways counted last, so that each is told once.
    private var counted: UInt32 = 0
    /// The turn hint counted last, so that each is told once.
    private var countedTurns: UInt32 = 0
    /// Whether "keine Abbiegehinweise" was told in this guidance.
    private var noTurnsTold = false
    private var noTurnsUntil = 0
    /// The deviation rerouted last, so that each is rerouted once.
    private var rerouted: UInt32 = 0
    /// The deviation whose reroute waits while the user is off the network of ways.
    private var heldOffNetwork: UInt32?
    private var watches: [AnyCancellable] = []
    private var arrivedUntil = 0

    /// Fires once each time the user gets off the network of ways, when `offNetworkSeq` grows.
    var newOffNetwork: AnyPublisher<GuidanceInfo, Never> { offNetworks.eraseToAnyPublisher() }
    /// Fires once per turn hint, when `turnSeq` grows.
    var newTurn: AnyPublisher<TurnInfo, Never> { turns.eraseToAnyPublisher() }
    var active: Bool { info != nil }

    init(
        core: CoreApp, location: LocationService, changes: AnyPublisher<GuidanceInfo?, Never>,
        overlay: GuideOverlay? = nil, camera: GuideCamera? = nil
    ) {
        self.core = core
        self.location = location
        self.overlay = overlay
        self.camera = camera
        watches.append(changes.sink { [weak self] info in self?.changed(info) })
        watches.append(location.$last.compactMap { $0 }.sink { [weak self] last in
            guard let self, self.following, UIApplication.shared.applicationState == .active else { return }
            self.camera?.follow(last)
        })
        camera?.onUserMove = { [weak self] in self?.following = false }
        // Running already, as when the controller is made anew.
        if let running = core.guidanceStatus() { changed(running) }
    }

    /// From the core's listener only, which tells the changes in their order.
    /// The return values of start and stop are not taken: the listener's news
    /// lag behind them, and a status queued before a stop would look like a
    /// new start after it.
    private func changed(_ new: GuidanceInfo?) {
        let was = info
        info = new
        overlay?.show(new)
        guard let new else {
            if was != nil {
                location.release(.guidance)
                following = false
                hideNoTurns()
                announcer?.cleared()
                if was?.phase == "arrived" { showArrived() }
            }
            return
        }
        if was == nil {
            onFirstStart?()
            onFirstStart = nil
            // A fresh guide counts from zero.
            counted = 0
            countedTurns = 0
            rerouted = 0
            heldOffNetwork = nil
            noTurnsTold = false
            arrived = false
            location.require(.guidance)
            following = true
        }
        if new.phase == "arrived" {
            if was?.phase != "arrived" { announcer?.arrived() }
            showArrived()
        }
        // A guide set anew (another start, a changed route) counts from zero again.
        if new.offNetworkSeq < counted { counted = new.offNetworkSeq }
        if new.offNetworkSeq > counted {
            counted = new.offNetworkSeq
            offNetworks.send(new)
            announcer?.offNetwork(new.offNetwork?.spoken ?? DeviationAlert.title)
        }
        // As the deviations: a guide set anew counts from zero again.
        if new.turnSeq < countedTurns { countedTurns = new.turnSeq }
        if new.turnSeq > countedTurns {
            countedTurns = new.turnSeq
            if let turn = new.turn {
                haptics?.turn()
                turns.send(turn)
                announcer?.turn(turn, appActive: UIApplication.shared.applicationState == .active)
            }
        }
        if was?.turn != nil, new.turn == nil { announcer?.cleared() }
        if !new.hasTurns, !noTurnsTold {
            noTurnsTold = true
            showNoTurns()
        }
        // The way back may come later than the deviation (a retry after "no way back"),
        // and the user may reach the network only later. Off it, the route stays.
        if new.deviation < rerouted { rerouted = new.deviation }
        if new.phase != "off_route" { heldOffNetwork = nil }
        if new.offNetwork != nil { heldOffNetwork = new.deviation }
        if new.phase == "off_route", new.offNetwork == nil, new.deviation > rerouted,
            let start = new.wayBack.first, let next = new.nextWaypoint
        {
            rerouted = new.deviation
            // Held while off the network, the way back started where the user
            // was then; they have walked on since and are on a way now.
            var from = start
            if heldOffNetwork == new.deviation, let last = location.last {
                from = LatLon(lat: last.coordinate.latitude, lon: last.coordinate.longitude)
            }
            heldOffNetwork = nil
            onReroute?(from, Int(next))
        }
    }

    private func showArrived() {
        arrived = true
        arrivedUntil += 1
        let mine = arrivedUntil
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: 8_000_000_000)
            guard let self, self.arrivedUntil == mine else { return }
            self.arrived = false
        }
    }

    private func showNoTurns() {
        noTurnsNotice = true
        noTurnsUntil += 1
        let mine = noTurnsUntil
        let nanoseconds = UInt64(noTurnsSeconds * 1_000_000_000)
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: nanoseconds)
            guard let self, self.noTurnsUntil == mine else { return }
            self.noTurnsNotice = false
        }
    }

    private func hideNoTurns() {
        noTurnsUntil += 1
        noTurnsNotice = false
    }

    /// The switch "Führen".
    func toggle() {
        if active {
            core.stopGuidance()
        } else {
            start { try $0.startGuidance() }
        }
    }

    /// Starts the guidance along the planned route, unless it runs.
    func startIfIdle() {
        if !active { start { try $0.startGuidance() } }
    }

    /// "Ab hier führen": the guidance starts at waypoint `index` (from 0).
    func startFromWaypoint(_ index: Int) {
        start { try $0.guideFromWaypoint(index: UInt32(index)) }
    }

    private func start(_ call: (CoreApp) throws -> GuidanceInfo) {
        do {
            // The listener tells about it.
            _ = try call(core)
        } catch {
            message = "Führen geht nicht: \(describe(error))"
        }
    }
}
