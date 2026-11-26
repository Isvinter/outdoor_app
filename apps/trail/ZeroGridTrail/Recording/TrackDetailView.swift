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
            // With one section, its numbers are those above.
            if track.sections.count > 1 {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Abschnitte")
                        .font(.subheadline.weight(.semibold))
                    ForEach(track.sections, id: \.index) { section in
                        SectionRow(tracks: tracks, section: section)
                    }
                }
            }
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

/// One section of a track: its activity as a menu (not for a rest), its time and numbers.
private struct SectionRow: View {
    @ObservedObject var tracks: TracksController
    let section: TrackSection

    var body: some View {
        let id = Int(section.index)
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if section.rest {
                Text("Rast \(TracksController.time(section.startMs))–\(TracksController.time(section.endMs))")
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("section-\(id)-title")
                Spacer(minLength: 0)
            } else {
                Menu {
                    ForEach(Activity.all, id: \.self) { choice in
                        Button(Activity.name(choice)) {
                            tracks.setSectionActivity(index: id, activity: choice)
                        }
                        .accessibilityIdentifier("section-\(id)-\(choice)")
                    }
                } label: {
                    Text("\(Activity.name(section.activity)) ▾")
                        .fontWeight(.semibold)
                }
                .accessibilityIdentifier("section-\(id)-activity")
                Text("\(TracksController.time(section.startMs))–\(TracksController.time(section.endMs))")
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Text(numbers)
                    .monospacedDigit()
                    .accessibilityIdentifier("section-\(id)-numbers")
            }
        }
        .font(.callout)
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("section-\(id)")
    }

    private var numbers: String {
        var parts = [Format.distance(section.distanceM), Format.hours(section.movingS)]
        if let ascent = section.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
