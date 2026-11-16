import SwiftUI

/// "MTB + Wandern": the names of the profiles, each once, in order.
func profileNames(_ profiles: [String]) -> String {
    var seen: [String] = []
    for profile in profiles where !seen.contains(profile) {
        seen.append(profile)
    }
    return seen.map(PlanTexts.profile).joined(separator: " + ")
}

/// The buttons that store the planned route. Both ask for the name first.
struct SaveButtons: View {
    @ObservedObject var plan: PlanController
    /// Whether the dialog stores a new route, while it is open.
    @State private var savingAsNew: Bool?
    @State private var name = ""

    var body: some View {
        VStack(spacing: 8) {
            Button {
                name = plan.loaded?.name ?? plan.defaultName
                savingAsNew = false
            } label: {
                Text("Speichern")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("route-save")
            Button {
                name = plan.defaultName
                savingAsNew = true
            } label: {
                Text("Als neue Route speichern")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityIdentifier("route-save-new")
        }
        .disabled(plan.plan == nil || plan.planning)
        .alert(
            savingAsNew == true ? "Als neue Route speichern" : "Route speichern",
            isPresented: Binding(get: { savingAsNew != nil }, set: { if !$0 { savingAsNew = nil } })
        ) {
            TextField("Name der Route", text: $name)
            Button("Speichern") { plan.save(name: name, asNew: savingAsNew == true) }
            Button("Abbrechen", role: .cancel) {}
        }
    }
}

/// The saved routes. A tap loads one; swiping offers to rename or delete it.
struct RoutesList: View {
    @ObservedObject var plan: PlanController
    @ObservedObject var regions: RegionsController
    @State private var renaming: SavedRoute?
    @State private var name = ""

    var body: some View {
        Section {
            if plan.routes.isEmpty {
                Text("Noch keine gespeicherten Routen")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(plan.routes.enumerated()), id: \.element.id) { index, route in
                Button {
                    plan.load(route)
                } label: {
                    row(route)
                }
                .accessibilityIdentifier("route-row-\(index + 1)")
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    Button("Löschen", role: .destructive) { plan.delete(route) }
                    Button("Umbenennen") { rename(route) }
                        .tint(.blue)
                }
                .contextMenu {
                    Button("Umbenennen") { rename(route) }
                    Button("Löschen", role: .destructive) { plan.delete(route) }
                }
            }
        } header: {
            HStack {
                Text("Routen")
                Spacer()
                Button("Regionen") { regions.showing = true }
                    .font(.callout)
                    .textCase(nil)
                    .accessibilityIdentifier("regions-open")
            }
        }
        .alert(
            "Route umbenennen",
            isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
        ) {
            TextField("Name der Route", text: $name)
            Button("Umbenennen") {
                if let renaming { plan.rename(renaming, to: name) }
            }
            Button("Abbrechen", role: .cancel) {}
        }
    }

    private func rename(_ route: SavedRoute) {
        name = route.name
        renaming = route
    }

    private func row(_ route: SavedRoute) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(route.name)
                    .foregroundStyle(Color.primary)
                Text(meta(route))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if route.id == plan.loaded?.id {
                Image(systemName: "checkmark")
                    .accessibilityLabel("geladen")
            }
        }
        .contentShape(Rectangle())
    }

    private func meta(_ route: SavedRoute) -> String {
        var parts = [profileNames(route.profile.split(separator: "+").map(String.init)), Format.distance(route.distanceM)]
        if let ascent = route.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
