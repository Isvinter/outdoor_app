import MetricKit
import SwiftUI

// Wegwerfcode. Misst, was die Ortung auf dem iPhone an Akku kostet
// (docs/superpowers/spikes/2026-11-16-ios-ortung-energie-spike.md).

final class AppDelegate: NSObject, UIApplicationDelegate, MXMetricManagerSubscriber {
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions opts: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        // Der CLLocationManager muss hier entstehen, sonst gehen die Ortungs-Ereignisse
        // nach einem Neustart durch iOS verloren.
        Recorder.shared.appGestartet(durchOrtung: opts?[.location] != nil)
        MXMetricManager.shared.add(self)
        return true
    }

    // MetricKit liefert einmal am Tag u. a. die GPS-Zeit je Genauigkeit.
    func didReceive(_ payloads: [MXMetricPayload]) {
        for p in payloads { sichern(p.jsonRepresentation(), art: "metrik") }
    }

    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        for p in payloads { sichern(p.jsonRepresentation(), art: "diagnose") }
    }

    private func sichern(_ daten: Data, art: String) {
        let name = "metrickit-\(art)-\(Int(Date().timeIntervalSince1970)).json"
        try? daten.write(to: Protokoll.ordner.appendingPathComponent(name))
        DispatchQueue.main.async { Recorder.shared.dateienAktualisieren() }
    }
}

@main
struct SpikeApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var delegate

    var body: some Scene {
        WindowGroup { Anzeige(r: Recorder.shared) }
    }
}

struct Anzeige: View {
    @ObservedObject var r: Recorder

    var body: some View {
        NavigationStack {
            Form {
                Section("Lauf") {
                    zeile("Laufzeit", dauer(r.laufzeit))
                    zeile("Punkte", "\(r.punkte)")
                    zeile("Strecke", String(format: "%.2f km", r.strecke / 1000))
                    zeile("Letzte Genauigkeit", r.letzteGenauigkeit < 0 ? "–" : String(format: "%.0f m", r.letzteGenauigkeit))
                    zeile("Akku Start / jetzt", "\(prozent(r.akkuStart)) / \(prozent(r.akkuJetzt))")
                    zeile("Verbrauch", r.prozentProStundeText)
                    zeile("Phase", r.phase.rawValue)
                    zeile("Erlaubnis", r.erlaubnis)
                }
                Section("Einstellung (umschaltbar während der Messung)") {
                    Picker("Stufe", selection: Binding(get: { r.stufe }, set: { r.wechseln(zu: $0) })) {
                        ForEach(Stufe.allCases) { Text($0.titel).tag($0) }
                    }
                    Picker("Aktivität", selection: $r.aktivitaet) {
                        ForEach(Aktivitaet.allCases) { Text($0.titel).tag($0) }
                    }
                    .onChange(of: r.aktivitaet) { _ in r.aktivitaetGeaendert() }
                    Toggle("Neustart-Absicherung (Frage 5)", isOn: $r.absicherung)
                        .onChange(of: r.absicherung) { _ in r.optionenGeaendert() }
                    Toggle("Region-Weckruf bei iOS-Pause", isOn: $r.regionWeckruf)
                        .onChange(of: r.regionWeckruf) { _ in r.optionenGeaendert() }
                }
                Section {
                    if r.laeuft {
                        Button("Stopp", role: .destructive) { r.stoppen() }
                        Button("Beenden-Test (in 20 s)") { r.beendenTest() }
                    } else {
                        Button("Start") { r.starten() }
                    }
                    Button("Ortung „Immer“ erlauben") { r.immerErlauben() }
                }
                Section("Ereignisse") {
                    ForEach(Array(r.ereignisse.enumerated()), id: \.offset) { Text($0.element).font(.caption.monospaced()) }
                }
                Section("Protokolle") {
                    ForEach(r.dateien, id: \.self) { url in
                        ShareLink(item: url) { Text(url.lastPathComponent).font(.caption) }
                    }
                }
            }
            .navigationTitle("Ortung-Test")
            .onAppear { r.dateienAktualisieren() }
        }
    }

    private func zeile(_ titel: String, _ wert: String) -> some View {
        HStack {
            Text(titel)
            Spacer()
            Text(wert).monospacedDigit().foregroundStyle(.secondary)
        }
    }

    private func dauer(_ s: TimeInterval) -> String {
        let t = Int(s)
        return String(format: "%d:%02d:%02d", t / 3600, t / 60 % 60, t % 60)
    }

    private func prozent(_ f: Float) -> String { f < 0 ? "–" : String(format: "%.0f %%", f * 100) }
}
