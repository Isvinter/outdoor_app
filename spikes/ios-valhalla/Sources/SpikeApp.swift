import SwiftUI
import Valhalla
import ValhallaConfigModels

// Wegwerfcode. Rechnet die beiden Referenzrouten aus dem Spike-Dokument
// mit der gebündelten routing.tar und zeigt Länge, Punktzahl und Dauer.

struct Reference {
    let name: String
    let request: String
    let expectedMeters: Double
    let expectedPoints: Int
}

let references = [
    Reference(
        name: "Wandern, 3 Punkte",
        request: #"{"costing":"pedestrian","costing_options":{"pedestrian":{"max_hiking_difficulty":3,"use_hills":0.8}},"directions_type":"none","locations":[{"lat":47.6803,"lon":11.5741},{"lat":47.6625,"lon":11.5233},{"lat":47.645,"lon":11.556}],"units":"kilometers"}"#,
        expectedMeters: 12353, expectedPoints: 718),
    Reference(
        name: "MTB, 2 Punkte",
        request: #"{"costing":"bicycle","costing_options":{"bicycle":{"avoid_bad_surfaces":0.0,"bicycle_type":"mountain","use_hills":0.8,"use_roads":0.1}},"directions_type":"none","locations":[{"lat":47.645,"lon":11.556},{"lat":47.68,"lon":11.57}],"units":"kilometers"}"#,
        expectedMeters: 5832, expectedPoints: 260),
]

struct Answer: Decodable {
    struct Trip: Decodable {
        struct Leg: Decodable { let shape: String }
        struct Summary: Decodable { let length: Double }
        let legs: [Leg]
        let summary: Summary
    }
    let trip: Trip
}

/// polyline6 -> Punkte als ganzzahlige (lat, lon) * 1e6.
func decodePolyline6(_ s: String) -> [SIMD2<Int32>] {
    var out: [SIMD2<Int32>] = []
    var lat: Int32 = 0, lon: Int32 = 0
    var it = s.utf8.makeIterator()
    func next() -> Int32? {
        var result: Int32 = 0, shift: Int32 = 0
        while let c = it.next() {
            let b = Int32(c) - 63
            result |= (b & 0x1f) << shift
            shift += 5
            if b < 0x20 { return (result & 1) != 0 ? ~(result >> 1) : (result >> 1) }
        }
        return nil
    }
    while let dLat = next(), let dLon = next() {
        lat += dLat; lon += dLon
        out.append(SIMD2(lat, lon))
    }
    return out
}

func ms(_ block: () throws -> Void) rethrows -> Double {
    let t = DispatchTime.now().uptimeNanoseconds
    try block()
    return Double(DispatchTime.now().uptimeNanoseconds - t) / 1e6
}

func runSpike() -> String {
    var lines: [String] = []
    #if targetEnvironment(simulator)
    lines.append("Umgebung: Simulator")
    #else
    lines.append("Umgebung: Gerät")
    #endif
    do {
        guard let tar = Bundle.main.url(forResource: "routing", withExtension: "tar") else {
            return "routing.tar fehlt im Bundle"
        }
        var valhalla: Valhalla!
        let openMs = try ms { valhalla = try Valhalla(try ValhallaConfig(tileExtractTar: tar)) }
        lines.append(String(format: "Öffnen (Konfiguration, tzdata, Akteur): %.1f ms", openMs))

        for ref in references {
            var times: [Double] = []
            var raw = ""
            for _ in 0..<20 {
                times.append(try ms { raw = try valhalla.route(rawRequest: ref.request) })
            }
            let answer = try JSONDecoder().decode(Answer.self, from: Data(raw.utf8))
            var line: [SIMD2<Int32>] = []
            for leg in answer.trip.legs {
                for p in decodePolyline6(leg.shape) where line.last != p { line.append(p) }
            }
            let meters = answer.trip.summary.length * 1000
            let ok = abs(meters - ref.expectedMeters) < 0.5 && line.count == ref.expectedPoints
            let sorted = times.sorted()
            lines.append("")
            lines.append("\(ref.name): \(ok ? "STIMMT" : "WEICHT AB")")
            lines.append(String(format: "  Länge %.0f m (erwartet %.0f)", meters, ref.expectedMeters))
            lines.append("  Punkte \(line.count) (erwartet \(ref.expectedPoints))")
            lines.append(String(format: "  Dauer: erster Lauf %.1f ms, Median %.1f ms, Minimum %.1f ms (20 Läufe)",
                                times[0], sorted[10], sorted[0]))
        }
    } catch {
        lines.append("FEHLER: \(error)")
    }
    return lines.joined(separator: "\n")
}

@main
struct SpikeApp: App {
    @State private var output = "Noch nicht gerechnet."
    @State private var busy = false

    var body: some Scene {
        WindowGroup {
            VStack(alignment: .leading, spacing: 16) {
                Button(busy ? "Rechnet …" : "Referenzrouten rechnen") { run() }
                    .buttonStyle(.borderedProminent)
                    .disabled(busy)
                ScrollView {
                    Text(output)
                        .font(.system(.footnote, design: .monospaced))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
            }
            .padding()
            .onAppear {
                // Für den automatischen Lauf: Startargument -autorun
                if ProcessInfo.processInfo.arguments.contains("-autorun") { run() }
            }
        }
    }

    private func run() {
        busy = true
        DispatchQueue.global(qos: .userInitiated).async {
            let text = runSpike()
            print("SPIKE-ERGEBNIS-ANFANG\n\(text)\nSPIKE-ERGEBNIS-ENDE")
            if let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first {
                try? text.write(to: docs.appendingPathComponent("ergebnis.txt"), atomically: true, encoding: .utf8)
            }
            DispatchQueue.main.async { output = text; busy = false }
        }
    }
}
