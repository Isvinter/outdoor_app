import SwiftUI

/// German texts for the routes the desktop offers.
enum RemoteRouteTexts {
    /// What the core says about a route of the desktop, compared with the iPhone.
    static func state(_ state: String) -> String {
        switch state {
        case "new": return "Neu"
        case "up_to_date": return "Aktuell"
        case "desktop_newer": return "Am Desktop geändert"
        case "changed_here": return "Auf dem iPhone geändert – wird als Kopie übernommen"
        case "deleted_here": return "Auf dem iPhone gelöscht"
        default: return state
        }
    }

    /// What became of a route that was taken over; `name` is what it is called here.
    static func outcome(_ outcome: String, name: String) -> String {
        switch outcome {
        case "added": return "Übernommen"
        case "replaced": return "Ersetzt"
        case "restored": return "Wiederhergestellt"
        case "copied": return "Als Kopie „\(name)“ übernommen"
        default: return outcome
        }
    }
}

/// The saved routes of the desktop, each with what taking it over would do.
/// The core compares them with the routes here and applies the merge rule.
struct RemoteRoutesList: View {
    @ObservedObject var plan: PlanController
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if let notice = plan.remoteNotice {
                    Text(notice)
                        .font(.callout)
                        .accessibilityIdentifier("remote-notice")
                }
                if plan.remoteNoSource {
                    Text("Noch keine Adresse des Desktops. Richte sie unter Regionen → QR-Code scannen ein.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("remote-no-source")
                } else if let failure = plan.remoteFailure {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Desktop nicht erreichbar: \(failure)")
                            .font(.callout)
                            .accessibilityIdentifier("remote-error")
                        Button("Erneut versuchen") { plan.readRemoteRoutes() }
                            .buttonStyle(.bordered)
                    }
                } else if let routes = plan.remoteRoutes {
                    if routes.isEmpty {
                        Text("Am Desktop sind keine Routen gespeichert.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(Array(routes.enumerated()), id: \.element.id) { index, route in
                        row(route, number: index + 1)
                    }
                } else {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
            .refreshable { await plan.loadRemoteRoutes() }
            .navigationTitle("Vom Desktop")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Fertig") { dismiss() }
                        .accessibilityIdentifier("remote-done")
                }
            }
        }
        .onAppear {
            plan.remoteNotice = nil
            plan.readRemoteRoutes()
        }
    }

    private func row(_ route: RemoteRouteEntry, number: Int) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(route.name)
                Text(meta(route))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Text(RemoteRouteTexts.state(route.state))
                    .font(.footnote)
                    .accessibilityIdentifier("remote-state-\(number)")
            }
            Spacer()
            if route.state != "up_to_date" {
                Button("Übernehmen") { plan.importRemote(route) }
                    .buttonStyle(.borderedProminent)
                    .disabled(plan.remoteImporting)
                    .accessibilityIdentifier("remote-import-\(number)")
            }
        }
    }

    private func meta(_ route: RemoteRouteEntry) -> String {
        var parts = [profileNames(route.profile.split(separator: "+").map(String.init)), Format.distance(route.distanceM)]
        if let ascent = route.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        return parts.joined(separator: " · ")
    }
}
