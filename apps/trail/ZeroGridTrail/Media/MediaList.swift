import AVFoundation
import SwiftUI

/// Plays the voice notes, one at a time.
@MainActor
final class VoicePlayer: NSObject, ObservableObject, AVAudioPlayerDelegate {
    /// The note that plays.
    @Published private(set) var playing: String?
    private var player: AVAudioPlayer?

    /// Plays the note, or stops it if it plays.
    func toggle(_ id: String, url: URL) {
        let again = playing == id
        stop()
        guard !again else { return }
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .spokenAudio)
            try AVAudioSession.sharedInstance().setActive(true)
            let player = try AVAudioPlayer(contentsOf: url)
            player.delegate = self
            player.play()
            self.player = player
            playing = id
        } catch {
            playing = nil
        }
    }

    func stop() {
        player?.stop()
        player = nil
        if playing != nil {
            playing = nil
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        }
    }

    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor in self.stop() }
    }
}

/// The photos and notes of one section of a track, or of the whole tour
/// (`section == nil`). With `editing` ("Abschnitte bearbeiten", this section
/// chosen): add a note, put in pictures, change and delete.
struct MediaList: View {
    @ObservedObject var tracks: TracksController
    @ObservedObject var media: MediaController
    @ObservedObject var player: VoicePlayer
    let trackId: String
    let items: [MediaItem]
    let section: Int?
    let editing: Bool

    private var key: String { section.map(String.init) ?? "tour" }
    private var photos: [MediaItem] { items.filter { $0.kind == "photo" } }
    private var notes: [MediaItem] { items.filter { $0.kind != "photo" } }

    var body: some View {
        if !items.isEmpty || editing {
            VStack(alignment: .leading, spacing: 8) {
                if !photos.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 6) {
                            ForEach(Array(photos.enumerated()), id: \.element.id) { index, item in
                                if let url = tracks.fileURL(item) {
                                    Button {
                                        media.viewing = ViewedPhoto(item: item, url: url, editing: editing)
                                    } label: {
                                        MediaThumbnail(url: url)
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityIdentifier("photo-\(key)-\(index)")
                                    .accessibilityLabel("Foto \(time(item))")
                                }
                            }
                        }
                    }
                }
                ForEach(Array(notes.enumerated()), id: \.element.id) { index, item in
                    NoteRow(
                        tracks: tracks, media: media, player: player, item: item, identifier: "note-\(key)-\(index)",
                        editing: editing)
                }
                if editing {
                    HStack(spacing: 8) {
                        Button {
                            media.note = .section(track: trackId, section: section)
                        } label: {
                            Label("Notiz", systemImage: "square.and.pencil")
                                .frame(maxWidth: .infinity, minHeight: 44)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("add-note-\(key)")
                        Button {
                            media.picking = PickTarget(track: trackId, section: section)
                        } label: {
                            Label("Bild einfügen", systemImage: "photo.badge.plus")
                                .frame(maxWidth: .infinity, minHeight: 44)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("insert-image-\(key)")
                    }
                    .font(.callout)
                }
            }
            .padding(.leading, 6)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("media-\(key)")
        }
    }

    private func time(_ item: MediaItem) -> String {
        item.timeMs.map(TracksController.time) ?? ""
    }
}

/// A note or a voice note: its text, playing, and in "Abschnitte bearbeiten"
/// changing, turning into text and deleting.
private struct NoteRow: View {
    @ObservedObject var tracks: TracksController
    @ObservedObject var media: MediaController
    @ObservedObject var player: VoicePlayer
    let item: MediaItem
    let identifier: String
    let editing: Bool

    private var voice: Bool { item.kind == "voice" }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            if voice, let url = tracks.fileURL(item) {
                Button {
                    player.toggle(item.id, url: url)
                } label: {
                    Image(systemName: player.playing == item.id ? "stop.circle.fill" : "play.circle.fill")
                        .font(.title2)
                        .frame(width: 44, height: 44)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("\(identifier)-play")
                .accessibilityLabel(player.playing == item.id ? "Anhalten" : "Abspielen")
            } else {
                Image(systemName: "note.text")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 44, height: 44)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(text)
                    .font(.callout)
                    .foregroundStyle(item.text == nil ? .secondary : .primary)
                    .lineLimit(editing ? nil : 3)
                    .accessibilityIdentifier("\(identifier)-text")
                if editing {
                    HStack(spacing: 14) {
                        Button("Bearbeiten") { media.note = .edit(item) }
                            .accessibilityIdentifier("\(identifier)-edit")
                        if voice && !item.transcribed {
                            if tracks.transcribing.contains(item.id) {
                                ProgressView()
                            } else {
                                Button("In Text umwandeln") { Task { await tracks.transcribe(id: item.id) } }
                                    .accessibilityIdentifier("\(identifier)-transcribe")
                            }
                        }
                        Button("Löschen", role: .destructive) { tracks.confirmDelete = item }
                            .accessibilityIdentifier("\(identifier)-delete")
                    }
                    .font(.footnote.weight(.semibold))
                    .buttonStyle(.borderless)
                }
            }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }

    private var text: String {
        let time = item.timeMs.map(TracksController.time).map { "\($0) · " } ?? ""
        if let text = item.text, !text.isEmpty { return text }
        return "\(time)Sprachnotiz"
    }
}
