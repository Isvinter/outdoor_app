import SwiftUI

/// One recorded track, while the map shows it: its numbers, its sections
/// with their photos and notes, and renaming, sharing as GPX and deleting.
/// Photos and notes are changed in "Abschnitte bearbeiten" only.
struct TrackDetailView: View {
    @ObservedObject var tracks: TracksController
    @ObservedObject var media: MediaController
    let track: TrackView
    @StateObject private var player = VoicePlayer()
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
            if let permission = tracks.blocked {
                PermissionNotice(permission: permission) { tracks.blocked = nil }
            }
            // With one section and nothing to it, its numbers are those above.
            if track.sections.count > 1 || tracks.editing || !track.media.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Abschnitte")
                        .font(.subheadline.weight(.semibold))
                    wholeTour
                    ForEach(track.sections, id: \.index) { section in
                        let index = Int(section.index)
                        SectionRow(tracks: tracks, section: section)
                        MediaList(
                            tracks: tracks, media: media, player: player, trackId: track.summary.id,
                            items: TracksController.media(of: track, section: index), section: index,
                            editing: tracks.editing && tracks.selectedSection == .track(index))
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
        .onDisappear { player.stop() }
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

    /// "Ganze Tour": photos and notes that belong to no section.
    @ViewBuilder private var wholeTour: some View {
        let items = TracksController.media(of: track, section: nil)
        if !items.isEmpty || tracks.editing {
            HStack {
                Text("Ganze Tour")
                    .font(.callout.weight(.semibold))
                    .accessibilityIdentifier("whole-tour")
                Spacer(minLength: 0)
                if tracks.editing && !tracks.untranscribed.isEmpty {
                    Button("Alle umwandeln") { Task { await tracks.transcribeAll() } }
                        .font(.footnote.weight(.semibold))
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("transcribe-all")
                }
            }
            .padding(.horizontal, 6)
            MediaList(
                tracks: tracks, media: media, player: player, trackId: track.summary.id, items: items, section: nil,
                editing: tracks.editing)
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

/// One section of a track: its activity, its time and numbers. A tap chooses it
/// (and switches to "Abschnitte bearbeiten"); there the activity is a menu, not for a rest.
private struct SectionRow: View {
    @ObservedObject var tracks: TracksController
    let section: TrackSection

    private var id: Int { Int(section.index) }
    private var chosen: Bool { tracks.selectedSection == .track(id) }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if section.rest {
                Text("Rast \(TracksController.time(section.startMs))–\(TracksController.time(section.endMs))")
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("section-\(id)-title")
                Spacer(minLength: 0)
            } else {
                if tracks.editing {
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
                } else {
                    Text(Activity.name(section.activity))
                        .fontWeight(.semibold)
                        .accessibilityIdentifier("section-\(id)-name")
                }
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
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(
            chosen ? Color(TrackOverlay.selectedColor).opacity(0.3) : Color.clear,
            in: RoundedRectangle(cornerRadius: 8))
        .contentShape(Rectangle())
        .onTapGesture { tracks.select(chosen ? nil : .track(id)) }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("section-\(id)")
        .accessibilityValue(chosen ? "gewählt" : "")
        .accessibilityAddTraits(.isButton)
    }

    private var numbers: String {
        var parts = [Format.distance(section.distanceM), Format.hours(section.movingS)]
        if let ascent = section.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
