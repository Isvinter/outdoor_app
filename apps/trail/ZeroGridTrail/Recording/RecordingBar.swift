import SwiftUI

/// Over the map while a tour is recorded: time, distance, ascent, the activity
/// of the running section, the rest, pause and end.
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
                activityMenu
            }
            .font(.headline.monospacedDigit())
            .lineLimit(1)
            .minimumScaleFactor(0.7)
            if recording.info.resting {
                HStack(spacing: 6) {
                    Image(systemName: "cup.and.saucer")
                        .accessibilityHidden(true)
                    Text(recording.throttle == .off ? "Rast" : "Rast – GPS gedrosselt")
                        .accessibilityIdentifier("rest-hint")
                }
                .font(.callout)
                .foregroundStyle(.secondary)
            }
            HStack {
                Button(recording.paused ? "Weiter" : "Pause") {
                    recording.paused ? recording.resume() : recording.pause()
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("recording-pause")
                Spacer()
                throttleMenu
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

    /// "MTB ▾": a new section with another activity.
    private var activityMenu: some View {
        Menu {
            ForEach(Activity.all, id: \.self) { choice in
                Button {
                    recording.switchActivity(choice)
                } label: {
                    if choice == recording.activity {
                        Label(Activity.name(choice), systemImage: "checkmark")
                    } else {
                        Text(Activity.name(choice))
                    }
                }
                .accessibilityIdentifier("switch-\(choice)")
            }
        } label: {
            Text("\(Activity.name(recording.activity)) ▾")
                .font(.callout.weight(.semibold))
        }
        .accessibilityIdentifier("recording-activity")
    }

    /// The same setting as "Drosseln bei Rast" in the settings of the map.
    private var throttleMenu: some View {
        Menu {
            Picker("Drosseln bei Rast", selection: $recording.throttle) {
                ForEach(RestThrottle.allCases, id: \.self) { level in
                    Text(level.name).tag(level)
                }
            }
        } label: {
            Label(recording.throttle.name, systemImage: "location.slash")
                .font(.callout)
        }
        .accessibilityIdentifier("recording-throttle")
        .accessibilityLabel("Drosseln bei Rast")
        .accessibilityValue(recording.throttle.name)
    }
}

/// The activities as chips in two rows, so that the names stay readable on a narrow phone.
struct ActivityChips: View {
    let selected: String
    let choose: (String) -> Void

    var body: some View {
        let rows = [Array(Activity.all.prefix(3)), Array(Activity.all.dropFirst(3))]
        ForEach(rows, id: \.self) { row in
            HStack(spacing: 8) {
                ForEach(row, id: \.self) { choice in chip(choice) }
            }
        }
    }

    private func chip(_ choice: String) -> some View {
        let isSelected = choice == selected
        return Button {
            choose(choice)
        } label: {
            Text(Activity.name(choice))
                .font(.callout.weight(isSelected ? .semibold : .regular))
                .foregroundStyle(isSelected ? Color.white : Color.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .frame(maxWidth: .infinity, minHeight: 36)
                .background(
                    isSelected ? Color.accentColor : Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier("activity-\(choice)")
        .accessibilityValue(isSelected ? "gewählt" : "")
    }
}

/// After the record button: a tap on an activity starts the recording.
struct StartRecordingPanel: View {
    @ObservedObject var recording: RecordingController

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Aufnahme starten")
                .font(.headline)
            ActivityChips(selected: recording.lastActivity) { recording.start(activity: $0) }
            HStack {
                Button("Abbrechen") { recording.cancelStart() }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("start-cancel")
                Spacer()
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("start-recording")
    }
}

/// After "Beenden": keep the track or discard it. The activities are those of its sections.
struct SaveTrackPanel: View {
    @ObservedObject var recording: RecordingController
    let track: TrackSummary
    @State private var confirmingDiscard = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(confirmingDiscard ? "Track verwerfen?" : "Track speichern?")
                .font(.headline)
            Text("\(RecordingController.name(startedMs: track.startedMs, activity: track.activity)) · "
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
                HStack {
                    Button("Verwerfen") { confirmingDiscard = true }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("track-discard")
                    Spacer()
                    Button("Speichern") { recording.save() }
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
}
