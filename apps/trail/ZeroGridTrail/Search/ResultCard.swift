import CoreLocation
import SwiftUI

/// The chosen hit: what it is, the way back to the list and what may become of it.
struct ResultCard: View {
    let hit: SearchHit
    @ObservedObject var search: SearchController
    @ObservedObject var flow: RouteHereFlow
    @ObservedObject var location: LocationService

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
            TargetActions(
                flow: flow, location: location, at: CLLocationCoordinate2D(latitude: hit.lat, longitude: hit.lon),
                placed: false)
        }
    }
}

/// A place tapped on the map: the same choices as for a search hit.
struct MapPointCard: View {
    @ObservedObject var flow: RouteHereFlow
    @ObservedObject var location: LocationService
    let point: MapPoint

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(point.placed ? "Von meinem Standort hierher?" : "Punkt auf der Karte")
                    .font(.headline)
                    .accessibilityIdentifier("point-title")
                Spacer()
                Button {
                    flow.dismiss()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .frame(minWidth: 32, minHeight: 32)
                }
                .accessibilityIdentifier("point-close")
                .accessibilityLabel("Schließen")
            }
            if let from = location.last, location.knowsPosition {
                let distance = from.distance(from: CLLocation(latitude: point.lat, longitude: point.lon))
                Text("\(Format.distance(distance)) Luftlinie")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            TargetActions(flow: flow, location: location, at: point.coordinate, placed: point.placed)
        }
        .padding(12)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("map-point")
    }
}

/// What a chosen place may become. With the position known a route from
/// there, also with the guidance; without, a route that begins at the place.
/// With a route on the map, also its next destination.
struct TargetActions: View {
    @ObservedObject var flow: RouteHereFlow
    @ObservedObject var location: LocationService
    let at: CLLocationCoordinate2D
    /// The place is the first waypoint already; only the way from the position is offered.
    let placed: Bool

    private var appends: Bool { !placed && flow.hasRoute }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let message = flow.message {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("route-here-message")
            }
            if location.knowsPosition {
                HStack {
                    Button("Route hierher") { flow.routeHere(to: at) }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("route-here")
                    Button("Hierher führen") { flow.guideHere(to: at) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("guide-here")
                    if flow.busy { ProgressView() }
                }
                .disabled(flow.busy)
            } else if !placed {
                Button(appends ? "Neue Route ab hier" : "Route planen") { flow.planFrom(at) }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("plan-from-here")
            }
            if appends {
                Button("Als nächstes Ziel anhängen") { flow.append(at) }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("route-here-append")
            }
        }
    }
}
