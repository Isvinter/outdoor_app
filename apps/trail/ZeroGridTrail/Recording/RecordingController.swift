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

/// How much the GPS is throttled during a rest ("Drosseln bei Rast").
enum RestThrottle: String, CaseIterable {
    case strong, moderate, off

    var name: String {
        switch self {
        case .strong: "Stark"
        case .moderate: "Mäßig"
        case .off: "Aus"
        }
    }

    /// The GPS during a rest; nil: as always.
    var profile: LocationProfile? {
        switch self {
        case .strong: .restStrong
        case .moderate: .restModerate
        case .off: nil
        }
    }
}

/// The recording of a tour: it starts with an activity, which may change on
/// the way; each activity and each rest is a section of its own. Holds the
/// GPS while it runs, throttles it during a rest and draws the line.
/// Independent of the planned route and the guidance.
@MainActor
final class RecordingController: ObservableObject {
    @Published private(set) var info: RecordingInfo
    /// "Aufnahme starten" asks for the activity.
    @Published private(set) var choosingActivity = false
    /// "Drosseln bei Rast"; also in the bar while recording.
    @Published var throttle: RestThrottle {
        didSet {
            defaults.set(throttle.rawValue, forKey: Self.throttleKey)
            applyThrottle()
        }
    }
    /// "Rast nach", in minutes.
    @Published var restDwellMin: Int {
        didSet {
            defaults.set(restDwellMin, forKey: Self.restDwellKey)
            applyRestDetection()
        }
    }
    /// "Rastumkreis", in metres.
    @Published var restRadiusM: Int {
        didSet {
            defaults.set(restRadiusM, forKey: Self.restRadiusKey)
            applyRestDetection()
        }
    }
    /// The activity the last recording started with; chosen first next time.
    private(set) var lastActivity: String
    /// The track just ended, waiting for "Speichern" or "Verwerfen". It is
    /// stored already, so nothing is lost if the app ends meanwhile.
    @Published private(set) var ended: TrackSummary?
    /// The photos and notes of the track just ended; "Verwerfen" says it deletes them.
    @Published private(set) var endedMediaCount = 0
    /// Something worth telling, such as a gap without GPS.
    @Published var notice: String?
    /// A failure worth telling the user, in German.
    @Published var message: String?
    /// A track was saved, renamed or discarded.
    var onTracksChanged: (() -> Void)?
    /// The recording is about to end: what still belongs to it goes in first.
    var onStopping: (() -> Void)?

    static let idle = RecordingInfo(
        phase: "idle", trackId: nil, activity: nil, resting: false, distanceM: 0, movingS: 0, elapsedS: 0, ascentM: nil,
        appended: [])
    /// Longer than this without a fix is a gap, as in the core (`GAP_MS`).
    static let gap: TimeInterval = 120
    static let interruptedNotice = "Eine Aufzeichnung wurde unterbrochen und als Track gespeichert."
    nonisolated static func gapNotice(minutes: Int) -> String {
        "GPS war \(minutes) min weg. Die Aufzeichnung läuft weiter."
    }
    /// How long the notice of a gap stays.
    static var gapNoticeSeconds: TimeInterval = 15
    static let throttleKey = "restThrottle"
    static let restDwellKey = "restDwellMin"
    static let restRadiusKey = "restRadiusM"
    static let lastActivityKey = "lastRecordingActivity"
    /// The choices of "Rast nach" and "Rastumkreis".
    static let restDwellChoices = [3, 5, 10, 15]
    static let restRadiusChoices = [30, 50, 100]

    /// Back to the settings of a first start; for the UI tests.
    nonisolated static func resetSettings(_ defaults: UserDefaults = .standard) {
        for key in [throttleKey, restDwellKey, restRadiusKey, lastActivityKey] {
            defaults.removeObject(forKey: key)
        }
    }

    private let core: CoreApp
    private let location: LocationService
    private let overlay: TrackOverlay?
    private let defaults: UserDefaults
    private var watches: [AnyCancellable] = []
    /// The time of the last fix during this recording.
    private var lastFix: Date?

    var active: Bool { info.phase != "idle" }
    var paused: Bool { info.phase == "paused" }
    /// The activity of the running section.
    var activity: String { info.activity ?? "other" }
    /// The sections with points so far; for the self-test status.
    var sectionCount: Int { core.recordingLine().filter { !$0.isEmpty }.count }
    /// The photos and notes of the recording that runs; for the self-test status.
    var mediaCount: Int {
        info.trackId.flatMap { try? core.trackDetail(id: $0) }?.media.count ?? 0
    }

    init(
        core: CoreApp, location: LocationService, changes: AnyPublisher<RecordingInfo, Never>,
        overlay: TrackOverlay? = nil, defaults: UserDefaults = .standard
    ) {
        self.core = core
        self.location = location
        self.overlay = overlay
        self.defaults = defaults
        throttle = defaults.string(forKey: Self.throttleKey).flatMap(RestThrottle.init) ?? .strong
        let dwell = defaults.integer(forKey: Self.restDwellKey)
        restDwellMin = Self.restDwellChoices.contains(dwell) ? dwell : 5
        let radius = defaults.integer(forKey: Self.restRadiusKey)
        restRadiusM = Self.restRadiusChoices.contains(radius) ? radius : 50
        lastActivity = defaults.string(forKey: Self.lastActivityKey).flatMap { Activity.all.contains($0) ? $0 : nil }
            ?? "other"
        info = core.recordingStatus()
        // The core does not keep the setting: it is told at every start.
        applyRestDetection()
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
            // The rest begins with the fix that told it, the first point of a new section.
            if status.resting && !info.resting { overlay?.markRecordingRest() }
        }
        var status = status
        status.appended = []
        info = status
        followSection()
    }

    /// The return values of the calls carry the same points as the listener;
    /// only the listener's are drawn.
    private func take(_ status: RecordingInfo) {
        var status = status
        status.appended = []
        info = status
        followSection()
    }

    /// The GPS follows the activity and the rest of the running section.
    private func followSection() {
        guard active else { return }
        if location.activity != activity { location.activity = activity }
        applyThrottle()
    }

    /// Throttles the GPS during a rest; the location service keeps it fine while guiding.
    private func applyThrottle() {
        let profile = active && info.resting ? throttle.profile : nil
        if location.restProfile != profile { location.restProfile = profile }
    }

    private func applyRestDetection() {
        core.setRestDetection(settings: RestSettings(dwellS: UInt32(restDwellMin * 60), radiusM: Double(restRadiusM)))
    }

    /// A gap of more than two minutes without GPS is told once the GPS is back,
    /// for a while: it is over by then.
    private func received(_ fix: CLLocation) {
        guard active else { return }
        if let lastFix, fix.timestamp.timeIntervalSince(lastFix) > Self.gap {
            let minutes = Int((fix.timestamp.timeIntervalSince(lastFix) / 60).rounded())
            let told = Self.gapNotice(minutes: minutes)
            notice = told
            Task { [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(Self.gapNoticeSeconds * 1_000_000_000))
                if self?.notice == told { self?.notice = nil }
            }
        }
        lastFix = max(lastFix ?? fix.timestamp, fix.timestamp)
    }

    /// Shows the whole line and holds the GPS, for a recording that runs.
    /// The line first: points taken in after it come through the listener.
    private func follow(fresh: Bool = false) {
        if fresh {
            overlay?.clearRecording()
        } else {
            let rests = info.trackId.flatMap { try? core.trackDetail(id: $0) }.map(TracksController.rests) ?? []
            overlay?.setRecording(core.recordingLine(), rests: rests)
        }
        lastFix = nil
        location.activity = activity
        applyThrottle()
        location.require(.recording)
    }

    /// The button on the map: asks for the activity first.
    func requestStart() {
        guard !active else { return }
        ended = nil
        choosingActivity = true
    }

    func cancelStart() {
        choosingActivity = false
    }

    /// Records with `activity` ("other" for none).
    func start(activity: String) {
        choosingActivity = false
        do {
            take(try core.startRecording(activity: activity))
            ended = nil
            lastActivity = activity
            defaults.set(activity, forKey: Self.lastActivityKey)
            follow(fresh: true)
        } catch {
            message = "Die Aufzeichnung konnte nicht starten: \(describe(error))"
        }
    }

    /// From the bar: a new section with `activity`.
    func switchActivity(_ activity: String) {
        guard active, activity != self.activity else { return }
        do {
            take(try core.switchRecordingActivity(activity: activity))
        } catch {
            message = "Die Sportart konnte nicht gewechselt werden: \(describe(error))"
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
    /// point and without photos or notes is gone at once.
    func stop() {
        onStopping?()
        defer {
            location.release(.recording)
            location.restProfile = nil
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
            endedMediaCount = (try? core.trackDetail(id: track.id))?.media.count ?? 0
            ended = track
            onTracksChanged?()
        } catch {
            message = "Die Aufzeichnung konnte nicht beendet werden: \(describe(error))"
            info = core.recordingStatus()
        }
    }

    /// "Speichern": keeps the track as it is, named after its first activity.
    func save() {
        guard ended != nil else { return }
        ended = nil
        onTracksChanged?()
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
