import SwiftUI

/// One recorded track, while the map shows it: its numbers, and renaming,
/// sharing as GPX and deleting.
struct TrackDetailView: View {
    @ObservedObject var tracks: TracksController
    let track: TrackView
    @State private var renaming = false
    @State private var deleting = false
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button {
                    tracks.close()
                } label: {
                    Label("Tracks", systemImage: "chevron.left")
                }
                .accessibilityIdentifier("track-back")
                Spacer()
            }
            Text(TracksController.title(track.summary))
                .font(.headline)
                .accessibilityIdentifier("track-title")
            VStack(spacing: 4) {
                ForEach(rows, id: \.0) { row in
                    HStack {
                        Text(row.0)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Text(row.1)
                            .monospacedDigit()
                    }
                }
            }
            .font(.callout)
            if let file = tracks.shareFile {
                ShareLink(item: file) {
                    Label("Als GPX teilen", systemImage: "square.and.arrow.up")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("track-share")
            }
            HStack {
                Button("Umbenennen") {
                    name = track.summary.name ?? ""
                    renaming = true
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("track-rename")
                Spacer()
                Button("Löschen", role: .destructive) { deleting = true }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("track-delete")
            }
        }
        .alert("Track umbenennen", isPresented: $renaming) {
            TextField("Name des Tracks", text: $name)
            Button("Umbenennen") { tracks.rename(track.summary, to: name) }
            Button("Abbrechen", role: .cancel) {}
        }
        .alert("Track löschen?", isPresented: $deleting) {
            Button("Löschen", role: .destructive) { tracks.delete(track.summary) }
            Button("Abbrechen", role: .cancel) {}
        }
    }

    private var rows: [(String, String)] {
        let summary = track.summary
        var rows = [
            ("Aktivität", Activity.name(summary.activity)),
            ("Start", TracksController.date(summary.startedMs)),
            ("Distanz", Format.distance(summary.distanceM)),
            ("In Bewegung", Format.hours(summary.movingS)),
            ("Gesamtzeit", Format.hours(track.elapsedS)),
        ]
        if let ascent = summary.ascentM { rows.append(("Aufstieg", Format.elevation(ascent))) }
        if let descent = track.descentM { rows.append(("Abstieg", Format.elevation(descent))) }
        if let top = track.maxEleM { rows.append(("Höchster Punkt", Format.elevation(top))) }
        return rows
    }
}
