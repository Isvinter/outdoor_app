import SwiftUI

/// Over the map while a tour is recorded: time, distance, ascent, pause and end.
struct RecordingBar: View {
    @ObservedObject var recording: RecordingController

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 14) {
                Image(systemName: recording.paused ? "pause.circle.fill" : "record.circle")
                    .foregroundStyle(recording.paused ? Color.secondary : Color(TrackOverlay.recordingColor))
                    .accessibilityLabel(recording.paused ? "Pausiert" : "Aufzeichnung läuft")
                Text(Format.hours(recording.info.movingS))
                    .accessibilityIdentifier("recording-time")
                Text(Format.distance(recording.info.distanceM))
                    .accessibilityIdentifier("recording-distance")
                Text("↑ \(Format.elevation(recording.info.ascentM))")
                    .accessibilityIdentifier("recording-ascent")
                Spacer(minLength: 0)
            }
            .font(.headline.monospacedDigit())
            .lineLimit(1)
            .minimumScaleFactor(0.7)
            HStack {
                Button(recording.paused ? "Weiter" : "Pause") {
                    recording.paused ? recording.resume() : recording.pause()
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("recording-pause")
                Spacer()
                Button("Beenden") { recording.stop() }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("recording-stop")
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("recording-bar")
    }
}

/// After "Beenden": keep the track, with an activity if wanted, or discard it.
struct SaveTrackPanel: View {
    @ObservedObject var recording: RecordingController
    let track: TrackSummary
    @State private var activity = "other"
    @State private var confirmingDiscard = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(confirmingDiscard ? "Track verwerfen?" : "Track speichern?")
                .font(.headline)
            Text("\(RecordingController.name(startedMs: track.startedMs, activity: activity)) · "
                + Format.distance(track.distanceM))
                .font(.callout)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("save-track-name")
            if confirmingDiscard {
                HStack {
                    Button("Abbrechen") { confirmingDiscard = false }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("discard-cancel")
                    Spacer()
                    Button("Verwerfen", role: .destructive) { recording.discard() }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)
                        .accessibilityIdentifier("discard-confirm")
                }
            } else {
                // Two rows, so that the names stay readable on a narrow phone.
                let rows = [Array(Activity.all.prefix(3)), Array(Activity.all.dropFirst(3))]
                ForEach(rows, id: \.self) { row in
                    HStack(spacing: 8) {
                        ForEach(row, id: \.self) { choice in chip(choice) }
                    }
                }
                HStack {
                    Button("Verwerfen") { confirmingDiscard = true }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("track-discard")
                    Spacer()
                    Button("Speichern") { recording.save(activity: activity) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("track-save")
                }
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("save-track")
    }

    private func chip(_ choice: String) -> some View {
        let selected = choice == activity
        return Button {
            activity = choice
        } label: {
            Text(Activity.name(choice))
                .font(.callout.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? Color.white : Color.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .frame(maxWidth: .infinity, minHeight: 36)
                .background(
                    selected ? Color.accentColor : Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier("activity-\(choice)")
        .accessibilityValue(selected ? "gewählt" : "")
    }
}
