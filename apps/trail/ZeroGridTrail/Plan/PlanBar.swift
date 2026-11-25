import SwiftUI

/// The controls of the planning: the profile for the last and all new legs,
/// plan mode, undo and clear.
struct PlanBar: View {
    @ObservedObject var plan: PlanController
    @ObservedObject var guide: GuideController

    var body: some View {
        HStack(spacing: 8) {
            ProfileMenu(plan: plan)

            Spacer(minLength: 0)

            // Only with a route to guide along.
            if plan.plan != nil || guide.active {
                Button {
                    guide.toggle()
                } label: {
                    Text("Führen")
                        .fontWeight(.semibold)
                        .foregroundStyle(guide.active ? Color.white : Color.accentColor)
                        .padding(.horizontal, 12)
                        .frame(minHeight: 40)
                        .background(
                            guide.active ? Color.accentColor : Color(.secondarySystemFill),
                            in: RoundedRectangle(cornerRadius: 10))
                }
                .accessibilityIdentifier("guide-toggle")
                .accessibilityValue(guide.active ? "an" : "aus")
            }

            Button {
                plan.picking.toggle()
            } label: {
                Text(plan.picking ? "Fertig" : "+ Punkt")
                    .fontWeight(.semibold)
                    .foregroundStyle(plan.picking ? Color.white : Color.accentColor)
                    .padding(.horizontal, 12)
                    .frame(minHeight: 40)
                    .background(
                        plan.picking ? Color.accentColor : Color(.secondarySystemFill),
                        in: RoundedRectangle(cornerRadius: 10))
            }
            .accessibilityIdentifier("plan-pick")
            .accessibilityLabel(plan.picking ? "Wegpunkte setzen beenden" : "Wegpunkte setzen")

            iconButton("arrow.uturn.backward", label: "Rückgängig", identifier: "plan-undo") { plan.undo() }
            iconButton("delete.left", label: "Leeren", identifier: "plan-clear") { plan.clear() }
        }
        .foregroundStyle(Color.primary)
    }

    private func iconButton(
        _ symbol: String, label: String, identifier: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .frame(width: 40, height: 40)
                .background(Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
        }
        .disabled(plan.request.waypoints.isEmpty)
        .accessibilityIdentifier(identifier)
        .accessibilityLabel(label)
    }
}

/// The menu of a waypoint: end a stage here, or take the waypoint away.
struct WaypointMenu: ViewModifier {
    @ObservedObject var plan: PlanController
    @ObservedObject var guide: GuideController

    func body(content: Content) -> some View {
        let index = plan.menuWaypoint ?? 0
        content.confirmationDialog(
            "Wegpunkt \(index + 1)",
            isPresented: Binding(get: { plan.menuWaypoint != nil }, set: { if !$0 { plan.menuWaypoint = nil } }),
            titleVisibility: .visible
        ) {
            if plan.canMarkStageEnd {
                Button(plan.endsStage(index) ? "Kein Etappenende mehr" : "Etappenende hier") {
                    plan.toggleStageEnd(index)
                }
            }
            if plan.plan != nil {
                Button("Ab hier führen") { guide.startFromWaypoint(index) }
            }
            Button("Entfernen", role: .destructive) { plan.removeWaypoint(index) }
            Button("Abbrechen", role: .cancel) {}
        }
    }
}
