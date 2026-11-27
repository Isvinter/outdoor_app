import Combine
import SwiftUI

/// The recorded tracks: the list, and the one being looked at, which the map shows.
@MainActor
final class TracksController: ObservableObject {
    @Published private(set) var tracks: [TrackSummary] = []
    /// The track being looked at, with the GPX file to share.
    @Published private(set) var selected: TrackView?
    @Published private(set) var shareFile: URL?
    @Published var message: String?
    /// The saved lists show the tracks rather than the routes.
    @Published var listShown = false
    /// "Abschnitte bearbeiten" is on: sections can be chosen and changed.
    @Published private(set) var editing = false
    /// The section chosen on the map or in the list.
    @Published private(set) var selectedSection: SectionRef?
    /// The chosen section of the recording that runs, as the core has it now.
    @Published private(set) var recordingSection: TrackSection?
    /// The recording that runs, while one of its sections is chosen: its photos and notes.
    @Published private(set) var recordingView: TrackView?
    /// Voice notes being turned into text.
    @Published private(set) var transcribing: Set<String> = []
    /// A photo or note to delete, once the user agrees.
    @Published var confirmDelete: MediaItem?
    /// The speech recognition was refused: the hint says where to allow it.
    @Published var blocked: MediaPermission?
    /// Turns voice notes into text, on the device only.
    var transcriber: Transcribing = SystemTranscriber()
    var permissions: PermissionAsking = SystemPermissions()
    static let noOnDeviceMessage = "Umwandeln geht für Deutsch auf diesem iPhone nur mit Netz. Das Audio bleibt."
    /// A row was tapped outside "Abschnitte bearbeiten": the mode is to change.
    var onWantsEditing: (() -> Void)?
    /// A section was chosen: a chosen leg of the route goes.
    var onSectionChosen: (() -> Void)?
    /// A track is opened; the map is fitted to it.
    var onOpen: (() -> Void)?

    private let core: CoreApp
    private let overlay: TrackOverlay?
    private var watch: AnyCancellable?

    init(core: CoreApp, overlay: TrackOverlay?, recordingChanges: AnyPublisher<RecordingInfo, Never>) {
        self.core = core
        self.overlay = overlay
        // A finished recording is a new track; its sections are no longer chosen.
        watch = recordingChanges.map(\.phase).removeDuplicates().filter { $0 == "idle" }.sink { [weak self] _ in
            guard let self else { return }
            if case .recording = self.selectedSection { self.select(nil) }
            self.refresh()
        }
    }

    func refresh() {
        do {
            tracks = try core.listTracks().sorted { $0.startedMs > $1.startedMs }
        } catch {
            message = "Die Tracks konnten nicht gelesen werden: \(describe(error))"
        }
    }

    func open(_ track: TrackSummary) {
        do {
            let view = try core.trackDetail(id: track.id)
            selected = view
            onOpen?()
            overlay?.show(track: view.segments, rests: Self.rests(view))
            if case .track = selectedSection { selectedSection = nil }
            shareFile = try writeGPX(view.summary)
        } catch {
            message = "Der Track konnte nicht geöffnet werden: \(describe(error))"
        }
    }

    /// From the detail: another activity for one section, not a rest.
    func setSectionActivity(index: Int, activity: String) {
        guard let track = selected, index >= 0 else { return }
        do {
            let view = try core.setSectionActivity(id: track.summary.id, index: UInt32(index), activity: activity)
            selected = view
            overlay?.setTrackRests(Self.rests(view))
            shareFile = try writeGPX(view.summary)
            refresh()
        } catch {
            message = "Die Sportart konnte nicht geändert werden: \(describe(error))"
        }
    }

    /// "10:05"
    nonisolated static func time(_ ms: Int64) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "de_DE")
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: Double(ms) / 1000))
    }

    nonisolated static func rests(_ view: TrackView) -> Set<Int> {
        Set(view.sections.filter(\.rest).map { Int($0.index) })
    }

    /// "Abschnitte bearbeiten" on or off; off, the choice goes.
    func setEditing(_ on: Bool) {
        editing = on
        overlay?.showsSections = on
        if !on { select(nil) }
    }

    /// Chooses a section on the map or in the list, or none.
    func select(_ section: SectionRef?) {
        if let section, !editing { onWantsEditing?() }
        selectedSection = section
        overlay?.select(section)
        recordingSection = nil
        recordingView = nil
        if case .recording(let index) = section, let id = core.recordingStatus().trackId {
            recordingView = try? core.trackDetail(id: id)
            recordingSection = recordingView?.sections.first { Int($0.index) == index }
        }
        if section != nil { onSectionChosen?() }
    }

    func close() {
        if case .track = selectedSection { select(nil) }
        selected = nil
        shareFile = nil
        overlay?.show(track: nil)
    }

    func rename(_ track: TrackSummary, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        do {
            try core.renameTrack(id: track.id, name: name)
            refresh()
            if selected?.summary.id == track.id {
                selected?.summary.name = name
                shareFile = try writeGPX(selected!.summary)
            }
        } catch {
            message = "Der Track konnte nicht umbenannt werden: \(describe(error))"
        }
    }

    func delete(_ track: TrackSummary) {
        do {
            try core.deleteTrack(id: track.id)
            if selected?.summary.id == track.id { close() }
            refresh()
        } catch {
            message = "Der Track konnte nicht gelöscht werden: \(describe(error))"
        }
    }

    // MARK: Photos and notes

    /// The photos and notes of a section (`nil`: the whole tour) of the track looked at.
    func media(for section: Int?) -> [MediaItem] {
        Self.media(of: selected, section: section)
    }

    nonisolated static func media(of view: TrackView?, section: Int?) -> [MediaItem] {
        view?.media.filter { $0.section.map(Int.init) == section } ?? []
    }

    /// Where the file of a photo or voice note lies.
    func fileURL(_ item: MediaItem) -> URL? {
        guard let file = item.file else { return nil }
        return URL(fileURLWithPath: core.mediaDir(), isDirectory: true).deletingLastPathComponent()
            .appendingPathComponent(file)
    }

    /// The photo or note and its track, in the track looked at or the recording.
    private func find(_ id: String) -> (MediaItem, String)? {
        for view in [selected, recordingView].compactMap({ $0 }) {
            if let item = view.media.first(where: { $0.id == id }) { return (item, view.summary.id) }
        }
        return nil
    }

    /// The photos and notes of `track` changed: what shows it is read anew.
    func mediaChanged(track: String) {
        if selected?.summary.id == track, let view = try? core.trackDetail(id: track) {
            selected = view
        }
        if let view = recordingView, view.summary.id == track, let fresh = try? core.trackDetail(id: track) {
            recordingView = fresh
            if let index = recordingSection?.index {
                recordingSection = fresh.sections.first { $0.index == index }
            }
        }
    }

    /// "Notiz" for a section or the whole tour; its time is the start of the section.
    func addNote(track: String, section: Int?, text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        do {
            _ = try core.addTrackMedia(
                trackId: track, section: section.map(UInt32.init),
                input: MediaInput(kind: "text", source: "typed", file: nil, text: text, timeMs: nil, lat: nil, lon: nil))
            mediaChanged(track: track)
        } catch {
            message = "Die Notiz konnte nicht gespeichert werden: \(describe(error))"
        }
    }

    /// "Bild einfügen": copies of the pictures go to `media/` in their own
    /// format, with time and place from their metadata if they have any.
    func insertImages(track: String, section: Int?, files: [URL]) {
        let directory = URL(fileURLWithPath: core.mediaDir(), isDirectory: true)
        for file in files {
            let (url, relative) = MediaController.newFileURL(in: directory, ext: LibraryImport.fileExtension(of: file))
            do {
                try FileManager.default.copyItem(at: file, to: url)
                let found = LibraryImport.metadata(of: url)
                _ = try core.addTrackMedia(
                    trackId: track, section: section.map(UInt32.init),
                    input: MediaInput(
                        kind: "photo", source: "library", file: relative, text: nil, timeMs: found.timeMs,
                        lat: found.lat, lon: found.lon))
            } catch {
                try? FileManager.default.removeItem(at: url)
                message = "Das Bild konnte nicht eingefügt werden: \(describe(error))"
            }
            try? FileManager.default.removeItem(at: file)
        }
        mediaChanged(track: track)
    }

    /// The text of a note or voice note changed; an empty note is offered for deleting.
    func editText(id: String, text: String) {
        guard let (item, track) = find(id) else { return }
        do {
            _ = try core.setMediaText(id: id, text: text.trimmingCharacters(in: .whitespacesAndNewlines))
            mediaChanged(track: track)
        } catch CoreError.Failed(let code, _) where code == "invalid_media" {
            confirmDelete = item
        } catch {
            message = "Die Notiz konnte nicht geändert werden: \(describe(error))"
        }
    }

    /// Deletes the photo or note and Trail's copy of its file (never the picture in the library).
    func delete(id: String) {
        let track = find(id)?.1
        do {
            try core.deleteMedia(id: id)
            if let track { mediaChanged(track: track) }
        } catch {
            message = "Das konnte nicht gelöscht werden: \(describe(error))"
        }
    }

    /// "In Text umwandeln": on the device only; the audio stays.
    func transcribe(id: String) async {
        guard let (item, track) = find(id), item.kind == "voice", let url = fileURL(item) else { return }
        guard transcriber.supportsOnDevice else {
            message = Self.noOnDeviceMessage
            return
        }
        guard await permissions.request(.speech) else {
            blocked = .speech
            return
        }
        transcribing.insert(id)
        defer { transcribing.remove(id) }
        do {
            let text = try await transcriber.transcribe(url).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else {
                message = "In der Sprachnotiz wurde nichts erkannt."
                return
            }
            _ = try core.setMediaTranscript(id: id, text: text)
            mediaChanged(track: track)
        } catch {
            message = "Die Sprachnotiz konnte nicht umgewandelt werden: \(describe(error))"
        }
    }

    /// The voice notes of the track looked at that have no text yet.
    var untranscribed: [MediaItem] {
        selected?.media.filter { $0.kind == "voice" && !$0.transcribed && ($0.text ?? "").isEmpty } ?? []
    }

    /// "Alle umwandeln": each voice note of the tour without text, one after the other.
    func transcribeAll() async {
        let pending = untranscribed
        guard !pending.isEmpty else { return }
        guard transcriber.supportsOnDevice else {
            message = Self.noOnDeviceMessage
            return
        }
        for item in pending {
            await transcribe(id: item.id)
            if blocked != nil { return }
        }
    }

    /// `<Name oder Datum>.gpx` in a folder of its own in the temporary directory.
    private func writeGPX(_ track: TrackSummary) throws -> URL {
        let gpx = try core.exportTrackGpx(id: track.id)
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("tracks", isDirectory: true)
            .appendingPathComponent(track.id, isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appendingPathComponent(Self.fileName(track))
        try gpx.write(to: file, atomically: true, encoding: .utf8)
        return file
    }

    nonisolated static func fileName(_ track: TrackSummary) -> String {
        let base = track.name ?? Self.date(track.startedMs).replacingOccurrences(of: ":", with: ".")
        let cleaned = base.components(separatedBy: CharacterSet(charactersIn: "/\\:")).joined(separator: "-")
        return "\(cleaned).gpx"
    }

    /// "24.11.2026, 10:05"
    nonisolated static func date(_ ms: Int64) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "de_DE")
        formatter.dateFormat = "dd.MM.yyyy, HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: Double(ms) / 1000))
    }

    nonisolated static func title(_ track: TrackSummary) -> String {
        track.name ?? date(track.startedMs)
    }
}

/// The recorded tracks, newest first. A tap opens one.
struct TracksList<Header: View>: View {
    @ObservedObject var tracks: TracksController
    @ViewBuilder let header: () -> Header

    var body: some View {
        Section {
            if tracks.tracks.isEmpty {
                Text("Noch keine Tracks")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("tracks-empty")
            }
            ForEach(Array(tracks.tracks.enumerated()), id: \.element.id) { index, track in
                Button {
                    tracks.open(track)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(TracksController.title(track))
                            .foregroundStyle(Color.primary)
                        Text(meta(track))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .accessibilityIdentifier("track-row-\(index + 1)")
            }
        } header: {
            header()
        }
        .onAppear { tracks.refresh() }
    }

    private func meta(_ track: TrackSummary) -> String {
        var parts = [Activity.name(track.activity)]
        if track.name != nil { parts.append(TracksController.date(track.startedMs)) }
        parts.append(Format.distance(track.distanceM))
        if let ascent = track.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
