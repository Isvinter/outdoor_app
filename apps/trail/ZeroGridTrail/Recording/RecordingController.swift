import Combine
import CoreLocation
import Foundation

/// The activities a track can be saved as, with their German names.
enum Activity {
    /// In the order offered when saving; "other" is "Keine Angabe".
    static let all = ["other", "bike", "mtb", "hike", "ski_tour"]

    static func name(_ activity: String) -> String {
        switch activity {
        case "bike": "Rad"
        case "mtb": "MTB"
        case "hike": "Wandern"
        case "ski_tour": "Skitour"
        case "other": "Keine Angabe"
        default: activity
        }
    }
}

/// The recording of a tour: one button starts it, the activity is chosen
/// when saving. Holds the GPS while it runs and draws its line. Independent
/// of the planned route and the guidance.
@MainActor
final class RecordingController: ObservableObject {
    @Published private(set) var info: RecordingInfo
    /// The track just ended, waiting for "Speichern" or "Verwerfen". It is
    /// stored already, so nothing is lost if the app ends meanwhile.
    @Published private(set) var ended: TrackSummary?
    /// Something worth telling, such as a gap without GPS.
    @Published var notice: String?
    /// A failure worth telling the user, in German.
    @Published var message: String?
    /// A track was saved, renamed or discarded.
    var onTracksChanged: (() -> Void)?

    static let idle = RecordingInfo(
        phase: "idle", trackId: nil, activity: nil, distanceM: 0, movingS: 0, elapsedS: 0, ascentM: nil, appended: [])
    /// Longer than this without a fix is a gap, as in the core (`GAP_MS`).
    static let gap: TimeInterval = 120
    static let interruptedNotice = "Eine Aufzeichnung wurde unterbrochen und als Track gespeichert."

    private let core: CoreApp
    private let location: LocationService
    private let overlay: TrackOverlay?
    private var watches: [AnyCancellable] = []
    /// The time of the last fix during this recording.
    private var lastFix: Date?

    var active: Bool { info.phase != "idle" }
    var paused: Bool { info.phase == "paused" }

    init(
        core: CoreApp, location: LocationService, changes: AnyPublisher<RecordingInfo, Never>,
        overlay: TrackOverlay? = nil
    ) {
        self.core = core
        self.location = location
        self.overlay = overlay
        info = core.recordingStatus()
        watches.append(changes.sink { [weak self] status in self?.changed(status) })
        watches.append(location.$last.compactMap { $0 }.sink { [weak self] fix in self?.received(fix) })
        saveInterrupted()
        if active { follow() }
    }

    /// "2026-11-24 10:05 Rad", without an activity only the time of the start.
    nonisolated static func name(startedMs: Int64, activity: String) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "de_DE")
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        let start = formatter.string(from: Date(timeIntervalSince1970: Double(startedMs) / 1000))
        return activity == "other" || activity.isEmpty ? start : "\(start) \(Activity.name(activity))"
    }

    /// A recording the app was ended during is kept as a track at once.
    private func saveInterrupted() {
        guard (try? core.interruptedRecording()) != nil else { return }
        do {
            if let track = try core.finishInterrupted() {
                try core.renameTrack(id: track.id, name: Self.name(startedMs: track.startedMs, activity: track.activity))
                notice = Self.interruptedNotice
            }
        } catch {
            message = "Die unterbrochene Aufzeichnung konnte nicht gespeichert werden: \(describe(error))"
        }
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

    /// A gap of more than two minutes without GPS is told once the GPS is back.
    private func received(_ fix: CLLocation) {
        guard active else { return }
        if let lastFix, fix.timestamp.timeIntervalSince(lastFix) > Self.gap {
            let minutes = Int((fix.timestamp.timeIntervalSince(lastFix) / 60).rounded())
            notice = "Kein GPS für \(minutes) min. Die Aufzeichnung läuft weiter."
        }
        lastFix = max(lastFix ?? fix.timestamp, fix.timestamp)
    }

    /// Shows the whole line and holds the GPS, for a recording that runs.
    /// The line first: points taken in after it come through the listener.
    private func follow(fresh: Bool = false) {
        if fresh {
            overlay?.clearRecording()
        } else {
            overlay?.setRecording(core.recordingLine())
        }
        lastFix = nil
        location.activity = "other"
        location.require(.recording)
    }

    /// The button on the map: records at once, the activity comes when saving.
    func start() {
        do {
            take(try core.startRecording(activity: "other"))
            ended = nil
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

    /// Ends the recording and asks whether to keep it. A recording without a
    /// point is gone at once.
    func stop() {
        defer {
            location.release(.recording)
            location.activity = ""
            lastFix = nil
        }
        do {
            let track = try core.stopRecording()
            info = Self.idle
            overlay?.clearRecording()
            guard let track else {
                notice = "Es wurde kein Punkt aufgezeichnet."
                return
            }
            // Named at once, so that it has a name even if the question stays unanswered.
            try core.renameTrack(id: track.id, name: Self.name(startedMs: track.startedMs, activity: track.activity))
            ended = track
            onTracksChanged?()
        } catch {
            message = "Die Aufzeichnung konnte nicht beendet werden: \(describe(error))"
            info = core.recordingStatus()
        }
    }

    /// "Speichern": keeps the track with `activity` ("other" for none) and its name.
    func save(activity: String) {
        guard let track = ended else { return }
        do {
            if activity != track.activity {
                _ = try core.setTrackActivity(id: track.id, activity: activity)
            }
            try core.renameTrack(id: track.id, name: Self.name(startedMs: track.startedMs, activity: activity))
            ended = nil
            onTracksChanged?()
        } catch {
            message = "Der Track konnte nicht gespeichert werden: \(describe(error))"
        }
    }

    /// "Verwerfen", after asking: the track is deleted.
    func discard() {
        guard let track = ended else { return }
        do {
            try core.deleteTrack(id: track.id)
            ended = nil
            onTracksChanged?()
        } catch {
            message = "Der Track konnte nicht verworfen werden: \(describe(error))"
        }
    }
}
