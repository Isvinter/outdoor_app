import Charts
import SwiftUI

/// The elevation profile of the planned route: coloured leg by leg, with a
/// line where a stage ends. A finger on it sets the cursor on the map; the
/// cursor stays where the finger left, so that the place can be looked at.
struct ProfileChart: View {
    @Environment(\.colorScheme) private var colorScheme
    let plan: PlanResult
    let cursor: ElevationSample?
    let onCursor: (ElevationSample) -> Void

    /// Samples with a known elevation, and the same as a run per colour.
    private struct Point: Identifiable {
        let id: Int
        let km: Double
        let ele: Double
    }

    private struct Run: Identifiable {
        let id: Int
        let color: Color
        var points: [Point]
    }

    private var known: [ElevationSample] {
        plan.elevation.filter { $0.ele != nil }
    }

    /// One run per stretch of the same colour; each starts where the one
    /// before ended, so that the areas join without a gap.
    private var runs: [Run] {
        var runs: [Run] = []
        var legOfRun: UInt32?
        for (index, sample) in known.enumerated() {
            let point = Point(id: index, km: sample.distanceM / 1000, ele: Double(sample.ele ?? 0))
            if sample.leg == legOfRun, !runs.isEmpty {
                runs[runs.count - 1].points.append(point)
            } else {
                let hex = Int(sample.leg) < plan.legs.count ? plan.legs[Int(sample.leg)].color : "#6a1b9a"
                let previous = runs.last?.points.last
                let shown = colorScheme == .dark ? RouteOverlay.readableOnBlack(hex) : hex
                runs.append(Run(id: runs.count, color: Color(UIColor(hex: shown)), points: (previous.map { [$0] } ?? []) + [point]))
                legOfRun = sample.leg
            }
        }
        return runs
    }

    /// Where each stage ends along the profile: the last sample before its end point.
    private var stageEnds: [Double] {
        plan.request.stageEnds.map { end in
            plan.elevation.filter { $0.leg < end }.map(\.distanceM).max() ?? 0
        }
    }

    var body: some View {
        let known = known
        let low = known.compactMap(\.ele).min().map(Double.init) ?? 0
        let high = known.compactMap(\.ele).max().map(Double.init) ?? 0
        let total = known.last?.distanceM ?? 0
        if known.count < 2 || total <= 0 {
            EmptyView()
        } else {
            // Flat routes should look flat: never stretch less than 50 m over the full height.
            let top = low + max(high - low, 50)
            VStack(spacing: 2) {
                Chart {
                    ForEach(runs) { run in
                        ForEach(run.points) { point in
                            AreaMark(
                                x: .value("km", point.km), yStart: .value("m", low), yEnd: .value("m", point.ele),
                                series: .value("Teilstück", run.id))
                                .foregroundStyle(run.color.opacity(0.22))
                            LineMark(
                                x: .value("km", point.km), y: .value("m", point.ele),
                                series: .value("Teilstück", run.id))
                                .foregroundStyle(run.color)
                                .lineStyle(StrokeStyle(lineWidth: 1.5))
                        }
                    }
                    ForEach(stageEnds, id: \.self) { at in
                        RuleMark(x: .value("km", at / 1000))
                            .foregroundStyle(Color.primary.opacity(0.7))
                            .lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                    }
                    if let cursor {
                        RuleMark(x: .value("km", cursor.distanceM / 1000))
                            .foregroundStyle(Color(UIColor(hex: "#0b63ce")))
                            .lineStyle(StrokeStyle(lineWidth: 1.5))
                    }
                }
                .chartXScale(domain: 0...(total / 1000))
                .chartYScale(domain: low...top)
                .chartLegend(.hidden)
                .chartOverlay { proxy in
                    GeometryReader { geometry in
                        Rectangle()
                            .fill(Color.clear)
                            .contentShape(Rectangle())
                            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                                let x = drag.location.x - geometry[proxy.plotAreaFrame].origin.x
                                guard let km: Double = proxy.value(atX: x) else { return }
                                let at = km * 1000
                                if let nearest = known.min(by: { abs($0.distanceM - at) < abs($1.distanceM - at) }) {
                                    onCursor(nearest)
                                }
                            })
                    }
                }
                HStack {
                    if let cursor {
                        Text("\(Format.elevation(cursor.ele.map(Double.init))) · \(Format.distance(cursor.distanceM))")
                    } else {
                        Text("\(Format.elevation(low)) – \(Format.elevation(high))")
                    }
                    Spacer()
                    Text(Format.distance(total))
                }
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityIdentifier("plan-chart")
            .accessibilityLabel("Höhenprofil")
            .accessibilityValue("\(Format.elevation(low)) bis \(Format.elevation(high)), \(Format.distance(total))")
        }
    }
}
