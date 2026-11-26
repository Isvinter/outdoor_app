import SwiftUI

/// Above the guide bar: the next turn with its arrow and the distance, which
/// counts down with each fix; "Jetzt" at the turn.
struct TurnBanner: View {
    let info: TurnInfo

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: Self.symbol(for: info.kind))
                .font(.system(size: 34, weight: .semibold))
                .frame(width: 44)
            Text(Self.distance(info))
                .font(.title2.weight(.bold).monospacedDigit())
            Text(info.label)
                .font(.title3.weight(.semibold))
            Spacer(minLength: 0)
        }
        .lineLimit(1)
        .minimumScaleFactor(0.6)
        .foregroundStyle(Color.primary)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: 480)
        .panelBackground(in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("turn-banner")
    }

    /// The SF Symbol of a kind of turn (the serde names of the core's `TurnKind`).
    static func symbol(for kind: String) -> String {
        switch kind {
        case "slight_left", "keep_left": return "arrow.up.left"
        case "slight_right", "keep_right": return "arrow.up.right"
        case "left": return "arrow.turn.up.left"
        case "right": return "arrow.turn.up.right"
        case "sharp_left": return "arrow.down.left"
        case "sharp_right": return "arrow.down.right"
        case "u_turn": return "arrow.uturn.down"
        // The number of the exit is in the label.
        case "roundabout": return "arrow.triangle.turn.up.right.circle"
        case "stage_end": return "flag.checkered"
        default: return "arrow.up"
        }
    }

    /// "200 m" before the turn; "Jetzt" at it and at a stage end.
    static func distance(_ info: TurnInfo) -> String {
        info.stage == "now" || info.kind == "stage_end" ? "Jetzt" : info.distanceText
    }
}
