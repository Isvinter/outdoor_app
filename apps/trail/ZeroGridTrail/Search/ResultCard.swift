import SwiftUI

/// The chosen hit: what it is, the way back to the list and "Route hierher".
struct ResultCard: View {
    let hit: SearchHit
    @ObservedObject var search: SearchController

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
            Button("Route hierher") { search.routeHere() }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("route-here")
        }
    }
}
