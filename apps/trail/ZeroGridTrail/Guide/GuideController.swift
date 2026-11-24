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
/// GPS while it runs, lets the map follow the position, and tells about each
/// new deviation once.
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
    /// How many waypoints the planned route has; to tell the destination from the others.
    var waypointCount = 0
    /// Before the guidance first runs, also when a recording starts it; to
    /// ask for the permission of notifications.
    var onFirstStart: (() -> Void)?
    /// Tells the user about each new deviation.
    var alert: DeviationAlert? {
        didSet {
            alerting = newDeviation.sink { [weak self] info in
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
    private let deviations = PassthroughSubject<GuidanceInfo, Never>()
    /// The deviation counted last, so that each is told once.
    private var counted: UInt32 = 0
    private var watches: [AnyCancellable] = []
    private var arrivedUntil = 0

    /// Fires once per deviation, when `deviation` grows.
    var newDeviation: AnyPublisher<GuidanceInfo, Never> { deviations.eraseToAnyPublisher() }
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
        // Running already: guidance during a recording that went on after a restart.
        if let running = core.guidanceStatus() { changed(running) }
    }

    /// From the core, and from the calls that start it.
    private func changed(_ new: GuidanceInfo?) {
        let was = info
        info = new
        overlay?.show(new)
        guard let new else {
            if was != nil {
                location.release(.guidance)
                following = false
                if was?.phase == "arrived" { showArrived() }
            }
            return
        }
        if was == nil {
            onFirstStart?()
            onFirstStart = nil
            // A fresh guide counts from zero.
            counted = 0
            arrived = false
            location.require(.guidance)
            following = true
        }
        if new.phase == "arrived" { showArrived() }
        // A guide set anew (another start, a changed route) counts from zero again.
        if new.deviation < counted { counted = new.deviation }
        if new.deviation > counted {
            counted = new.deviation
            deviations.send(new)
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

    /// The switch "Führen".
    func toggle() {
        if active {
            core.stopGuidance()
            changed(nil)
        } else {
            start { try $0.startGuidance() }
        }
    }

    /// "Ab hier führen": the guidance starts at waypoint `index` (from 0).
    func startFromWaypoint(_ index: Int) {
        start { try $0.guideFromWaypoint(index: UInt32(index)) }
    }

    private func start(_ call: (CoreApp) throws -> GuidanceInfo) {
        do {
            // The listener tells the same a moment later.
            changed(try call(core))
        } catch {
            message = "Führen geht nicht: \(describe(error))"
        }
    }
}
