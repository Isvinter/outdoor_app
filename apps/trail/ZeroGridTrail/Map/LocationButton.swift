import SwiftUI

/// Switches the own position on and centres the map on it, or switches it off.
struct LocationButton: View {
    /// How uncertain the position is, in metres; shown beside the button.
    let accuracy: Double?
    /// The position is shown and the GPS runs.
    let on: Bool
    let action: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            if let accuracy {
                Text(Self.text(accuracy: accuracy))
                    .font(.footnote.monospacedDigit())
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .panelBackground(in: Capsule())
                    .accessibilityIdentifier("location-accuracy")
                    .accessibilityLabel("Genauigkeit der Position: \(Self.text(accuracy: accuracy))")
            }
            button
        }
    }

    /// "± 12 m", from a kilometre on "± 1,2 km".
    static func text(accuracy: Double) -> String {
        if accuracy < 1000 {
            return "± \(Int(max(accuracy, 1).rounded())) m"
        }
        return "± " + String(format: "%.1f", accuracy / 1000).replacingOccurrences(of: ".", with: ",") + " km"
    }

    private var button: some View {
        Button(action: action) {
            Image(systemName: on ? "location.fill" : "location")
                .font(.title3)
                .frame(width: 48, height: 48)
                .panelBackground(in: Circle())
        }
        .accessibilityIdentifier("center-location")
        .accessibilityLabel(on ? "Position ausschalten" : "Position zeigen")
        .accessibilityValue(on ? "an" : "aus")
    }
}
