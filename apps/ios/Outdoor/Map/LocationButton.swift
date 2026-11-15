import SwiftUI

/// Centres the map on the own position.
struct LocationButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "location.fill")
                .font(.title3)
                .frame(width: 48, height: 48)
                .background(.regularMaterial, in: Circle())
        }
        .accessibilityIdentifier("center-location")
        .accessibilityLabel("Position zentrieren")
    }
}
