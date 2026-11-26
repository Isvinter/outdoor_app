import SwiftUI

/// The bar above the map: which map, light or dark, and how strongly the slope
/// shading and the relief show.
struct MapToolbar: View {
    @ObservedObject var controller: MapController

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 8) {
                HStack(spacing: 0) {
                    baseButton("Sommer", .summer, identifier: "base-summer")
                    baseButton("Winter", .winter, identifier: "base-winter")
                }
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
                HStack(spacing: 0) {
                    lookButton("Hell", dark: false, identifier: "look-light")
                    lookButton("Dunkel", dark: true, identifier: "look-dark")
                }
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
            if controller.base == .winter {
                HStack {
                    Text("Hangneigung")
                        .font(.callout)
                    Slider(value: $controller.slopeOpacity, in: 0...1)
                        .accessibilityIdentifier("slope-opacity")
                        .accessibilityLabel("Hangneigung")
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
            if controller.dark {
                HStack {
                    Text("Relief")
                        .font(.callout)
                    Slider(value: $controller.reliefStrength, in: 0...1)
                        .accessibilityIdentifier("relief-strength")
                        .accessibilityLabel("Relief")
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .panelBackground(in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .frame(maxWidth: 360)
    }

    private func baseButton(_ title: String, _ base: MapBase, identifier: String) -> some View {
        let selected = controller.base == base
        return segment(title, selected: selected, identifier: identifier) {
            if !selected { controller.show(base) }
        }
    }

    private func lookButton(_ title: String, dark: Bool, identifier: String) -> some View {
        segment(title, selected: controller.dark == dark, identifier: identifier) {
            controller.setDark(dark)
        }
    }

    private func segment(
        _ title: String, selected: Bool, identifier: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(title)
                .font(.callout.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? Color.white : Color.primary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
                .background(selected ? Color.accentColor : Color.clear, in: RoundedRectangle(cornerRadius: 10))
        }
        .accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// Above the location button: start a recording, and the settings of the guidance and the rests.
struct MapSideButtons: View {
    @ObservedObject var recording: RecordingController
    /// Starts a recording; nil while one runs.
    var record: (() -> Void)?
    @AppStorage(DeviationAlert.soundKey) private var deviationSound = true
    @AppStorage(TurnAnnouncer.notificationsKey) private var turnNotifications = false
    @AppStorage(TurnAnnouncer.speechKey) private var turnSpeech = false

    var body: some View {
        VStack(spacing: 12) {
            Menu {
                Toggle("Signalton bei Abweichung", isOn: $deviationSound)
                    .accessibilityIdentifier("deviation-sound")
                Toggle("Abbiegehinweise als Mitteilung", isOn: $turnNotifications)
                    .accessibilityIdentifier("turn-notifications")
                Toggle("Abbiegehinweise ansagen", isOn: $turnSpeech)
                    .accessibilityIdentifier("turn-speech")
                Picker("Rast nach", selection: $recording.restDwellMin) {
                    ForEach(RecordingController.restDwellChoices, id: \.self) { Text("\($0) min").tag($0) }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("rest-dwell")
                Picker("Rastumkreis", selection: $recording.restRadiusM) {
                    ForEach(RecordingController.restRadiusChoices, id: \.self) { Text("\($0) m").tag($0) }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("rest-radius")
                Picker("Drosseln bei Rast", selection: $recording.throttle) {
                    ForEach(RestThrottle.allCases, id: \.self) { Text($0.name).tag($0) }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("rest-throttle")
            } label: {
                Image(systemName: "ellipsis")
                    .font(.title3)
                    .foregroundStyle(Color.primary)
                    .frame(width: 48, height: 48)
                    .panelBackground(in: Circle())
            }
            .accessibilityIdentifier("map-settings")
            .accessibilityLabel("Einstellungen")
            if let record {
                Button(action: record) {
                    Image(systemName: "record.circle")
                        .font(.title2)
                        .foregroundStyle(Color(TrackOverlay.recordingColor))
                        .frame(width: 48, height: 48)
                        .panelBackground(in: Circle())
                }
                .accessibilityIdentifier("record")
                .accessibilityLabel("Aufzeichnen")
            }
        }
    }
}
