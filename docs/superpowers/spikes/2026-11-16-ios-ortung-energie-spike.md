# Messversuch: Ortung und Akku beim Aufzeichnen auf dem iPhone

Übergabe für die Sitzung auf dem MacBook. Stand 2026-11-16.

## Worum es geht

Nach I1–I4 (Karte, Höhenlinien, Planung, Freigabe im Heimnetz) kommt als Nächstes das
**Aufzeichnen von Tracks auf dem iPhone**. Später folgt ein Tourenbuch mit Statistik über
Zeiträume.

Der Rust-Kern kann schon aufzeichnen: Recorder mit Start, Pause und Fortsetzen, Segmente,
Wiederaufnahme nach Absturz, Statistik und SQLite (`core-track`, `core-app`). Auf iOS fehlt nur
die Ortung. Swift liefert die Positionen, der Kern macht den Rest.

**Für den Nutzer ist der Akkuverbrauch ein Hauptziel der ganzen App**, bei bestmöglicher
Genauigkeit. Sein Einsatzfall:
- 7+ Tage Radreise ohne Steckdose, Strom nur aus Powerbank und Solarzelle. Je nach Wetter steht
  nur halb so viel Energie zur Verfügung, oder dreimal so viel.
- Ebenso 1–2 Tage ganz ohne Strom.
- „Im Notfall die Genauigkeit runterdrehen statt ganz verzichten.“

Dieser Wegwerf-Test misst, was die Ortung auf dem iPhone wirklich kostet. **Erst danach werden
die Stufen festgelegt und die Aufzeichnung entworfen.**

## Schon entschieden (Gespräch mit dem Nutzer, 2026-11-16)

- **Stufen, umschaltbar auch während einer laufenden Aufzeichnung**, ohne Neustart und ohne
  Lücke. Vorschlag (Werte legt dieser Test fest):
  - **Genau:** GPS voll, alle 1 s (Rad) bzw. 3 s (Wandern), wie heute im Kern
    (`Activity::location_interval_ms`)
  - **Ausgewogen:** GPS voll, ein Punkt alle ~10 m, bei Stillstand keiner
  - **Sparsam:** gröbere Genauigkeit, ein Punkt alle ~50 m
  - **Notfall:** GPS nur zeitweise an (z. B. 30 s alle 5 min), grober Linienzug
- **Pause bei Stillstand, die Strom spart:** Steht man länger als ein paar Minuten, schaltet die
  Ortung auf sparsam und wird bei Bewegung wieder genau. Die Statistik trennt Bewegungszeit
  ohnehin. Der Nutzer vergisst das Pausieren oft.
- **Ein Track pro Abschnitt:** einer pro Tag, oder mehrere, wenn die Aufzeichnung unterbrochen
  wird. Die Bündelung zu einer „Reise“ kommt später im Tourenbuch.
- **Nach der Tour:** Der Track bleibt in der App, auf der Karte, mit Statistik, als GPX
  exportierbar. Abgleich mit Desktop und Server sowie mehrere Benutzerkonten kommen später.
- **Weg A:** klassische `CLLocationManager`-Schnittstelle (ab iOS 16.4), wegen der vollen
  Kontrolle über den Energieverbrauch. **Nicht** `CLLocationUpdate.liveUpdates` (iOS 17).
- Der Desktop bleibt unverändert. Seine Aufzeichnung ist ein Relikt, stört aber nicht.
- Ein Umbau der Oberfläche und ein OLED-Sparmodus kommen später.

## Fragen

1. **Akku pro Stunde** je Stufe bei gesperrtem Bildschirm, im Flugmodus mit Ortung an.
2. **Wie genau** ist der Track je Stufe: Punktabstand, `horizontalAccuracy`, Lücken, Ecken in
   Kurven?
3. **Notfall-Stufe:** Lässt iOS die App im Hintergrund am Leben, während der GPS-Empfänger
   zwischen den Messungen aus ist, und spart das wirklich? Zu prüfen sind diese Varianten:
   - **N1:** Die Ortung läuft weiter, aber zwischen den Messungen mit
     `desiredAccuracy = kCLLocationAccuracyThreeKilometers`, also ohne GPS. Für ~30 s geht sie
     auf `kCLLocationAccuracyBest`. Dieser bekannte Kniff hält die App wach.
   - **N2:** `startMonitoringSignificantLocationChanges` (Funkzellen-basiert). Wahrscheinlich
     nutzlos im Outback ohne Netz, nur zur Einordnung.
   - **Nicht** `stopUpdatingLocation` mit Timer: Im Hintergrund feuert der Timer dann nicht
     mehr. Das nur kurz bestätigen.
4. **Pause bei Stillstand:**
   - `pausesLocationUpdatesAutomatically` mit `activityType` hat einen bekannten Haken: iOS
     setzt die Ortung danach nicht von selbst fort, erst wenn die App in den Vordergrund
     kommt. Das bestätigen oder widerlegen.
   - Alternativ eigene Erkennung: Ist die Position x min stabil, Genauigkeit runter wie bei N1,
     und bei Bewegung (Positionssprung > Genauigkeit) zurück auf die Stufe.
   - Messen: Wie schnell wird die Bewegung erkannt, und was spart das?
5. **Überlebt die Aufzeichnung**, wenn iOS die App beendet, etwa bei Speicherdruck oder nach
   einem Neustart? Für die Rückkehr nach Beendigung gibt es
   `startMonitoringSignificantLocationChanges` oder die Region-Überwachung. Prüfen, ob iOS die
   App damit wieder startet. Der Kern übernimmt danach die Wiederaufnahme.

## Aufbau

Wegwerfcode: Mini-App unter `spikes/ios-location/` (nicht in `apps/`), ohne Rust-Kern, ohne
Tests.

- `UIBackgroundModes: location` und `NSLocationWhenInUseUsageDescription`. Falls „Immer“
  nötig ist, `NSLocationAlwaysAndWhenInUseUsageDescription`, und festhalten, ob das nötig war.
  `allowsBackgroundLocationUpdates = true`, `showsBackgroundLocationIndicator = true`.
- Auswahl der Stufe (Genau, Ausgewogen, Sparsam, N1, N2, Stillstand eigen,
  Stillstand-iOS-Pause) und der Aktivität (Rad 1 s, Wandern 3 s). Umschalten während der
  Messung muss gehen.
- Ein Protokoll je Lauf als CSV im Dokumentenverzeichnis, über die Dateien-App abholbar:
  - je Position: Zeit, Lat, Lon, Höhe, `horizontalAccuracy`, Geschwindigkeit
  - alle 5 min: `UIDevice.current.batteryLevel`, `ProcessInfo.thermalState`, Stufe,
    Hintergrund ja/nein
  - Ereignisse: Pausen, Fortsetzen, Beendigung, Neustart der App
- Die Anzeige: Laufzeit, Punkte, Akku-Start/jetzt, %/h. Bildschirm während der Messung
  gesperrt.

## Messablauf (der Nutzer, draußen)

- iPhone 13 Pro Max, Akku ≥ 80 %, Flugmodus an, Ortung an, keine anderen Apps aktiv,
  Helligkeit egal (Bildschirm aus).
- Jede Stufe **mindestens 1,5 h am Stück** in echter Bewegung (Rad oder zu Fuß), sonst sind die
  1-%-Schritte von `batteryLevel` zu grob. Dieselbe Strecke oder eine ähnliche.
- Dazu eine **Leerlaufmessung**: App ohne Ortung, 1,5 h gesperrt, als Grundlinie.
- Pause bei Stillstand: 20 min Pause mitten in einem Lauf.
- Notfall-Stufe: mindestens 3 h.

## Erfolg

1. Für jede Stufe: %/h (abzüglich Grundlinie), Punktabstand und Genauigkeit in einer Tabelle.
2. Antworten auf Frage 3–5 mit Beleg (Protokollauszug).
3. Daraus ein Vorschlag für die endgültigen Stufen mit geschätzter Laufzeit für einen vollen
   Akku. Beispielrechnung: Wie viele Tage reicht ein Akku plus eine 10-Ah-Powerbank je Stufe
   bei 8 h Fahrt am Tag?

## Stand Mac-Sitzung (2026-11-16)

Mini-App gebaut und im Simulator geprüft. Sie ist auf dem iPhone installiert, als
„Ortung-Test“. **Die Messungen draußen stehen noch aus.**

- Projekt: `spikes/ios-location/` (`xcodegen generate`, Schema `OrtungSpike`, für das Gerät
  Release bauen). Der Code liegt in `Sources/Recorder.swift`, die Stellschrauben stehen in
  `enum Werte`.
- Auswertung: `python3 spikes/ios-location/auswerten.py --grundlinie <%/h Leerlauf> lauf-*.csv`
  gibt die Tabelle für „Erfolg 1“ als Markdown aus. Außerdem legt das Skript neben jede CSV eine
  GPX, damit man im Desktop die Ecken in den Kurven ansehen kann.
- Im Simulator geprüft: Genau, „Stillstand eigen“ (erkennt den Stillstand, macht Proben und
  erkennt die Bewegung wieder), N1 (Takt an/aus) und das Fortsetzen im selben Protokoll nach
  einem Neustart. Der Simulator kann weder Akku noch Hintergrund-Verhalten noch Flugmodus
  zeigen, das geht nur auf dem Gerät.

### Was die Stufen genau tun

| Stufe | `desiredAccuracy` | `distanceFilter` | sonst |
| --- | --- | --- | --- |
| Leerlauf | – | – | Ortung aus, Grundlinie |
| Genau | Best | keiner | |
| Ausgewogen | Best | 10 m | |
| Sparsam | HundredMeters | 50 m | |
| N0 (Gegenprobe) | Best ↔ `stopUpdatingLocation` | keiner | Takt wie N1. Erwartet: Im Hintergrund bleibt sie nach dem ersten „aus“ stehen |
| N1 | Best ↔ ThreeKilometers | keiner | alle 5 min an, bis 5 s nach dem ersten Fix ≤ 25 m, höchstens 60 s |
| N2 | – | – | nur `startMonitoringSignificantLocationChanges` |
| Genau + Stillstand eigen | Best ↔ ThreeKilometers | keiner | 3 min innerhalb von 20 m → still. Dann alle 60 s eine Probe (Best, ≤ 30 s), sofort auch, wenn der Bewegungssensor Gehen/Rad meldet. Ab 25 m vom Anker gilt das als bewegt |
| Genau + Stillstand iOS | Best | keiner | `pausesLocationUpdatesAutomatically = true`, optional „Region-Weckruf“ (100-m-Region beim Pausieren) |

- Aktivität: Rad = `activityType .otherNavigation`, behält höchstens 1 Punkt/s. Wandern =
  `.fitness`, höchstens 1 Punkt alle 3 s. Mit `CLLocationManager` lässt sich der Takt des
  Empfängers nicht einstellen. Die 3 s werden also nur beim Speichern ausgedünnt und sparen
  kaum Strom. Das zeigt der Vergleich.
- „Neustart-Absicherung“ (für Frage 5) schaltet für jede Stufe
  `startMonitoringSignificantLocationChanges` und eine 100-m-Region um die letzte gute Position
  zu. **Nur für den Test von Frage 5 einschalten**, sonst verfälscht sie die Akkuwerte.
- Protokoll: Die Spalte `behalten` = 1 markiert die Punkte, die die echte App speichern würde.
  „Takt-Lücke“ heißt, der 5-s-Takt der App ist ausgefallen, die App war also angehalten. Das ist
  der Beleg für Frage 3. Akkuzeilen kommen alle 5 min und bei jedem 1-%-Schritt.
- MetricKit: Einmal am Tag legt iOS eine `metrickit-metrik-*.json` ab, die u. a. die
  GPS-Zeit je Genauigkeit enthält. Sie kommt frühestens am Tag nach der Messung und taucht
  dann unter „Protokolle“ auf.

### Messanleitung

1. Ortung-Test öffnen, „Beim Verwenden“ erlauben, dann „Ortung ‚Immer‘ erlauben“ tippen und
   in den Einstellungen auf „Immer“ stellen. Die Erlaubnis für Bewegung kommt beim ersten Lauf
   mit „Stillstand eigen“.
2. Stromsparmodus aus, Flugmodus an, Akku ≥ 80 %, nicht laden. Das Protokoll hält
   Stromsparmodus und Ladezustand fest.
3. Stufe und Aktivität wählen, „Start“, Bildschirm sperren. Nach ≥ 1,5 h die App öffnen,
   „Stopp“. Man kann auch während des Laufs die Stufe umstellen: Die Auswertung teilt nach Stufe
   auf, aber jeder Abschnitt braucht ≥ 1,5 h.
4. Reihenfolge (Vorschlag): Leerlauf (zu Hause), Genau, Ausgewogen, Sparsam, N1 (≥ 3 h),
   Stillstand eigen und Stillstand iOS mit je 20 min Pause, N0 kurz (15 min reichen), N2 kurz.
5. **Frage 5:** „Neustart-Absicherung“ an, Start, „Beenden-Test“, sofort sperren und
   weiterfahren. Im Protokoll suchen: „App neu gestartet, durch Ortung: true“? Danach einmal
   dasselbe mit einem Neustart des iPhones. Eine Gegenprobe ist das Beenden per Wischen in der
   App-Übersicht: Laut Apple startet iOS die App dann nicht neu.
6. Protokolle holen: In der App bei „Protokolle“ auf die Datei tippen und teilen, oder über
   die Dateien-App → Auf meinem iPhone → Ortung-Test.

Stolpersteine:
- **Kostenloses Entwicklerkonto:** Höchstens 3 selbst signierte Apps dürfen gleichzeitig
  installiert sein, und dabei zählt auch `OutdoorUITests-Runner` mit. Zum Installieren musste
  der Runner weg. Vor den nächsten UI-Tests von Outdoor muss Ortung-Test wieder
  gelöscht werden. Außerdem laufen die Profile nach 7 Tagen ab. Wenn die Messreihe länger
  dauert, die App neu aufspielen. Die Protokolle gehen dabei nicht verloren, solange die App
  nicht gelöscht wird.
- Simulator: Er liefert im Stand keine Positionen, deshalb zählt bei „Stillstand eigen“ auch
  „lange kein Punkt“ als Stillstand. Mit `-autostart <stufe> -schnell` (Zeiten /10) startet ein
  Lauf ohne Tippen.

## Ergebnis

*(nach den Messungen draußen auszufüllen)*
