import SwiftUI

/// Centres the map on the own position.
struct LocationButton: View {
    /// How uncertain the position is, in metres; shown beside the button.
    let accuracy: Double?
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
            Image(systemName: "location.fill")
                .font(.title3)
                .frame(width: 48, height: 48)
                .panelBackground(in: Circle())
        }
        .accessibilityIdentifier("center-location")
        .accessibilityLabel("Position zentrieren")
    }
}
