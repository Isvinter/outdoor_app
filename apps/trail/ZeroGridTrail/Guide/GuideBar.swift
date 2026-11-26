import SwiftUI

/// Over the map while guiding: what is left to the end of the stage and to
/// the destination, the way back when off the route, and following the position.
struct GuideBar: View {
    @ObservedObject var guide: GuideController

    var body: some View {
        if let info = guide.info {
            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Self.lines(info), id: \.self) { line in
                        Text(line)
                    }
                    .font(.headline.monospacedDigit())
                    if let off = info.offNetwork {
                        Text(off.text)
                            .font(.callout.weight(.semibold))
                            .foregroundStyle(Color(GuideOverlay.wayBackColor))
                            .accessibilityIdentifier("guide-off-network")
                    }
                    if let wayBack = Self.wayBackText(info, waypoints: guide.waypointCount) {
                        Text(wayBack)
                            .font(.callout.weight(.semibold))
                            .foregroundStyle(Color(GuideOverlay.wayBackColor))
                            .accessibilityIdentifier("guide-way-back")
                    }
                    if info.phase == "no_way_back" {
                        Text("Kein Weg zurück gefunden")
                            .font(.callout.weight(.semibold))
                            .accessibilityIdentifier("guide-no-way-back")
                    }
                    if guide.noTurnsNotice {
                        Text("Diese Route hat keine Abbiegehinweise")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("guide-no-turns")
                    }
                    if info.phase == "waiting" {
                        Text("Warte auf die Position …")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                }
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                Spacer(minLength: 0)
                if !guide.following {
                    Button {
                        guide.following = true
                    } label: {
                        Image(systemName: "location.north.line.fill")
                            .frame(width: 40, height: 40)
                            .background(Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
                    }
                    .accessibilityIdentifier("guide-follow")
                    .accessibilityLabel("Position folgen")
                }
                Button {
                    guide.toggle()
                } label: {
                    Image(systemName: "xmark")
                        .frame(width: 40, height: 40)
                        .background(Color(.secondarySystemFill), in: RoundedRectangle(cornerRadius: 10))
                }
                .accessibilityIdentifier("guide-stop")
                .accessibilityLabel("Führen beenden")
            }
            .foregroundStyle(Color.primary)
            .padding(12)
            .frame(maxWidth: 480)
            .panelBackground(in: RoundedRectangle(cornerRadius: 12))
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("guide-bar")
        } else if guide.arrived {
            Text("Ziel erreicht")
                .font(.headline)
                .padding(12)
                .frame(maxWidth: 480)
                .panelBackground(in: RoundedRectangle(cornerRadius: 12))
                .accessibilityIdentifier("guide-arrived")
        }
    }

    /// "Etappe: 12,35 km · 0:48 h" when the route has stages, and "Ziel: …".
    static func lines(_ info: GuidanceInfo) -> [String] {
        var lines: [String] = []
        if let stage = info.toStageEndM {
            lines.append("Etappe: " + rest(stage, info.toStageEndS))
        }
        lines.append((lines.isEmpty ? "" : "Ziel: ") + rest(info.toDestinationM, info.toDestinationS))
        if lines.count == 1 { lines[0] = "Ziel: " + lines[0] }
        return lines
    }

    private static func rest(_ metres: Double, _ seconds: Double?) -> String {
        [Format.distance(metres), seconds.map(Format.hours)].compactMap { $0 }.joined(separator: " · ")
    }

    /// "Weg zurück zu Wegpunkt 4: 650 m" or "Weg zurück zum Ziel: 650 m".
    static func wayBackText(_ info: GuidanceInfo, waypoints: Int) -> String? {
        guard info.phase == "off_route", let metres = info.wayBackM else { return nil }
        return "\(target(info, waypoints: waypoints)): \(Format.distance(metres))"
    }

    /// "Weg zurück zu Wegpunkt 4" (counted from 1 as in the list of waypoints),
    /// or "Weg zurück zum Ziel" when it leads to the last waypoint.
    static func target(_ info: GuidanceInfo, waypoints: Int) -> String {
        if let next = info.nextWaypoint, Int(next) < waypoints - 1 {
            return "Weg zurück zu Wegpunkt \(next + 1)"
        }
        return "Weg zurück zum Ziel"
    }
}
