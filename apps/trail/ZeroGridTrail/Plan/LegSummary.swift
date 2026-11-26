import SwiftUI

/// The chosen leg in place of the totals: its number, length and climb, or
/// why it failed; and the buttons to split it and to let go of it.
struct LegSummary: View {
    @ObservedObject var plan: PlanController
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        if let index = plan.selectedLeg, let result = plan.plan, result.legs.indices.contains(index) {
            let leg = result.legs[index]
            HStack(spacing: 8) {
                Circle()
                    .fill(Color(UIColor(hex: color(of: leg))))
                    .frame(width: 10, height: 10)
                    .accessibilityHidden(true)
                Text(text(of: leg, number: index + 1))
                    .font(.headline)
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .accessibilityIdentifier("leg-summary")
                if plan.planning {
                    ProgressView()
                }
                Spacer(minLength: 0)
                Button("In der Mitte teilen") { plan.splitSelectedLeg() }
                    .font(.callout)
                    .lineLimit(1)
                    .accessibilityHint("Punkt in der Mitte einfügen")
                    .accessibilityIdentifier("leg-split")
                Button {
                    plan.selectLeg(nil)
                } label: {
                    Image(systemName: "xmark")
                        .font(.callout.weight(.semibold))
                        .frame(width: 28, height: 22)
                }
                .accessibilityLabel("Auswahl aufheben")
                .accessibilityIdentifier("leg-close")
            }
            .buttonStyle(.borderless)
            .disabled(plan.planning)
            .frame(minHeight: 22)
        }
    }

    /// As the map draws it: a failed leg in the colour for failures.
    private func color(of leg: PlanLeg) -> String {
        let hex = leg.errorCode == nil ? leg.color : failedLegColor()
        return colorScheme == .dark ? RouteOverlay.readableOnBlack(hex) : hex
    }

    private func text(of leg: PlanLeg, number: Int) -> String {
        var parts = ["Teilstück \(number)"]
        if let code = leg.errorCode {
            parts.append(PlanTexts.error(code: code, fallback: leg.errorMessage ?? ""))
        } else {
            parts.append(Format.distance(leg.distanceM))
            if let ascent = leg.ascentM { parts.append("↑ \(Format.elevation(ascent))") }
            if let descent = leg.descentM { parts.append("↓ \(Format.elevation(descent))") }
        }
        return parts.joined(separator: "  ·  ")
    }
}
