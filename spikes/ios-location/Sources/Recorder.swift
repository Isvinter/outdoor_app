import CoreLocation
import CoreMotion
import UIKit

// Wegwerfcode. Alles läuft auf dem Main-Thread: der CLLocationManager wird dort
// erzeugt, also kommen auch seine Delegate-Aufrufe dort an.

enum Stufe: String, CaseIterable, Identifiable {
    case leerlauf, genau, ausgewogen, sparsam, n0, n1, n2, stillEigen, stillIOS
    var id: String { rawValue }
    var titel: String {
        switch self {
        case .leerlauf: return "Leerlauf (ohne Ortung)"
        case .genau: return "Genau (Best, jeder Punkt)"
        case .ausgewogen: return "Ausgewogen (Best, 10 m)"
        case .sparsam: return "Sparsam (100 m, 50 m)"
        case .n0: return "N0: Gegenprobe stop + Timer"
        case .n1: return "N1: Notfall 3 km / Best"
        case .n2: return "N2: Funkzellen-Wechsel"
        case .stillEigen: return "Genau + Stillstand eigen"
        case .stillIOS: return "Genau + Stillstand iOS"
        }
    }
}

enum Aktivitaet: String, CaseIterable, Identifiable {
    case rad, wandern
    var id: String { rawValue }
    var titel: String { self == .rad ? "Rad (1 s)" : "Wandern (3 s)" }
    var intervall: TimeInterval { self == .rad ? 1 : 3 }
    var typ: CLActivityType { self == .rad ? .otherNavigation : .fitness }
}

/// Zustand innerhalb einer Stufe. `laeuft` für die einfachen Stufen.
enum Phase: String {
    case laeuft, an, aus, still, probe, pausiert
}

// Stellschrauben der Notfall- und Stillstand-Stufen.
enum Werte {
    // Für den Simulator: -schnell teilt alle Zeiten durch 10.
    static let f: TimeInterval = ProcessInfo.processInfo.arguments.contains("-schnell") ? 0.1 : 1
    static let akkuZeileAlle: TimeInterval = 300 * f
    static let tick: TimeInterval = 5 * f
    // N0/N1: alle 5 min an, bis zum ersten guten Fix + 5 s, höchstens 60 s.
    static let notfallPeriode: TimeInterval = 300 * f
    static let notfallMaxAn: TimeInterval = 60 * f
    static let notfallNachFix: TimeInterval = 5 * f
    static let guterFix: CLLocationAccuracy = 25
    // Stillstand eigen: 3 min innerhalb von 20 m, dann alle 60 s eine Probe von bis zu 30 s.
    static let stillDauer: TimeInterval = 180 * f
    static let stillRadius: CLLocationDistance = 20
    static let probeAlle: TimeInterval = 60 * f
    static let probeMax: TimeInterval = 30 * f
    static let bewegtAb: CLLocationDistance = 25
    static let regionRadius: CLLocationDistance = 100
}

final class Recorder: NSObject, ObservableObject, CLLocationManagerDelegate {
    static let shared = Recorder()

    // Für die Anzeige
    @Published private(set) var laeuft = false
    @Published private(set) var stufe: Stufe = .genau
    @Published private(set) var phase: Phase = .laeuft
    @Published var aktivitaet: Aktivitaet = .rad
    @Published var absicherung = false
    @Published var regionWeckruf = false
    @Published private(set) var start: Date?
    @Published private(set) var punkte = 0
    @Published private(set) var strecke: CLLocationDistance = 0
    @Published private(set) var akkuStart: Float = -1
    @Published private(set) var akkuJetzt: Float = -1
    @Published private(set) var letzteGenauigkeit: CLLocationAccuracy = -1
    @Published private(set) var ereignisse: [String] = []
    @Published private(set) var jetzt = Date()
    @Published private(set) var dateien: [URL] = []
    @Published private(set) var erlaubnis = ""

    private let manager = CLLocationManager()
    private let bewegung = CMMotionActivityManager()
    private var protokoll: Protokoll?
    private var timer: Timer?
    private var letzterTick = Date()
    private var letzteAkkuZeile = Date.distantPast
    private var letzterBehalten: CLLocation?
    private var letzteGute: CLLocation?
    private var phaseSeit = Date()
    // N0/N1
    private var periodeStart = Date()
    private var ersterGuterFix: Date?
    // Stillstand eigen
    private var fenster: [CLLocation] = []
    private var anker: CLLocation?
    private var stillSeit: Date?
    private var letzteAktivitaet = ""
    // Genaue %/h aus den 1-%-Schritten von batteryLevel
    private var akkuSchritte: [(Date, Float)] = []
    private var beendenGeplant = false

    private let d = UserDefaults.standard

    override init() {
        super.init()
        manager.delegate = self
        manager.allowsBackgroundLocationUpdates = true
        manager.showsBackgroundLocationIndicator = true
        manager.pausesLocationUpdatesAutomatically = false
        UIDevice.current.isBatteryMonitoringEnabled = true
        aktivitaet = Aktivitaet(rawValue: d.string(forKey: "aktivitaet") ?? "") ?? .rad
        stufe = Stufe(rawValue: d.string(forKey: "stufe") ?? "") ?? .genau
        absicherung = d.bool(forKey: "absicherung")
        regionWeckruf = d.bool(forKey: "regionWeckruf")
        beobachten()
        erlaubnisAktualisieren()
        dateienAktualisieren()
    }

    // MARK: Start, Stopp, Umschalten

    func starten() {
        guard !laeuft else { return }
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd-HHmm"
        let name = "lauf-\(f.string(from: Date()))-\(stufe.rawValue)-\(aktivitaet.rawValue).csv"
        let jetzt = Date()
        d.set(name, forKey: "lauf")
        d.set(jetzt.timeIntervalSince1970, forKey: "start")
        d.set(UIDevice.current.batteryLevel, forKey: "akkuStart")
        speichern()
        oeffnen(name: name)
        start = jetzt
        akkuStart = UIDevice.current.batteryLevel
        punkte = 0
        strecke = 0
        akkuSchritte = []
        ereignisse = []
        laeuft = true
        ereignis("Start: \(stufe.rawValue), \(aktivitaet.rawValue), Absicherung \(absicherung), Region-Weckruf \(regionWeckruf)")
        ereignis("Gerät: \(UIDevice.current.model) iOS \(UIDevice.current.systemVersion), Stromsparmodus \(ProcessInfo.processInfo.isLowPowerModeEnabled), Ladezustand \(ladezustand())")
        ereignis("Erlaubnis: \(erlaubnis)")
        stufeBeginnen()
        akkuZeile()
        timerStarten()
    }

    func stoppen() {
        guard laeuft else { return }
        ereignis("Stopp nach \(Int(-(start ?? Date()).timeIntervalSinceNow)) s, \(punkte) Punkte, \(Int(strecke)) m, \(prozentProStundeText)")
        akkuZeile()
        laeuft = false
        manager.stopUpdatingLocation()
        manager.stopMonitoringSignificantLocationChanges()
        for r in manager.monitoredRegions { manager.stopMonitoring(for: r) }
        bewegung.stopActivityUpdates()
        timer?.invalidate()
        protokoll?.schliessen()
        protokoll = nil
        d.removeObject(forKey: "lauf")
        dateienAktualisieren()
    }

    private func stoppenNachAbsturz() {
        d.removeObject(forKey: "lauf")
    }

    func wechseln(zu neu: Stufe) {
        guard neu != stufe else { return }
        stufe = neu
        speichern()
        guard laeuft else { return }
        ereignis("Stufe → \(neu.rawValue)")
        stufeBeginnen()
    }

    func aktivitaetGeaendert() {
        speichern()
        if laeuft {
            ereignis("Aktivität → \(aktivitaet.rawValue)")
            anwenden()
        }
    }

    func optionenGeaendert() {
        speichern()
        if laeuft {
            ereignis("Absicherung \(absicherung), Region-Weckruf \(regionWeckruf)")
            absicherungAnwenden()
        }
    }

    /// Test für Frage 5: so tun, als ob iOS die App wegen Speicherdruck beendet.
    func beendenTest() {
        guard !beendenGeplant else { return }
        beendenGeplant = true
        ereignis("Test: App beendet sich in 20 s selbst (Bildschirm jetzt sperren)")
        DispatchQueue.main.asyncAfter(deadline: .now() + 20) { [self] in
            ereignis("exit(0)")
            protokoll?.schliessen()
            exit(0)
        }
    }

    func immerErlauben() { manager.requestAlwaysAuthorization() }

    /// Beim Start der App. Läuft noch ein Lauf, geht es im selben Protokoll weiter.
    func appGestartet(durchOrtung: Bool) {
        if manager.authorizationStatus == .notDetermined { manager.requestWhenInUseAuthorization() }
        // Für den Simulator: -autostart <stufe> startet sofort einen Lauf.
        let args = ProcessInfo.processInfo.arguments
        if let i = args.firstIndex(of: "-autostart"), i + 1 < args.count, let s = Stufe(rawValue: args[i + 1]) {
            if d.string(forKey: "lauf") != nil { stoppenNachAbsturz() }
            stufe = s
            starten()
            return
        }
        guard let name = d.string(forKey: "lauf") else { return }
        oeffnen(name: name)
        start = Date(timeIntervalSince1970: d.double(forKey: "start"))
        akkuStart = d.float(forKey: "akkuStart")
        laeuft = true
        let zustand = UIApplication.shared.applicationState == .background ? "Hintergrund" : "Vordergrund"
        ereignis("App neu gestartet, durch Ortung: \(durchOrtung), \(zustand), Regionen: \(manager.monitoredRegions.map(\.identifier))")
        stufeBeginnen()
        akkuZeile()
        timerStarten()
    }

    // MARK: Ortung einstellen

    private func stufeBeginnen() {
        switch stufe {
        case .n0, .n1:
            phaseSetzen(.an)
            periodeStart = Date()
            ersterGuterFix = nil
        default:
            phaseSetzen(.laeuft)
        }
        fenster = []
        anker = nil
        stillSeit = nil
        if stufe == .stillEigen, CMMotionActivityManager.isActivityAvailable() {
            bewegung.startActivityUpdates(to: .main) { [weak self] a in
                if let a { self?.bewegungErkannt(a) }
            }
        } else {
            bewegung.stopActivityUpdates()
        }
        anwenden()
        absicherungAnwenden()
    }

    private func phaseSetzen(_ p: Phase) {
        phase = p
        phaseSeit = Date()
    }

    private func anwenden() {
        let m = manager
        m.activityType = aktivitaet.typ
        m.pausesLocationUpdatesAutomatically = stufe == .stillIOS
        if stufe != .n2 && !absicherung { m.stopMonitoringSignificantLocationChanges() }

        func ein(_ genauigkeit: CLLocationAccuracy, _ filter: CLLocationDistance) {
            m.desiredAccuracy = genauigkeit
            m.distanceFilter = filter
            m.startUpdatingLocation()
        }
        switch (stufe, phase) {
        case (.leerlauf, _):
            m.stopUpdatingLocation()
        case (.ausgewogen, _):
            ein(kCLLocationAccuracyBest, 10)
        case (.sparsam, _):
            ein(kCLLocationAccuracyHundredMeters, 50)
        case (.n1, .aus), (.stillEigen, .still):
            ein(kCLLocationAccuracyThreeKilometers, kCLDistanceFilterNone)
        case (.n0, .aus):
            m.stopUpdatingLocation()
        case (.n2, _):
            m.stopUpdatingLocation()
            m.startMonitoringSignificantLocationChanges()
        case (.stillIOS, .pausiert):
            break // iOS hat pausiert, nichts anfassen
        default:
            ein(kCLLocationAccuracyBest, kCLDistanceFilterNone)
        }
    }

    /// Frage 5: Funkzellen-Wechsel und eine Region um die letzte Position,
    /// damit iOS die App nach einer Beendigung wieder startet.
    private func absicherungAnwenden() {
        if absicherung {
            manager.startMonitoringSignificantLocationChanges()
            if let l = letzteGute { regionSetzen("absicherung", um: l) }
        } else {
            if stufe != .n2 { manager.stopMonitoringSignificantLocationChanges() }
            regionEntfernen("absicherung")
        }
        if !(stufe == .stillIOS && regionWeckruf) { regionEntfernen("weckruf") }
    }

    private func regionSetzen(_ id: String, um l: CLLocation) {
        guard CLLocationManager.isMonitoringAvailable(for: CLCircularRegion.self) else { return }
        let r = CLCircularRegion(center: l.coordinate, radius: Werte.regionRadius, identifier: id)
        r.notifyOnEntry = false
        r.notifyOnExit = true
        manager.startMonitoring(for: r)
    }

    private func regionEntfernen(_ id: String) {
        for r in manager.monitoredRegions where r.identifier == id { manager.stopMonitoring(for: r) }
    }

    // MARK: Takt (Timer, auch im Hintergrund solange die Ortung läuft)

    private func timerStarten() {
        timer?.invalidate()
        letzterTick = Date()
        let t = Timer(timeInterval: Werte.tick, repeats: true) { [weak self] _ in self?.tick() }
        t.tolerance = Werte.tick / 5
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    private func tick() {
        let n = Date()
        let luecke = n.timeIntervalSince(letzterTick)
        if luecke > Werte.tick * 3 { ereignis("Takt-Lücke \(Int(luecke)) s (App war angehalten)") }
        letzterTick = n
        if UIApplication.shared.applicationState == .active { jetzt = n }
        guard laeuft else { return }

        switch (stufe, phase) {
        case (.n0, .an), (.n1, .an):
            let fixFertig = ersterGuterFix.map { n.timeIntervalSince($0) >= Werte.notfallNachFix } ?? false
            if fixFertig || n.timeIntervalSince(phaseSeit) >= Werte.notfallMaxAn {
                let wie = ersterGuterFix.map { "Fix nach \(Int($0.timeIntervalSince(periodeStart))) s" } ?? "ohne Fix"
                phaseSetzen(.aus)
                ereignis("GPS aus (\(wie))")
                anwenden()
            }
        case (.n0, .aus), (.n1, .aus):
            if n.timeIntervalSince(periodeStart) >= Werte.notfallPeriode {
                ereignis("GPS an (\(Int(n.timeIntervalSince(periodeStart))) s nach letztem Start)")
                periodeStart = n
                ersterGuterFix = nil
                phaseSetzen(.an)
                anwenden()
            }
        case (.stillEigen, .laeuft):
            stillstandPruefen(n)
        case (.stillEigen, .still):
            if n.timeIntervalSince(phaseSeit) >= Werte.probeAlle { probeStarten("Takt") }
        case (.stillEigen, .probe):
            if n.timeIntervalSince(phaseSeit) >= Werte.probeMax {
                ereignis("Probe ohne guten Fix")
                phaseSetzen(.still)
                anwenden()
            }
        default:
            break
        }

        if n.timeIntervalSince(letzteAkkuZeile) >= Werte.akkuZeileAlle { akkuZeile() }
    }

    // MARK: Stillstand eigen

    private func stillstandPruefen(_ n: Date) {
        // Der jüngste Punkt bleibt immer drin: Kommt lange keiner mehr (Simulator,
        // oder iOS liefert im Stand nichts), zählt das auch als Stillstand.
        let juengster = fenster.last
        fenster.removeAll { n.timeIntervalSince($0.timestamp) > Werte.stillDauer }
        if fenster.isEmpty, let juengster { fenster = [juengster] }
        guard let erster = fenster.first,
              n.timeIntervalSince(phaseSeit) >= Werte.stillDauer,
              n.timeIntervalSince(erster.timestamp) >= Werte.stillDauer * 0.8 else { return }
        let lat = fenster.map(\.coordinate.latitude).reduce(0, +) / Double(fenster.count)
        let lon = fenster.map(\.coordinate.longitude).reduce(0, +) / Double(fenster.count)
        let mitte = CLLocation(latitude: lat, longitude: lon)
        let weitester = fenster.map { $0.distance(from: mitte) }.max() ?? 0
        guard weitester <= Werte.stillRadius else { return }
        anker = mitte
        stillSeit = n
        ereignis("Stillstand erkannt (\(fenster.count) Punkte, weitester \(Int(weitester)) m)")
        phaseSetzen(.still)
        anwenden()
    }

    private func probeStarten(_ grund: String) {
        ereignis("Probe (\(grund))")
        phaseSetzen(.probe)
        anwenden()
    }

    private func probeAuswerten(_ l: CLLocation) {
        guard let anker, l.horizontalAccuracy <= 30 else { return }
        let abstand = l.distance(from: anker)
        if abstand > max(Werte.bewegtAb, 2 * l.horizontalAccuracy) {
            let still = stillSeit.map { Int(Date().timeIntervalSince($0)) } ?? -1
            ereignis("Bewegung erkannt: \(Int(abstand)) m vom Anker, still seit \(still) s")
            fenster = []
            phaseSetzen(.laeuft)
        } else {
            ereignis("Probe: weiter still (\(Int(abstand)) m)")
            phaseSetzen(.still)
        }
        anwenden()
    }

    private func bewegungErkannt(_ a: CMMotionActivity) {
        var art: [String] = []
        if a.stationary { art.append("steht") }
        if a.walking { art.append("geht") }
        if a.running { art.append("läuft") }
        if a.cycling { art.append("Rad") }
        if a.automotive { art.append("Auto") }
        if a.unknown { art.append("unbekannt") }
        let text = art.joined(separator: "+") + " (\(["niedrig", "mittel", "hoch"][a.confidence.rawValue]))"
        if text != letzteAktivitaet {
            letzteAktivitaet = text
            ereignis("Bewegungssensor: \(text)")
        }
        let bewegt = a.walking || a.running || a.cycling || a.automotive
        if laeuft, phase == .still, bewegt, a.confidence != .low { probeStarten("Bewegungssensor") }
    }

    // MARK: CLLocationManagerDelegate

    func locationManager(_ m: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard laeuft else { return }
        for l in locations { position(l) }
    }

    private func position(_ l: CLLocation) {
        let alter = Date().timeIntervalSince(l.timestamp)
        let acc = l.horizontalAccuracy
        let grenze: CLLocationAccuracy = stufe == .sparsam ? 150 : stufe == .n2 ? 5000 : 50
        let gut = acc >= 0 && acc <= grenze && alter < 10
        let grobePhase = phase == .aus || phase == .still
        let abstandOk = letzterBehalten.map { l.timestamp.timeIntervalSince($0.timestamp) >= aktivitaet.intervall - 0.2 } ?? true
        let behalten = gut && !grobePhase && abstandOk

        protokoll?.zeile([
            Protokoll.zeitFormat.string(from: l.timestamp), "P", stufe.rawValue, phase.rawValue, hintergrund(),
            String(format: "%.7f", l.coordinate.latitude), String(format: "%.7f", l.coordinate.longitude),
            String(format: "%.1f", l.altitude), String(format: "%.1f", acc), String(format: "%.1f", l.verticalAccuracy),
            String(format: "%.2f", l.speed), String(format: "%.0f", l.course), String(format: "%.1f", alter),
            behalten ? "1" : "0", "", "", "",
        ])
        letzteGenauigkeit = acc

        if behalten {
            if let v = letzterBehalten { strecke += l.distance(from: v) }
            letzterBehalten = l
            punkte += 1
        }
        guard acc >= 0 && acc <= Werte.guterFix && alter < 10 else { return }
        if letzteGute == nil, absicherung { regionSetzen("absicherung", um: l) }
        letzteGute = l

        switch (stufe, phase) {
        case (.n0, .an), (.n1, .an):
            if ersterGuterFix == nil { ersterGuterFix = Date() }
        case (.stillEigen, .laeuft):
            fenster.append(l)
        case (.stillEigen, .probe):
            probeAuswerten(l)
        default:
            break
        }
    }

    func locationManagerDidPauseLocationUpdates(_ m: CLLocationManager) {
        ereignis("iOS hat die Ortung pausiert")
        if laeuft, stufe == .stillIOS {
            stillSeit = Date()
            phaseSetzen(.pausiert)
            if regionWeckruf, let l = letzteGute { regionSetzen("weckruf", um: l) }
        }
    }

    func locationManagerDidResumeLocationUpdates(_ m: CLLocationManager) {
        let still = stillSeit.map { Int(Date().timeIntervalSince($0)) } ?? -1
        ereignis("iOS setzt die Ortung fort (pausiert seit \(still) s)")
        if phase == .pausiert { phaseSetzen(.laeuft) }
    }

    func locationManager(_ m: CLLocationManager, didExitRegion region: CLRegion) {
        ereignis("Region verlassen: \(region.identifier)")
        guard laeuft else { return }
        if region.identifier == "weckruf", phase == .pausiert {
            let still = stillSeit.map { Int(Date().timeIntervalSince($0)) } ?? -1
            ereignis("Weckruf: Ortung neu gestartet (pausiert seit \(still) s)")
            regionEntfernen("weckruf")
            phaseSetzen(.laeuft)
            manager.startUpdatingLocation()
        }
        if region.identifier == "absicherung", let l = m.location { regionSetzen("absicherung", um: l) }
    }

    func locationManager(_ m: CLLocationManager, monitoringDidFailFor region: CLRegion?, withError error: Error) {
        ereignis("Region-Fehler \(region?.identifier ?? "-"): \(error.localizedDescription)")
    }

    func locationManager(_ m: CLLocationManager, didFailWithError error: Error) {
        ereignis("Ortungsfehler: \(error.localizedDescription)")
    }

    func locationManagerDidChangeAuthorization(_ m: CLLocationManager) {
        erlaubnisAktualisieren()
        if laeuft { ereignis("Erlaubnis: \(erlaubnis)") }
    }

    // MARK: Protokoll und Akku

    private func oeffnen(name: String) {
        protokoll = try? Protokoll(url: Protokoll.ordner.appendingPathComponent(name))
        dateienAktualisieren()
    }

    private func ereignis(_ text: String) {
        protokoll?.zeile(zeile(typ: "E", text: text))
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        ereignisse.insert("\(f.string(from: Date())) \(text)", at: 0)
        if ereignisse.count > 40 { ereignisse.removeLast() }
    }

    private func akkuZeile() {
        letzteAkkuZeile = Date()
        akkuJetzt = UIDevice.current.batteryLevel
        protokoll?.zeile(zeile(typ: "A", text: ladezustand()))
    }

    private func zeile(typ: String, text: String) -> [String] {
        [Protokoll.zeitFormat.string(from: Date()), typ, stufe.rawValue, phase.rawValue, hintergrund(),
         "", "", "", "", "", "", "", "", "",
         String(format: "%.2f", UIDevice.current.batteryLevel), thermisch(), text]
    }

    private func hintergrund() -> String {
        UIApplication.shared.applicationState == .active ? "0" : "1"
    }

    private func thermisch() -> String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: return "normal"
        case .fair: return "warm"
        case .serious: return "heiss"
        case .critical: return "kritisch"
        @unknown default: return "?"
        }
    }

    private func ladezustand() -> String {
        switch UIDevice.current.batteryState {
        case .unplugged: return "entlädt"
        case .charging: return "lädt"
        case .full: return "voll"
        default: return "unbekannt"
        }
    }

    private func beobachten() {
        let nc = NotificationCenter.default
        let app: [(Notification.Name, String)] = [
            (UIApplication.didEnterBackgroundNotification, "Hintergrund"),
            (UIApplication.willEnterForegroundNotification, "Vordergrund"),
            (UIApplication.willTerminateNotification, "App wird beendet"),
            (UIApplication.didReceiveMemoryWarningNotification, "Speicherwarnung"),
            (ProcessInfo.thermalStateDidChangeNotification, "Temperatur geändert"),
            (Notification.Name.NSProcessInfoPowerStateDidChange, "Stromsparmodus geändert"),
            (UIDevice.batteryStateDidChangeNotification, "Ladezustand geändert"),
        ]
        for (name, text) in app {
            nc.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                guard let self, self.laeuft else { return }
                self.ereignis("\(text): \(self.ladezustand()), Stromsparmodus \(ProcessInfo.processInfo.isLowPowerModeEnabled)")
                if name == UIApplication.willEnterForegroundNotification {
                    self.jetzt = Date()
                    self.akkuZeile()
                }
                if name == UIApplication.willTerminateNotification { self.protokoll?.schliessen() }
            }
        }
        nc.addObserver(forName: UIDevice.batteryLevelDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self, self.laeuft else { return }
            self.akkuSchritte.append((Date(), UIDevice.current.batteryLevel))
            self.akkuZeile()
        }
    }

    private func erlaubnisAktualisieren() {
        let s: String
        switch manager.authorizationStatus {
        case .authorizedAlways: s = "Immer"
        case .authorizedWhenInUse: s = "Beim Verwenden"
        case .denied: s = "Abgelehnt"
        case .restricted: s = "Eingeschränkt"
        default: s = "Nicht gefragt"
        }
        let genau = manager.accuracyAuthorization == .fullAccuracy ? "genau" : "ungefähr"
        erlaubnis = "\(s), \(genau)"
    }

    private func speichern() {
        d.set(stufe.rawValue, forKey: "stufe")
        d.set(aktivitaet.rawValue, forKey: "aktivitaet")
        d.set(absicherung, forKey: "absicherung")
        d.set(regionWeckruf, forKey: "regionWeckruf")
    }

    func dateienAktualisieren() {
        let alle = (try? FileManager.default.contentsOfDirectory(at: Protokoll.ordner, includingPropertiesForKeys: nil)) ?? []
        dateien = alle.filter { ["csv", "json"].contains($0.pathExtension) }
            .sorted { $0.lastPathComponent > $1.lastPathComponent }
    }

    // MARK: Anzeige

    var laufzeit: TimeInterval { start.map { jetzt.timeIntervalSince($0) } ?? 0 }

    /// %/h zwischen dem ersten und letzten 1-%-Schritt (genauer), sonst Start bis jetzt.
    var prozentProStundeText: String {
        var teile: [String] = []
        if akkuStart >= 0, akkuJetzt >= 0, laufzeit > 60 {
            teile.append(String(format: "%.2f %%/h gesamt", Double(akkuStart - akkuJetzt) * 100 / (laufzeit / 3600)))
        }
        if let a = akkuSchritte.first, let b = akkuSchritte.last, b.0.timeIntervalSince(a.0) > 600 {
            let h = b.0.timeIntervalSince(a.0) / 3600
            teile.append(String(format: "%.2f %%/h Schritte", Double(a.1 - b.1) * 100 / h))
        }
        return teile.isEmpty ? "–" : teile.joined(separator: ", ")
    }
}
