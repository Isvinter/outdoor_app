import CoreLocation
import SwiftUI

/// The chosen hit: what it is, the way back to the list and "Route hierher",
/// or "Hier starten" while the start of a route is chosen.
struct ResultCard: View {
    let hit: SearchHit
    @ObservedObject var search: SearchController
    /// The title of the button; none for the target itself while the start is chosen.
    let action: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                search.back()
            } label: {
                Label("Zurück", systemImage: "chevron.left")
                    .frame(minHeight: 32)
            }
            .accessibilityIdentifier("search-back")
            Text(SearchTexts.title(hit))
                .font(.headline)
                .accessibilityIdentifier("search-card-title")
            Text(SearchTexts.details(hit, distance: Format.distance))
                .font(.footnote)
                .foregroundStyle(.secondary)
            if let action {
                Button(action) { search.routeHere() }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("route-here")
            }
        }
    }
}

/// A place tapped on the map while the own position is known: a route there,
/// or a route and the guidance at once.
struct MapPointCard: View {
    @ObservedObject var flow: RouteHereFlow
    let at: CLLocationCoordinate2D
    let from: CLLocation?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Punkt auf der Karte")
                    .font(.headline)
                    .accessibilityIdentifier("point-title")
                Spacer()
                Button {
                    flow.dismissPoint()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .frame(minWidth: 32, minHeight: 32)
                }
                .accessibilityIdentifier("point-close")
                .accessibilityLabel("Schließen")
            }
            if let from {
                let distance = from.distance(from: CLLocation(latitude: at.latitude, longitude: at.longitude))
                Text("\(Format.distance(distance)) Luftlinie")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Button("Route hierher") { flow.routeToPoint() }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("point-route-here")
                Spacer()
                Button("Hierher führen") { flow.guideToPoint() }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("point-guide-here")
            }
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("map-point")
    }
}
