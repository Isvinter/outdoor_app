import Combine
import Foundation

/// The activities a tour is recorded as, with their German names.
enum Activity {
    static let all = ["bike", "mtb", "hike", "ski_tour"]

    static func name(_ activity: String) -> String {
        switch activity {
        case "bike": "Rad"
        case "mtb": "MTB"
        case "hike": "Wandern"
        case "ski_tour": "Skitour"
        default: activity
        }
    }
}

/// The recording of a tour: starts and ends it in the core, holds the GPS
/// while it runs, and draws its line.
@MainActor
final class RecordingController: ObservableObject {
    @Published private(set) var info: RecordingInfo
    /// A recording the app was ended during; offered to go on or to keep as a track.
    @Published private(set) var interrupted: TrackSummary?
    /// A failure worth telling the user, in German.
    @Published var message: String?

    static let lastActivityKey = "lastActivity"
    static let idle = RecordingInfo(
        phase: "idle", trackId: nil, activity: nil, distanceM: 0, movingS: 0, elapsedS: 0, ascentM: nil, appended: [])

    private let core: CoreApp
    private let location: LocationService
    private let defaults: UserDefaults
    private let overlay: TrackOverlay?
    private var watch: AnyCancellable?

    var active: Bool { info.phase != "idle" }
    var paused: Bool { info.phase == "paused" }

    /// The activity chosen the last time; "bike" before the first recording.
    var lastActivity: String {
        let stored = defaults.string(forKey: Self.lastActivityKey) ?? ""
        return Activity.all.contains(stored) ? stored : "bike"
    }

    init(
        core: CoreApp, location: LocationService, changes: AnyPublisher<RecordingInfo, Never>,
        overlay: TrackOverlay? = nil, defaults: UserDefaults = .standard
    ) {
        self.core = core
        self.location = location
        self.overlay = overlay
        self.defaults = defaults
        info = core.recordingStatus()
        interrupted = try? core.interruptedRecording()
        watch = changes.sink { [weak self] status in self?.changed(status) }
        if active { follow() }
    }

    /// From the core's listener: the state, and the points since the last change.
    private func changed(_ status: RecordingInfo) {
        // Late news of a recording that has ended, or of the time before this one began.
        guard status.trackId == info.trackId else { return }
        // The last points come with the end, after the line was taken away.
        if status.phase != "idle" {
            overlay?.append(status.appended)
        }
        var status = status
        status.appended = []
        info = status
    }

    /// The return values of the calls carry the same points as the listener;
    /// only the listener's are drawn.
    private func take(_ status: RecordingInfo) {
        var status = status
        status.appended = []
        info = status
    }

    /// Shows the whole line and holds the GPS, for a recording that runs.
    /// The line first: points taken in after it come through the listener.
    private func follow(fresh: Bool = false) {
        if fresh {
            overlay?.clearRecording()
        } else {
            overlay?.setRecording(core.recordingLine())
        }
        location.activity = info.activity ?? ""
        location.require(.recording)
    }

    func start(activity: String) {
        do {
            defaults.set(activity, forKey: Self.lastActivityKey)
            // Before the GPS: the first fixes already count.
            take(try core.startRecording(activity: activity))
            interrupted = nil
            follow(fresh: true)
        } catch {
            message = "Die Aufzeichnung konnte nicht starten: \(describe(error))"
        }
    }

    func pause() {
        do {
            take(try core.pauseRecording())
        } catch {
            message = describe(error)
        }
    }

    func resume() {
        do {
            take(try core.resumeRecording())
        } catch {
            message = describe(error)
        }
    }

    /// Ends the recording; nil if nothing was recorded.
    @discardableResult
    func stop() -> TrackSummary? {
        defer {
            location.release(.recording)
            location.activity = ""
        }
        do {
            let track = try core.stopRecording()
            info = Self.idle
            overlay?.clearRecording()
            return track
        } catch {
            message = "Die Aufzeichnung konnte nicht beendet werden: \(describe(error))"
            info = core.recordingStatus()
            return nil
        }
    }

    func continueInterrupted() {
        do {
            take(try core.continueInterrupted())
            interrupted = nil
            follow()
        } catch {
            message = "Die Aufzeichnung konnte nicht fortgesetzt werden: \(describe(error))"
        }
    }

    func finishInterrupted() {
        do {
            _ = try core.finishInterrupted()
            interrupted = nil
        } catch {
            message = "Der Track konnte nicht gespeichert werden: \(describe(error))"
        }
    }
}
