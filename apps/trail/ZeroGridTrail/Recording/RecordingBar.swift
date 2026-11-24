import SwiftUI

/// Over the map while a tour is recorded: time, distance, ascent, pause and end.
/// Ending asks first, in the bar itself, because the map already presents the sheet.
struct RecordingBar: View {
    @ObservedObject var recording: RecordingController
    @State private var confirmingStop = false

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
            if confirmingStop {
                Text("Aufzeichnung beenden?")
                    .font(.callout)
                HStack {
                    Button("Abbrechen") { confirmingStop = false }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("recording-stop-cancel")
                    Spacer()
                    Button("Beenden", role: .destructive) {
                        confirmingStop = false
                        recording.stop()
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("recording-stop-confirm")
                }
            } else {
                HStack {
                    Button(recording.paused ? "Weiter" : "Pause") {
                        recording.paused ? recording.resume() : recording.pause()
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("recording-pause")
                    Spacer()
                    Button("Beenden") { confirmingStop = true }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("recording-stop")
                }
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("recording-bar")
    }
}

/// Asked at the start after the app was ended during a recording.
struct InterruptedRecordingPrompt: View {
    @ObservedObject var recording: RecordingController

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Eine Aufzeichnung wurde unterbrochen.")
                .font(.callout)
            HStack {
                Button("Fortsetzen") { recording.continueInterrupted() }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("recording-continue")
                Spacer()
                Button("Als Track speichern") { recording.finishInterrupted() }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("recording-finish")
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("recording-interrupted")
    }
}
