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

    private let core: CoreApp
    private let overlay: TrackOverlay?
    private var watch: AnyCancellable?

    init(core: CoreApp, overlay: TrackOverlay?, recordingChanges: AnyPublisher<RecordingInfo, Never>) {
        self.core = core
        self.overlay = overlay
        // A finished recording is a new track.
        watch = recordingChanges.map(\.phase).removeDuplicates().filter { $0 == "idle" }.sink { [weak self] _ in
            self?.refresh()
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
            overlay?.show(track: view.segments)
            shareFile = try writeGPX(view.summary)
        } catch {
            message = "Der Track konnte nicht geöffnet werden: \(describe(error))"
        }
    }

    func close() {
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
