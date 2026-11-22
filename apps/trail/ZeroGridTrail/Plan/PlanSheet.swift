import SwiftUI

/// The area below the map that can be pulled up, in three steps: the totals
/// and the controls; with the settings, the elevation profile and the stages;
/// and all of it with the profile large.
struct PlanSheet: View {
    @ObservedObject var plan: PlanController
    @ObservedObject var regions: RegionsController
    @Binding var detent: PresentationDetent
    @State private var showingRemoteRoutes = false

    static let collapsed = PresentationDetent.height(112)
    static let detents: Set<PresentationDetent> = [collapsed, .medium, .large]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            totals
            PlanBar(plan: plan)
            if let notice = plan.notice, detent != Self.collapsed {
                Text(notice)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("plan-notice")
            }
            if detent != Self.collapsed {
                // A list, for the swipe actions of the saved routes.
                List {
                    Section {
                        details
                            .listRowSeparator(.hidden)
                    }
                    // The saved routes show when the sheet is pulled up all the way.
                    if detent == .large {
                        RoutesList(plan: plan, regions: regions) { showingRemoteRoutes = true }
                    }
                }
                .listStyle(.plain)
                .padding(.horizontal, -12)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 22)
        .frame(maxHeight: .infinity, alignment: .top)
        .modifier(WaypointMenu(plan: plan))
        // From here, because the map already presents this sheet.
        .sheet(isPresented: $regions.showing) {
            RegionsScreen(regions: regions)
        }
        .sheet(isPresented: $showingRemoteRoutes) {
            RemoteRoutesList(plan: plan)
        }
        .onAppear {
            plan.onLoaded = { detent = Self.collapsed }
        }
        .onChange(of: plan.request) { _ in plan.notice = nil }
    }

    /// Distance, ascent and time; before there is a route, what to do.
    private var totals: some View {
        HStack(spacing: 8) {
            Text(totalsText)
                .font(plan.plan == nil ? .callout : .headline)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                .accessibilityIdentifier("plan-totals")
            if plan.planning {
                ProgressView()
            }
            Spacer(minLength: 0)
        }
        .frame(minHeight: 22)
    }

    private var totalsText: String {
        guard let total = plan.plan?.total else {
            return plan.picking
                ? "Tippen auf die Karte setzt einen Wegpunkt."
                : "„+ Punkt“ drücken und auf die Karte tippen."
        }
        var parts = [Format.distance(total.distanceM)]
        if let ascent = total.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
        if let time = total.timeS { parts.append(Format.hours(time)) }
        return parts.joined(separator: "  ·  ")
    }

    @ViewBuilder private var details: some View {
        VStack(alignment: .leading, spacing: 14) {
            if plan.profile == "manual" {
                Text("Manuell: gerade Linien zwischen den Wegpunkten, Höhen aus dem Geländemodell.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            ProfileOptions(plan: plan)
            if let result = plan.plan {
                ProfileChart(plan: result, cursor: plan.cursor) { plan.showCursor($0) }
                    .frame(height: detent == .large ? 280 : 140)
                legend(result)
                stats(result.total)
                if result.stages.count > 1 {
                    stages(result)
                }
                let notes = notes(result.total)
                if !notes.isEmpty {
                    Text(notes)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan-note")
                }
                SaveButtons(plan: plan)
            }
        }
        .listRowInsets(EdgeInsets(top: 4, leading: 12, bottom: 12, trailing: 12))
    }

    private func swatch(_ hex: String) -> some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(Color(UIColor(hex: hex)))
            .frame(width: 12, height: 12)
    }

    /// One entry per profile that occurs in the route, in order.
    private func legend(_ result: PlanResult) -> some View {
        var seen: [(profile: String, color: String)] = []
        for leg in result.legs where !seen.contains(where: { $0.profile == leg.profile }) {
            seen.append((leg.profile, leg.color))
        }
        return HStack(spacing: 12) {
            ForEach(seen, id: \.profile) { entry in
                HStack(spacing: 4) {
                    swatch(entry.color)
                    Text(PlanTexts.profile(entry.profile))
                }
            }
        }
        .font(.caption)
    }

    private func stats(_ total: PlanTotals) -> some View {
        var rows = [("Distanz", Format.distance(total.distanceM))]
        if let ascent = total.ascentM, let descent = total.descentM {
            rows.append(("Aufstieg", Format.elevation(ascent)))
            rows.append(("Abstieg", Format.elevation(descent)))
            rows.append(("Höhe min / max", "\(Format.elevation(total.minEleM)) / \(Format.elevation(total.maxEleM))"))
        }
        if let time = total.timeS { rows.append(("Zeit", Format.hours(time))) }
        return VStack(spacing: 4) {
            ForEach(rows, id: \.0) { row in
                HStack {
                    Text(row.0)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Text(row.1)
                        .monospacedDigit()
                }
            }
        }
        .font(.callout)
    }

    /// One row per stage, only when there are several.
    private func stages(_ result: PlanResult) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(result.stages.enumerated()), id: \.offset) { index, stage in
                HStack(spacing: 4) {
                    ForEach(stage.colors, id: \.self) { swatch($0) }
                    Text("Etappe \(index + 1)")
                    Spacer()
                    Text(stageText(stage))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("plan-stage-\(index + 1)")
            }
        }
        .font(.callout)
    }

    private func stageText(_ stage: StageSummary) -> String {
        var parts = [Format.distance(stage.distanceM)]
        if let ascent = stage.ascentM, let descent = stage.descentM {
            parts.append("↑ \(Format.elevation(ascent)) ↓ \(Format.elevation(descent))")
        }
        if let time = stage.timeS { parts.append(Format.hours(time)) }
        return parts.joined(separator: " · ")
    }

    private func notes(_ total: PlanTotals) -> String {
        var notes: [String] = []
        if total.failedLegs > 0 { notes.append("Ohne \(total.failedLegs) nicht berechnete Teilstücke.") }
        if total.ascentM == nil { notes.append("Keine Höhendaten für diese Route.") }
        return notes.joined(separator: " ")
    }
}
