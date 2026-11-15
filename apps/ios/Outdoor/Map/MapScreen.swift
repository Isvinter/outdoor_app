import SwiftUI

struct MapScreen: View {
    @ObservedObject var controller: MapController
    let start: AppStart.Ready

    var body: some View {
        ZStack(alignment: .top) {
            MapView(controller: controller)
                .ignoresSafeArea()
            VStack(spacing: 8) {
                MapToolbar(controller: controller)
                if start.regions.isEmpty {
                    Notice(text: "Noch keine Region installiert. Lege einen Regionsordner per Finder unter "
                        + "Dateien → Outdoor → Import ab und starte die App neu.")
                        .accessibilityIdentifier("no-region-hint")
                }
                ForEach(start.failedImports, id: \.self) { failure in
                    Notice(text: failure, warning: true)
                }
                if let message = controller.message {
                    Notice(text: message, warning: true)
                        .onTapGesture { controller.message = nil }
                }
            }
            .padding()
            if start.options.selftest {
                SelfTestStatus(controller: controller, start: start)
            }
        }
    }
}

/// A short text over the map.
struct Notice: View {
    let text: String
    var warning = false

    var body: some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(warning ? Color.white : Color.primary)
            .padding(12)
            .frame(maxWidth: 480)
            .background(
                warning ? AnyShapeStyle(Color.red.opacity(0.85)) : AnyShapeStyle(.regularMaterial),
                in: RoundedRectangle(cornerRadius: 12))
    }
}

/// What the UI tests read: an invisible element whose accessibility value is
/// `rendered=<0|1> served=<n> failed=<n> …`, refreshed while the app runs.
private struct SelfTestStatus: View {
    @ObservedObject var controller: MapController
    let start: AppStart.Ready

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.25)) { _ in
            Color.clear
                .frame(width: 1, height: 1)
                .accessibilityElement()
                .accessibilityIdentifier("map-status")
                .accessibilityLabel("Kartenstatus")
                .accessibilityValue(status)
        }
        .allowsHitTesting(false)
    }

    private var status: String {
        let stats = TileURLProtocol.stats
        var parts = [
            "rendered=\(controller.rendered ? 1 : 0)",
            "served=\(stats.served)",
            "failed=\(stats.failed + controller.loadFailures)",
            "base=\(controller.base.rawValue)",
            "regions=\(start.regions.count)",
        ]
        if let slope = controller.slopeOpacityOnMap {
            parts.append(String(format: "slope=%.2f", slope))
        }
        return parts.joined(separator: " ")
    }
}
